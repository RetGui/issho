use alloc::{
    rc::{Rc, Weak},
    string::String,
    vec::Vec,
};

use core::cell::RefCell;

use hashbrown::HashMap;

use objc2::MainThreadMarker;
use objc2::rc::{Retained, Weak as ObjcWeak};
use objc2_app_kit::{
    NSAccessibilityPostNotification, NSAccessibilitySelectedChildrenChangedNotification, NSView,
};
use objc2_foundation::{NSPoint, NSRect};

use super::{
    element::Element,
    geometry::{tree_point, view_rect},
    model::{ElementId, ElementKind, NodeState, is_leaf_control},
};

use crate::access_tree::WeakAccessTree;
use crate::{
    AccessEvent, AccessKey, AccessNodeContext, AccessRect, AccessTree, AccessWindow, Role,
    ScrollData, SelectionData,
};

pub(super) trait AccessTreeAdapter {
    fn state(&self, key: AccessKey) -> Option<NodeState>;
    fn parent(&self, key: AccessKey) -> Option<AccessKey>;
    fn is_included(&self, key: AccessKey) -> bool;
    fn filtered_parent(&self, key: AccessKey) -> Option<AccessKey>;
    fn filtered_children(&self, key: AccessKey) -> Vec<AccessKey>;
    fn filtered_roots(&self) -> Vec<AccessKey>;
    fn name(&self, key: AccessKey) -> Option<String>;
    fn value(&self, key: AccessKey) -> Option<String>;
    fn focus(&self) -> Option<AccessKey>;
    fn set_focus(&self, key: AccessKey);
    fn element(&self, key: AccessKey) -> Option<Retained<Element>>;
    fn element_for(&self, id: ElementId) -> Option<Retained<Element>>;
    fn view(&self) -> Option<Retained<NSView>>;
    fn frame(&self, key: AccessKey) -> NSRect;
    fn hit_test(&self, point: NSPoint) -> Option<AccessKey>;
    fn dispatch(&self, key: AccessKey, event: AccessEvent) -> bool;
    fn prune(&self);
    fn selection(&self, key: AccessKey) -> Vec<AccessKey>;
    fn set_selected(&self, key: AccessKey, selected: bool) -> bool;
    fn replace_selection(&self, group: AccessKey, desired: &[AccessKey]) -> bool;
}

type ElementCache = HashMap<ElementId, Retained<Element>>;

pub(super) struct MacAccessTreeAdapter<T: AccessWindow, U: AccessNodeContext> {
    tree: WeakAccessTree<T, U>,
    view: ObjcWeak<NSView>,
    root: AccessKey,
    own: Weak<Self>,
    elements: RefCell<ElementCache>,
}

impl<T: AccessWindow, U: AccessNodeContext> MacAccessTreeAdapter<T, U> {
    pub(super) fn new(
        tree: &AccessTree<T, U>,
        view: &Retained<NSView>,
        root: AccessKey,
    ) -> Rc<Self> {
        Self::from_tree(tree, ObjcWeak::from_retained(view), root)
    }

    fn from_tree(tree: &AccessTree<T, U>, view: ObjcWeak<NSView>, root: AccessKey) -> Rc<Self> {
        Rc::new_cyclic(|own| Self {
            tree: tree.downgrade(),
            view,
            root,
            own: own.clone(),
            elements: RefCell::new(HashMap::new()),
        })
    }

    fn tree(&self, key: AccessKey) -> Option<AccessTree<T, U>> {
        let tree = self.tree.upgrade()?;
        if tree.get_node_root(key) != Some(self.root)
            || (tree.is_hidden(key) && tree.get_focus(self.root) != Some(key))
        {
            return None;
        }
        let mut parent = tree.get_parent(key);
        while let Some(key) = parent {
            let node = tree.get_node(key)?;
            if is_leaf_control(node.role(), *node.scroll_data()) {
                return None;
            }
            parent = node.parent;
        }
        Some(tree)
    }

