use alloc::boxed::Box;
use alloc::rc::Rc;

use core::cell::OnceCell;
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};

use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::{
    E_INVALIDARG, E_OUTOFMEMORY, HWND as WindowHandle, LPARAM, LRESULT, POINT, RECT, WPARAM,
};
use windows::Win32::Graphics::Gdi::ClientToScreen;
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize, SAFEARRAY,
};
use windows::Win32::System::Ole::{SafeArrayCreateVector, SafeArrayDestroy, SafeArrayPutElement};
use windows::Win32::System::Variant::{VARIANT, VT_I4};
use windows::Win32::UI::Accessibility::{
    Assertive as UiaAssertive, IRawElementProviderFragment, IRawElementProviderFragment_Impl,
    IRawElementProviderFragmentRoot, IRawElementProviderFragmentRoot_Impl,
    IRawElementProviderSimple, IRawElementProviderSimple_Impl, IToggleProvider,
    IToggleProvider_Impl, NavigateDirection, NavigateDirection_FirstChild,
    NavigateDirection_LastChild, NavigateDirection_NextSibling, NavigateDirection_Parent,
    NavigateDirection_PreviousSibling, Off as UiaOff, Polite as UiaPolite, ProviderOptions,
    ProviderOptions_ServerSideProvider, ProviderOptions_UseComThreading, ToggleState,
    ToggleState_Off, ToggleState_On, UIA_AutomationFocusChangedEventId, UIA_ButtonControlTypeId,
    UIA_CheckBoxControlTypeId, UIA_ComboBoxControlTypeId, UIA_ControlTypePropertyId,
    UIA_E_ELEMENTNOTAVAILABLE, UIA_EditControlTypeId, UIA_FrameworkIdPropertyId,
    UIA_GroupControlTypeId, UIA_HasKeyboardFocusPropertyId, UIA_HyperlinkControlTypeId,
    UIA_ImageControlTypeId, UIA_IsContentElementPropertyId, UIA_IsControlElementPropertyId,
    UIA_IsEnabledPropertyId, UIA_IsKeyboardFocusablePropertyId, UIA_ListControlTypeId,
    UIA_ListItemControlTypeId, UIA_LiveRegionChangedEventId, UIA_LiveSettingPropertyId,
    UIA_MenuControlTypeId, UIA_MenuItemControlTypeId, UIA_NamePropertyId,
    UIA_NativeWindowHandlePropertyId, UIA_PATTERN_ID, UIA_PROPERTY_ID, UIA_PaneControlTypeId,
    UIA_ProgressBarControlTypeId, UIA_RadioButtonControlTypeId, UIA_ScrollBarControlTypeId,
    UIA_SeparatorControlTypeId, UIA_SliderControlTypeId, UIA_TabControlTypeId,
    UIA_TabItemControlTypeId, UIA_TextControlTypeId, UIA_TogglePatternId,
    UIA_ToggleToggleStatePropertyId, UIA_ToolBarControlTypeId, UIA_TreeControlTypeId,
    UIA_TreeItemControlTypeId, UIA_ValueValuePropertyId, UIA_WindowControlTypeId,
    UiaAppendRuntimeId, UiaHostProviderFromHwnd, UiaRaiseAutomationEvent,
    UiaRaiseAutomationPropertyChangedEvent, UiaRect, UiaReturnRawElementProvider, UiaRootObjectId,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, WM_GETOBJECT, WM_NCDESTROY};
use windows::core::{BSTR, IUnknown, IUnknownImpl, Interface, implement};

use crate::access_tree::WeakAccessTree;
use crate::platforms::AccessPlatform;
use crate::{
    AccessKey, AccessProperty, AccessPropertyValue, AccessRect, AccessTree, AccessWindow,
    LiveSetting, Role,
};

// https://learn.microsoft.com/en-us/windows/win32/winauto/uiauto-providersoverview

static NEXT_PLATFORM_ID: AtomicUsize = AtomicUsize::new(1);

fn string_variant(value: &str) -> VARIANT {
    BSTR::from(value).into()
}

fn i32_variant(value: i32) -> VARIANT {
    value.into()
}

fn bool_variant(value: bool) -> VARIANT {
    value.into()
}

