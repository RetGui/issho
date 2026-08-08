use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::String;

#[cfg(target_os = "windows")]
use alloc::rc::Weak;

use core::cell::{Ref, RefCell, RefMut};

use hashbrown::HashMap;

use slotmap::{DefaultKey, SlotMap};
use smol_str::SmolStr;

use crate::access_node::AccessNode;
use crate::access_window::AccessNodeContext;
use crate::platforms::{AccessPlatform, BlankPlatform};
use crate::{
    AccessEvent, AccessKey, AccessProperty, AccessPropertyValue, AccessWindow, IsshoError,
};

/// Handles accessibility events.
pub trait AccessEventHandler<T: AccessWindow, U: AccessNodeContext>:
    Fn(&AccessTree<T, U>, AccessKey, AccessEvent) -> Result<(), IsshoError> + 'static
{
}

impl<T, U, F> AccessEventHandler<T, U> for F
where
    T: AccessWindow,
    U: AccessNodeContext,
    F: Fn(&AccessTree<T, U>, AccessKey, AccessEvent) -> Result<(), IsshoError> + 'static,
{
}

/// A collection of accessibility nodes exposed to the native platform.
///
/// A tree can contain multiple roots, with each root representing a window.
/// Cloning an `AccessTree` creates another handle to the same collection.
#[derive(Clone)]
pub struct AccessTree<T: AccessWindow, U: AccessNodeContext> {
    internal: Rc<RefCell<AccessTreeInternal<T, U>>>,
}

#[cfg(target_os = "windows")]
#[derive(Clone)]
pub(crate) struct WeakAccessTree<T: AccessWindow, U: AccessNodeContext> {
    internal: Weak<RefCell<AccessTreeInternal<T, U>>>,
}

impl<T: AccessWindow, U: AccessNodeContext> AccessTree<T, U> {
    /// Creates an empty accessibility tree backed by a no-op platform.
    pub fn new() -> Self {
        Self {
            internal: Rc::new(RefCell::new(AccessTreeInternal::new())),
        }
    }

    /// Adds a node to the tree and returns its key.
    ///
    /// If `parent` is provided, the new node is appended to the parent's list of
    /// children. Otherwise, the node becomes a new root.
    ///
    /// # Panics
    ///
    /// Panics if `parent` is `Some` but is not a node in this tree.
    pub fn insert_node(&self, node: AccessNode<U>, parent: Option<AccessKey>) -> AccessKey {
        self.internal.borrow_mut().insert_node(node, parent)
    }

