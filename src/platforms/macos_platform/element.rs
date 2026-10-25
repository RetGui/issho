#![allow(non_snake_case)]
use alloc::{
    format,
    rc::{Rc, Weak},
    string::{String, ToString},
    vec::Vec,
};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{
    NSAccessibilityElement, NSAccessibilityOrientation, NSAccessibilityRoleDescription,
};
use objc2_foundation::{NSArray, NSNumber, NSPoint, NSRange, NSRect, NSString};

use super::{
    access_tree_adapter::AccessTreeAdapter,
    actions::Action,
    model::{Axis, ElementId, ElementKind, NodeState},
};

use crate::{AccessEvent, AccessKey, Role, ScrollAmount, ScrollData, SupportedTextSelection};

pub(super) struct ElementInstanceVariables {
    tree: Weak<dyn AccessTreeAdapter>,
    id: ElementId,
}

define_class!(
    // SAFETY: NSAccessibilityElement has no additional subclassing requirements.
    #[unsafe(super = NSAccessibilityElement)]
    #[thread_kind = MainThreadOnly]
    #[ivars = ElementInstanceVariables]
    pub(super) struct Element;

    impl Element {
        /// Returns a Boolean value that determines whether the accessibility element participates in the accessibility hierarchy.
        #[unsafe(method(isAccessibilityElement))]
        fn isAccessibilityElement(&self) -> bool {
            self.state().is_some() && self.tree().is_some_and(|tree| tree.is_included(self.key()))
        }

        /// Returns the type of interface element that the accessibility element represents.
        #[unsafe(method_id(accessibilityRole))]
        fn accessibilityRole(&self) -> Option<Retained<NSString>> {
            self.role()
        }

        /// Returns a localized, human-intelligible description of the accessibility element’s role, such as radio button.
        #[unsafe(method_id(accessibilityRoleDescription))]
        fn accessibilityRoleDescription(&self) -> Option<Retained<NSString>> {
            self.role()
                .and_then(|role| NSAccessibilityRoleDescription(&role, None))
        }

        /// Returns a short description of the accessibility element.
        #[unsafe(method_id(accessibilityLabel))]
        fn accessibilityLabel(&self) -> Option<Retained<NSString>> {
            (|| -> Option<Retained<NSString>> {
                self.state()?;
                let name = match self.kind().axis() {
                    Some(Axis::Horizontal) => String::from("Horizontal scroll"),
                    Some(Axis::Vertical) => String::from("Vertical scroll"),
                    None => self.tree()?.name(self.key())?,
                };
                Some(NSString::from_str(&name))
            })()
        }

        /// Returns the accessibility element’s identity.
        #[unsafe(method_id(accessibilityIdentifier))]
        fn accessibilityIdentifier(&self) -> Option<Retained<NSString>> {
            (|| -> Option<Retained<NSString>> {
                Some(NSString::from_str(&format!(
                    "issho-{}{}",
                    self.state()?.id,
                    match self.kind().axis() {
                        Some(Axis::Horizontal) => "-horizontal",
                        Some(Axis::Vertical) => "-vertical",
                        None => "",
                    }
                )))
            })()
        }

        /// Returns the accessibility element’s value.
        #[unsafe(method_id(accessibilityValue))]
        fn accessibilityValue(&self) -> Option<Retained<AnyObject>> {
            (|| -> Option<Retained<AnyObject>> {
                let node = self.state()?;
                if let Some(axis) = self.kind().axis() {
                    let ScrollData::ScrollContainer(data) = node.scroll else {
                        return None;
                    };
                    return Some(NSNumber::new_f64(axis.position(data)? / 100.).into());
                }
                match node.role {
                    Role::CheckBox => return Some(NSNumber::new_bool(node.checked).into()),
                    Role::RadioButton | Role::Tab => {
                        return Some(NSNumber::new_bool(node.selected.unwrap_or(node.checked)).into());
                    }
                    _ => {}
                }
                let value = self.text_value()?;
                match node.role {
                    Role::Slider | Role::ProgressBar | Role::ScrollBar => value
                        .parse::<f64>()
                        .ok()
                        .map(|value| NSNumber::new_f64(value).into()),
                    Role::Label => Some(NSString::from_str(&value).into()),
                    _ if !value.is_empty() => Some(NSString::from_str(&value).into()),
                    _ => None,
                }
            })()
        }

        /// Returns a Boolean value that determines whether the accessibility element responds to user events.
        #[unsafe(method(isAccessibilityEnabled))]
        fn isAccessibilityEnabled(&self) -> bool {
            self.state().is_some_and(|node| node.enabled)
        }

        /// Returns a Boolean value that determines whether the accessibility element has the keyboard focus.
        #[unsafe(method(isAccessibilityFocused))]
        fn isAccessibilityFocused(&self) -> bool {
            self.kind().is_node()
                && self.state().is_some()
                && self
                    .tree()
                    .is_some_and(|tree| tree.focus() == Some(self.key()))
        }

        /// Sets a Boolean value that determines whether the accessibility element has the keyboard focus.
        #[unsafe(method(setAccessibilityFocused:))]
        fn setAccessibilityFocused(&self, focused: bool) {
            if focused
                && self.kind().is_node()
                && self.state().is_some()
                && let Some(tree) = self.tree()
            {
                tree.set_focus(self.key());
            }
        }

        /// Returns a Boolean value that determines whether the accessibility element is currently in a selected state.
        #[unsafe(method(isAccessibilitySelected))]
        fn isAccessibilitySelected(&self) -> bool {
            (|| -> bool {
                if !self.kind().is_node() {
                    return false;
                }
                self.state().and_then(|node| node.selected).unwrap_or(false)
            })()
        }

        /// Sets a Boolean value that determines whether the accessibility element is currently in a selected state.
        #[unsafe(method(setAccessibilitySelected:))]
        fn setAccessibilitySelected(&self, selected: bool) {
            if self.kind().is_node()
                && let Some(tree) = self.tree()
            {
                tree.set_selected(self.key(), selected);
            }
        }

        /// Returns the accessibility element’s parent in the accessibility hierarchy.
        #[unsafe(method_id(accessibilityParent))]
        fn accessibilityParent(&self) -> Option<Retained<AnyObject>> {
            (|| -> Option<Retained<AnyObject>> {
                let tree = self.tree()?;
                self.state()?;
                if !self.kind().is_node() {
                    return Some(tree.element(self.key())?.into());
                }
                match tree.filtered_parent(self.key()) {
                    Some(parent) => Some(tree.element(parent)?.into()),
                    None => unsafe { msg_send![&*tree.view()?, accessibilityParent] },
                }
            })()
        }

        /// Returns the window that contains the accessibility element.
        #[unsafe(method_id(accessibilityWindow))]
        fn accessibilityWindow(&self) -> Option<Retained<AnyObject>> {
            self.window()
        }

        /// Returns the top-level element that contains the accessibility element.
        #[unsafe(method_id(accessibilityTopLevelUIElement))]
        fn accessibilityTopLevelUIElement(&self) -> Option<Retained<AnyObject>> {
            self.window()
        }

        /// Returns the child accessibility elements in the accessibility hierarchy.
        #[unsafe(method_id(accessibilityChildren))]
        fn accessibilityChildren(&self) -> Retained<NSArray<Element>> {
            let mut children = self.node_children();
            children.extend(
                [Axis::Horizontal, Axis::Vertical]
                    .into_iter()
                    .filter_map(|axis| self.scroll_bar(axis)),
            );
            NSArray::from_retained_slice(&children)
        }

        /// Returns the accessibility element’s currently selected children.
        #[unsafe(method_id(accessibilitySelectedChildren))]
        fn accessibilitySelectedChildren(&self) -> Retained<NSArray<Element>> {
            (|| -> Retained<NSArray<Element>> {
                if !self.kind().is_node() {
                    return NSArray::new();
                }
                let selected = self
                    .tree()
                    .map(|tree| {
                        tree.selection(self.key())
                            .into_iter()
                            .filter(|key| tree.is_included(*key))
                            .filter_map(|key| tree.element(key))
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                NSArray::from_retained_slice(&selected)
            })()
        }

        /// Sets the accessibility element’s currently selected children.
        #[unsafe(method(setAccessibilitySelectedChildren:))]
        fn setAccessibilitySelectedChildren(&self, children: Option<&NSArray<AnyObject>>) {
            let Some(tree) = self.tree() else {
                return;
            };
            if !self.kind().is_node() {
                return;
            }
            let mut desired = Vec::new();
            if let Some(children) = children {
                for index in 0..children.count() {
                    let child = children.objectAtIndex(index);
                    let Some(child) = child.downcast_ref::<Element>() else {
                        return;
                    };
                    let Some(owner) = child.tree() else {
                        return;
                    };
                    let key = child.key();
                    if !Rc::ptr_eq(&tree, &owner) || !child.kind().is_node() {
                        return;
                    }
                    if !desired.contains(&key) {
                        desired.push(key);
                    }
                }
            }
            tree.replace_selection(self.key(), &desired);
        }

        /// Returns a Boolean value that determines whether the accessibility element must have content for successful submission of a form.
        #[unsafe(method(isAccessibilityRequired))]
        fn isAccessibilityRequired(&self) -> bool {
            self.kind().is_node()
                && self
                    .state()
                    .and_then(|node| node.selection_group)
                    .is_some_and(|group| group.is_mandatory)
        }

        /// Selects the accessibility element.
        #[unsafe(method(accessibilityPerformPick))]
        fn accessibilityPerformPick(&self) -> bool {
            self.perform(Action::Pick)
        }

        /// Returns the horizontal scroll bar for the scroll view.
        #[unsafe(method_id(accessibilityHorizontalScrollBar))]
        fn accessibilityHorizontalScrollBar(&self) -> Option<Retained<Element>> {
            self.scroll_bar(Axis::Horizontal)
        }

        /// Returns the vertical scroll bar for the scroll view.
        #[unsafe(method_id(accessibilityVerticalScrollBar))]
        fn accessibilityVerticalScrollBar(&self) -> Option<Retained<Element>> {
            self.scroll_bar(Axis::Vertical)
        }

        /// Returns the human-readable description of the accessibility element’s value.
        #[unsafe(method_id(accessibilityValueDescription))]
        fn accessibilityValueDescription(&self) -> Option<Retained<NSString>> {
            (|| -> Option<Retained<NSString>> {
                let axis = self.kind().axis()?;
                let ScrollData::ScrollContainer(data) = self.state()?.scroll else {
                    return None;
                };
                let size = match axis {
                    Axis::Horizontal => data.horizontal_size,
                    Axis::Vertical => data.vertical_size,
                };
                Some(NSString::from_str(&format!(
                    "{}% scrolled, {}% visible",
                    axis.position(data)?,
                    size
                )))
            })()
        }

        /// Returns the contents of the current accessibility element.
        #[unsafe(method_id(accessibilityContents))]
        fn accessibilityContents(&self) -> Retained<NSArray<Element>> {
            NSArray::from_retained_slice(&self.node_children())
        }

        /// Returns the orientation of the accessibility element.
        #[unsafe(method(accessibilityOrientation))]
        fn accessibilityOrientation(&self) -> NSAccessibilityOrientation {
            match self.kind() {
                ElementKind::Node => NSAccessibilityOrientation::Unknown,
                ElementKind::ScrollBar(Axis::Horizontal) => NSAccessibilityOrientation::Horizontal,
                ElementKind::ScrollBar(Axis::Vertical) => NSAccessibilityOrientation::Vertical,
            }
        }

        /// Returns the minimum value for the accessibility element.
        #[unsafe(method_id(accessibilityMinValue))]
        fn accessibilityMinValue(&self) -> Option<Retained<NSNumber>> {
            self.scroll_limit(0.0)
        }

        /// Returns the maximum value for the accessibility element.
        #[unsafe(method_id(accessibilityMaxValue))]
        fn accessibilityMaxValue(&self) -> Option<Retained<NSNumber>> {
            self.scroll_limit(1.0)
        }

        /// Sets the accessibility element’s value.
        #[unsafe(method(setAccessibilityValue:))]
        fn setAccessibilityValue(&self, value: Option<&AnyObject>) {
            let Some(axis) = self.kind().axis() else {
                return;
            };
            let Some(value) = value.and_then(|value| value.downcast_ref::<NSNumber>()) else {
                return;
            };
            let value = value.doubleValue();
            if self.state().is_some() && value.is_finite() && (0.0..=1.0).contains(&value) {
                self.dispatch(axis.event(ScrollAmount::GoToPercentage(value * 100.)));
            }
        }

        /// Returns an array of action names supported by the accessibility element.
        #[unsafe(method_id(accessibilityActionNames))]
        fn accessibilityActionNames(&self) -> Retained<NSArray<NSString>> {
            (|| -> Retained<NSArray<NSString>> {
                let Some(node) = self.state() else {
                    return NSArray::new();
                };
                let names: Vec<_> = Action::ALL
                    .into_iter()
                    .filter(|action| action.event(&node, self.kind()).is_some())
                    .map(|action| NSString::from_str(action.name()))
                    .collect();
                NSArray::from_retained_slice(&names)
            })()
        }

        /// Performs the action associated with the specified action.
        #[unsafe(method(accessibilityPerformAction:))]
        fn accessibilityPerformAction(&self, action: &NSString) {
            if let Some(action) = Action::from_name(&action.to_string()) {
                self.perform(action);
            }
        }

        /// Returns the accessibility element’s frame in screen coordinates.
        #[unsafe(method(accessibilityFrame))]
        fn accessibilityFrame(&self) -> NSRect {
            self.frame()
        }

        /// Returns the accessibility element at the specified point.s
        #[unsafe(method_id(accessibilityHitTest:))]
        fn accessibilityHitTest(&self, point: NSPoint) -> Option<Retained<Element>> {
            (|| -> Option<Retained<Element>> {
                let tree = self.tree()?;
                if let Some(axis) = self.kind().axis() {
                    self.state()?;
                    let frame = self.frame();
                    return (point.x >= frame.origin.x
                        && point.y >= frame.origin.y
                        && point.x < frame.origin.x + frame.size.width
                        && point.y < frame.origin.y + frame.size.height)
                        .then(|| {
                            tree.element_for(ElementId {
                                key: self.key(),
                                kind: ElementKind::ScrollBar(axis),
                            })
                        })
                        .flatten();
                }
                tree.element(tree.hit_test(point)?)
            })()
        }

        /// Return the child accessibility element with the current focus.
        #[unsafe(method_id(accessibilityFocusedUIElement))]
        fn accessibilityFocusedUIElement(&self) -> Option<Retained<Element>> {
            (|| -> Option<Retained<Element>> {
                self.state()?;
                let tree = self.tree()?;
                if self.kind().is_node()
                    && let Some(focus) = tree.focus()
                {
                    let mut current = Some(focus);
                    while let Some(key) = current {
                        if key == self.key() {
                            return tree.element(focus);
                        }
                        current = tree.parent(key);
                    }
                }
                tree.element_for(self.ivars().id)
            })()
        }

        /// Returns a Boolean value that indicates whether assistive apps can invoke the specified selector on the accessibility element.
        #[unsafe(method(isAccessibilitySelectorAllowed:))]
        fn isAccessibilitySelectorAllowed(&self, selector: Sel) -> bool {
            (|| -> bool {
                let Some(node) = self.state() else {
                    return false;
                };
                if let Some(action) = Action::from_selector(selector) {
                    return action.event(&node, self.kind()).is_some();
                }
                if selector == sel!(accessibilityNumberOfCharacters)
                    || selector == sel!(accessibilityStringForRange:)
                {
                    return self.kind().is_node() && matches!(node.role, Role::Label | Role::TextInput);
                }
                if selector == sel!(accessibilitySelectedChildren)
                    || selector == sel!(isAccessibilityRequired)
                {
                    return self.kind().is_node() && node.selection_group.is_some();
                }
                if selector == sel!(isAccessibilitySelected) {
                    return self.kind().is_node() && node.selected.is_some();
                }
                if selector == sel!(accessibilityOrientation)
                    || selector == sel!(accessibilityMinValue)
                    || selector == sel!(accessibilityMaxValue)
                    || selector == sel!(accessibilityValueDescription)
                {
                    return !self.kind().is_node();
                }
                if selector == sel!(setAccessibilityValue:) {
                    return node.enabled && !self.kind().is_node();
                }
                if selector == sel!(setAccessibilitySelectedChildren:) {
                    return node.enabled && self.kind().is_node() && node.selection_group.is_some();
                }
                if selector == sel!(setAccessibilityFocused:) {
                    return node.enabled && self.kind().is_node() && node.role.is_keyboard_focusable();
                }
                if selector == sel!(setAccessibilitySelected:) {
                    return node.enabled && self.kind().is_node() && node.selected.is_some();
                }
                if selector == sel!(setAccessibilitySelectedTextRange:) {
                    return node.enabled
                        && self.kind().is_node()
                        && node.text_selection != SupportedTextSelection::None;
                }
                let name = selector.name().to_bytes();
                if name.starts_with(b"accessibilityPerform") || name.starts_with(b"setAccessibility") {
                    return false;
                }
                // SAFETY: Delegate unrelated read-only queries to AppKit's policy.
                unsafe { msg_send![super(self), isAccessibilitySelectorAllowed: selector] }
            })()
        }

        /// Simulates clicking the accessibility element.
        #[unsafe(method(accessibilityPerformPress))]
        fn accessibilityPerformPress(&self) -> bool {
            self.perform(Action::Press)
        }

        /// Increments the accessibility element’s value.
        #[unsafe(method(accessibilityPerformIncrement))]
        fn accessibilityPerformIncrement(&self) -> bool {
            self.perform(Action::Increment)
        }

        /// Decrements the accessibility element’s value.
        #[unsafe(method(accessibilityPerformDecrement))]
        fn accessibilityPerformDecrement(&self) -> bool {
            self.perform(Action::Decrement)
        }

        /// Returns the number of characters in the text.
        #[unsafe(method(accessibilityNumberOfCharacters))]
        fn accessibilityNumberOfCharacters(&self) -> usize {
            self.character_count()
        }

        /// Returns the substring for the specified range.
        #[unsafe(method_id(accessibilityStringForRange:))]
        fn accessibilityStringForRange(&self, range: NSRange) -> Option<Retained<NSString>> {
            (|| -> Option<Retained<NSString>> {
                let value = NSString::from_str(&self.text_value()?);
                if range.location.checked_add(range.length)? > value.len() {
                    return None;
                }
                Some(value.substringWithRange(range))
            })()
        }

        /// Sets the range of the currently selected text.
        #[unsafe(method(setAccessibilitySelectedTextRange:))]
        fn setAccessibilitySelectedTextRange(&self, range: NSRange) {
            if self.kind().is_node()
                && self
                    .state()
                    .is_some_and(|node| node.text_selection != SupportedTextSelection::None)
                && let Some(end) = range.location.checked_add(range.length)
                && end <= self.character_count()
            {
                self.dispatch(AccessEvent::TextSelection(
                    range.location as u64..end as u64,
                ));
            }
        }
    }
);

impl Element {
    pub(super) fn new(
        tree: Weak<dyn AccessTreeAdapter>,
        id: ElementId,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ElementInstanceVariables { tree, id });
        // SAFETY: NSAccessibilityElement's designated initializer.
        unsafe { msg_send![super(this), init] }
    }

    fn key(&self) -> AccessKey {
        self.ivars().id.key
    }

    fn kind(&self) -> ElementKind {
        self.ivars().id.kind
    }

    fn tree(&self) -> Option<Rc<dyn AccessTreeAdapter>> {
        self.ivars().tree.upgrade()
    }

    fn state(&self) -> Option<NodeState> {
        let node = self.tree()?.state(self.key())?;
        if let Some(axis) = self.kind().axis() {
            let ScrollData::ScrollContainer(data) = node.scroll else {
                return None;
            };
            axis.position(data)?;
        }
        Some(node)
    }

    fn dispatch(&self, event: AccessEvent) -> bool {
        self.tree()
            .is_some_and(|tree| tree.dispatch(self.key(), event))
    }

    fn node_children(&self) -> Vec<Retained<Element>> {
        if !self.kind().is_node() {
            return Vec::new();
        }
        self.tree()
            .map(|tree| {
                tree.filtered_children(self.key())
                    .into_iter()
                    .filter_map(|key| tree.element(key))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn scroll_limit(&self, value: f64) -> Option<Retained<NSNumber>> {
        self.kind().axis()?;
        self.state()?;
        Some(NSNumber::new_f64(value))
    }

    fn scroll_bar(&self, axis: Axis) -> Option<Retained<Element>> {
        if !self.kind().is_node() {
            return None;
        }
        let ScrollData::ScrollContainer(data) = self.state()?.scroll else {
            return None;
        };
        axis.position(data)?;
        self.tree()?.element_for(ElementId {
            key: self.key(),
            kind: ElementKind::ScrollBar(axis),
        })
    }

    fn perform(&self, action: Action) -> bool {
        self.state()
            .and_then(|node| action.event(&node, self.kind()))
            .is_some_and(|event| self.dispatch(event))
    }

    fn role(&self) -> Option<Retained<NSString>> {
        let node = self.state()?;
        if !self.kind().is_node() {
            return Some(NSString::from_str("AXScrollBar"));
        }
        Some(NSString::from_str(
            if matches!(node.scroll, ScrollData::ScrollContainer(_)) {
                "AXScrollArea"
            } else {
                role_name(node.role)
            },
        ))
    }

    fn window(&self) -> Option<Retained<AnyObject>> {
        Some(self.tree()?.view()?.window()?.into())
    }

    fn frame(&self) -> NSRect {
        if self.state().is_none() {
            return NSRect::default();
        }
        self.tree()
            .map(|tree| tree.frame(self.key()))
            .unwrap_or_default()
    }

    fn character_count(&self) -> usize {
        self.text_value()
            .map(|text| text.encode_utf16().count())
            .unwrap_or(0)
    }

    fn text_value(&self) -> Option<String> {
        if !self.kind().is_node() {
            return None;
        }
        let node = self.state()?;
        let tree = self.tree()?;
        let value = tree.value(self.key())?;
        if node.role == Role::Label && value.is_empty() {
            tree.name(self.key())
        } else {
            Some(value)
        }
    }
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Window | Role::Group => "AXGroup",
        Role::GenericContainer => "AXUnknown",
        Role::Button => "AXButton",
        Role::CheckBox => "AXCheckBox",
        Role::ComboBox => "AXComboBox",
        Role::Label => "AXStaticText",
        Role::Link => "AXLink",
        Role::Image => "AXImage",
        Role::TextInput => "AXTextField",
        Role::List => "AXList",
        Role::ListItem => "AXRow",
        Role::Menu => "AXMenu",
        Role::MenuItem => "AXMenuItem",
        Role::ProgressBar => "AXProgressIndicator",
        Role::RadioButton => "AXRadioButton",
        Role::ScrollBar => "AXScrollBar",
        Role::Slider => "AXSlider",
        Role::TabList => "AXTabGroup",
        Role::Tab => "AXRadioButton",
        Role::ToolBar => "AXToolbar",
        Role::Tree => "AXOutline",
        Role::TreeItem => "AXRow",
        Role::Separator => "AXSplitter",
    }
}
