use raw_window_handle::HasWindowHandle;

/// A clonable window handle that can be associated with an accessibility tree.
pub trait AccessWindow: HasWindowHandle + Clone + 'static {}
pub trait AccessNodeContext: Clone + 'static {}

impl<T: HasWindowHandle + Clone + 'static> AccessWindow for T {}

impl<T: Clone + 'static> AccessNodeContext for T {}