    /// Returns `true` if both handles refer to the same tree.
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.internal, &other.internal)
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn downgrade(&self) -> WeakAccessTree<T, U> {
        WeakAccessTree {
            internal: Rc::downgrade(&self.internal),
        }
    }

    /// Returns `true` if `node` belongs to this tree.
    pub fn contains_node(&self, node: AccessKey) -> bool {
        self.internal.borrow().nodes.contains_key(node)
    }

    /// Replaces a node without changing its identity or position in the tree.
    ///
    /// The replacement inherits the existing node's ID, parent, and children.
    /// Changes to its name, value, and checked state are reported to the native
    /// accessibility platform.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not in this tree.
    pub fn update_node(&self, node: AccessKey, mut replacement: AccessNode<U>) {
        let (old_name, old_value, old_checked, name_changed, value_changed, checked_changed) = {
            let mut internal = self.internal.borrow_mut();
            let current = internal.nodes.get_mut(node).expect("node not found");

            replacement.parent = current.parent;
            replacement.children = core::mem::take(&mut current.children);
            replacement.id = current.id;

            let name_changed = current.name() != replacement.name();
            let value_changed = current.value() != replacement.value();
            let checked_changed = current.checked() != replacement.checked();
            let old_name = name_changed.then(|| String::from(current.name()));
            let old_value = value_changed.then(|| String::from(current.value()));
            let old_checked = current.checked();
            *current = replacement;

            (
                old_name,
                old_value,
                old_checked,
                name_changed,
                value_changed,
                checked_changed,
            )
        };

        let platform = self.internal.borrow().platform.clone();
        if checked_changed {
            let current_checked = self.get_node(node).expect("node not found").checked();
            let _ = platform.property_changed(
                node,
                AccessProperty::Checked,
                AccessPropertyValue::Bool(old_checked),
                AccessPropertyValue::Bool(current_checked),
                self,
            );
        }
        if name_changed {
            let current_name = String::from(self.get_node(node).expect("node not found").name());
            let _ = platform.property_changed(
                node,
                AccessProperty::Name,
                AccessPropertyValue::Text(
                    old_name.as_deref().expect("changed name has an old value"),
                ),
                AccessPropertyValue::Text(&current_name),
                self,
            );
        }
        if value_changed {
            let current_value = String::from(self.get_node(node).expect("node not found").value());
            let _ = platform.property_changed(
                node,
                AccessProperty::Value,
                AccessPropertyValue::Text(
                    old_value
                        .as_deref()
                        .expect("changed value has an old value"),
                ),
                AccessPropertyValue::Text(&current_value),
                self,
            );
        }
    }

    /// Replaces the ordered list of children belonging to `parent`.
    ///
    /// Existing children omitted from the new list are removed along with their
    /// descendants. Nodes that already belong to the tree are moved under
    /// `parent` without changing their identity.
    ///
    /// # Panics
    ///
    /// Panics if `parent` or any entry in `children` is not in this tree.
    pub fn set_children(&self, parent: AccessKey, children: &[AccessKey]) {
        self.internal.borrow_mut().set_children(parent, children);
    }

    /// Removes a node and all of its descendants.
    ///
    /// Does nothing if `node` is not in this tree.
    pub fn remove_node(&self, node: AccessKey) {
        self.internal.borrow_mut().remove_node(node);
    }

    /// Detaches a node from its parent while preserving its descendants.
    ///
    /// The node becomes a root, but is not associated with a window.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not in this tree.
    pub fn detach_node(&self, node: AccessKey) {
        self.internal.borrow_mut().detach_node(node);
    }

    /// Moves `child` to the end of `parent`'s children.
    ///
    /// The child's descendants are preserved. If it already has a parent, it is
    /// first removed from that parent's child list.
    ///
    /// # Panics
    ///
    /// Panics if either node is not in this tree, if `parent` and `child` are the
    /// same node, or if moving the child would create a cycle.
    pub fn append_child(&self, parent: AccessKey, child: AccessKey) {
        self.internal.borrow_mut().append_child(parent, child);
    }

    /// Returns a shared reference to `node`.
    ///
    /// Returns `None` if `node` does not belong to this tree.
    pub fn get_node(&self, node: AccessKey) -> Option<Ref<'_, AccessNode<U>>> {
        Ref::filter_map(self.internal.borrow(), |internal| internal.nodes.get(node)).ok()
    }

    /// Returns mutable access to `node`.
    ///
    /// Returns `None` if `node` does not belong to this tree. Changes made
    /// through the returned guard are not reported to the native accessibility
    /// platform.
    pub fn get_node_mut(&self, node: AccessKey) -> Option<RefMut<'_, AccessNode<U>>> {
        RefMut::filter_map(self.internal.borrow_mut(), |internal| {
            internal.nodes.get_mut(node)
        })
        .ok()
    }

    /// Returns the accessible name for `node`.
    ///
    /// If the node has no name of its own, its descendants are searched in tree
    /// order for the first non-empty name. Returns an empty string if no name is
    /// found, or `None` if `node` does not belong to this tree.
    pub fn accessible_name(&self, node: AccessKey) -> Option<String> {
        self.internal.borrow().accessible_name(node)
    }

    /// Updates a node's name and notifies the native accessibility platform.
    ///
    /// No notification is sent if the name is unchanged.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not in this tree.
    pub fn set_name(&self, node: AccessKey, name: impl Into<SmolStr>) {
        let new_name = name.into();
        let old_name = {
            let mut node = self.get_node_mut(node).expect("node not found");
            if node.name() == new_name {
                return;
            }

            node.replace_name(new_name.clone())
        };
        let platform = {
            let internal = self.internal.borrow();
            internal.platform.clone()
        };

        let _ = platform.property_changed(
            node,
            AccessProperty::Name,
            AccessPropertyValue::Text(&old_name),
            AccessPropertyValue::Text(&new_name),
            self,
        );
    }

    /// Updates a node's value and notifies the native accessibility platform.
    ///
    /// No notification is sent if the value is unchanged.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not in this tree.
    pub fn set_value(&self, node: AccessKey, value: impl Into<String>) {
        let new_value = value.into();
        let old_value = {
            let mut node = self.get_node_mut(node).expect("node not found");
            if node.value() == new_value {
                return;
            }

            node.replace_value(new_value.clone())
        };
        let platform = {
            let internal = self.internal.borrow();
            internal.platform.clone()
        };

        let _ = platform.property_changed(
            node,
            AccessProperty::Value,
            AccessPropertyValue::Text(&old_value),
            AccessPropertyValue::Text(&new_value),
            self,
        );
    }

    /// Updates a node's checked state and notifies the native platform.
    ///
    /// No notification is sent if the checked state is unchanged.
    ///
    /// # Panics
    ///
    /// Panics if `node` is not in this tree.
    pub fn set_checked(&self, node: AccessKey, checked: bool) {
        let old_checked = {
            let mut node = self.get_node_mut(node).expect("node not found");
            if node.checked() == checked {
                return;
            }

            let old_checked = node.checked();
            node.set_checked(checked);
            old_checked
        };
        let platform = {
            let internal = self.internal.borrow();
            internal.platform.clone()
        };

        let _ = platform.property_changed(
            node,
            AccessProperty::Checked,
            AccessPropertyValue::Bool(old_checked),
            AccessPropertyValue::Bool(checked),
            self,
        );
    }

    /// Associates a root node with a window.
    ///
    /// # Panics
    ///
    /// Panics if `root` is not a root node in this tree.
    pub fn set_root_window(&self, root: AccessKey, window: T) {
        self.internal.borrow_mut().set_root_window(root, window);
    }

    /// Returns the window associated with a root node.
    ///
    /// Returns `None` if no window has been associated with the root.
    ///
    /// # Panics
    ///
    /// Panics if `root` is not a root node in this tree.
    pub fn get_root_window(&self, root: AccessKey) -> Option<T> {
        self.internal.borrow().get_root_window(root).cloned()
    }

    /// Sets the focused node within `root`.
    ///
    /// No notification is sent if the focus has not changed. Passing `None`
    /// clears the focus without notifying the platform.
    ///
    /// # Panics
    ///
    /// Panics if `root` is not a root node in this tree, or if `focus` is `Some`
    /// but is not within that root's subtree.
    pub fn set_focus(&self, root: AccessKey, focus: Option<AccessKey>) {
        if self.get_focus(root) == focus {
            return;
        }
        self.internal.borrow_mut().set_focus(root, focus);

        if let Some(focus) = focus {
            let platform = self.internal.borrow().platform.clone();
            let _ = platform.focus_changed(focus, self);
        }
    }

    /// Returns the focused node within `root`, if any.
    pub fn get_focus(&self, root: AccessKey) -> Option<AccessKey> {
        self.internal.borrow().get_focus(root)
    }

    /// Sets the name of the UI framework that owns this tree.
    ///
    /// On Windows, this is exposed through `UIA_FrameworkIdPropertyId`.
    pub fn set_framework_name(&self, framework_name: impl Into<String>) {
        self.internal.borrow_mut().framework_name = framework_name.into();
    }

    /// Returns the name of the UI framework that owns this tree.
    pub fn framework_name(&self) -> String {
        self.internal.borrow().framework_name.clone()
    }

    /// Returns the root node associated with `window`.
    ///
    /// Returns `None` if the window does not expose a raw handle or no root is
    /// associated with the same raw handle.
    pub fn get_window_root(&self, window: &T) -> Option<AccessKey> {
        let window_handle = window.window_handle().ok()?.as_raw();
        self.internal
            .borrow()
            .roots
            .iter()
            .find_map(|(root, root_window)| {
                let root_window_handle = root_window.as_ref()?.window_handle().ok()?.as_raw();
                (root_window_handle == window_handle).then_some(*root)
            })
    }

    /// Returns the parent of `node`.
    ///
    /// Returns `None` if the node is a root or does not belong to this tree.
    pub fn get_parent(&self, node: AccessKey) -> Option<AccessKey> {
        self.internal.borrow().nodes.get(node)?.parent
    }

    /// Returns the root that contains `node`.
    ///
    /// Returns `None` if `node` is not in the tree.
    pub fn get_node_root(&self, node: AccessKey) -> Option<AccessKey> {
        self.internal.borrow().get_node_root(node)
    }

    /// Returns `true` if `node` is the focused node within its root.
    ///
    /// Returns `false` if `node` is not in the tree.
    pub fn is_focused(&self, node: AccessKey) -> bool {
        let internal = self.internal.borrow();
        let Some(root) = internal.get_node_root(node) else {
            return false;
        };
        internal.get_focus(root) == Some(node)
    }

    /// Returns the first child of `node`, if it has one.
    ///
    /// Returns `None` if `node` is not in the tree.
    pub fn get_first_child(&self, node: AccessKey) -> Option<AccessKey> {
        self.internal
            .borrow()
            .nodes
            .get(node)?
            .children
            .first()
            .copied()
    }

    /// Returns the last child of `node`, if it has one.
    ///
    /// Returns `None` if `node` is not in the tree.
    pub fn get_last_child(&self, node: AccessKey) -> Option<AccessKey> {
        self.internal
            .borrow()
            .nodes
            .get(node)?
            .children
            .last()
            .copied()
    }

    /// Returns the sibling immediately after `node`, if there is one.
    ///
    /// Returns `None` if `node` is a root or is not in the tree.
    pub fn get_next_sibling(&self, node: AccessKey) -> Option<AccessKey> {
        self.internal.borrow().get_sibling(node, 1)
    }

    /// Returns the sibling immediately before `node`, if there is one.
    ///
    /// Returns `None` if `node` is a root or is not in the tree.
    pub fn get_previous_sibling(&self, node: AccessKey) -> Option<AccessKey> {
        self.internal.borrow().get_sibling(node, -1)
    }

    /// Finds the deepest node beneath `root` that contains `(x, y)`.
    ///
    /// Coordinates are relative to the root window's client area. Later
    /// children are treated as appearing on top of earlier siblings. Returns
    /// `None` if `root` does not belong to this tree or the point lies outside
    /// its bounds.
    pub fn element_from_point(&self, root: AccessKey, x: f64, y: f64) -> Option<AccessKey> {
        self.internal.borrow().element_from_point(root, x, y)
    }

    /// Installs the accessibility platform used by this tree.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the platform cannot be registered. The current platform
    /// remains installed if registration fails.
    pub fn set_platform(&self, platform: Box<dyn AccessPlatform<T, U>>) -> Result<(), ()> {
        self.internal.borrow_mut().set_platform(platform)
    }

    /// Installs the native accessibility platform for the current target.
    ///
    /// If registration fails, the current platform remains installed. Targets
    /// without a native implementation use a no-op platform.
    pub fn set_native_platform(&self) {
        let _ = self.internal.borrow_mut().set_native_platform();
    }

    /// Sets the callback to be used when ui related accessibility events occur.
    ///
    /// By default, the callback is a no-op.
    pub fn set_on_access_event<EventHandler>(&self, on_access_event: EventHandler)
    where
        EventHandler: AccessEventHandler<T, U>,
    {
        self.internal
            .borrow_mut()
            .set_on_access_event(on_access_event);
    }

    /// Registers `window` with the tree's current accessibility platform.
    ///
    /// The window must first be associated with a root using
    /// [`set_root_window`](Self::set_root_window).
    ///
    /// Registration errors are ignored.
    pub fn register_window(&self, window: T) {
        let _ = self
            .internal
            .borrow()
            .platform
            .register_window(window.clone(), self);
    }

    /// Dispatches an access event.
    pub fn dispatch_access_event(
        &self,
        node: AccessKey,
        event: AccessEvent,
    ) -> Result<(), IsshoError> {
        let handler = self.internal.borrow().on_access_event.clone();
        handler(self, node, event)
    }
}

