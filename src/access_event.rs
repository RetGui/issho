use core::ops::Range;

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
}