const fn uia_property_id(property: AccessProperty) -> UIA_PROPERTY_ID {
    match property {
        AccessProperty::Name => UIA_NamePropertyId,
        AccessProperty::Value => UIA_ValueValuePropertyId,
        AccessProperty::Checked => UIA_ToggleToggleStatePropertyId,
    }
}

fn uia_property_variant(
    property: AccessProperty,
    value: AccessPropertyValue<'_>,
) -> Option<VARIANT> {
    match (property, value) {
        (AccessProperty::Name | AccessProperty::Value, AccessPropertyValue::Text(value)) => {
            Some(string_variant(value))
        }
        (AccessProperty::Checked, AccessPropertyValue::Bool(value)) => {
            Some(i32_variant(uia_toggle_state(value).0))
        }
        _ => None,
    }
}

const fn control_type(role: Role) -> i32 {
    match role {
        Role::Window => UIA_WindowControlTypeId.0,
        Role::GenericContainer => UIA_PaneControlTypeId.0,
        Role::Group => UIA_GroupControlTypeId.0,
        Role::Button => UIA_ButtonControlTypeId.0,
        Role::CheckBox => UIA_CheckBoxControlTypeId.0,
        Role::ComboBox => UIA_ComboBoxControlTypeId.0,
        Role::Label => UIA_TextControlTypeId.0,
        Role::Link => UIA_HyperlinkControlTypeId.0,
        Role::Image => UIA_ImageControlTypeId.0,
        Role::TextInput => UIA_EditControlTypeId.0,
        Role::List => UIA_ListControlTypeId.0,
        Role::ListItem => UIA_ListItemControlTypeId.0,
        Role::Menu => UIA_MenuControlTypeId.0,
        Role::MenuItem => UIA_MenuItemControlTypeId.0,
        Role::ProgressBar => UIA_ProgressBarControlTypeId.0,
        Role::RadioButton => UIA_RadioButtonControlTypeId.0,
        Role::ScrollBar => UIA_ScrollBarControlTypeId.0,
        Role::Slider => UIA_SliderControlTypeId.0,
        Role::TabList => UIA_TabControlTypeId.0,
        Role::Tab => UIA_TabItemControlTypeId.0,
        Role::ToolBar => UIA_ToolBarControlTypeId.0,
        Role::Tree => UIA_TreeControlTypeId.0,
        Role::TreeItem => UIA_TreeItemControlTypeId.0,
        Role::Separator => UIA_SeparatorControlTypeId.0,
    }
}

fn is_content_element(role: Role) -> bool {
    role != Role::GenericContainer
}

fn uia_live_setting(live_setting: LiveSetting) -> i32 {
    match live_setting {
        LiveSetting::Off => UiaOff.0,
        LiveSetting::Polite => UiaPolite.0,
        LiveSetting::Assertive => UiaAssertive.0,
    }
}

fn uia_toggle_state(checked: bool) -> ToggleState {
    if checked {
        ToggleState_On
    } else {
        ToggleState_Off
    }
}

fn get_window_handle<T: HasWindowHandle>(window: &T) -> windows_core::Result<WindowHandle> {
    let raw_window = window
        .window_handle()
        .map_err(|_| windows_core::Error::from_hresult(E_INVALIDARG))?;
    let RawWindowHandle::Win32(win32) = raw_window.as_raw() else {
        return Err(windows_core::Error::from_hresult(E_INVALIDARG));
    };

    Ok(WindowHandle(win32.hwnd.get() as *mut _))
}

fn element_not_available() -> windows_core::Error {
    windows_core::Error::from_hresult(windows_core::HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32))
}

fn root_window_handle<T: AccessWindow>(
    access_tree: &AccessTree<T>,
    root: AccessKey,
) -> windows_core::Result<WindowHandle> {
    let window = access_tree
        .get_root_window(root)
        .ok_or_else(element_not_available)?;
    get_window_handle(&window)
}

fn client_origin(window_handle: WindowHandle) -> windows_core::Result<POINT> {
    let mut origin = POINT::default();
    unsafe { ClientToScreen(window_handle, &mut origin) }.ok()?;
    Ok(origin)
}

fn screen_rect(rect: AccessRect, origin: POINT) -> UiaRect {
    UiaRect {
        left: rect.x + f64::from(origin.x),
        top: rect.y + f64::from(origin.y),
        width: rect.width,
        height: rect.height,
    }
}