    pub(super) fn children(&self, key: AccessKey) -> Vec<AccessKey> {
        let Some(tree) = self.tree(key) else {
            return Vec::new();
        };
        if tree
            .get_node(key)
            .is_some_and(|node| is_leaf_control(node.role(), *node.scroll_data()))
        {
            return Vec::new();
        }
        tree.get_node(key)
            .map(|node| {
                node.children
                    .iter()
                    .copied()
                    .filter(|key| !tree.is_hidden(*key))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn bounds(&self, key: AccessKey) -> Option<AccessRect> {
        Some(self.tree(key)?.get_node(key)?.bounding_rect())
    }

    fn hit(&self, x: f64, y: f64) -> Option<AccessKey> {
        let tree = self.tree.upgrade()?;
        let mut pending = Vec::from([(self.root, false)]);
        while let Some((key, test_bounds)) = pending.pop() {
            if test_bounds {
                if tree.get_node(key)?.bounding_rect().contains(x, y) {
                    return Some(key);
                }
                continue;
            }
            if self.tree(key).is_none() {
                continue;
            }
            if self.is_included(key) {
                pending.push((key, true));
            }
            let node = tree.get_node(key)?;
            if !is_leaf_control(node.role(), *node.scroll_data()) {
                pending.extend(node.children.iter().copied().map(|key| (key, false)));
            }
        }
        None
    }

    pub(super) fn selection_group(&self, key: AccessKey) -> Option<AccessKey> {
        self.state(key)?.selected?;
        let mut parent = self.parent(key);
        while let Some(key) = parent {
            if self.state(key)?.selection_group.is_some() {
                return Some(key);
            }
            parent = self.parent(key);
        }
        None
    }

    pub(super) fn selection_items(&self, key: AccessKey) -> Vec<AccessKey> {
        let mut result = Vec::new();
        let mut pending = self.children(key);
        pending.reverse();
        while let Some(key) = pending.pop() {
            let Some(state) = self.state(key) else {
                continue;
            };
            if state.selected.is_some() {
                result.push(key);
            }
            if state.selection_group.is_none() {
                pending.extend(self.children(key).into_iter().rev());
            }
        }
        result
    }

    pub(super) fn selection(&self, key: AccessKey) -> Vec<AccessKey> {
        self.selection_items(key)
            .into_iter()
            .filter(|key| {
                self.state(*key)
                    .is_some_and(|state| state.selected == Some(true))
            })
            .collect()
    }

    pub(super) fn set_selected(&self, key: AccessKey, selected: bool) -> bool {
        let Some(state) = self.state(key) else {
            return false;
        };
        if !state.enabled || state.selected.is_none() {
            return false;
        }
        if state.selected == Some(selected) {
            return true;
        }
        let Some(group) = self.selection_group(key) else {
            return false;
        };
        let Some(policy) = group_policy(self, group) else {
            return false;
        };
        if !selected && policy.mandatory && self.selection(group).len() <= 1 {
            return false;
        }
        self.dispatch(
            key,
            if !selected {
                AccessEvent::UnSelect
            } else if policy.multiple {
                AccessEvent::AddToSelection
            } else {
                AccessEvent::Select
            },
        )
    }

    pub(super) fn replace_selection(&self, group: AccessKey, desired: &[AccessKey]) -> bool {
        let Some(policy) = group_policy(self, group) else {
            return false;
        };
        let request = SelectionRequest {
            group,
            policy,
            desired,
        };
        if !request.is_current(self) {
            return false;
        }
        let selected = self.selection(group);
        if selected.len() == desired.len() && selected.iter().all(|key| desired.contains(key)) {
            return true;
        }
        if !policy.multiple
            && let Some(key) = desired.first()
        {
            return self.dispatch(*key, AccessEvent::Select);
        }
        for key in desired {
            if !request.is_current(self) {
                return false;
            }
            if !self
                .state(*key)
                .is_some_and(|state| state.selected == Some(true))
                && !self.dispatch(*key, AccessEvent::AddToSelection)
            {
                return false;
            }
        }
        for key in selected.into_iter().filter(|key| !desired.contains(key)) {
            if !request.is_current(self) || self.selection_group(key) != Some(group) {
                return false;
            }
            let Some(state) = self.state(key) else {
                return false;
            };
            if state.selected != Some(true) {
                continue;
            }
            if !state.enabled || (policy.mandatory && self.selection(group).len() <= 1) {
                return false;
            }
            if !self.dispatch(key, AccessEvent::UnSelect) {
                return false;
            }
        }
        true
    }
}

impl<T: AccessWindow, U: AccessNodeContext> AccessTreeAdapter for MacAccessTreeAdapter<T, U> {
    fn state(&self, key: AccessKey) -> Option<NodeState> {
        let tree = self.tree(key)?;
        let node = tree.get_node(key)?;
        Some(NodeState {
            id: node.id(),
            role: node.role(),
            checked: node.checked(),
            enabled: node.enabled(),
            selected: match node.selection_data() {
                Some(SelectionData::SelectionGroupItem(item)) => Some(item.is_selected),
                _ => None,
            },
            selection_group: match node.selection_data() {
                Some(SelectionData::SelectionGroup(group)) => Some(group.clone()),
                _ => None,
            },
            text_selection: node.supports_text_selection(),
            scroll: *node.scroll_data(),
        })
    }

    fn parent(&self, key: AccessKey) -> Option<AccessKey> {
        self.tree(key)?.get_parent(key)
    }

    fn is_included(&self, key: AccessKey) -> bool {
        let Some(tree) = self.tree(key) else {
            return false;
        };
        tree.get_focus(self.root) == Some(key)
            || tree.get_node(key).is_some_and(|node| {
                node.role() != Role::GenericContainer
                    || matches!(node.scroll_data(), ScrollData::ScrollContainer(_))
            })
    }

    fn filtered_parent(&self, key: AccessKey) -> Option<AccessKey> {
        let tree = self.tree(key)?;
        let mut parent = tree.get_parent(key);
        while let Some(key) = parent {
            if self.is_included(key) {
                return Some(key);
            }
            parent = tree.get_parent(key);
        }
        None
    }

    fn filtered_children(&self, key: AccessKey) -> Vec<AccessKey> {
        let Some(tree) = self.tree(key) else {
            return Vec::new();
        };
        let Some(node) = tree.get_node(key) else {
            return Vec::new();
        };
        if is_leaf_control(node.role(), *node.scroll_data()) {
            return Vec::new();
        }
        let mut pending: Vec<_> = node.children.iter().copied().rev().collect();
        drop(node);
        let mut result = Vec::new();
        while let Some(key) = pending.pop() {
            if self.tree(key).is_none() {
                continue;
            }
            if self.is_included(key) {
                result.push(key);
            } else if let Some(node) = tree.get_node(key) {
                pending.extend(node.children.iter().copied().rev());
            }
        }
        result
    }

    fn filtered_roots(&self) -> Vec<AccessKey> {
        if self.is_included(self.root) {
            Vec::from([self.root])
        } else {
            self.filtered_children(self.root)
        }
    }

    fn name(&self, key: AccessKey) -> Option<String> {
        let tree = self.tree(key)?;
        let node = tree.get_node(key)?;
        if matches!(
            node.role(),
            Role::GenericContainer | Role::Group | Role::Window
        ) || matches!(node.scroll_data(), ScrollData::ScrollContainer(_))
        {
            return Some(String::from(node.name()));
        }
        drop(node);
        tree.accessible_name(key)
    }

    fn value(&self, key: AccessKey) -> Option<String> {
        Some(String::from(self.tree(key)?.get_node(key)?.value()))
    }

    fn focus(&self) -> Option<AccessKey> {
        let tree = self.tree(self.root)?;
        tree.get_focus(self.root)
            .filter(|key| self.tree(*key).is_some())
    }

    fn set_focus(&self, key: AccessKey) {
        if let Some(tree) = self.tree(key)
            && tree
                .get_node(key)
                .is_some_and(|node| node.enabled() && node.role().is_keyboard_focusable())
        {
            tree.set_focus(self.root, Some(key));
        }
    }

    fn element(&self, key: AccessKey) -> Option<Retained<Element>> {
        self.element_for(ElementId {
            key,
            kind: ElementKind::Node,
        })
    }

    fn element_for(&self, id: ElementId) -> Option<Retained<Element>> {
        self.state(id.key)?;
        let mtm = MainThreadMarker::new()?;
        if let Some(element) = self.elements.borrow().get(&id) {
            return Some(element.clone());
        }
        let adapter: Rc<dyn AccessTreeAdapter> = self.own.upgrade()?;
        let element = Element::new(Rc::downgrade(&adapter), id, mtm);
        self.elements.borrow_mut().insert(id, element.clone());
        Some(element)
    }

    fn view(&self) -> Option<Retained<NSView>> {
        self.view.load()
    }

    fn frame(&self, key: AccessKey) -> NSRect {
        let frame = || {
            let rect = self.bounds(key)?;
            let view = self.view()?;
            let window = view.window()?;
            let local = view_rect(
                rect,
                view.bounds(),
                window.backingScaleFactor(),
                view.isFlipped(),
            );
            Some(window.convertRectToScreen(view.convertRect_toView(local, None)))
        };
        frame().unwrap_or_default()
    }

    fn hit_test(&self, point: NSPoint) -> Option<AccessKey> {
        let view = self.view()?;
        let window = view.window()?;
        let local = view.convertPoint_fromView(window.convertPointFromScreen(point), None);
        let (x, y) = tree_point(
            local,
            view.bounds(),
            window.backingScaleFactor(),
            view.isFlipped(),
        );
        self.hit(x, y)
            .or_else(|| self.tree(self.root).map(|_| self.root))
    }

    fn dispatch(&self, key: AccessKey, event: AccessEvent) -> bool {
        let Some(tree) = self.tree(key) else {
            return false;
        };
        if !tree.get_node(key).is_some_and(|node| node.enabled()) {
            return false;
        }
        let selection = matches!(
            event,
            AccessEvent::Select | AccessEvent::AddToSelection | AccessEvent::UnSelect
        );
        let group = selection.then(|| self.selection_group(key)).flatten();
        let before = group.map(|group| self.selection(group));
        if tree.dispatch_access_event(key, event).is_err() {
            return false;
        }
        if let Some(group) = group
            && before.as_ref() != Some(&self.selection(group))
            && let Some(element) = self.element(group)
        {
            unsafe {
                NSAccessibilityPostNotification(
                    &element,
                    NSAccessibilitySelectedChildrenChangedNotification,
                );
            }
        }
        true
    }

    fn prune(&self) {
        let tree = self.tree.upgrade();
        self.elements.borrow_mut().retain(|id, _| {
            tree.as_ref()
                .is_some_and(|tree| tree.get_node_root(id.key) == Some(self.root))
        });
    }

    fn selection(&self, key: AccessKey) -> Vec<AccessKey> {
        MacAccessTreeAdapter::selection(self, key)
    }

    fn set_selected(&self, key: AccessKey, selected: bool) -> bool {
        MacAccessTreeAdapter::set_selected(self, key, selected)
    }

    fn replace_selection(&self, group: AccessKey, desired: &[AccessKey]) -> bool {
        MacAccessTreeAdapter::replace_selection(self, group, desired)
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct SelectionPolicy {
    mandatory: bool,
    multiple: bool,
}
fn group_policy<T: AccessWindow, U: AccessNodeContext>(
    tree: &MacAccessTreeAdapter<T, U>,
    key: AccessKey,
) -> Option<SelectionPolicy> {
    let state = tree.state(key)?;
    if !state.enabled {
        return None;
    }
    let group = state.selection_group?;
    Some(SelectionPolicy {
        mandatory: group.is_mandatory,
        multiple: group.multiple_selectable,
    })
}

struct SelectionRequest<'a> {
    group: AccessKey,
    policy: SelectionPolicy,
    desired: &'a [AccessKey],
}

impl SelectionRequest<'_> {
    fn is_current<T: AccessWindow, U: AccessNodeContext>(
        &self,
        tree: &MacAccessTreeAdapter<T, U>,
    ) -> bool {
        group_policy(tree, self.group) == Some(self.policy)
            && (self.policy.multiple || self.desired.len() <= 1)
            && (!self.policy.mandatory || !self.desired.is_empty())
            && self.desired.iter().all(|key| {
                tree.selection_group(*key) == Some(self.group)
                    && tree.state(*key).is_some_and(|state| state.enabled)
            })
    }
}
