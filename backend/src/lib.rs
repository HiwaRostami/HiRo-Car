mod app;
#[cfg(target_os = "android")]
mod bluetooth;
mod bridge;
mod frames;
mod cmd;
pub mod dicts;
pub mod mid;
pub mod obd;
mod paths;
mod pid;
mod port;
mod replay;
mod response;
pub mod scalar;
mod stats;
pub mod vin;

use std::sync::atomic::AtomicUsize;

pub use cmd::*;
pub use obd::*;
pub use pid::*;
pub use response::*;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    app::start();
}

pub(crate) use app::{connect_obd, track_data};

/// Whether or not to pause OBD threads
/// Keeps track of how many threads want to pause obd
/// If > 0, do not do low priority actions
pub static PAUSE_OBD_COUNT: AtomicUsize = AtomicUsize::new(0);