fn uia_rect(rect: RECT) -> UiaRect {
    UiaRect {
        left: f64::from(rect.left),
        top: f64::from(rect.top),
        width: f64::from(rect.right - rect.left),
        height: f64::from(rect.bottom - rect.top),
    }
}

fn window_bounding_rect(window_handle: WindowHandle) -> windows_core::Result<UiaRect> {
    let mut rect = RECT::default();
    unsafe { GetWindowRect(window_handle, &mut rect) }?;
    Ok(uia_rect(rect))
}

fn window_point(x: f64, y: f64, origin: POINT) -> (f64, f64) {
    (x - f64::from(origin.x), y - f64::from(origin.y))
}

struct WindowsPlatformState {
    id: usize,
    com_apartment: OnceCell<ComApartment>,
}

#[derive(Debug)]
struct ComApartment {
    platform_id: usize,
}

impl ComApartment {
    fn initialize(platform_id: usize) -> windows_core::Result<Self> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.ok()?;
        Ok(Self { platform_id })
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        log::debug!(
            "Unregistering WindowsPlatform {}: calling CoUninitialize",
            self.platform_id
        );
        unsafe { CoUninitialize() };
    }
}

struct SubclassData<T: AccessWindow> {
    platform: Rc<WindowsPlatformState>,
    access_tree: WeakAccessTree<T>,
    root: AccessKey,
}

unsafe extern "system" fn subclass_proc<T>(
    window_handle: WindowHandle,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    id: usize,
    data: usize,
) -> LRESULT
where
    T: AccessWindow,
{
    let data_ptr = data as *mut SubclassData<T>;
    let data = unsafe { &*data_ptr };

    if msg == WM_GETOBJECT {
        if lparam.0 == UiaRootObjectId as isize {
            let Some(access_tree) = data.access_tree.upgrade() else {
                log::warn!(
                    "The accessibility tree for window {:?} is no longer available",
                    window_handle
                );
                return unsafe { DefSubclassProc(window_handle, msg, wparam, lparam) };
            };
            if !access_tree.contains_node(data.root) {
                log::warn!(
                    "The accessibility root for window {:?} is no longer available",
                    window_handle
                );
                return unsafe { DefSubclassProc(window_handle, msg, wparam, lparam) };
            }
            let provider: IRawElementProviderSimple = WindowsProvider {
                platform: data.platform.clone(),
                access_tree: access_tree.downgrade(),
                node: data.root,
            }
            .into();
            return unsafe {
                UiaReturnRawElementProvider(window_handle, wparam, lparam, &provider)
            };
        }
    }

    let result = unsafe { DefSubclassProc(window_handle, msg, wparam, lparam) };

    if msg == WM_NCDESTROY {
        let _ = unsafe { RemoveWindowSubclass(window_handle, Some(subclass_proc::<T>), id) };
        unsafe {
            drop(Box::from_raw(data_ptr));
        }
    }

    result
}

pub struct WindowsPlatform {
    state: Rc<WindowsPlatformState>,
}

impl WindowsPlatform {
    pub fn new() -> Self {
        Self {
            state: Rc::new(WindowsPlatformState {
                id: NEXT_PLATFORM_ID.fetch_add(1, Ordering::Relaxed),
                com_apartment: OnceCell::new(),
            }),
        }
    }
}

impl Default for WindowsPlatform {
    fn default() -> Self {
        Self::new()
    }
}

#[implement(
    IRawElementProviderSimple,
    IRawElementProviderFragment,
    IRawElementProviderFragmentRoot,
    IToggleProvider
)]
struct WindowsProvider<T>
where
    T: AccessWindow,
{
    platform: Rc<WindowsPlatformState>,
    access_tree: WeakAccessTree<T>,
    node: AccessKey,
}

impl<T: AccessWindow> WindowsProvider<T> {
    fn access_tree(&self) -> windows_core::Result<AccessTree<T>> {
        self.access_tree.upgrade().ok_or_else(element_not_available)
    }
}

impl<T: AccessWindow> AccessPlatform<T> for WindowsPlatform {
    fn register_platform(&self) -> Result<(), ()> {
        if self.state.com_apartment.get().is_some() {
            return Ok(());
        }

        let result = ComApartment::initialize(self.state.id);
        log::debug!(
            "Registered WindowsPlatform {}: CoInitializeEx result: {:?}",
            self.state.id,
            result
        );
        let apartment = result.map_err(|_| ())?;
        self.state.com_apartment.set(apartment).map_err(|_| ())
    }

