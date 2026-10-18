//! Note templates and variable rendering (`BRAIN.md` §4.5; spec Task 13.2).
//!
//! Templates are markdown files in the vault's templates folder. They support
//! the variables `{{date}}`, `{{time}}`, `{{datetime}}`, `{{title}}`,
//! `{{selection}}`, `{{clipboard}}`, and `{{cursor}}`. Rendering is pure: it
//! takes the template text plus the resolved variables and returns the content
//! with the cursor's byte offset. Unknown `{{...}}` placeholders are left
//! untouched so an unrecognised variable is never silently dropped.

use std::path::{Path, PathBuf};

use crate::error::IndexError;

/// The format used for `{{date}}` (decision D10).
pub const DATE_FORMAT: &str = "%Y-%m-%d";
/// The format used for `{{time}}`.
pub const TIME_FORMAT: &str = "%H:%M";
/// The format used for `{{datetime}}`.
pub const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M";

/// The values substituted into a template's variables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TemplateVariables {
    /// `{{date}}`.
    pub date: String,
    /// `{{time}}`.
    pub time: String,
    /// `{{datetime}}`.
    pub datetime: String,
    /// `{{title}}` (prompted on insert).
    pub title: String,
    /// `{{selection}}` (the current editor selection).
    pub selection: String,
    /// `{{clipboard}}`.
    pub clipboard: String,
}

impl TemplateVariables {
    /// Derive the date/time variables from a wall-clock instant (`now`), using
    /// the default formats. `title`, `selection`, and `clipboard` start empty.
    pub fn from_now(now: chrono::NaiveDateTime) -> Self {
        Self {
            date: now.format(DATE_FORMAT).to_string(),
            time: now.format(TIME_FORMAT).to_string(),
            datetime: now.format(DATETIME_FORMAT).to_string(),
            title: String::new(),
            selection: String::new(),
            clipboard: String::new(),
        }
    }

    fn get(&self, key: &str) -> Option<&str> {
        match key {
            "date" => Some(&self.date),
            "time" => Some(&self.time),
            "datetime" => Some(&self.datetime),
            "title" => Some(&self.title),
            "selection" => Some(&self.selection),
            "clipboard" => Some(&self.clipboard),
            _ => None,
        }
    }
}

/// The result of rendering a template.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedTemplate {
    /// The rendered content, with every `{{cursor}}` token removed.
    pub content: String,
    /// The byte offset where the first `{{cursor}}` token appeared, if any.
    pub cursor: Option<usize>,
}

/// The `{{cursor}}` marker.
const CURSOR_TOKEN: &str = "cursor";

/// Render `template`, substituting `vars` and resolving `{{cursor}}`.
pub fn render(template: &str, vars: &TemplateVariables) -> RenderedTemplate {
    let mut content = String::with_capacity(template.len());
    let mut cursor = None;
    let mut rest = template;

    while let Some(open) = rest.find("{{") {
        content.push_str(&rest[..open]);
        let after = &rest[open + 2..];
        let Some(close) = after.find("}}") else {
            // Unclosed `{{` — treat the remainder as literal text.
            content.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let key = after[..close].trim();
        if key == CURSOR_TOKEN {
            if cursor.is_none() {
                cursor = Some(content.len());
            }
        } else if let Some(value) = vars.get(key) {
            content.push_str(value);
        } else {
            // Unknown variable: keep the placeholder verbatim.
            content.push_str(&rest[open..open + 2 + close + 2]);
        }
        rest = &after[close + 2..];
    }
    content.push_str(rest);

    RenderedTemplate { content, cursor }
}

/// A template file discovered in the templates folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateInfo {
    /// The template's name (its file name without the `.md` extension).
    pub name: String,
    /// The template's absolute path on disk.
    pub path: PathBuf,
}

/// List the markdown templates in `dir`, sorted by name.
///
/// A missing directory yields an empty list (a brain need not have templates).
pub fn list_templates(dir: &Path) -> Result<Vec<TemplateInfo>, IndexError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(IndexError::Io(e.to_string())),
    };

    let mut templates = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| IndexError::Io(e.to_string()))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Some(stem) = name.strip_suffix(".md") else {
            continue;
        };
        templates.push(TemplateInfo {
            name: stem.to_string(),
            path,
        });
    }
    templates.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(templates)
}

