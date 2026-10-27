//! The Plugin Factory: an isolated workspace where the AI scaffolds, builds,
//! tests, and packages plugins and skills (`spec.md` §6, decision D18).
//!
//! The factory never installs anything. `install` only resolves the workspace
//! that a normal two-phase install review must inspect and confirm.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

use fergents_events::EventBus;
use serde_json::{json, Value};

/// The template catalogue.
pub const TEMPLATES: [&str; 6] = [
    "kernel-tool",
    "ui-panel",
    "hybrid",
    "theme",
    "skill-prompt",
    "skill-code",
];

/// Wall-clock limit for one build step.
pub const BUILD_TIMEOUT: Duration = Duration::from_secs(300);

/// The workspace metadata written by `scaffold` and read by every later step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkspaceConfig {
    /// `plugin` or `skill`.
    pub kind: String,
    /// `none` or `cargo`.
    pub build: String,
    /// Relative manifest path (`plugin.toml` or `SKILL.md`).
    pub manifest: String,
}

/// What `factory.install` hands to the caller (never an automatic install).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactoryTarget {
    /// `plugin` or `skill`.
    pub kind: String,
    /// The workspace directory.
    pub dir: PathBuf,
    /// The manifest id.
    pub id: String,
}

/// Factory failures with stable codes.
#[derive(Debug, thiserror::Error)]
pub enum FactoryError {
    /// Filesystem failure.
    #[error("factory io error: {0}")]
    Io(#[from] std::io::Error),
    /// Invalid request.
    #[error("invalid factory request: {0}")]
    Invalid(String),
    /// Unknown run id.
    #[error("unknown factory run: {0}")]
    RunNotFound(String),
    /// Unknown template name.
    #[error("unknown factory template: {0}")]
    UnknownTemplate(String),
    /// A required toolchain is not installed.
    #[error("toolchain `{tool}` is missing: {hint}")]
    ToolchainMissing {
        /// The missing binary.
        tool: String,
        /// How to install it.
        hint: String,
    },
    /// The build step failed.
    #[error("factory build failed (exit {code:?}): {stderr}")]
    BuildFailed {
        /// Process exit code, when known.
        code: Option<i32>,
        /// Tail of the error output.
        stderr: String,
    },
    /// The build step exceeded its wall-clock limit.
    #[error("factory build timed out after {}s", .0.as_secs())]
    Timeout(Duration),
    /// A package failure.
    #[error(transparent)]
    Package(#[from] fergents_packages::PackageError),
    /// A plugin manifest failure.
    #[error(transparent)]
    Plugin(#[from] fergents_plugins::PluginError),
    /// A skill manifest failure.
    #[error(transparent)]
    Skill(#[from] fergents_skills::SkillError),
}

impl FactoryError {
    /// The stable error code returned over the protocol.
    pub fn code(&self) -> &'static str {
        match self {
            FactoryError::Io(_) => "factory_io",
            FactoryError::Invalid(_) => "factory_invalid",
            FactoryError::RunNotFound(_) => "factory_run_not_found",
            FactoryError::UnknownTemplate(_) => "factory_unknown_template",
            FactoryError::ToolchainMissing { .. } => "factory_toolchain_missing",
            FactoryError::BuildFailed { .. } => "factory_build_failed",
            FactoryError::Timeout(_) => "factory_build_timeout",
            FactoryError::Package(_) => "package_invalid",
            FactoryError::Plugin(error) => error.code(),
            FactoryError::Skill(_) => "skill_invalid",
        }
    }
}

/// The factory service.
pub struct FactoryService {
    root: PathBuf,
    bus: Arc<EventBus>,
}

impl FactoryService {
    /// Build the service rooted at `$FERGENTS_HOME/factory`.
    pub fn new(data_dir: &Path, bus: Arc<EventBus>) -> Self {
        Self {
            root: data_dir.join("factory"),
            bus,
        }
    }

    /// The workspace root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Copy a template into a fresh workspace.
    pub fn scaffold(&self, template: &str, id: &str, name: &str) -> Result<Value, FactoryError> {
        if !TEMPLATES.contains(&template) {
            return Err(FactoryError::UnknownTemplate(template.to_string()));
        }
        let id = sanitize_id(id)?;
        let run_id = ulid::Ulid::new().to_string();
        let dir = self.root.join(&run_id);
        fs::create_dir_all(&dir)?;
        let files = render_template(template, &id, name, &dir)?;
        self.publish(
            "factory.scaffolded",
            json!({ "run_id": run_id, "template": template, "kind": template_kind(template) }),
        );
        Ok(json!({
            "run_id": run_id,
            "template": template,
            "kind": template_kind(template),
            "path": dir,
            "files": files,
        }))
    }

    /// Run the template's build step, if it has one.
    pub fn build(&self, run_id: &str) -> Result<Value, FactoryError> {
        let dir = self.workspace(run_id)?;
        let config = read_config(&dir)?;
        if config.build == "none" {
            return Ok(json!({
                "run_id": run_id,
                "skipped": true,
                "reason": "this template has no build step",
            }));
        }
        ensure_toolchain("cargo")?;
        let log = run_process(
            &dir,
            "cargo",
            &["component", "build", "--release"],
            BUILD_TIMEOUT,
        )?;
        fs::write(dir.join("build.log"), &log)?;
        self.publish("factory.built", json!({ "run_id": run_id }));
        Ok(json!({ "run_id": run_id, "skipped": false, "log": log }))
    }

    /// Validate the workspace manifest, files, and declared permissions.
    pub fn test(&self, run_id: &str) -> Result<Value, FactoryError> {
        let dir = self.workspace(run_id)?;
        let config = read_config(&dir)?;
        let report = if config.kind == "skill" {
            test_skill(&dir, &config)?
        } else {
            test_plugin(&dir, &config)?
        };
        fs::write(
            dir.join("test.json"),
            serde_json::to_vec_pretty(&report).unwrap_or_default(),
        )?;
        self.publish(
            "factory.tested",
            json!({
                "run_id": run_id,
                "ok": report.get("ok").and_then(Value::as_bool).unwrap_or(false),
            }),
        );
        Ok(report)
    }

    /// Produce a `.fergplugin`/`.fergskill` archive from the workspace.
    pub fn package(&self, run_id: &str) -> Result<Value, FactoryError> {
        let dir = self.workspace(run_id)?;
        let config = read_config(&dir)?;
        let dist = dir.join("dist");
        fs::create_dir_all(&dist)?;
        let id = manifest_id(&dir, &config)?;
        let (extension, path) = if config.kind == "skill" {
            let path = dist.join(format!("{id}.fergskill"));
            fergents_skills::package_skill(&dir, &path)?;
            ("fergskill", path)
        } else {
            let path = dist.join(format!("{id}.fergplugin"));
            fergents_plugins::package_plugin(&dir, &path)?;
            ("fergplugin", path)
        };
        self.publish(
            "factory.packaged",
            json!({ "run_id": run_id, "kind": config.kind }),
        );
        Ok(json!({
            "run_id": run_id,
            "kind": config.kind,
            "extension": extension,
            "path": path,
        }))
    }

    /// Resolve the workspace that an install review must inspect.
    pub fn target(&self, run_id: &str) -> Result<FactoryTarget, FactoryError> {
        let dir = self.workspace(run_id)?;
        let config = read_config(&dir)?;
        let id = manifest_id(&dir, &config)?;
        Ok(FactoryTarget {
            kind: config.kind,
            dir,
            id,
        })
    }

    /// A summary of the workspace: config, files, build log, and test report.
    pub fn report(&self, run_id: &str) -> Result<Value, FactoryError> {
        let dir = self.workspace(run_id)?;
        let config = read_config(&dir)?;
        let files = list_files(&dir)?;
        let build = fs::read_to_string(dir.join("build.log")).ok();
        let test = fs::read_to_string(dir.join("test.json"))
            .ok()
            .and_then(|text| serde_json::from_str::<Value>(&text).ok());
        let packaged = files.iter().find(|file| file.starts_with("dist/")).cloned();
        Ok(json!({
            "run_id": run_id,
            "kind": config.kind,
            "manifest": config.manifest,
            "path": dir,
            "files": files,
            "build_log": build,
            "test": test,
            "packaged": packaged,
        }))
    }

    /// Known workspaces, newest first.
    pub fn list(&self) -> Vec<String> {
        let mut entries: Vec<_> = match fs::read_dir(&self.root) {
            Ok(reader) => reader
                .flatten()
                .filter(|entry| entry.path().is_dir())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .collect(),
            Err(_) => Vec::new(),
        };
        entries.sort();
        entries.reverse();
        entries
    }

    fn workspace(&self, run_id: &str) -> Result<PathBuf, FactoryError> {
        let valid = !run_id.is_empty()
            && run_id
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_');
        if !valid {
            return Err(FactoryError::Invalid(format!("invalid run id: {run_id}")));
        }
        let dir = self.root.join(run_id);
        if !dir.is_dir() {
            return Err(FactoryError::RunNotFound(run_id.to_string()));
        }
        Ok(dir)
    }

    fn publish(&self, topic: &str, payload: Value) {
        let _ = self
            .bus
            .publish(topic.to_string(), "factory".to_string(), payload);
    }
}

fn test_plugin(dir: &Path, config: &WorkspaceConfig) -> Result<Value, FactoryError> {
    let text = fs::read_to_string(dir.join(&config.manifest))?;
    let manifest = fergents_plugins::PluginManifest::parse(&text)?;
    let mut problems = Vec::new();
    if let Err(err) = manifest.validate_files(dir) {
        problems.push(err.to_string());
    }
    let declared: Vec<String> = manifest.permissions.keys().cloned().collect();
    let warnings = style_warnings(dir)?;
    let plugin_kind = manifest.kind();
    Ok(json!({
        "kind": "plugin",
        "ok": problems.is_empty(),
        "id": manifest.plugin.id,
        "version": manifest.plugin.version,
        "plugin_kind": plugin_kind,
        "slots": manifest.slots,
        "commands": manifest.commands.iter().map(|command| command.id.clone()).collect::<Vec<_>>(),
        "capabilities": manifest.capabilities.iter().map(|cap| cap.id.clone()).collect::<Vec<_>>(),
        "settings": manifest.settings.iter().map(|setting| setting.key.clone()).collect::<Vec<_>>(),
        "permissions": { "declared": declared, "granted": Vec::<String>::new() },
        "problems": problems,
        "warnings": warnings,
    }))
}

fn test_skill(dir: &Path, config: &WorkspaceConfig) -> Result<Value, FactoryError> {
    let text = fs::read_to_string(dir.join(&config.manifest))?;
    let manifest = fergents_skills::parse(&text)?;
    let mut problems = Vec::new();
    if let Some(entry) = &manifest.entry {
        if !dir.join(entry).is_file() {
            problems.push(format!("missing entry: {entry}"));
        }
    }
    let cache = manifest.cache_enabled();
    let declared = manifest.permissions.clone();
    let skill_kind = manifest.kind.as_str();
    Ok(json!({
        "kind": "skill",
        "ok": problems.is_empty(),
        "id": manifest.id,
        "version": manifest.version,
        "skill_kind": skill_kind,
        "permissions": { "declared": declared, "granted": Vec::<String>::new() },
        "cache": cache,
        "problems": problems,
        "warnings": Vec::<String>::new(),
    }))
}

fn manifest_id(dir: &Path, config: &WorkspaceConfig) -> Result<String, FactoryError> {
    let text = fs::read_to_string(dir.join(&config.manifest))?;
    if config.kind == "skill" {
        Ok(fergents_skills::parse(&text)?.id)
    } else {
        Ok(fergents_plugins::PluginManifest::parse(&text)?.plugin.id)
    }
}

fn read_config(dir: &Path) -> Result<WorkspaceConfig, FactoryError> {
    let text = fs::read_to_string(dir.join("factory.json"))
        .map_err(|_| FactoryError::Invalid("workspace has no factory.json".to_string()))?;
    serde_json::from_str(&text).map_err(|err| FactoryError::Invalid(err.to_string()))
}

fn list_files(dir: &Path) -> Result<Vec<String>, FactoryError> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|err| FactoryError::Io(std::io::Error::other(err)))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(dir)
            .map_err(|_| FactoryError::Invalid("file outside the workspace".to_string()))?;
        files.push(relative.to_string_lossy().replace('\\', "/"));
    }
    files.sort();
    Ok(files)
}