#[cfg(target_os = "windows")]
impl<T: AccessWindow, U: AccessNodeContext> WeakAccessTree<T, U> {
    pub(crate) fn upgrade(&self) -> Option<AccessTree<T, U>> {
        Some(AccessTree {
            internal: self.internal.upgrade()?,
        })
    }
}

/// Stores accessibility data.
struct AccessTreeInternal<T: AccessWindow, U: AccessNodeContext> {
    roots: HashMap<DefaultKey, Option<T>>,
    nodes: SlotMap<DefaultKey, AccessNode<U>>,
    focused_nodes: HashMap<DefaultKey, DefaultKey>,
    framework_name: String,
    platform: Rc<dyn AccessPlatform<T, U>>,
    on_access_event: Rc<dyn AccessEventHandler<T, U>>,
}

impl<T: AccessWindow, U: AccessNodeContext> Default for AccessTreeInternal<T, U> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: AccessWindow, U: AccessNodeContext> Default for AccessTree<T, U> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: AccessWindow, U: AccessNodeContext> AccessTreeInternal<T, U> {
    fn new() -> Self {
        Self {
            roots: HashMap::default(),
            nodes: SlotMap::new(),
            focused_nodes: HashMap::default(),
            framework_name: String::new(),
            platform: Rc::new(BlankPlatform::new()),
            on_access_event: Rc::new(|_, _, _| Ok(())),
        }
    }