/// Create a template `name` (with the `.md` extension added if missing) in
/// `dir`, containing `content`. Fails if the template already exists.
pub fn create_template(dir: &Path, name: &str, content: &str) -> Result<PathBuf, IndexError> {
    let file_name = template_file_name(name)?;
    std::fs::create_dir_all(dir).map_err(|e| IndexError::Io(e.to_string()))?;
    let path = dir.join(file_name);
    if path.exists() {
        return Err(IndexError::Invalid(format!(
            "template '{name}' already exists"
        )));
    }
    std::fs::write(&path, content).map_err(|e| IndexError::Io(e.to_string()))?;
    Ok(path)
}

/// The `.md` file name for a template `name`, validating that it is a plain
/// file name (no path separators) and not empty.
pub fn template_file_name(name: &str) -> Result<String, IndexError> {
    let stem = name.strip_suffix(".md").unwrap_or(name);
    if stem.trim().is_empty() {
        return Err(IndexError::Invalid(
            "template name must not be empty".into(),
        ));
    }
    if stem.contains('/') || stem.contains('\\') || stem.contains("..") {
        return Err(IndexError::Invalid(format!(
            "template name '{name}' must be a plain file name"
        )));
    }
    Ok(format!("{stem}.md"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn vars() -> TemplateVariables {
        TemplateVariables {
            date: "2025-10-18".into(),
            time: "09:30".into(),
            datetime: "2025-10-18 09:30".into(),
            title: "Standup".into(),
            selection: "selected".into(),
            clipboard: "copied".into(),
        }
    }

    #[test]
    fn substitutes_every_variable() {
        let template =
            "{{title}} on {{date}} at {{time}} ({{datetime}})\n> {{selection}}\n> {{clipboard}}\n";
        let rendered = render(template, &vars());
        assert_eq!(
            rendered.content,
            "Standup on 2025-10-18 at 09:30 (2025-10-18 09:30)\n> selected\n> copied\n"
        );
        assert_eq!(rendered.cursor, None);
    }

    #[test]
    fn cursor_is_removed_and_its_byte_offset_returned() {
        let rendered = render("Hello {{cursor}}world", &vars());
        assert_eq!(rendered.content, "Hello world");
        assert_eq!(rendered.cursor, Some(6));
    }

    #[test]
    fn only_the_first_cursor_is_recorded_but_all_are_removed() {
        let rendered = render("a{{cursor}}b{{cursor}}c", &vars());
        assert_eq!(rendered.content, "abc");
        assert_eq!(rendered.cursor, Some(1));
    }

    #[test]
    fn unknown_variables_are_left_verbatim() {
        let rendered = render("keep {{nope}} and {{date}}", &vars());
        assert_eq!(rendered.content, "keep {{nope}} and 2025-10-18");
    }

    #[test]
    fn unclosed_placeholder_is_literal() {
        let rendered = render("broken {{date", &vars());
        assert_eq!(rendered.content, "broken {{date");
    }

    #[test]
    fn from_now_uses_the_default_formats() {
        let now = NaiveDate::from_ymd_opt(2025, 10, 18)
            .unwrap()
            .and_hms_opt(9, 30, 0)
            .unwrap();
        let vars = TemplateVariables::from_now(now);
        assert_eq!(vars.date, "2025-10-18");
        assert_eq!(vars.time, "09:30");
        assert_eq!(vars.datetime, "2025-10-18 09:30");
        assert!(vars.title.is_empty());
    }

    #[test]
    fn create_then_list_round_trips_and_sorts() {
        let dir = tempfile::tempdir().unwrap();
        create_template(dir.path(), "daily", "# {{date}}").unwrap();
        create_template(dir.path(), "meeting.md", "## {{title}}").unwrap();

        let templates = list_templates(dir.path()).unwrap();
        let names: Vec<_> = templates.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, vec!["daily", "meeting"]);
        assert_eq!(templates[0].path, dir.path().join("daily.md"));
    }

    #[test]
    fn create_rejects_duplicates_and_bad_names() {
        let dir = tempfile::tempdir().unwrap();
        create_template(dir.path(), "daily", "x").unwrap();
        assert!(matches!(
            create_template(dir.path(), "daily", "y"),
            Err(IndexError::Invalid(_))
        ));
        assert!(matches!(
            create_template(dir.path(), "", "x"),
            Err(IndexError::Invalid(_))
        ));
        assert!(matches!(
            create_template(dir.path(), "../escape", "x"),
            Err(IndexError::Invalid(_))
        ));
    }

    #[test]
    fn list_missing_directory_is_empty() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("nope");
        assert!(list_templates(&missing).unwrap().is_empty());
    }
}
