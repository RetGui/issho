/// Controls how assistive technology announces changes to a live region.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LiveSetting {
    /// Changes are not announced automatically.
    #[default]
    Off,

    /// Changes are announced after the current announcement finishes.
    Polite,

    /// Changes are announced immediately.
    Assertive,
}
