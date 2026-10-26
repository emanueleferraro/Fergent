//! `SKILL.md` frontmatter (`SKILLS.md` §3, §11).

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::error::SkillError;

/// The four skill kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SkillKind {
    /// A prompt sent to a model.
    Prompt,
    /// A wrapper around a kernel or MCP tool.
    Tool,
    /// A WASM component.
    Code,
    /// A WASM component plus a prompt, in declared order.
    Hybrid,
}

impl SkillKind {
    /// The persisted label.
    pub fn as_str(self) -> &'static str {
        match self {
            SkillKind::Prompt => "prompt",
            SkillKind::Tool => "tool",
            SkillKind::Code => "code",
            SkillKind::Hybrid => "hybrid",
        }
    }
}

/// Cache settings from the frontmatter.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Explicit enable/disable.
    #[serde(default)]
    pub enabled: Option<bool>,
    /// Time-to-live in seconds.
    #[serde(default)]
    pub ttl_s: Option<u64>,
}

/// Declared permissions a skill may request.
pub const PERMISSIONS: [&str; 7] = [
    "storage.local",
    "fs.read",
    "fs.write",
    "net.http",
    "llm.call",
    "mcp.connect",
    "skill.invoke",
];

/// The skill manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillManifest {
    /// Stable id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// Semver.
    pub version: String,
    /// One-line description.
    #[serde(default)]
    pub description: Option<String>,
    /// The execution kind.
    pub kind: SkillKind,
    /// Preferred model route.
    #[serde(default)]
    pub model: Option<String>,
    /// Fallback models.
    #[serde(default)]
    pub model_fallback: Vec<String>,
    /// Sampling temperature.
    #[serde(default = "default_temperature")]
    pub temperature: f64,
    /// Output token cap.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// Input JSON Schema.
    #[serde(default)]
    pub input: Option<Value>,
    /// Output JSON Schema.
    #[serde(default)]
    pub output: Option<Value>,
    /// WASM entry for `code`/`hybrid`.
    #[serde(default)]
    pub entry: Option<String>,
    /// Source language label.
    #[serde(default)]
    pub language: Option<String>,
    /// Wrapped tool for `tool` skills.
    #[serde(default)]
    pub wraps: Option<String>,
    /// Declared permissions.
    #[serde(default)]
    pub permissions: Vec<String>,
    /// Cache policy.
    #[serde(default)]
    pub cache: CacheConfig,
    /// Hybrid step order (`code`, `prompt`).
    #[serde(default)]
    pub steps: Vec<String>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Author.
    #[serde(default)]
    pub author: Option<String>,
}

fn default_temperature() -> f64 {
    0.2
}

fn default_max_tokens() -> u32 {
    2048
}

/// A parsed skill folder.
#[derive(Debug, Clone)]
pub struct ParsedSkill {
    /// The frontmatter.
    pub manifest: SkillManifest,
    /// The prompt body.
    pub prompt: String,
}

impl SkillManifest {
    /// The input schema (closed object by default).
    pub fn input_schema(&self) -> Value {
        self.input.clone().unwrap_or_else(closed_object)
    }

    /// The output schema (closed object by default).
    pub fn output_schema(&self) -> Value {
        self.output.clone().unwrap_or_else(closed_object)
    }

    /// Whether prompt output may be cached.
    pub fn cache_enabled(&self) -> bool {
        self.cache.enabled.unwrap_or(self.permissions.is_empty())
    }

    /// Cache TTL in seconds.
    pub fn cache_ttl_s(&self) -> u64 {
        self.cache.ttl_s.unwrap_or(3_600)
    }

    /// The hybrid step order, defaulting to code then prompt.
    pub fn steps(&self) -> Vec<String> {
        if self.steps.is_empty() {
            vec!["code".to_string(), "prompt".to_string()]
        } else {
            self.steps.clone()
        }
    }