/// Warn about hardcoded colors and raw pixel spacing in generated UI.
fn style_warnings(dir: &Path) -> Result<Vec<String>, FactoryError> {
    let mut warnings = Vec::new();
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        let entry = entry.map_err(|err| FactoryError::Io(std::io::Error::other(err)))?;
        if !entry.file_type().is_file() {
            continue;
        }
        let extension = entry
            .path()
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        if !matches!(extension, "js" | "jsx" | "ts" | "tsx" | "css") {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let name = entry
            .path()
            .strip_prefix(dir)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .replace('\\', "/");
        if text.contains('#') && has_hex_color(&text) {
            warnings.push(format!("{name}: hardcoded hex color; use style/ tokens"));
        }
        if text.contains("px") && has_pixel_value(&text) {
            warnings.push(format!("{name}: raw pixel spacing; use style/ tokens"));
        }
    }
    Ok(warnings)
}

fn has_hex_color(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.iter().enumerate().any(|(index, byte)| {
        if *byte != b'#' {
            return false;
        }
        let three = bytes.len() >= index + 4
            && bytes[index + 1..index + 4]
                .iter()
                .all(u8::is_ascii_hexdigit);
        let six = bytes.len() >= index + 7
            && bytes[index + 1..index + 7]
                .iter()
                .all(u8::is_ascii_hexdigit);
        three || six
    })
}

