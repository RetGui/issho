use alloc::boxed::Box;
use alloc::string::String;

use core::sync::atomic::{AtomicU64, Ordering};

use smallvec::SmallVec;
use smol_str::SmolStr;

use crate::access_rect::AccessRect;
use crate::access_window::AccessNodeContext;
use crate::live_setting::LiveSetting;
use crate::roles::Role;
use crate::text::{SupportedTextSelection, TextData};
use crate::{AccessKey, SelectionData};

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn next_id() -> u64 {
    NEXT_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("access node ID space exhausted")
}

/// A node in an accessibility tree.
pub struct AccessNode<T: AccessNodeContext> {
    pub(crate) parent: Option<AccessKey>,
    pub(crate) children: SmallVec<[AccessKey; 4]>,
    /// A globally unique id for this node.
    pub(crate) id: u64,
    bounding_rect: AccessRect,
    checked: bool,
    enabled: bool,
    live_setting: LiveSetting,
    name: SmolStr,
    role: Role,
    value: String,

    text_selection: SupportedTextSelection,
    text_data: Option<Box<TextData>>,
    context: Option<T>,

    selection_data: Option<SelectionData>,
}

impl<T: AccessNodeContext> Clone for AccessNode<T> {
    fn clone(&self) -> Self {
        Self {
            parent: None,
            children: SmallVec::new(),
            id: next_id(),
            bounding_rect: self.bounding_rect,
            checked: self.checked,
            enabled: self.enabled,
            live_setting: self.live_setting,
            name: self.name.clone(),
            role: self.role,
            value: self.value.clone(),
            text_selection: self.text_selection,
            text_data: None,
            context: self.context.clone(),
            selection_data: self.selection_data.clone(),
        }
    }
}

impl<T: AccessNodeContext> AccessNode<T> {
    /// Creates a node with default accessibility properties.
    pub fn new() -> Self {
        Self {
            parent: None,
            children: SmallVec::new(),
            id: next_id(),
            bounding_rect: AccessRect::default(),
            checked: false,
            enabled: true,
            live_setting: LiveSetting::Off,
            name: SmolStr::default(),
            role: Role::GenericContainer,
            value: String::new(),
            text_selection: SupportedTextSelection::None,
            text_data: None,
            context: None,
            selection_data: None,
        }
    }

    /// Returns the globally unique ID of this node.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Returns the node's bounding rectangle relative to its root window's client area.
    pub fn bounding_rect(&self) -> AccessRect {
        self.bounding_rect
    }

    /// Sets the node's bounding rectangle relative to its root window's client area.
    pub const fn set_bounding_rect(&mut self, bounding_rect: AccessRect) {
        self.bounding_rect = bounding_rect;
    }

    /// Returns whether this node represents a checked toggle control.
    pub fn checked(&self) -> bool {
        self.checked
    }

    /// Sets whether this node represents a checked toggle control.
    pub fn set_checked(&mut self, checked: bool) {
        self.checked = checked;
    }

    /// Returns whether this node is enabled for user interaction.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Sets whether this node is enabled for user interaction.
    pub const fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Returns the accessible name of this node.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Sets the accessible name of this node.
    pub fn set_name(&mut self, name: impl Into<SmolStr>) {
        self.name = name.into();
    }

    pub(crate) const fn replace_name(&mut self, name: SmolStr) -> SmolStr {
        core::mem::replace(&mut self.name, name)
    }

    /// Returns how changes to this live region should be announced.
    pub fn live_setting(&self) -> LiveSetting {
        self.live_setting
    }

    /// Sets how changes to this live region should be announced.
    pub const fn set_live_setting(&mut self, live_setting: LiveSetting) {
        self.live_setting = live_setting;
    }

    /// Returns the semantic role of this node.
    pub const fn role(&self) -> Role {
        self.role
    }

    /// Sets the semantic role of this node.
    pub fn set_role(&mut self, role: Role) {
        self.role = role;
    }

    /// Returns the current value represented by this node.
    pub fn value(&self) -> &str {
        &self.value
    }

    /// Sets the current value represented by this node.
    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
    }

    pub(crate) fn replace_value(&mut self, value: String) -> String {
        core::mem::replace(&mut self.value, value)
    }

    /// Sets the supported text selection mode.
    pub fn set_text_supported_text_selection(
        &mut self,
        text_selection_support: SupportedTextSelection,
    ) {
        self.text_selection = text_selection_support;
    }

    /// Returns the supported text selection of this node.
    pub fn supports_text_selection(&self) -> SupportedTextSelection {
        self.text_selection
    }

    /// Sets the node context. This is likely some id or pointer to your UI tree.
    pub fn set_context(&mut self, context: T) {
        self.context = Some(context);
    }

    /// Gets the node context.
    pub fn context(&self) -> Option<&T> {
        self.context.as_ref()
    }
}

impl<T: AccessNodeContext> Default for AccessNode<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::AccessNode;
    #[derive(Clone)]
    struct TestContext;

    #[test]
    fn nodes_and_clones_have_unique_ids() {
        let first = AccessNode::<TestContext>::new();
        let second = AccessNode::<TestContext>::new();
        let clone = first.clone();

        assert_ne!(first.id(), second.id());
        assert_ne!(first.id(), clone.id());
        assert_ne!(second.id(), clone.id());
    }

    #[test]
    fn ids_are_unique_across_threads() {
        let first = AccessNode::<TestContext>::new().id();
        let second = std::thread::spawn(|| AccessNode::<TestContext>::new().id())
            .join()
            .unwrap();

        assert_ne!(first, second);
    }
}