    fn insert_node(&mut self, mut node: AccessNode<U>, parent: Option<DefaultKey>) -> DefaultKey {
        node.parent = parent;
        let key = self.nodes.insert(node);
        if let Some(parent) = parent {
            let parent = self.nodes.get_mut(parent).expect("Parent not found");
            parent.children.push(key);
        } else {
            self.roots.insert(key, None);
        }
        key
    }

    fn set_children(&mut self, parent: DefaultKey, children: &[DefaultKey]) {
        assert!(self.nodes.contains_key(parent), "parent not found");
        assert!(
            children.iter().all(|child| self.nodes.contains_key(*child)),
            "child not found"
        );

        let old_children = core::mem::take(&mut self.nodes.get_mut(parent).unwrap().children);
        for old_child in old_children {
            if !children.contains(&old_child) {
                self.remove_node(old_child);
            }
        }

        for child in children {
            let old_parent = self.nodes.get(*child).unwrap().parent;
            if let Some(old_parent) = old_parent
                && old_parent != parent
                && let Some(old_parent) = self.nodes.get_mut(old_parent)
            {
                old_parent.children = old_parent
                    .children
                    .iter()
                    .copied()
                    .filter(|candidate| candidate != child)
                    .collect();
            }
            self.roots.remove(child);
            self.focused_nodes.remove(child);
            self.nodes.get_mut(*child).unwrap().parent = Some(parent);
        }

        self.nodes
            .get_mut(parent)
            .unwrap()
            .children
            .extend(children.iter().copied());
    }

