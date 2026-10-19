//! Runtime environment flags read by the kernel.

/// Whether the kernel must run in offline mode.
///
/// This is a **runtime environment flag**: when `FERGENTS_OFFLINE` is set to
/// anything other than `0`, the kernel must never touch the network. In that
/// mode every network path (e.g. downloading a model) must refuse to run
/// rather than attempt a connection. An unset variable or the value `0` means
/// online.
pub fn offline() -> bool {
    match std::env::var("FERGENTS_OFFLINE") {
        Ok(value) => value != "0",
        Err(_) => false,
    }
}
