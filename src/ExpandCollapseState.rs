/// Contains values that specify the state of a UI element that can be expanded and collapsed.
#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub enum ExpandCollapseState {
    /// No children are visible.
    Collapsed,
    /// All children are visible.
    Expanded,
    /// Some, but not all, children are visible.
    PartiallyExpanded,
    #[default]
    /// The element does not expand or collapse.
    LeafNode,
}