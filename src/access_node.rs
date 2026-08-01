use alloc::rc::Rc;
use alloc::string::String;

use core::sync::atomic::{AtomicU64, Ordering};

use smol_str::SmolStr;
use smolvec::SmolVec;

use crate::AccessKey;
use crate::access_rect::AccessRect;
use crate::live_setting::LiveSetting;
use crate::roles::Role;

static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn next_id() -> u64 {
    NEXT_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |id| id.checked_add(1))
        .expect("access node ID space exhausted")
}

/// A node in an accessibility tree.
pub struct AccessNode {
    pub(crate) parent: Option<AccessKey>,
    pub(crate) children: SmolVec<AccessKey>,
    /// A globally unique id for this node.
    pub(crate) id: u64,
    bounding_rect: AccessRect,
    checked: bool,
    enabled: bool,
    live_setting: LiveSetting,
    name: SmolStr,
    role: Role,
    value: String,
    toggle_action: Option<Rc<dyn Fn()>>,
}

impl Clone for AccessNode {
    fn clone(&self) -> Self {
        Self {
            parent: None,
            children: SmolVec::new(),
            id: next_id(),
            bounding_rect: self.bounding_rect,
            checked: self.checked,
            enabled: self.enabled,
            live_setting: self.live_setting,
            name: self.name.clone(),
            role: self.role,
            value: self.value.clone(),
            toggle_action: self.toggle_action.clone(),
        }
    }
}

impl AccessNode {
    /// Creates a node with default accessibility properties.
    pub fn new() -> Self {
        Self {
            parent: None,
            children: SmolVec::new(),
            id: next_id(),
            bounding_rect: AccessRect::default(),
            checked: false,
            enabled: true,
            live_setting: LiveSetting::Off,
            name: SmolStr::default(),
            role: Role::GenericContainer,
            value: String::new(),
            toggle_action: None,
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

    /// Sets the action invoked when an accessibility client toggles this node.
    ///
    /// A control must cycle through its ToggleState in this order:
    /// ToggleState_On, ToggleState_Off and, if supported, ToggleState_Indeterminate.
    pub fn set_toggle_action(&mut self, action: impl Fn() + 'static) {
        self.toggle_action = Some(Rc::new(action));
    }

    pub(crate) fn toggle_action(&self) -> Option<Rc<dyn Fn()>> {
        self.toggle_action.clone()
    }
}

impl Default for AccessNode {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use super::AccessNode;

    #[test]
    fn nodes_and_clones_have_unique_ids() {
        let first = AccessNode::new();
        let second = AccessNode::new();
        let clone = first.clone();

        assert_ne!(first.id(), second.id());
        assert_ne!(first.id(), clone.id());
        assert_ne!(second.id(), clone.id());
    }

    #[test]
    fn ids_are_unique_across_threads() {
        let first = AccessNode::new().id();
        let second = std::thread::spawn(|| AccessNode::new().id())
            .join()
            .unwrap();

        assert_ne!(first, second);
    }
}