    fn detach_node(&mut self, node: DefaultKey) {
        assert!(self.nodes.contains_key(node), "node not found");
        let old_root = self.get_node_root(node).expect("node not found");
        let old_parent = self.nodes.get(node).unwrap().parent;
        if let Some(old_parent) = old_parent
            && let Some(parent) = self.nodes.get_mut(old_parent)
        {
            parent.children = parent
                .children
                .iter()
                .copied()
                .filter(|child| *child != node)
                .collect();
        }

        self.nodes.get_mut(node).unwrap().parent = None;
        self.roots.insert(node, None);

        if let Some(focus) = self.focused_nodes.get(&old_root).copied()
            && self.get_node_root(focus) == Some(node)
        {
            self.focused_nodes.remove(&old_root);
        }
    }

    fn append_child(&mut self, parent: DefaultKey, child: DefaultKey) {
        assert!(self.nodes.contains_key(parent), "parent not found");
        assert!(self.nodes.contains_key(child), "child not found");
        assert_ne!(parent, child, "node cannot parent itself");

        let mut ancestor = Some(parent);
        while let Some(current) = ancestor {
            assert_ne!(current, child, "reparenting would create a cycle");
            ancestor = self.nodes.get(current).and_then(|node| node.parent);
        }

        if let Some(old_parent) = self.nodes.get(child).unwrap().parent
            && let Some(old_parent) = self.nodes.get_mut(old_parent)
        {
            old_parent.children = old_parent
                .children
                .iter()
                .copied()
                .filter(|candidate| *candidate != child)
                .collect();
        }

        self.roots.remove(&child);
        self.focused_nodes.remove(&child);
        self.nodes.get_mut(child).unwrap().parent = Some(parent);
        let children = &mut self.nodes.get_mut(parent).unwrap().children;
        if !children.contains(&child) {
            children.push(child);
        }
    }

    fn remove_node(&mut self, node: DefaultKey) {
        let Some(removed) = self.nodes.remove(node) else {
            return;
        };

        if let Some(parent) = removed.parent
            && let Some(parent) = self.nodes.get_mut(parent)
        {
            parent.children = parent
                .children
                .iter()
                .copied()
                .filter(|child| *child != node)
                .collect();
        }

        self.roots.remove(&node);
        self.focused_nodes
            .retain(|root, focus| *root != node && *focus != node);

        for child in removed.children {
            self.remove_node(child);
        }
    }

    /// Associates a root node with a window.
    fn set_root_window(&mut self, root: AccessKey, window: T) {
        let root_window = self.roots.get_mut(&root).expect("root not found");
        *root_window = Some(window);
    }

    fn get_root_window(&self, root: AccessKey) -> Option<&T> {
        let root_window = self.roots.get(&root).expect("root not found");
        root_window.as_ref()
    }

    fn set_focus(&mut self, root: AccessKey, focus: Option<AccessKey>) {
        assert!(self.roots.contains_key(&root), "root not found");

        if let Some(focus) = focus {
            let focus_root = self.get_node_root(focus).expect("focused node not found");
            assert_eq!(
                focus_root, root,
                "focused node does not belong to the specified root"
            );
            self.focused_nodes.insert(root, focus);
        } else {
            self.focused_nodes.remove(&root);
        }
    }

    fn get_focus(&self, root: AccessKey) -> Option<AccessKey> {
        self.focused_nodes.get(&root).copied()
    }