    /// Validate the manifest.
    pub fn validate(&self) -> Result<(), SkillError> {
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ".-_".contains(ch))
            || matches!(self.id.chars().next(), Some('.') | Some('-') | Some('_'))
        {
            return Err(SkillError::Invalid(format!(
                "invalid skill id: {:?}",
                self.id
            )));
        }
        fergents_packages::parse_version(&self.version)?;
        for permission in &self.permissions {
            if !PERMISSIONS.contains(&permission.as_str()) {
                return Err(SkillError::Invalid(format!(
                    "unknown skill permission: {permission}"
                )));
            }
        }
        match self.kind {
            SkillKind::Code => {
                if self.entry.as_deref().unwrap_or("").is_empty() {
                    return Err(SkillError::Invalid(
                        "a code skill needs `entry`".to_string(),
                    ));
                }
            }
            SkillKind::Tool => {
                if self.wraps.as_deref().unwrap_or("").is_empty() {
                    return Err(SkillError::Invalid(
                        "a tool skill needs `wraps`".to_string(),
                    ));
                }
            }
            SkillKind::Hybrid => {
                for step in self.steps() {
                    if !matches!(step.as_str(), "code" | "prompt") {
                        return Err(SkillError::Invalid(format!("unknown hybrid step: {step}")));
                    }
                }
                if self.steps().iter().any(|step| step == "code")
                    && self.entry.as_deref().unwrap_or("").is_empty()
                {
                    return Err(SkillError::Invalid(
                        "a hybrid skill with a code step needs `entry`".to_string(),
                    ));
                }
            }
            SkillKind::Prompt => {}
        }
        Ok(())
    }
}

fn closed_object() -> Value {
    json!({ "type": "object", "additionalProperties": false })
}

/// Parse `SKILL.md` text into a manifest plus prompt body.
pub fn parse(text: &str) -> Result<SkillManifest, SkillError> {
    let (frontmatter, _body) = split_frontmatter(text)
        .ok_or_else(|| SkillError::Invalid("SKILL.md has no frontmatter".to_string()))?;
    let manifest: SkillManifest =
        serde_yaml::from_str(frontmatter).map_err(|err| SkillError::Invalid(err.to_string()))?;
    manifest.validate()?;
    Ok(manifest)
}

/// Split a `---` frontmatter document into (frontmatter, body).
pub fn split_frontmatter(text: &str) -> Option<(&str, &str)> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let rest = text.strip_prefix("---")?;
    let rest = rest
        .strip_prefix("\r\n")
        .or_else(|| rest.strip_prefix('\n'))?;
    let end = rest.find("\n---")?;
    let frontmatter = &rest[..end];
    let after = &rest[end + 4..];
    let body = after
        .strip_prefix("\r\n")
        .or_else(|| after.strip_prefix('\n'))
        .unwrap_or(after);
    Some((frontmatter, body))
}

/// Build the prompt body of a skill (the Markdown after the frontmatter).
pub fn body(text: &str) -> String {
    split_frontmatter(text)
        .map(|(_, body)| body.trim().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PROMPT: &str = "---\nid: com.fergents.summarize\nname: Summarize\nversion: 1.0.0\nkind: prompt\ninput:\n  type: object\n  properties:\n    text: { type: string }\n  required: [text]\n  additionalProperties: false\noutput:\n  type: object\n  properties:\n    summary: { type: string }\n  required: [summary]\n  additionalProperties: false\n---\n\nSummarize the input text.\n";

    #[test]
    fn parses_a_prompt_skill() {
        let manifest = parse(PROMPT).unwrap();
        assert_eq!(manifest.kind, SkillKind::Prompt);
        assert_eq!(manifest.id, "com.fergents.summarize");
        assert_eq!(body(PROMPT), "Summarize the input text.");
        assert!(manifest.cache_enabled());
    }

    #[test]
    fn rejects_unknown_permissions() {
        let text = PROMPT.replace("kind: prompt", "kind: prompt\npermissions: [root]");
        assert!(parse(&text).is_err());
    }

    #[test]
    fn code_requires_an_entry() {
        let text = "---\nid: x\nname: X\nversion: 1.0.0\nkind: code\n---\n";
        assert!(parse(text).is_err());
    }
}