    fn register_window(&self, window: T, access_tree: &AccessTree<T>) -> Result<(), ()> {
        let window_handle = get_window_handle(&window).map_err(|_| ())?;
        let root = access_tree.get_window_root(&window).ok_or(())?;
        log::debug!(
            "Registering window handle {:?} with WindowsPlatform {}",
            window_handle,
            self.state.id
        );

        let data = Box::new(SubclassData {
            platform: self.state.clone(),
            access_tree: access_tree.downgrade(),
            root,
        });
        let data = Box::into_raw(data);
        let id = data as usize;

        let result = unsafe {
            SetWindowSubclass(window_handle, Some(subclass_proc::<T>), id, data as usize)
        }
        .ok()
        .map_err(|_| ());

        if result.is_err() {
            unsafe {
                drop(Box::from_raw(data));
            }
        }

        result
    }

    fn focus_changed(&self, node: AccessKey, access_tree: &AccessTree<T>) -> Result<(), ()> {
        let provider: IRawElementProviderSimple = WindowsProvider {
            platform: self.state.clone(),
            access_tree: access_tree.downgrade(),
            node,
        }
        .into();

        unsafe { UiaRaiseAutomationEvent(&provider, UIA_AutomationFocusChangedEventId) }
            .map_err(|_| ())
    }

    fn property_changed(
        &self,
        node: AccessKey,
        property: AccessProperty,
        old_value: AccessPropertyValue<'_>,
        new_value: AccessPropertyValue<'_>,
        access_tree: &AccessTree<T>,
    ) -> Result<(), ()> {
        let property_id = uia_property_id(property);
        let old_value = uia_property_variant(property, old_value).ok_or(())?;
        let new_value = uia_property_variant(property, new_value).ok_or(())?;

        let provider: IRawElementProviderSimple = WindowsProvider {
            platform: self.state.clone(),
            access_tree: access_tree.downgrade(),
            node,
        }
        .into();

        let property_result = unsafe {
            UiaRaiseAutomationPropertyChangedEvent(&provider, property_id, &old_value, &new_value)
        };
        let live_region_result = if property == AccessProperty::Name
            && access_tree
                .get_node(node)
                .is_some_and(|node| node.live_setting() != LiveSetting::Off)
        {
            unsafe { UiaRaiseAutomationEvent(&provider, UIA_LiveRegionChangedEventId) }
        } else {
            Ok(())
        };

        property_result.and(live_region_result).map_err(|_| ())
    }
}

