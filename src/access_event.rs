use core::ops::Range;

use crate::ScrollEvent;

/// Accessibility Events.
pub enum AccessEvent {
    /// Select a text range.
    TextSelection(Range<u64>),
    /// Cycles through the toggle states of a control.
    Toggle,
    /// Performs a singular action e.g. a button click.
    Invoke,
    /// Deselects any selected items and then selects the current element.
    Select,
    /// Adds the current element to the collection of selected items.
    AddToSelection,
    /// Removes the current element from the collection of selected items.
    UnSelect,
    /// Scroll.
    Scroll(ScrollEvent),
    /// Scroll the node into the viewport.
    ScrollIntoView,
    /// Hides all child nodes, controls, or content of this element.
    Collapse,
    /// Displays all child nodes, controls, or content of the control.
    Expand,
}
