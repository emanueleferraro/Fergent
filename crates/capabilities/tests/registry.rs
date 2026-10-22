//! Capability registry behaviour tests.

use std::path::PathBuf;
use std::sync::Arc;

use fergents_capabilities::{default_catalogue, Capability, CapabilityError, CapabilityRegistry};
use fergents_events::EventBus;
use fergents_storage::{SqliteStorage, Storage};

fn migrations_dir() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        let candidate = dir.join("migrations");
        if candidate.is_dir() {
            return candidate;
        }
        assert!(dir.pop(), "migrations dir not found");
    }
}

fn registry_with(store: Arc<SqliteStorage>) -> (CapabilityRegistry, Arc<EventBus>) {
    let dyn_store: Arc<dyn Storage> = store;
    let bus = Arc::new(EventBus::new(dyn_store.clone(), 256));
    let registry = CapabilityRegistry::new(dyn_store, bus.clone()).unwrap();
    (registry, bus)
}

fn in_memory() -> (CapabilityRegistry, Arc<EventBus>) {
    let store = Arc::new(SqliteStorage::open_in_memory(&migrations_dir()).unwrap());
    registry_with(store)
}

fn custom(id: &str, deps: &[&str], default_enabled: bool, hot_toggle: bool) -> Capability {
    Capability {
        id: id.to_string(),
        name: id.to_string(),
        version: "1.0.0".to_string(),
        source: "plugin:test".to_string(),
        description: None,
        dependencies: deps.iter().map(|d| (*d).to_string()).collect(),
        resources: Vec::new(),
        default_enabled,
        hot_toggle,
        config: None,
    }
}

#[test]
fn seed_registers_the_catalogue_with_defaults() {
    let (registry, _bus) = in_memory();
    let added = registry.seed(&default_catalogue()).unwrap();
    assert_eq!(added, default_catalogue().len());
    assert_eq!(registry.count(), default_catalogue().len());

    assert!(registry.is_enabled("capability.search"));
    assert!(!registry.is_enabled("capability.charts"));
    assert!(!registry.is_enabled("capability.self-improve"));

    // Seeding is idempotent.
    assert_eq!(registry.seed(&default_catalogue()).unwrap(), 0);
}

#[test]
fn duplicate_ids_are_rejected() {
    let (registry, _bus) = in_memory();
    registry
        .register(custom("test.dup", &[], true, true))
        .unwrap();
    let err = registry
        .register(custom("test.dup", &[], true, true))
        .unwrap_err();
    assert!(matches!(err, CapabilityError::DuplicateId(id) if id == "test.dup"));
}

#[test]
fn unknown_dependencies_are_rejected() {
    let (registry, _bus) = in_memory();
    let err = registry
        .register(custom("test.child", &["test.missing"], true, true))
        .unwrap_err();
    assert!(matches!(err, CapabilityError::UnknownDependency { .. }));
}

#[test]
fn invalid_ids_are_rejected() {
    let (registry, _bus) = in_memory();
    assert!(matches!(
        registry.register(custom("Bad Id", &[], true, true)),
        Err(CapabilityError::InvalidId(_))
    ));
    assert!(matches!(
        registry.register(custom("", &[], true, true)),
        Err(CapabilityError::InvalidId(_))
    ));
}

#[test]
fn enabling_a_capability_requires_its_dependencies() {
    let (registry, _bus) = in_memory();
    registry
        .register(custom("test.y", &[], false, true))
        .unwrap();
    registry
        .register(custom("test.x", &["test.y"], false, true))
        .unwrap();

    let err = registry.set_enabled("test.x", true, false).unwrap_err();
    assert!(matches!(
        err,
        CapabilityError::DependencyDisabled { dependency, .. } if dependency == "test.y"
    ));
    assert!(!registry.is_enabled("test.x"));

    // With auto_enable_deps, the dependency is enabled first.
    let view = registry.set_enabled("test.x", true, true).unwrap();
    assert!(view.enabled);
    assert!(registry.is_enabled("test.y"));
}

#[test]
fn disabling_is_blocked_by_enabled_dependents() {
    let (registry, _bus) = in_memory();
    registry
        .register(custom("test.y", &[], true, true))
        .unwrap();
    registry
        .register(custom("test.x", &["test.y"], true, true))
        .unwrap();

    let err = registry.set_enabled("test.y", false, false).unwrap_err();
    assert!(matches!(
        err,
        CapabilityError::HasEnabledDependents { dependent, .. } if dependent == "test.x"
    ));
    assert!(registry.is_enabled("test.y"));
}

#[test]
fn require_guard_reports_disabled() {
    let (registry, _bus) = in_memory();
    registry.seed(&default_catalogue()).unwrap();
    assert!(registry.require("capability.charts").is_err());

    let err = registry.require("capability.charts").unwrap_err();
    assert_eq!(err.code(), "capability_disabled");

    // Unknown capabilities are treated as enabled (gating is opt-in).
    assert!(registry.require("capability.not-registered").is_ok());
}

#[test]
fn hot_and_cold_toggles_set_restart_required() {
    let (registry, _bus) = in_memory();
    registry
        .register(custom("test.hot", &[], true, true))
        .unwrap();
    registry
        .register(custom("test.cold", &[], true, false))
        .unwrap();

    let hot = registry.set_enabled("test.hot", false, false).unwrap();
    assert!(hot.hot_toggle);
    assert!(!hot.restart_required);

    let cold = registry.set_enabled("test.cold", false, false).unwrap();
    assert!(!cold.hot_toggle);
    assert!(cold.restart_required);
    assert!(registry.restart_required("test.cold"));
}

#[test]
fn state_persists_across_restart() {
    let db = tempfile::NamedTempFile::new().unwrap();
    let path = db.path().to_path_buf();
    let migrations = migrations_dir();

    {
        let store = Arc::new(SqliteStorage::open(&path, &migrations).unwrap());
        let (registry, _bus) = registry_with(store);
        registry.seed(&default_catalogue()).unwrap();
        registry
            .set_enabled("capability.charts", true, true)
            .unwrap();
        assert!(registry.is_enabled("capability.charts"));
    }

    let store = Arc::new(SqliteStorage::open(&path, &migrations).unwrap());
    let (registry, _bus) = registry_with(store);
    assert_eq!(registry.count(), default_catalogue().len());
    assert!(registry.is_enabled("capability.charts"));
    // A fresh start does not carry a pending-restart flag.
    assert!(!registry.restart_required("capability.charts"));
}

#[tokio::test]
async fn lifecycle_events_are_emitted() {
    let (registry, bus) = in_memory();
    let mut sub = bus.subscribe();

    registry
        .register(custom("test.hot", &[], true, true))
        .unwrap();
    let registered = sub.recv().await.unwrap();
    assert_eq!(registered.kind, "capability.registered");

    registry.set_enabled("test.hot", false, false).unwrap();
    let disabled = sub.recv().await.unwrap();
    assert_eq!(disabled.kind, "capability.disabled");
    let changed = sub.recv().await.unwrap();
    assert_eq!(changed.kind, "capability.changed");

    registry.set_enabled("test.hot", true, false).unwrap();
    assert_eq!(sub.recv().await.unwrap().kind, "capability.enabled");
    assert_eq!(sub.recv().await.unwrap().kind, "capability.changed");
}
