//! Native accessibility integration using a retained API.

#![no_std]
extern crate alloc;

mod access_node;
mod access_property;
mod access_rect;
mod access_tree;
mod access_window;
mod live_setting;
mod roles;

pub mod platforms;

pub use crate::access_node::AccessNode;
pub use crate::access_property::{AccessProperty, AccessPropertyValue};
pub use crate::access_rect::AccessRect;
pub use crate::access_tree::AccessTree;
pub use crate::access_window::AccessWindow;
pub use crate::live_setting::LiveSetting;
pub use crate::roles::Role;

pub use slotmap::DefaultKey as AccessKey;