fn has_pixel_value(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes
        .windows(3)
        .any(|window| window[0].is_ascii_digit() && window[1] == b'p' && window[2] == b'x')
}

fn sanitize_id(id: &str) -> Result<String, FactoryError> {
    let id = id.trim();
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ".-_".contains(ch))
        && !matches!(id.chars().next(), Some('.') | Some('-') | Some('_'));
    if valid {
        Ok(id.to_string())
    } else {
        Err(FactoryError::Invalid(format!(
            "invalid package id: {id:?} (use lowercase letters, digits, dots, dashes)"
        )))
    }
}

fn template_kind(template: &str) -> &'static str {
    if template.starts_with("skill-") {
        "skill"
    } else {
        "plugin"
    }
}

fn ensure_toolchain(tool: &str) -> Result<(), FactoryError> {
    let found = Command::new(tool)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if found {
        Ok(())
    } else {
        Err(FactoryError::ToolchainMissing {
            tool: tool.to_string(),
            hint: "install Rust and cargo-component (cargo install cargo-component)".to_string(),
        })
    }
}

/// Run a process with a wall-clock limit and return its combined log.
fn run_process(
    dir: &Path,
    program: &str,
    args: &[&str],
    timeout: Duration,
) -> Result<String, FactoryError> {
    let mut child = Command::new(program)
        .args(args)
        .current_dir(dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| FactoryError::ToolchainMissing {
            tool: program.to_string(),
            hint: "install Rust and cargo-component (cargo install cargo-component)".to_string(),
        })?;
    let stdout = child.stdout.take().map(read_stream);
    let stderr = child.stderr.take().map(read_stream);
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(FactoryError::Timeout(timeout));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let mut log = format!("$ {program} {}\n", args.join(" "));
    if let Some(handle) = stdout {
        log.push_str(&handle.join().unwrap_or_default());
    }
    let errors = stderr
        .map(|handle| handle.join().unwrap_or_default())
        .unwrap_or_default();
    if !errors.is_empty() {
        log.push_str(&errors);
    }
    if !status.success() {
        return Err(FactoryError::BuildFailed {
            code: status.code(),
            stderr: tail(&errors, 2000),
        });
    }
    Ok(log)
}

