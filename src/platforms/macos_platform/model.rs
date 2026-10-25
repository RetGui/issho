use crate::{
    AccessEvent, AccessKey, Role, ScrollAmount, ScrollData, ScrollEvent, SupportedTextSelection,
};

pub(super) struct NodeState {
    pub(super) id: u64,
    pub(super) role: Role,
    pub(super) enabled: bool,
    pub(super) selected: Option<bool>,
    pub(super) selection_group: Option<crate::SelectionGroup>,
    pub(super) checked: bool,
    pub(super) text_selection: SupportedTextSelection,
    pub(super) scroll: ScrollData,
}

pub(super) fn is_leaf_control(role: Role, scroll: ScrollData) -> bool {
    !matches!(scroll, ScrollData::ScrollContainer(_))
        && matches!(
            role,
            Role::Button
                | Role::CheckBox
                | Role::RadioButton
                | Role::Tab
                | Role::Label
                | Role::Link
                | Role::Image
                | Role::Slider
                | Role::ProgressBar
                | Role::ScrollBar
        )
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum Axis {
    Horizontal,
    Vertical,
}

impl Axis {
    pub(super) fn position(self, data: crate::ScrollContainerData) -> Option<f64> {
        match self {
            Self::Horizontal => data.horizontal_percentage,
            Self::Vertical => data.vertical_percentage,
        }
    }

    pub(super) fn preferred(data: crate::ScrollContainerData) -> Option<Self> {
        [Self::Vertical, Self::Horizontal]
            .into_iter()
            .find(|axis| axis.position(data).is_some())
    }

    pub(super) fn event(self, amount: ScrollAmount) -> AccessEvent {
        AccessEvent::Scroll(ScrollEvent {
            horizontal: if self == Self::Horizontal {
                amount
            } else {
                ScrollAmount::NoChange
            },
            vertical: if self == Self::Vertical {
                amount
            } else {
                ScrollAmount::NoChange
            },
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum ElementKind {
    Node,
    ScrollBar(Axis),
}

impl ElementKind {
    pub(super) fn axis(self) -> Option<Axis> {
        match self {
            Self::Node => None,
            Self::ScrollBar(axis) => Some(axis),
        }
    }

    pub(super) fn is_node(self) -> bool {
        matches!(self, Self::Node)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct ElementId {
    pub(super) key: AccessKey,
    pub(super) kind: ElementKind,
}
