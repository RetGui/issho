use core::ops::Range;

/// Accessibility Events.
pub enum AccessEvent {
    TextSelection(Range<u64>),
    Toggle,
}