fn read_stream(mut stream: impl Read + Send + 'static) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stream.read_to_string(&mut text);
        text
    })
}

fn tail(text: &str, limit: usize) -> String {
    if text.len() <= limit {
        text.to_string()
    } else {
        text[text.len() - limit..].to_string()
    }
}

fn render_template(
    template: &str,
    id: &str,
    name: &str,
    dir: &Path,
) -> Result<Vec<String>, FactoryError> {
    let mut files = Vec::new();
    let (config, manifest, extra): (WorkspaceConfig, String, Vec<(&str, String)>) = match template {
        "kernel-tool" => (
            WorkspaceConfig {
                kind: "plugin".to_string(),
                build: "cargo".to_string(),
                manifest: "plugin.toml".to_string(),
            },
            plugin_manifest(id, name, Some("plugin.wasm"), None, ""),
            vec![
                ("src/lib.rs", kernel_tool_source().to_string()),
                ("Cargo.toml", component_cargo(id)),
                ("examples/input.json", "{\"text\":\"hello\"}\n".to_string()),
            ],
        ),
        "ui-panel" => (
            WorkspaceConfig {
                kind: "plugin".to_string(),
                build: "none".to_string(),
                manifest: "plugin.toml".to_string(),
            },
            plugin_manifest(
                id,
                name,
                None,
                Some("ui.js"),
                "slots = [\"rightpanel.section\"]\n",
            ),
            vec![("ui.js", ui_source())],
        ),
        "hybrid" => (
            WorkspaceConfig {
                kind: "plugin".to_string(),
                build: "cargo".to_string(),
                manifest: "plugin.toml".to_string(),
            },
            plugin_manifest(
                id,
                name,
                Some("plugin.wasm"),
                Some("ui.js"),
                "slots = [\"main.tab\"]\n",
            ),
            vec![
                ("src/lib.rs", kernel_tool_source().to_string()),
                ("ui.js", ui_source()),
                ("Cargo.toml", component_cargo(id)),
            ],
        ),
        "theme" => (
            WorkspaceConfig {
                kind: "plugin".to_string(),
                build: "none".to_string(),
                manifest: "plugin.toml".to_string(),
            },
            plugin_manifest(
                id,
                name,
                None,
                Some("ui.js"),
                "slots = [\"theme.tokens\"]\n",
            ),
            vec![
                ("ui.js", ui_source()),
                (
                    "theme.css",
                    ":root { --fg-accent: var(--accent); }\n".to_string(),
                ),
            ],
        ),
        "skill-prompt" => (
            WorkspaceConfig {
                kind: "skill".to_string(),
                build: "none".to_string(),
                manifest: "SKILL.md".to_string(),
            },
            prompt_skill(id, name),
            vec![("examples/input.json", "{\"text\":\"hello\"}\n".to_string())],
        ),
        "skill-code" => (
            WorkspaceConfig {
                kind: "skill".to_string(),
                build: "cargo".to_string(),
                manifest: "SKILL.md".to_string(),
            },
            code_skill(id, name),
            vec![
                ("src/lib.rs", skill_source().to_string()),
                ("Cargo.toml", component_cargo(id)),
            ],
        ),
        other => return Err(FactoryError::UnknownTemplate(other.to_string())),
    };
    fs::write(dir.join(&config.manifest), manifest)?;
    files.push(config.manifest.clone());
    for (path, contents) in extra {
        let target = dir.join(path);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&target, contents)?;
        files.push(path.to_string());
    }
    fs::write(
        dir.join("factory.json"),
        serde_json::to_vec_pretty(&config).unwrap_or_default(),
    )?;
    files.push("factory.json".to_string());
    fs::write(dir.join("README.md"), readme(id, name, template))?;
    files.push("README.md".to_string());
    Ok(files)
}

