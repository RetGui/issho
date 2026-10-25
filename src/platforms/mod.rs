#[cfg(target_os = "windows")]
mod windows_platform;

#[cfg(target_os = "macos")]
mod macos_platform;

mod access_platform;
mod blank_platform;

pub use access_platform::AccessPlatform;
pub use blank_platform::BlankPlatform;
#[cfg(target_os = "macos")]
pub use macos_platform::MacOsPlatform;
#[cfg(target_os = "windows")]
pub use windows_platform::WindowsPlatform;
