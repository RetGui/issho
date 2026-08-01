use crate::{
    AccessKey, AccessProperty, AccessPropertyValue, AccessTree, AccessWindow,
    platforms::AccessPlatform,
};

#[derive(Copy, Clone, Default)]
pub struct BlankPlatform;

impl<T: AccessWindow> AccessPlatform<T> for BlankPlatform {
    fn register_platform(&self) -> Result<(), ()> {
        Ok(())
    }

    fn register_window(&self, _window: T, _access_tree: &AccessTree<T>) -> Result<(), ()> {
        Ok(())
    }

    fn focus_changed(&self, _node: AccessKey, _access_tree: &AccessTree<T>) -> Result<(), ()> {
        Ok(())
    }

    fn property_changed(
        &self,
        _node: AccessKey,
        _property: AccessProperty,
        _old_value: AccessPropertyValue<'_>,
        _new_value: AccessPropertyValue<'_>,
        _access_tree: &AccessTree<T>,
    ) -> Result<(), ()> {
        Ok(())
    }
}

impl BlankPlatform {
    pub const fn new() -> Self {
        Self {}
    }
}