fn plugin_manifest(
    id: &str,
    name: &str,
    module: Option<&str>,
    ui: Option<&str>,
    extra: &str,
) -> String {
    let mut text = String::new();
    if !extra.is_empty() {
        // Top-level keys must precede the first table header.
        text.push_str(extra);
        text.push('\n');
    }
    text.push_str(&format!(
        "[plugin]\nid = \"{id}\"\nname = \"{name}\"\nversion = \"0.1.0\"\napi_version = 1\n\n"
    ));
    if let Some(module) = module {
        text.push_str(&format!("[kernel]\nmodule = \"{module}\"\n\n"));
    }
    if let Some(ui) = ui {
        text.push_str(&format!("[ui]\nentry = \"{ui}\"\n\n"));
    }
    text
}

fn prompt_skill(id: &str, name: &str) -> String {
    format!(
        "---\nid: {id}\nname: {name}\nversion: 0.1.0\nkind: prompt\ndescription: {name}\ninput:\n  type: object\n  properties:\n    text: {{ type: string }}\n  required: [text]\n  additionalProperties: false\noutput:\n  type: object\n  properties:\n    summary: {{ type: string }}\n  required: [summary]\n  additionalProperties: false\n---\n\n{name}: summarize the input text in one sentence.\n"
    )
}

