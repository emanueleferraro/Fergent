//! The Fergents sandbox: a wasmtime Component Model host with one gated host
//! interface (`spec.md` D1–D2, Appendix E).

mod engine;
mod error;
mod host;
mod limits;

pub use engine::{ComponentHandle, Sandbox};
pub use error::SandboxError;
pub use host::{DenyAllBridge, HostBridge, HostError};
pub use limits::Limits;

/// The host interface name exported by `wit/host.wit`.
pub const HOST_INTERFACE: &str = "fergents:host/host@1.0.0";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_an_engine() {
        let sandbox = Sandbox::new().unwrap();
        assert!(sandbox.load(b"not a component").is_err());
    }

    #[test]
    fn loads_and_instantiates_an_empty_component() {
        let sandbox = Sandbox::new().unwrap();
        let bytes = wat::parse_str("(component)").unwrap();
        let handle = sandbox.load(&bytes).unwrap();
        // An empty component has no `init`, which is valid for a plugin.
        sandbox
            .init_plugin(&handle, std::sync::Arc::new(DenyAllBridge), &Limits::PLUGIN)
            .unwrap();
        let err = sandbox
            .invoke_skill(
                &handle,
                std::sync::Arc::new(DenyAllBridge),
                "{}",
                &Limits::SKILL,
            )
            .unwrap_err();
        assert!(matches!(err, SandboxError::MissingExport(_)));
    }

    #[test]
    fn defaults_to_plugin_limits() {
        assert_eq!(Limits::default(), Limits::PLUGIN);
        assert_eq!(Limits::SKILL.max_input_bytes, 8 * 1024 * 1024);
    }

    #[test]
    fn deny_all_bridge_denies() {
        let bridge = DenyAllBridge;
        let err = bridge.call("fs.read", "{}").unwrap_err();
        assert_eq!(err.code, "plugin_permission_denied");
    }
}