    fn get_node_root(&self, node: AccessKey) -> Option<AccessKey> {
        let mut current = node;

        loop {
            let parent = self.nodes.get(current)?.parent;
            match parent {
                Some(parent) => current = parent,
                None => return Some(current),
            }
        }
    }

    fn accessible_name(&self, node: AccessKey) -> Option<String> {
        let node = self.nodes.get(node)?;
        if !node.name().is_empty() {
            return Some(String::from(node.name()));
        }

        for child in &node.children {
            if let Some(name) = self.accessible_name(*child)
                && !name.is_empty()
            {
                return Some(name);
            }
        }

        Some(String::new())
    }

    fn get_sibling(&self, node: AccessKey, offset: isize) -> Option<AccessKey> {
        let parent = self.nodes.get(node)?.parent?;
        let siblings = &self.nodes.get(parent)?.children;
        let index = siblings.iter().position(|sibling| *sibling == node)?;
        let sibling_index = index.checked_add_signed(offset)?;
        siblings.get(sibling_index).copied()
    }

    fn element_from_point(&self, node: AccessKey, x: f64, y: f64) -> Option<AccessKey> {
        let node_data = self.nodes.get(node)?;
        if !node_data.bounding_rect().contains(x, y) {
            return None;
        }

        for child in node_data.children.iter().rev() {
            if let Some(hit) = self.element_from_point(*child, x, y) {
                return Some(hit);
            }
        }

        Some(node)
    }

    pub fn set_platform(&mut self, platform: Box<dyn AccessPlatform<T, U>>) -> Result<(), ()> {
        platform.register_platform()?;
        self.platform = Rc::from(platform);
        Ok(())
    }

    pub fn set_native_platform(&mut self) -> Result<(), ()> {
        let platform = cfg_select! {
            target_os = "windows" => Box::new(crate::platforms::WindowsPlatform::new()),
            target_os = "macos" => Box::new(BlankPlatform::new()),
            target_family = "wasm" => Box::new(BlankPlatform::new()),
            target_os = "linux" => Box::new(BlankPlatform::new()),
            _ => Box::new(BlankPlatform::new()),
        };
        self.set_platform(platform)
    }

    fn set_on_access_event<EventHandler>(&mut self, on_access_event: EventHandler)
    where
        EventHandler: AccessEventHandler<T, U>,
    {
        self.on_access_event = Rc::new(on_access_event);
    }
}

#[cfg(test)]
mod tests {
    use core::cell::Cell;

    use raw_window_handle::{HandleError, HasWindowHandle, WindowHandle};

    use super::*;
    use crate::{AccessRect, LiveSetting, Role};

    #[derive(Clone)]
    struct TestWindow;

