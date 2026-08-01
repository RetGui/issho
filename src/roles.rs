/// The semantic role of an accessibility node.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Role {
    /// A top-level application window.
    Window,

    /// A structural container with no more specific role.
    #[default]
    GenericContainer,

    /// A named group of related elements.
    Group,

    /// A control that performs an action when invoked.
    Button,

    /// A control that can be checked or unchecked.
    CheckBox,

    /// A control that combines an editable field or button with a list of choices.
    ComboBox,

    /// Static text that conveys information.
    Label,

    /// A link to another location or resource.
    Link,

    /// An image or graphic.
    Image,

    /// An editable text field.
    TextInput,

    /// A container of selectable items.
    List,

    /// An item in a list.
    ListItem,

    /// A container of commands or options.
    Menu,

    /// A command or option in a menu.
    MenuItem,

    /// An indicator showing the progress of an operation.
    ProgressBar,

    /// One option in a set of mutually exclusive choices.
    RadioButton,

    /// A control for scrolling through content.
    ScrollBar,

    /// A control for selecting a value from a continuous range.
    Slider,

    /// A container of tabs.
    TabList,

    /// A selectable tab in a tab list.
    Tab,

    /// A container of commonly used commands.
    ToolBar,

    /// A hierarchical list of items.
    Tree,

    /// An item in a tree.
    TreeItem,

    /// A visual divider between groups of content.
    Separator,
}

impl Role {
    /// Returns whether an element with this role can receive keyboard focus.
    pub const fn is_keyboard_focusable(self) -> bool {
        matches!(
            self,
            Self::Window
                | Self::Button
                | Self::CheckBox
                | Self::ComboBox
                | Self::Link
                | Self::TextInput
                | Self::ListItem
                | Self::MenuItem
                | Self::RadioButton
                | Self::ScrollBar
                | Self::Slider
                | Self::Tab
                | Self::TreeItem
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Role;

    #[test]
    fn interactive_roles_are_keyboard_focusable() {
        assert!(Role::Window.is_keyboard_focusable());
        assert!(Role::Button.is_keyboard_focusable());
        assert!(Role::CheckBox.is_keyboard_focusable());
        assert!(Role::RadioButton.is_keyboard_focusable());
        assert!(Role::TextInput.is_keyboard_focusable());
        assert!(!Role::GenericContainer.is_keyboard_focusable());
        assert!(!Role::Label.is_keyboard_focusable());
    }
}