fn code_skill(id: &str, name: &str) -> String {
    format!(
        "---\nid: {id}\nname: {name}\nversion: 0.1.0\nkind: code\nentry: skill.wasm\n---\n\n{name} runs in the WASM sandbox.\n"
    )
}

fn component_cargo(id: &str) -> String {
    format!(
        "[package]\nname = \"{}\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[dependencies]\nwit-bindgen = \"0.36\"\n",
        id.replace(['.', '-'], "_")
    )
}

fn kernel_tool_source() -> &'static str {
    "// Implement `list-tools`, `call-tool`, and the host `call` import.\n// See PLUGINS.md §4 for the component contract.\n"
}

fn skill_source() -> &'static str {
    "// Export `run(input: string) -> result<string, string>`.\n"
}

fn ui_source() -> String {
    "import { definePlugin } from '@fergents/plugin-sdk';\n\n\
     export default definePlugin({\n  render(ctx) {\n    return null;\n  },\n});\n"
        .to_string()
}

fn readme(id: &str, name: &str, template: &str) -> String {
    format!(
        "# {name}\n\nGenerated by the Fergents Plugin Factory from the `{template}` template.\n\n- id: `{id}`\n- Review `factory.test` before packaging.\n- Installation always requires an explicit review approval.\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service(dir: &Path) -> FactoryService {
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("migrations");
        let storage = Arc::new(
            fergents_storage::SqliteStorage::open(&dir.join("global.sqlite"), &migrations).unwrap(),
        );
        let bus = Arc::new(EventBus::new(storage, 32));
        FactoryService::new(dir, bus)
    }

    #[test]
    fn scaffolds_tests_and_packages_a_prompt_skill() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(dir.path());
        let scaffold = service
            .scaffold("skill-prompt", "com.example.summary", "Summary")
            .unwrap();
        let run_id = scaffold["run_id"].as_str().unwrap().to_string();
        assert!(scaffold["files"]
            .as_array()
            .unwrap()
            .iter()
            .any(|file| file == "SKILL.md"));

        let build = service.build(&run_id).unwrap();
        assert_eq!(build["skipped"], json!(true));

        let report = service.test(&run_id).unwrap();
        assert_eq!(report["ok"], json!(true));
        assert_eq!(report["kind"], json!("skill"));
        assert_eq!(report["id"], json!("com.example.summary"));

        let packaged = service.package(&run_id).unwrap();
        let path = PathBuf::from(packaged["path"].as_str().unwrap());
        assert!(path.is_file());
        assert!(service.report(&run_id).unwrap()["packaged"].is_string());
    }

    #[test]
    fn scaffolds_a_ui_plugin_with_style_warnings() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(dir.path());
        let scaffold = service
            .scaffold("ui-panel", "com.example.panel", "Panel")
            .unwrap();
        let run_id = scaffold["run_id"].as_str().unwrap().to_string();
        let report = service.test(&run_id).unwrap();
        assert_eq!(report["ok"], json!(true));
        assert_eq!(report["plugin_kind"], json!("ui"));
        assert!(report["slots"]
            .as_array()
            .unwrap()
            .iter()
            .any(|slot| slot == "rightpanel.section"));
    }

    #[test]
    fn rejects_unknown_templates_and_unsafe_runs() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(dir.path());
        let error = service.scaffold("nope", "com.example.x", "X").unwrap_err();
        assert_eq!(error.code(), "factory_unknown_template");
        assert!(service.target("../escape").is_err());
        assert_eq!(service.list().len(), 0);
    }

    #[test]
    fn kernel_tool_template_requires_a_build_toolchain() {
        let dir = tempfile::tempdir().unwrap();
        let service = service(dir.path());
        let scaffold = service
            .scaffold("kernel-tool", "com.example.tool", "Tool")
            .unwrap();
        let run_id = scaffold["run_id"].as_str().unwrap().to_string();
        let report = service.test(&run_id).unwrap();
        assert_eq!(report["ok"], json!(false));
        assert!(report["problems"]
            .as_array()
            .unwrap()
            .iter()
            .any(|problem| problem.as_str().unwrap().contains("plugin.wasm")));
    }
}