    impl HasWindowHandle for TestWindow {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            Err(HandleError::Unavailable)
        }
    }

    #[derive(Clone, Default)]
    struct FocusRecordingPlatform {
        event_count: Rc<Cell<u32>>,
        last_focused: Rc<Cell<Option<AccessKey>>>,
        checked_event_count: Rc<Cell<u32>>,
        name_event_count: Rc<Cell<u32>>,
        value_event_count: Rc<Cell<u32>>,
    }

    impl AccessPlatform<TestWindow, ()> for FocusRecordingPlatform {
        fn register_platform(&self) -> Result<(), ()> {
            Ok(())
        }

        fn register_window(
            &self,
            _window: TestWindow,
            _access_tree: &AccessTree<TestWindow, ()>,
        ) -> Result<(), ()> {
            Ok(())
        }

        fn focus_changed(
            &self,
            node: AccessKey,
            _access_tree: &AccessTree<TestWindow, ()>,
        ) -> Result<(), ()> {
            self.event_count.set(self.event_count.get() + 1);
            self.last_focused.set(Some(node));
            Ok(())
        }

        fn property_changed(
            &self,
            _node: AccessKey,
            property: AccessProperty,
            _old_value: AccessPropertyValue<'_>,
            _new_value: AccessPropertyValue<'_>,
            _access_tree: &AccessTree<TestWindow, ()>,
        ) -> Result<(), ()> {
            match property {
                AccessProperty::Checked => self
                    .checked_event_count
                    .set(self.checked_event_count.get() + 1),
                AccessProperty::Name => self.name_event_count.set(self.name_event_count.get() + 1),
                AccessProperty::Value => {
                    self.value_event_count.set(self.value_event_count.get() + 1)
                }
            }
            Ok(())
        }
    }

    #[test]
    fn framework_name_can_be_updated() {
        let tree = AccessTree::<TestWindow, ()>::new();

        assert_eq!(tree.framework_name(), "");

        tree.set_framework_name("Qt");

        assert_eq!(tree.framework_name(), "Qt");
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn weak_tree_does_not_retain_the_tree() {
        let weak_tree = {
            let tree = AccessTree::<TestWindow, ()>::new();
            tree.downgrade()
        };

        assert!(weak_tree.upgrade().is_none());
    }

    #[test]
    fn node_semantics_can_be_initialized_and_updated() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let mut node = AccessNode::new();
        node.set_name("Initial");
        node.set_role(Role::Button);
        node.set_enabled(false);
        let node = tree.insert_node(node, None);

        assert_eq!(tree.get_node(node).unwrap().name(), "Initial");
        assert_eq!(tree.get_node(node).unwrap().role(), Role::Button);
        assert!(!tree.get_node(node).unwrap().enabled());

        tree.get_node_mut(node).unwrap().set_name("Updated");
        tree.get_node_mut(node).unwrap().set_enabled(true);

        assert_eq!(tree.get_node(node).unwrap().name(), "Updated");
        assert!(tree.get_node(node).unwrap().enabled());
    }

    #[test]
    fn nodes_are_enabled_by_default() {
        assert!(AccessNode::<()>::new().enabled());
        assert!(AccessNode::<()>::default().enabled());
    }

    #[test]
    fn access_event_handler_can_update_the_tree() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let node = tree.insert_node(AccessNode::new(), None);
        tree.set_on_access_event(|tree, node, event| {
            if matches!(event, AccessEvent::Toggle) {
                tree.set_checked(node, true);
            }
            Ok(())
        });

        assert!(
            tree.dispatch_access_event(node, AccessEvent::Toggle)
                .is_ok()
        );

        assert!(tree.get_node(node).unwrap().checked());
    }

    #[test]
    fn focus_is_retained_per_root() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let first_root = tree.insert_node(AccessNode::new(), None);
        let first_child = tree.insert_node(AccessNode::new(), Some(first_root));
        let second_root = tree.insert_node(AccessNode::new(), None);
        let second_child = tree.insert_node(AccessNode::new(), Some(second_root));

        tree.set_focus(first_root, Some(first_child));
        tree.set_focus(second_root, Some(second_child));

        assert_eq!(tree.get_focus(first_root), Some(first_child));
        assert_eq!(tree.get_focus(second_root), Some(second_child));

        tree.set_focus(first_root, None);

        assert_eq!(tree.get_focus(first_root), None);
        assert_eq!(tree.get_focus(second_root), Some(second_child));
    }

    #[test]
    fn focus_notification_is_forwarded_to_the_platform() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let platform = FocusRecordingPlatform::default();
        tree.set_platform(Box::new(platform.clone())).unwrap();
        let root = tree.insert_node(AccessNode::new(), None);
        let child = tree.insert_node(AccessNode::new(), Some(root));

        tree.set_focus(root, Some(child));
        tree.set_focus(root, Some(child));

        assert_eq!(platform.event_count.get(), 1);
        assert_eq!(platform.last_focused.get(), Some(child));
    }

    #[test]
    fn changed_name_notifies_the_platform_once() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let platform = FocusRecordingPlatform::default();
        tree.set_platform(Box::new(platform.clone())).unwrap();
        let mut node = AccessNode::new();
        node.set_name("0");
        node.set_live_setting(LiveSetting::Assertive);
        let node = tree.insert_node(node, None);

        tree.set_name(node, "1");
        tree.set_name(node, "1");

        assert_eq!(tree.get_node(node).unwrap().name(), "1");
        assert_eq!(
            tree.get_node(node).unwrap().live_setting(),
            LiveSetting::Assertive
        );
        assert_eq!(platform.name_event_count.get(), 1);
    }

    #[test]
    fn value_can_be_initialized_and_updated() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let mut node = AccessNode::new();
        node.set_value("Initial");
        let node = tree.insert_node(node, None);

        assert_eq!(tree.get_node(node).unwrap().value(), "Initial");

        tree.get_node_mut(node).unwrap().set_value("Updated");

        assert_eq!(tree.get_node(node).unwrap().value(), "Updated");
    }

    #[test]
    fn changed_value_notifies_the_platform_once() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let platform = FocusRecordingPlatform::default();
        tree.set_platform(Box::new(platform.clone())).unwrap();
        let node = tree.insert_node(AccessNode::new(), None);

        tree.set_value(node, "1");
        tree.set_value(node, "1");

        assert_eq!(tree.get_node(node).unwrap().value(), "1");
        assert_eq!(platform.value_event_count.get(), 1);
    }

    #[test]
    fn bounding_rect_can_be_initialized_and_updated() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let initial = AccessRect::new(10.0, 20.0, 100.0, 50.0);
        let mut node = AccessNode::new();
        node.set_bounding_rect(initial);
        let node = tree.insert_node(node, None);

        assert_eq!(tree.get_node(node).unwrap().bounding_rect(), initial);

        let updated = AccessRect::new(30.0, 40.0, 200.0, 75.0);
        tree.get_node_mut(node).unwrap().set_bounding_rect(updated);

        assert_eq!(tree.get_node(node).unwrap().bounding_rect(), updated);
    }

    #[test]
    fn point_lookup_returns_the_topmost_deepest_node() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let mut root = AccessNode::new();
        root.set_bounding_rect(AccessRect::new(0.0, 0.0, 100.0, 100.0));
        let root = tree.insert_node(root, None);

        let mut lower_child = AccessNode::new();
        lower_child.set_bounding_rect(AccessRect::new(10.0, 10.0, 50.0, 50.0));
        let _lower_child = tree.insert_node(lower_child, Some(root));

        let mut upper_child = AccessNode::new();
        upper_child.set_bounding_rect(AccessRect::new(10.0, 10.0, 50.0, 50.0));
        let upper_child = tree.insert_node(upper_child, Some(root));

        let mut grandchild = AccessNode::new();
        grandchild.set_bounding_rect(AccessRect::new(20.0, 20.0, 10.0, 10.0));
        let grandchild = tree.insert_node(grandchild, Some(upper_child));

        assert_eq!(tree.element_from_point(root, 5.0, 5.0), Some(root));
        assert_eq!(tree.element_from_point(root, 15.0, 15.0), Some(upper_child));
        assert_eq!(tree.element_from_point(root, 25.0, 25.0), Some(grandchild));
        assert_eq!(tree.element_from_point(root, 100.0, 100.0), None);
    }

    #[test]
    fn update_node_preserves_identity_and_children() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let mut root = AccessNode::new();
        root.set_name("before");
        let root = tree.insert_node(root, None);
        let child = tree.insert_node(AccessNode::new(), Some(root));
        let id = tree.get_node(root).unwrap().id();

        let mut replacement = AccessNode::new();
        replacement.set_name("after");
        replacement.set_role(Role::Group);
        tree.update_node(root, replacement);

        assert_eq!(tree.get_node(root).unwrap().name(), "after");
        assert_eq!(tree.get_node(root).unwrap().role(), Role::Group);
        assert_eq!(tree.get_node(root).unwrap().id(), id);
        assert_eq!(tree.get_first_child(root), Some(child));
        assert_eq!(tree.get_parent(child), Some(root));
    }

    #[test]
    fn set_children_reorders_and_removes_omitted_subtrees() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let root = tree.insert_node(AccessNode::new(), None);
        let first = tree.insert_node(AccessNode::new(), Some(root));
        let grandchild = tree.insert_node(AccessNode::new(), Some(first));
        let second = tree.insert_node(AccessNode::new(), Some(root));

        tree.set_children(root, &[second]);

        assert_eq!(tree.get_first_child(root), Some(second));
        assert!(!tree.contains_node(first));
        assert!(!tree.contains_node(grandchild));
    }

    #[test]
    fn unnamed_parent_derives_name_from_retained_contents() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let parent = tree.insert_node(AccessNode::new(), None);
        let mut child = AccessNode::new();
        child.set_name("Open");
        let child = tree.insert_node(child, Some(parent));

        assert_eq!(tree.accessible_name(parent).as_deref(), Some("Open"));

        tree.set_name(child, "Close");

        assert_eq!(tree.accessible_name(parent).as_deref(), Some("Close"));
    }

    #[test]
    fn detached_node_keeps_identity_when_reparented() {
        let tree = AccessTree::<TestWindow, ()>::new();
        let first_parent = tree.insert_node(AccessNode::new(), None);
        let second_parent = tree.insert_node(AccessNode::new(), None);
        let child = tree.insert_node(AccessNode::new(), Some(first_parent));
        let grandchild = tree.insert_node(AccessNode::new(), Some(child));
        let id = tree.get_node(child).unwrap().id();

        tree.detach_node(child);

        assert_eq!(tree.get_parent(child), None);
        assert_eq!(tree.get_parent(grandchild), Some(child));

        tree.append_child(second_parent, child);

        assert_eq!(tree.get_parent(child), Some(second_parent));
        assert_eq!(tree.get_node(child).unwrap().id(), id);
        assert_eq!(tree.get_parent(grandchild), Some(child));
    }
}
