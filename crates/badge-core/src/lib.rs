//! Data model and wire protocol for the "LS32"-style LED name badges
//! (USB HID `0416:5020`), reverse-engineered from BmpBadge V9.18.

pub mod files;
mod framebuffer;
mod model;
pub mod protocol;
pub mod wording;

pub use framebuffer::{Framebuffer, MAX_WIDTH};
pub use model::*;
