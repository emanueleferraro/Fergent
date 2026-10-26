//! The Fergents plugin host (`PLUGINS.md`, `spec.md` §1–3).
//!
//! Plugins are WASM components plus an optional UI module. The service owns
//! the registry, installation reviews, loading, tool dispatch, dev links, and
//! the gated host bridge.

pub mod error;
pub mod host;
pub mod manifest;
pub mod record;
pub mod service;

pub use error::PluginError;
pub use host::{Grants, HostServices, PluginHostBridge, UnavailableHostServices};
pub use manifest::{
    CapabilityMeta, CommandMeta, KernelMeta, PermissionScope, PluginManifest, PluginMeta,
    SettingMeta, UiMeta, PERMISSIONS, SLOTS,
};
pub use record::{InstallReview, PermissionGrant, PluginRecord, PluginView, ToolSpec};
pub use service::{package_plugin, PluginService, PluginSource};

/// The plugin API version implemented by this kernel.
pub const API_VERSION: u32 = fergents_packages::KERNEL_API_VERSION;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn manifest_text(id: &str) -> String {
        format!(
            r#"
[plugin]
id = "{id}"
name = "Test"
version = "1.0.0"

[ui]
entry = "ui.js"

slots = ["rail.item"]
"#
        )
    }

    fn write_plugin(dir: &std::path::Path, id: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("plugin.toml"), manifest_text(id)).unwrap();
        std::fs::write(dir.join("ui.js"), "export default {}").unwrap();
    }

    fn service(data: &std::path::Path) -> Arc<PluginService> {
        let storage = Arc::new(
            fergents_storage::SqliteStorage::open(
                &data.join("global.sqlite"),
                &fergents_core_migrations(),
            )
            .unwrap(),
        );
        let bus = Arc::new(fergents_events::EventBus::new(storage.clone(), 32));
        let capabilities = Arc::new(
            fergents_capabilities::CapabilityRegistry::new(storage.clone(), bus.clone()).unwrap(),
        );
        let sandbox = Arc::new(fergents_wasm::Sandbox::new().unwrap());
        let host = Arc::new(UnavailableHostServices::new(data));
        Arc::new(PluginService::new(
            storage,
            bus,
            data.to_path_buf(),
            sandbox,
            host,
            None,
            capabilities,
        ))
    }

    fn fergents_core_migrations() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("migrations")
    }

    #[test]
    fn installs_a_ui_plugin_after_approval() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        write_plugin(source.path(), "com.example.test");
        let service = service(data.path());
        let review = service
            .inspect(&PluginSource::Dir(source.path().to_path_buf()))
            .unwrap();
        assert_eq!(review.signature_status, "none");
        let view = service.install(&review.review_id, &[]).unwrap();
        assert!(view.record.enabled);
        assert_eq!(view.record.kind, "ui");
        assert_eq!(view.tools.len(), 0);
        assert!(service.get("com.example.test").is_ok());
        service.uninstall("com.example.test", true).unwrap();
        assert!(service.get("com.example.test").is_err());
    }

    #[test]
    fn install_requires_every_permission() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        std::fs::write(
            source.path().join("plugin.toml"),
            r#"
[plugin]
id = "com.example.perm"
name = "Perm"
version = "1.0.0"

[ui]
entry = "ui.js"

[permissions]
net.http = ["https://example.com/**"]
"#,
        )
        .unwrap();
        std::fs::write(source.path().join("ui.js"), "x").unwrap();
        let service = service(data.path());
        let review = service
            .inspect(&PluginSource::Dir(source.path().to_path_buf()))
            .unwrap();
        assert!(service.install(&review.review_id, &[]).is_err());
    }
}
