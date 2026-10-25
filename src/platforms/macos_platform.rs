use alloc::rc::Rc;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2_app_kit::{
    NSAccessibilityAnnouncementKey, NSAccessibilityAnnouncementRequestedNotification,
    NSAccessibilityFocusedUIElementChangedNotification, NSAccessibilityLayoutChangedNotification,
    NSAccessibilityPostNotification, NSAccessibilityPostNotificationWithUserInfo,
    NSAccessibilityPriorityKey, NSAccessibilityPriorityLevel,
    NSAccessibilityTitleChangedNotification, NSAccessibilityValueChangedNotification,
};
use objc2_foundation::{NSDictionary, NSNumber, NSString};

use super::AccessPlatform;

use crate::{
    AccessKey, AccessNodeContext, AccessProperty, AccessPropertyValue, AccessTree, AccessWindow,
    LiveSetting,
};

mod access_tree_adapter;
mod actions;
mod element;
mod geometry;
mod host;
mod model;

use access_tree_adapter::{AccessTreeAdapter, MacAccessTreeAdapter};
use element::Element;

#[derive(Clone, Copy, Default)]
pub struct MacOsPlatform;

impl MacOsPlatform {
    pub const fn new() -> Self {
        Self
    }
}

impl<T: AccessWindow, U: AccessNodeContext> AccessPlatform<T, U> for MacOsPlatform {
    fn register_platform(&self) -> Result<(), ()> {
        MainThreadMarker::new().map(|_| ()).ok_or(())
    }

    fn register_window(&self, window: T, tree: &AccessTree<T, U>) -> Result<(), ()> {
        let main_thread_marker = MainThreadMarker::new().ok_or(())?;
        let root = tree.get_window_root(&window).ok_or(())?;
        let view = host::window_view(&window)?;
        host::attach_native_tree_to_window(
            &view,
            MacAccessTreeAdapter::new(tree, &view, root),
            main_thread_marker,
        )
    }

    fn children_changed(&self, key: AccessKey, tree: &AccessTree<T, U>) -> Result<(), ()> {
        let adapter = native_adapter(tree, key)?;
        adapter.prune();
        let element: Retained<AnyObject> = match adapter.element(key) {
            Some(element) => element.into(),
            None => adapter.view().ok_or(())?.into(),
        };
        // SAFETY: A live AppKit element on the main thread.
        unsafe {
            NSAccessibilityPostNotification(&element, NSAccessibilityLayoutChangedNotification);
        }
        Ok(())
    }

    fn focus_changed(&self, key: AccessKey, tree: &AccessTree<T, U>) -> Result<(), ()> {
        let element = native_element(tree, key)?;
        // SAFETY: A valid AppKit accessibility element on the main thread.
        unsafe {
            NSAccessibilityPostNotification(
                &element,
                NSAccessibilityFocusedUIElementChangedNotification,
            );
        }
        Ok(())
    }

    fn property_changed(
        &self,
        key: AccessKey,
        property: AccessProperty,
        _old: AccessPropertyValue<'_>,
        new_value: AccessPropertyValue<'_>,
        tree: &AccessTree<T, U>,
    ) -> Result<(), ()> {
        let element = native_element(tree, key)?;
        // SAFETY: Constants are AppKit notification names; element is live.
        unsafe {
            NSAccessibilityPostNotification(
                &element,
                match property {
                    AccessProperty::Name => NSAccessibilityTitleChangedNotification,
                    AccessProperty::Value | AccessProperty::Checked => {
                        NSAccessibilityValueChangedNotification
                    }
                },
            );
        }
        if !matches!(property, AccessProperty::Name | AccessProperty::Value) {
            return Ok(());
        }
        let live = tree.get_node(key).ok_or(())?.live_setting();
        let AccessPropertyValue::Text(message) = new_value else {
            return Ok(());
        };
        if live == LiveSetting::Off || message.is_empty() {
            return Ok(());
        }
        let message = NSString::from_str(message);
        let priority = NSNumber::new_isize(match live {
            LiveSetting::Assertive => NSAccessibilityPriorityLevel::High.0,
            _ => NSAccessibilityPriorityLevel::Medium.0,
        });
        // SAFETY: AppKit's documented announcement keys and value types.
        unsafe {
            let info = NSDictionary::from_slices(
                &[NSAccessibilityAnnouncementKey, NSAccessibilityPriorityKey],
                &[&*message as &AnyObject, &*priority as &AnyObject],
            );
            NSAccessibilityPostNotificationWithUserInfo(
                &element,
                NSAccessibilityAnnouncementRequestedNotification,
                Some(&info),
            );
        }
        Ok(())
    }
}

fn native_element<T: AccessWindow, U: AccessNodeContext>(
    tree: &AccessTree<T, U>,
    key: AccessKey,
) -> Result<Retained<Element>, ()> {
    native_adapter(tree, key)?.element(key).ok_or(())
}

fn native_adapter<T: AccessWindow, U: AccessNodeContext>(
    tree: &AccessTree<T, U>,
    key: AccessKey,
) -> Result<Rc<dyn AccessTreeAdapter>, ()> {
    let root = tree.get_node_root(key).ok_or(())?;
    let window = tree.get_root_window(root).ok_or(())?;
    let view = host::window_view(&window)?;
    host::adapter(&view).ok_or(())
}
