mod backlight;
mod ddc;
mod edid;

mod error;
pub use error::*;

mod monitor;
pub use monitor::*;

pub mod event;

mod brightness;
pub use brightness::Brightness;
