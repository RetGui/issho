#[cfg(target_os = "windows")]
mod windows_platform;

mod access_platform;
mod blank_platform;

pub use access_platform::AccessPlatform;
pub use blank_platform::BlankPlatform;
#[cfg(target_os = "windows")]
pub use windows_platform::WindowsPlatform;
