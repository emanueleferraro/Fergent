//! Secret vault behaviour tests.

use std::path::PathBuf;
use std::sync::Arc;

use fergents_secrets::{Keychain, MemoryKeychain, SecretError, SecretVault};
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

fn vault() -> (SecretVault, Arc<MemoryKeychain>) {
    let store: Arc<dyn Storage> =
        Arc::new(SqliteStorage::open_in_memory(&migrations_dir()).unwrap());
    let keychain = Arc::new(MemoryKeychain::new());
    (SecretVault::new(store, keychain.clone()), keychain)
}

#[test]
fn set_stores_the_value_only_in_the_keychain() {
    let (vault, keychain) = vault();
    let stored = vault
        .set("anthropic.api_key", "sk-test", Some("anthropic"))
        .unwrap();

    assert_eq!(stored.name, "anthropic.api_key");
    assert_eq!(stored.provider.as_deref(), Some("anthropic"));
    assert_eq!(
        keychain.get("anthropic.api_key").unwrap().as_deref(),
        Some("sk-test")
    );

    let listed = vault.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].name, "anthropic.api_key");
}

#[test]
fn list_never_exposes_values() {
    let (vault, _) = vault();
    vault.set("openai.api_key", "secret-value", None).unwrap();

    let listed = vault.list().unwrap();
    let rendered = serde_json::to_string(&listed).unwrap();
    assert!(!rendered.contains("secret-value"));
    assert!(rendered.contains("openai.api_key"));
}

#[test]
fn test_reports_presence_without_the_value() {
    let (vault, _) = vault();
    assert!(!vault.test("groq.api_key").unwrap().present);
    vault.set("groq.api_key", "gsk-1", Some("groq")).unwrap();
    let tested = vault.test("groq.api_key").unwrap();
    assert!(tested.present);
    assert_eq!(tested.name, "groq.api_key");
}

#[test]
fn delete_removes_the_reference_and_the_value() {
    let (vault, keychain) = vault();
    vault.set("x.api_key", "v", None).unwrap();
    assert!(vault.delete("x.api_key").unwrap());
    assert!(vault.list().unwrap().is_empty());
    assert!(keychain.get("x.api_key").unwrap().is_none());
    assert!(!vault.delete("x.api_key").unwrap());
}

#[test]
fn invalid_names_are_rejected() {
    let (vault, _) = vault();
    assert!(matches!(
        vault.set("", "v", None),
        Err(SecretError::InvalidName(_))
    ));
    assert!(matches!(
        vault.set("has space", "v", None),
        Err(SecretError::InvalidName(_))
    ));
}

#[test]
fn resolve_returns_unknown_for_missing_names() {
    let (vault, _) = vault();
    assert!(matches!(
        vault.resolve("missing"),
        Err(SecretError::Unknown(_))
    ));
    vault.set("present", "value", None).unwrap();
    assert_eq!(vault.resolve("present").unwrap(), "value");
}
