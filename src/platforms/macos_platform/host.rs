use alloc::{ffi::CString, format, rc::Rc, vec::Vec};

use core::{ffi::c_void, ptr};

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, NSObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::NSView;
use objc2_foundation::{NSArray, NSPoint};

use raw_window_handle::RawWindowHandle;

use super::{access_tree_adapter::AccessTreeAdapter, element::Element};

use crate::AccessWindow;

pub(super) fn attach_native_tree_to_window(
    view: &NSView,
    tree: Rc<dyn AccessTreeAdapter>,
    main_thread_marker: MainThreadMarker,
) -> Result<(), ()> {
    install_view(view, &ViewState::new(tree, main_thread_marker))
}

static VIEW_STATE_KEY: u8 = 0;
#[link(name = "objc")]
unsafe extern "C-unwind" {
    fn objc_setAssociatedObject(
        object: &AnyObject,
        key: *const c_void,
        value: &AnyObject,
        policy: usize,
    );
    fn objc_getAssociatedObject(object: &AnyObject, key: *const c_void) -> *mut AnyObject;
}

define_class!(
    // SAFETY: NSObject has no additional subclassing requirements.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Rc<dyn AccessTreeAdapter>]
    struct ViewState;
);

impl ViewState {
    fn new(
        tree: Rc<dyn AccessTreeAdapter>,
        main_thread_marker: MainThreadMarker,
    ) -> Retained<Self> {
        let this = Self::alloc(main_thread_marker).set_ivars(tree);
        // SAFETY: NSObject's designated initializer.
        unsafe { msg_send![super(this), init] }
    }
}

pub(super) fn adapter(view: &AnyObject) -> Option<Rc<dyn AccessTreeAdapter>> {
    MainThreadMarker::new()?;
    // SAFETY: Our private key is only ever associated with a retained ViewState.
    let state = unsafe { objc_getAssociatedObject(view, ptr::from_ref(&VIEW_STATE_KEY).cast()) };
    if state.is_null() {
        return None;
    }
    Some(unsafe { &*state.cast::<ViewState>() }.ivars().clone())
}

unsafe extern "C-unwind" fn view_children(view: &AnyObject, _: Sel) -> *mut NSArray<Element> {
    let elements: Vec<_> = adapter(view)
        .map(|tree| {
            tree.filtered_roots()
                .into_iter()
                .filter_map(|key| tree.element(key))
                .collect()
        })
        .unwrap_or_default();
    Retained::autorelease_ptr(NSArray::from_retained_slice(&elements))
}
unsafe extern "C-unwind" fn view_focused(view: &AnyObject, _: Sel) -> *mut Element {
    adapter(view)
        .and_then(|tree| tree.element(tree.focus()?))
        .map(Retained::autorelease_ptr)
        .unwrap_or(ptr::null_mut())
}
unsafe extern "C-unwind" fn view_hit_test(
    view: &AnyObject,
    _: Sel,
    point: NSPoint,
) -> *mut Element {
    adapter(view)
        .and_then(|tree| tree.element(tree.hit_test(point)?))
        .map(Retained::autorelease_ptr)
        .unwrap_or(ptr::null_mut())
}
unsafe extern "C-unwind" fn view_is_element(_: &AnyObject, _: Sel) -> Bool {
    Bool::NO
}
fn install_view(view: &NSView, state: &ViewState) -> Result<(), ()> {
    if adapter(view).is_some() {
        return Ok(());
    }
    let original_class = view.class();
    let name = CString::new(format!(
        "IsshoAccessibility_{}_{}",
        env!("CARGO_PKG_VERSION"),
        original_class.name().to_string_lossy()
    ))
    .map_err(|_| ())?;
    let class = if let Some(class) = AnyClass::get(&name) {
        class
    } else {
        let mut class = ClassBuilder::new(&name, original_class).ok_or(())?;
        // SAFETY: These selectors use the AppKit signatures and return
        // autoreleased objects, as required for non-owning method families.
        unsafe {
            class.add_method(
                sel!(accessibilityChildren),
                view_children as unsafe extern "C-unwind" fn(_, _) -> _,
            );
            class.add_method(
                sel!(accessibilityFocusedUIElement),
                view_focused as unsafe extern "C-unwind" fn(_, _) -> _,
            );
            class.add_method(
                sel!(accessibilityHitTest:),
                view_hit_test as unsafe extern "C-unwind" fn(_, _, _) -> _,
            );
            class.add_method(
                sel!(isAccessibilityElement),
                view_is_element as unsafe extern "C-unwind" fn(_, _) -> _,
            );
        }
        class.register()
    };
    // SAFETY: Main-thread only, same layout as the original NSView subclass.
    // OBJC_ASSOCIATION_RETAIN_NONATOMIC (1) keeps state alive with the host view.
    unsafe {
        objc_setAssociatedObject(view, ptr::from_ref(&VIEW_STATE_KEY).cast(), state, 1);
        AnyObject::set_class(view, class);
    }
    Ok(())
}

pub(super) fn window_view<T: AccessWindow>(window: &T) -> Result<Retained<NSView>, ()> {
    MainThreadMarker::new().ok_or(())?;
    let handle = window.window_handle().map_err(|_| ())?;
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return Err(());
    };
    // SAFETY: The window handle guarantees a live NSView; retain while in use.
    unsafe { Retained::retain(handle.ns_view.as_ptr().cast::<NSView>()) }.ok_or(())
}
