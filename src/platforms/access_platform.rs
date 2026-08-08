use crate::access_window::AccessNodeContext;
use crate::{AccessKey, AccessProperty, AccessPropertyValue, AccessTree, AccessWindow};

pub trait AccessPlatform<T: AccessWindow, U: AccessNodeContext> {
    /// Called when an `AccessPlatform` is set on an `AccessTree`.
    ///
    /// This should only be called once.
    fn register_platform(&self) -> Result<(), ()>;

    /// Register a window.
    ///
    /// This should only be called once per a window.
    fn register_window(&self, window: T, access_tree: &AccessTree<T, U>) -> Result<(), ()>;

    /// Notify the native accessibility platform that focus moved to a node.
    fn focus_changed(&self, node: AccessKey, access_tree: &AccessTree<T, U>) -> Result<(), ()>;

    /// Notify the native accessibility platform that a retained property changed.
    fn property_changed(
        &self,
        node: AccessKey,
        property: AccessProperty,
        old_value: AccessPropertyValue<'_>,
        new_value: AccessPropertyValue<'_>,
        access_tree: &AccessTree<T, U>,
    ) -> Result<(), ()>;
}