// Defines methods and properties that expose simple UI elements.
impl<T> IRawElementProviderSimple_Impl for WindowsProvider_Impl<T>
where
    T: AccessWindow,
{
    /// Specifies the type of Microsoft UI Automation provider; for example, whether it is a client-side (proxy) or server-side provider.
    #[allow(non_snake_case)]
    fn ProviderOptions(&self) -> windows_core::Result<ProviderOptions> {
        // The method must return either ProviderOptions_ServerSideProvider or ProviderOptions_ClientSideProvider.
        //
        // UI Automation handles the various types of providers differently.
        // For example, events from a server-side provider are broadcast to all listening clients,
        // but events from client-side (proxy) providers remain in the client.
        Ok(ProviderOptions_ServerSideProvider | ProviderOptions_UseComThreading)
    }

    /// Retrieves a pointer to an object that provides support for a control pattern on a Microsoft UI Automation element.
    #[allow(non_snake_case)]
    fn GetPatternProvider(&self, pattern_id: UIA_PATTERN_ID) -> windows_core::Result<IUnknown> {
        let access_tree = self.access_tree()?;
        if pattern_id == UIA_TogglePatternId
            && access_tree
                .get_node(self.node)
                .is_some_and(|node| node.role() == Role::CheckBox)
        {
            let provider: IToggleProvider = self.to_interface();
            return provider.cast();
        }

        Err(windows_core::Error::empty())
    }

    /// Retrieves the value of a property supported by the Microsoft UI Automation provider.
    #[allow(non_snake_case)]
    fn GetPropertyValue(&self, property_id: UIA_PROPERTY_ID) -> windows_core::Result<VARIANT> {
        let access_tree = self.access_tree()?;
        if property_id == UIA_FrameworkIdPropertyId {
            return Ok(string_variant(&access_tree.framework_name()));
        }

        let Some(node) = access_tree.get_node(self.node) else {
            return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
                UIA_E_ELEMENTNOTAVAILABLE as i32,
            )));
        };
        let role = node.role();

        if property_id == UIA_NamePropertyId {
            drop(node);
            let name = access_tree
                .accessible_name(self.node)
                .ok_or_else(element_not_available)?;
            return Ok(string_variant(&name));
        }
        if property_id == UIA_ValueValuePropertyId {
            return Ok(string_variant(node.value()));
        }
        if property_id == UIA_ToggleToggleStatePropertyId && role == Role::CheckBox {
            return Ok(i32_variant(uia_toggle_state(node.checked()).0));
        }
        if property_id == UIA_LiveSettingPropertyId {
            return Ok(i32_variant(uia_live_setting(node.live_setting())));
        }
        if property_id == UIA_NativeWindowHandlePropertyId
            && access_tree.get_parent(self.node).is_none()
        {
            let Some(window) = access_tree.get_root_window(self.node) else {
                return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
                    UIA_E_ELEMENTNOTAVAILABLE as i32,
                )));
            };
            return Ok(i32_variant(get_window_handle(&window)?.0 as isize as i32));
        }
        if property_id == UIA_ControlTypePropertyId {
            return Ok(i32_variant(control_type(role)));
        }
        if property_id == UIA_IsControlElementPropertyId {
            return Ok(bool_variant(true));
        }
        if property_id == UIA_IsContentElementPropertyId {
            return Ok(bool_variant(is_content_element(role)));
        }
        if property_id == UIA_IsEnabledPropertyId {
            return Ok(bool_variant(node.enabled()));
        }
        if property_id == UIA_IsKeyboardFocusablePropertyId {
            return Ok(bool_variant(role.is_keyboard_focusable()));
        }
        if property_id == UIA_HasKeyboardFocusPropertyId {
            return Ok(bool_variant(access_tree.is_focused(self.node)));
        }

        Ok(VARIANT::default())
    }

    /// Specifies the host provider for this element.
    #[allow(non_snake_case)]
    fn HostRawElementProvider(&self) -> windows_core::Result<IRawElementProviderSimple> {
        // This property is generally the Microsoft UI Automation provider for the window of a custom control.
        // UI Automation uses this provider in combination with the custom provider.
        // For example, the runtime identifier of the element is usually obtained from the host provider.
        //
        // A host provider must be returned in the following cases: when the element is a fragment root,
        // when the element is a simple element (such as a push button), and when the provider is a repositioning
        // placeholder (for more information, see Provider Repositioning). In other cases, the property should be NULL.
        let access_tree = self.access_tree()?;
        if access_tree.get_parent(self.node).is_some() {
            return Err(windows_core::Error::empty());
        }

        let Some(window) = access_tree.get_root_window(self.node) else {
            return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
                UIA_E_ELEMENTNOTAVAILABLE as i32,
            )));
        };

        unsafe { UiaHostProviderFromHwnd(get_window_handle(&window)?) }
    }
}

// Provides access to controls that can cycle through a set of states and maintain a state after it is set.
#[allow(non_snake_case)]
impl<T> IToggleProvider_Impl for WindowsProvider_Impl<T>
where
    T: AccessWindow,
{
    /// Cycles through the toggle states of a control.
    #[allow(non_snake_case)]
    fn Toggle(&self) -> windows_core::Result<()> {
        // A control must cycle through its ToggleState in this order:
        // ToggleState_On, ToggleState_Off and, if supported, ToggleState_Indeterminate.
        if self.access_tree()?.invoke_toggle(self.node) {
            Ok(())
        } else {
            Err(windows_core::Error::empty())
        }
    }

    /// Specifies the toggle state of the control.
    #[allow(non_snake_case)]
    fn ToggleState(&self) -> windows_core::Result<ToggleState> {
        // A control must cycle through its ToggleState in this order:
        // ToggleState_On, ToggleState_Off and, if supported, ToggleState_Indeterminate.
        let access_tree = self.access_tree()?;
        let node = access_tree
            .get_node(self.node)
            .ok_or_else(element_not_available)?;
        if node.role() != Role::CheckBox {
            return Err(windows_core::Error::empty());
        }

        Ok(uia_toggle_state(node.checked()))
    }
}

