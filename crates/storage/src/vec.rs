//! Registration of the statically linked `sqlite-vec` extension.
//!
//! The `vec0` virtual-table module must be registered process-globally before
//! any SQLite connection is opened that needs it. Registration is idempotent,
//! so calling it from multiple entry points is safe.

use std::sync::Once;

/// Register the `sqlite-vec` (`vec0`) extension for every connection opened
/// afterwards.
///
/// This is process-global and idempotent.
pub fn register_sqlite_vec() {
    static INIT: Once = Once::new();
    INIT.call_once(|| unsafe {
        let init: rusqlite::auto_extension::RawAutoExtension =
            std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ());
        let _ = rusqlite::auto_extension::register_auto_extension(init);
    });
}
