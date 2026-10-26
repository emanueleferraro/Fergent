//! The Fergents skill runtime (`SKILLS.md`, `spec.md` §5).
//!
//! Skills are `SKILL.md` folders with a prompt, a wrapped tool, a WASM
//! component, or a hybrid of the two. Execution is schema-validated,
//! permission-gated, chain-limited, and cached.

pub mod error;
pub mod manifest;
pub mod schema;
pub mod service;

pub use error::SkillError;
pub use manifest::{parse, CacheConfig, ParsedSkill, SkillKind, SkillManifest, PERMISSIONS};
pub use service::{
    package_skill, SkillRecord, SkillReview, SkillService, SkillServices, SkillSource, SkillTool,
    SkillView, UnavailableSkillServices,
};

/// The maximum skill chaining depth (`spec.md` D14).
pub const MAX_DEPTH: usize = 5;

/// The skill API version implemented by this kernel.
pub const API_VERSION: u32 = fergents_packages::KERNEL_API_VERSION;

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    use fergents_storage::{SqliteStorage, Storage};
    use serde_json::{json, Value};

    use super::*;

    struct StubServices {
        data_dir: std::path::PathBuf,
        calls: AtomicUsize,
    }

    impl SkillServices for StubServices {
        fn data_dir(&self) -> &std::path::Path {
            &self.data_dir
        }

        fn llm_chat(
            &self,
            _model: &str,
            _prompt: &str,
            input: &Value,
            _temperature: f64,
            _max_tokens: u32,
        ) -> Result<String, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(
                json!({ "summary": input.get("text").and_then(Value::as_str).unwrap_or("") })
                    .to_string(),
            )
        }

        fn tool_call(&self, tool: &str, input: Value) -> Result<Value, String> {
            Ok(json!({ "tool": tool, "echo": input }))
        }
    }

    fn service(data: &std::path::Path) -> (Arc<SkillService>, Arc<StubServices>) {
        let storage: Arc<dyn Storage> =
            Arc::new(SqliteStorage::open(&data.join("global.sqlite"), &migrations()).unwrap());
        let bus = Arc::new(fergents_events::EventBus::new(storage.clone(), 32));
        let services = Arc::new(StubServices {
            data_dir: data.to_path_buf(),
            calls: AtomicUsize::new(0),
        });
        let sandbox = Arc::new(fergents_wasm::Sandbox::new().unwrap());
        let service = Arc::new(SkillService::new(
            storage,
            bus,
            data.to_path_buf(),
            sandbox,
            services.clone(),
        ));
        (service, services)
    }

    fn migrations() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("migrations")
    }

    fn write_skill(dir: &std::path::Path, frontmatter: &str, body: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(
            dir.join("SKILL.md"),
            format!("---\n{frontmatter}\n---\n\n{body}\n"),
        )
        .unwrap();
    }

    #[test]
    fn installs_and_invokes_a_prompt_skill() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        write_skill(
            source.path(),
            "id: com.fergents.summarize\nname: Summarize\nversion: 1.0.0\nkind: prompt\ninput:\n  type: object\n  properties: { text: { type: string } }\n  required: [text]\n  additionalProperties: false\noutput:\n  type: object\n  properties: { summary: { type: string } }\n  required: [summary]\n  additionalProperties: false",
            "Summarize the text.",
        );
        let (service, _) = service(data.path());
        let view = service
            .install(&SkillSource::Dir(source.path().to_path_buf()), true)
            .unwrap();
        assert!(view.record.enabled);
        let output = service
            .invoke("com.fergents.summarize", json!({ "text": "hi" }))
            .unwrap();
        assert_eq!(output["summary"], "hi");
        assert!(service
            .invoke("com.fergents.summarize", json!({ "text": 1 }))
            .is_err());
    }

    #[test]
    fn caches_prompt_outputs() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        write_skill(
            source.path(),
            "id: com.fergents.cache\nname: Cache\nversion: 1.0.0\nkind: prompt\noutput:\n  type: object\n  properties: { summary: { type: string } }\n  required: [summary]\n  additionalProperties: false",
            "Echo.",
        );
        let (service, services) = service(data.path());
        service
            .install(&SkillSource::Dir(source.path().to_path_buf()), true)
            .unwrap();
        service.invoke("com.fergents.cache", json!({})).unwrap();
        service.invoke("com.fergents.cache", json!({})).unwrap();
        assert_eq!(services.calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn delegates_tool_skills() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        write_skill(
            source.path(),
            "id: com.fergents.echo\nname: Echo\nversion: 1.0.0\nkind: tool\nwraps: kernel:echo",
            "",
        );
        let (service, _) = service(data.path());
        service
            .install(&SkillSource::Dir(source.path().to_path_buf()), true)
            .unwrap();
        let output = service
            .invoke("com.fergents.echo", json!({ "x": 1 }))
            .unwrap();
        assert_eq!(output["tool"], "kernel:echo");
    }

    #[test]
    fn permissioned_skills_install_disabled() {
        let data = tempfile::tempdir().unwrap();
        let source = tempfile::tempdir().unwrap();
        write_skill(
            source.path(),
            "id: com.fergents.net\nname: Net\nversion: 1.0.0\nkind: prompt\npermissions: [net.http]",
            "Fetch.",
        );
        let (service, _) = service(data.path());
        let view = service
            .install(&SkillSource::Dir(source.path().to_path_buf()), false)
            .unwrap();
        assert!(!view.record.enabled);
        assert!(service.invoke("com.fergents.net", json!({})).is_err());
    }
}