// Exposes methods and properties on UI elements that are part of a structure more than one level deep,
// such as a list box or list item. Implemented by Microsoft UI Automation provider.
impl<T> IRawElementProviderFragment_Impl for WindowsProvider_Impl<T>
where
    T: AccessWindow,
{
    /// Retrieves the Microsoft UI Automation element in a specified direction within the UI Automation tree.
    #[allow(non_snake_case, non_upper_case_globals)]
    fn Navigate(
        &self,
        direction: NavigateDirection,
    ) -> windows_core::Result<IRawElementProviderFragment> {
        /*
        The UI Automation server's implementations of this method define the structure of the UI
        Automation tree.
        Navigation must be supported upward to the parent, downward to the first and last child,
        and laterally to the next and previous siblings, as applicable.
        Each child node has only one parent and must be placed in the chain of siblings reached
        from the parent by NavigateDirection_FirstChild and NavigateDirection_LastChild.
        Relationships among siblings must be identical in both directions: if A is B's previous
        sibling (NavigateDirection_PreviousSibling), then B is A's next sibling
        (NavigateDirection_NextSibling). A first child (NavigateDirection_FirstChild) has no
        previous sibling, and a last child (NavigateDirection_LastChild) has no next sibling.
        Fragment roots do not enable navigation to a parent or siblings; navigation among
        fragment roots is handled by the default window providers. Elements in fragments must
        navigate only to other elements within that fragment.
         */
        let access_tree = self.access_tree()?;
        let node = match direction {
            NavigateDirection_FirstChild => access_tree.get_first_child(self.node),
            NavigateDirection_LastChild => access_tree.get_last_child(self.node),
            NavigateDirection_NextSibling => access_tree.get_next_sibling(self.node),
            NavigateDirection_Parent => access_tree.get_parent(self.node),
            NavigateDirection_PreviousSibling => access_tree.get_previous_sibling(self.node),
            _ => unreachable!(),
        };

        let Some(node) = node else {
            return Err(windows_core::Error::empty());
        };

        Ok(WindowsProvider {
            platform: self.platform.clone(),
            access_tree: self.access_tree.clone(),
            node,
        }
        .into())
    }

    /// Retrieves the runtime identifier of an element.
    #[allow(non_snake_case)]
    fn GetRuntimeId(&self) -> windows_core::Result<*mut SAFEARRAY> {
        let access_tree = self.access_tree()?;
        let Some(node) = access_tree.get_node(self.node) else {
            return Err(element_not_available());
        };

        // Implementations should return NULL for a top-level element that is hosted in a window.
        if access_tree.get_parent(self.node).is_none() {
            return Ok(ptr::null_mut());
        }

        // Other elements should return an array that contains UiaAppendRuntimeId,
        // followed by a value that is unique within an instance of the fragment.
        let id = node.id();

        let runtime_id = [
            UiaAppendRuntimeId as i32,
            id as u32 as i32,
            (id >> 32) as u32 as i32,
        ];

        // SAFETY: The array has exactly `runtime_id.len()` VT_I4 elements, and each
        // call supplies an in-bounds index and a valid pointer to an i32 value.
        let array = unsafe {
            let array = SafeArrayCreateVector(VT_I4, 0, runtime_id.len() as u32);
            if array.is_null() {
                return Err(windows_core::Error::from_hresult(E_OUTOFMEMORY));
            }

            for (index, value) in runtime_id.iter().enumerate() {
                let index = index as i32;
                if let Err(error) = SafeArrayPutElement(array, &index, ptr::from_ref(value).cast())
                {
                    let _ = SafeArrayDestroy(array);
                    return Err(error);
                }
            }

            array
        };

        Ok(array)
    }

    /// Specifies the bounding rectangle of this element.
    #[allow(non_snake_case)]
    fn BoundingRectangle(&self) -> windows_core::Result<UiaRect> {
        // The bounding rectangle is defined by the location of the top left corner on the screen, and the dimensions.
        // No clipping is required if the element is partly obscured or partly off-screen.
        // The IsOffscreen property should be set to indicate whether the rectangle is actually visible.
        // Not all points within the bounding rectangle are necessarily clickable.
        let access_tree = self.access_tree()?;
        let Some(node) = access_tree.get_node(self.node) else {
            return Err(element_not_available());
        };
        let rect = node.bounding_rect();
        let Some(root) = access_tree.get_node_root(self.node) else {
            return Err(element_not_available());
        };
        let window_handle = root_window_handle(&access_tree, root)?;

        if self.node == root && node.role() == Role::Window {
            return window_bounding_rect(window_handle);
        }

        let origin = client_origin(window_handle)?;

        Ok(screen_rect(rect, origin))
    }

    /// Retrieves an array of root fragments that are embedded in the Microsoft UI Automation tree rooted at the current element.
    #[allow(non_snake_case)]
    fn GetEmbeddedFragmentRoots(&self) -> windows_core::Result<*mut SAFEARRAY> {
        // This method returns an array of fragments only if the current element is hosting another automation framework.
        // Most providers return NULL.
        Ok(ptr::null_mut())
    }

    /// Sets the focus to this element.
    #[allow(non_snake_case)]
    fn SetFocus(&self) -> windows_core::Result<()> {
        // The Microsoft UI Automation framework will ensure that the part of the interface that hosts
        // this fragment is already focused before calling this method. Your implementation should
        // update only its internal focus state; for example, by repainting a list item to show
        // that it has the focus. If you prefer that UI Automation not focus the parent window,
        // set ProviderOptions_ProviderOwnsSetFocus in IRawElementProviderSimple::ProviderOptions
        // for the fragment root.
        let access_tree = self.access_tree()?;
        let Some(root) = access_tree.get_node_root(self.node) else {
            return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
                UIA_E_ELEMENTNOTAVAILABLE as i32,
            )));
        };

        access_tree.set_focus(root, Some(self.node));
        Ok(())
    }

    /// Specifies the root node of the fragment.
    #[allow(non_snake_case)]
    fn FragmentRoot(&self) -> windows_core::Result<IRawElementProviderFragmentRoot> {
        // A provider for a fragment root should return a pointer to its own implementation of IRawElementProviderFragmentRoot.
        let access_tree = self.access_tree()?;
        let Some(root) = access_tree.get_node_root(self.node) else {
            return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
                UIA_E_ELEMENTNOTAVAILABLE as i32,
            )));
        };

        if root == self.node {
            return Ok(self.to_interface());
        }

        Ok(WindowsProvider {
            platform: self.platform.clone(),
            access_tree: self.access_tree.clone(),
            node: root,
        }
        .into())
    }
}

