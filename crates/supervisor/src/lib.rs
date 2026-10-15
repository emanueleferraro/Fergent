//! Fergents supervisor: PID file management and shutdown signals.

mod pid;
mod signals;

pub use pid::PidFile;
pub use signals::shutdown_signal;
