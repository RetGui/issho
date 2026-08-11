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

mod access_event;
mod error;
mod selection_data;
mod text;

pub mod platforms;

pub use crate::access_event::AccessEvent;
pub use crate::access_node::AccessNode;
pub use crate::access_property::{AccessProperty, AccessPropertyValue};
pub use crate::access_rect::AccessRect;
pub use crate::access_tree::{AccessEventHandler, AccessTree};
pub use crate::access_window::{AccessNodeContext, AccessWindow};
pub use crate::error::IsshoError;
pub use crate::live_setting::LiveSetting;
pub use crate::roles::Role;
pub use crate::selection_data::SelectionData;
pub use crate::text::SupportedTextSelection;

pub use slotmap::DefaultKey as AccessKey;