// Exposes methods and properties on the root element in a fragment.
impl<T> IRawElementProviderFragmentRoot_Impl for WindowsProvider_Impl<T>
where
    T: AccessWindow,
{
    /// Retrieves the provider of the element that is at the specified point in this fragment.
    #[allow(non_snake_case)]
    fn ElementProviderFromPoint(
        &self,
        x: f64,
        y: f64,
    ) -> windows_core::Result<IRawElementProviderFragment> {
        // The returned provider should correspond to the element that would receive mouse input at
        // the specified point.
        // If the point is on this element but not on any child element, either NULL or the provider
        // of the fragment root is returned. If the point is on an element in another
        // framework that is hosted by this fragment, the method returns the element
        // that hosts that fragment (as indicated by IRawElementProviderFragment::GetEmbeddedFragmentRoots).
        let access_tree = self.access_tree()?;
        let Some(root) = access_tree.get_node_root(self.node) else {
            return Err(element_not_available());
        };
        let origin = client_origin(root_window_handle(&access_tree, root)?)?;
        let (x, y) = window_point(x, y, origin);
        let Some(node) = access_tree.element_from_point(root, x, y) else {
            return Err(windows_core::Error::empty());
        };

        Ok(WindowsProvider {
            platform: self.platform.clone(),
            access_tree: self.access_tree.clone(),
            node,
        }
        .into())
    }

    /// Retrieves the element in this fragment that has the input focus.
    #[allow(non_snake_case)]
    fn GetFocus(&self) -> windows_core::Result<IRawElementProviderFragment> {
        let access_tree = self.access_tree()?;
        let Some(root) = access_tree.get_node_root(self.node) else {
            return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
                UIA_E_ELEMENTNOTAVAILABLE as i32,
            )));
        };
        let Some(focus) = access_tree.get_focus(root) else {
            return Err(windows_core::Error::empty());
        };

        Ok(WindowsProvider {
            platform: self.platform.clone(),
            access_tree: self.access_tree.clone(),
            node: focus,
        }
        .into())
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use core::cell::Cell;

    use raw_window_handle::{HandleError, HasWindowHandle, WindowHandle};
    use windows::Win32::System::Com::COINIT_MULTITHREADED;

    use super::*;

    #[derive(Clone)]
    struct TestWindow;

    impl HasWindowHandle for TestWindow {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            Err(HandleError::Unavailable)
        }
    }

    #[test]
    fn converts_between_window_and_screen_coordinates() {
        let origin = POINT { x: 100, y: 200 };
        let rect = screen_rect(AccessRect::new(10.5, 20.5, 30.0, 40.0), origin);

        assert_eq!(rect.left, 110.5);
        assert_eq!(rect.top, 220.5);
        assert_eq!(rect.width, 30.0);
        assert_eq!(rect.height, 40.0);
        assert_eq!(window_point(110.5, 220.5, origin), (10.5, 20.5));

        let rect = uia_rect(RECT {
            left: 100,
            top: 200,
            right: 500,
            bottom: 600,
        });
        assert_eq!(rect.left, 100.0);
        assert_eq!(rect.top, 200.0);
        assert_eq!(rect.width, 400.0);
        assert_eq!(rect.height, 400.0);
    }

    #[test]
    fn com_initialization_is_balanced_and_idempotent() {
        std::thread::spawn(|| {
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                .ok()
                .unwrap();

            let platform = WindowsPlatform::new();
            <WindowsPlatform as AccessPlatform<TestWindow>>::register_platform(&platform).unwrap();
            <WindowsPlatform as AccessPlatform<TestWindow>>::register_platform(&platform).unwrap();
            drop(platform);

            unsafe { CoUninitialize() };

            let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.ok();
            assert!(
                result.is_ok(),
                "COM remained initialized as an STA: {result:?}"
            );
            unsafe { CoUninitialize() };
        })
        .join()
        .unwrap();
    }

    #[test]
    fn checkbox_exposes_the_toggle_pattern_and_checked_state() {
        let tree = AccessTree::<TestWindow>::new();
        let toggle_count = Rc::new(Cell::new(0));
        let mut node = crate::AccessNode::new();
        node.set_role(Role::CheckBox);
        node.set_checked(true);
        node.set_toggle_action({
            let toggle_count = toggle_count.clone();
            move || toggle_count.set(toggle_count.get() + 1)
        });
        let node = tree.insert_node(node, None);
        let provider: IRawElementProviderSimple = WindowsProvider {
            platform: Rc::new(WindowsPlatformState {
                id: 1,
                com_apartment: OnceCell::new(),
            }),
            access_tree: tree.downgrade(),
            node,
        }
        .into();

        let pattern = unsafe { provider.GetPatternProvider(UIA_TogglePatternId) }.unwrap();
        let toggle = pattern.cast::<IToggleProvider>().unwrap();

        assert_eq!(unsafe { toggle.ToggleState() }.unwrap(), ToggleState_On);
        unsafe { toggle.Toggle() }.unwrap();
        assert_eq!(toggle_count.get(), 1);
    }

    #[test]
    fn subclass_data_and_providers_do_not_retain_the_tree() {
        let tree = AccessTree::<TestWindow>::new();
        let root = tree.insert_node(crate::AccessNode::new(), None);
        let platform = Rc::new(WindowsPlatformState {
            id: 1,
            com_apartment: OnceCell::new(),
        });
        let subclass_data = SubclassData {
            platform: platform.clone(),
            access_tree: tree.downgrade(),
            root,
        };
        let provider: IRawElementProviderSimple = WindowsProvider {
            platform,
            access_tree: tree.downgrade(),
            node: root,
        }
        .into();

        drop(tree);

        assert!(subclass_data.access_tree.upgrade().is_none());
        let Err(error) = (unsafe { provider.GetPropertyValue(UIA_NamePropertyId) }) else {
            panic!("provider retained the accessibility tree");
        };
        assert_eq!(
            error.code(),
            windows_core::HRESULT(UIA_E_ELEMENTNOTAVAILABLE as i32)
        );
    }
}
