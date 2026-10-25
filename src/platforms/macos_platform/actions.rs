use objc2::{runtime::Sel, sel};

use super::model::{Axis, ElementKind, NodeState};

use crate::{AccessEvent, Role, ScrollAmount, ScrollData};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum Action {
    Press,
    Pick,
    Increment,
    Decrement,
    ScrollIntoView,
    ScrollLeftByPage,
    ScrollRightByPage,
    ScrollUpByPage,
    ScrollDownByPage,
}

impl Action {
    pub(super) const ALL: [Self; 9] = [
        Self::Press,
        Self::Pick,
        Self::Increment,
        Self::Decrement,
        Self::ScrollIntoView,
        Self::ScrollLeftByPage,
        Self::ScrollRightByPage,
        Self::ScrollUpByPage,
        Self::ScrollDownByPage,
    ];

    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Press => "AXPress",
            Self::Pick => "AXPick",
            Self::Increment => "AXIncrement",
            Self::Decrement => "AXDecrement",
            Self::ScrollIntoView => "AXScrollToVisible",
            Self::ScrollLeftByPage => "AXScrollLeftByPage",
            Self::ScrollRightByPage => "AXScrollRightByPage",
            Self::ScrollUpByPage => "AXScrollUpByPage",
            Self::ScrollDownByPage => "AXScrollDownByPage",
        }
    }

    pub(super) fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|action| action.name() == name)
    }

    fn selector(self) -> Option<Sel> {
        match self {
            Self::Press => Some(sel!(accessibilityPerformPress)),
            Self::Pick => Some(sel!(accessibilityPerformPick)),
            Self::Increment => Some(sel!(accessibilityPerformIncrement)),
            Self::Decrement => Some(sel!(accessibilityPerformDecrement)),
            _ => None,
        }
    }

    pub(super) fn from_selector(selector: Sel) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|action| action.selector() == Some(selector))
    }

    pub(super) fn event(self, node: &NodeState, kind: ElementKind) -> Option<AccessEvent> {
        if !node.enabled {
            return None;
        }
        match (self, kind) {
            (Self::Press, ElementKind::Node) => press_event(node),
            (Self::Pick, ElementKind::Node) if node.selected.is_some() => Some(AccessEvent::Select),
            (Self::ScrollIntoView, ElementKind::Node) => Some(AccessEvent::ScrollIntoView),
            _ => self.scroll_event(node, kind),
        }
    }

    fn scroll_event(self, node: &NodeState, kind: ElementKind) -> Option<AccessEvent> {
        let ScrollData::ScrollContainer(data) = node.scroll else {
            return None;
        };
        let (axis, amount) = match self {
            Self::Increment => (
                kind.axis().or_else(|| Axis::preferred(data))?,
                ScrollAmount::SmallIncrement,
            ),
            Self::Decrement => (
                kind.axis().or_else(|| Axis::preferred(data))?,
                ScrollAmount::SmallDecrement,
            ),
            Self::ScrollLeftByPage => (Axis::Horizontal, ScrollAmount::LargeDecrement),
            Self::ScrollRightByPage => (Axis::Horizontal, ScrollAmount::LargeIncrement),
            Self::ScrollUpByPage => (Axis::Vertical, ScrollAmount::LargeDecrement),
            Self::ScrollDownByPage => (Axis::Vertical, ScrollAmount::LargeIncrement),
            _ => return None,
        };
        if kind.axis().is_some_and(|own_axis| own_axis != axis) {
            return None;
        }
        axis.position(data)?;
        Some(axis.event(amount))
    }
}

fn press_event(node: &NodeState) -> Option<AccessEvent> {
    match node.role {
        Role::CheckBox => Some(AccessEvent::Toggle),
        Role::RadioButton | Role::Tab | Role::ListItem | Role::TreeItem
            if node.selected.is_some() =>
        {
            Some(AccessEvent::Select)
        }
        Role::RadioButton => Some(AccessEvent::Select),
        Role::Button | Role::Link | Role::MenuItem | Role::ComboBox => Some(AccessEvent::Invoke),
        _ => None,
    }
}
