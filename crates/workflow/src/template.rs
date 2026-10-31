//! The `{{ … }}` template engine (spec §1.5, Appendix D).
//!
//! A template is literal text with embedded expressions. An expression is a
//! context path plus an optional chain of filters. There is no arithmetic, no
//! function call, and no evaluation of user text as code — by construction.
//!
//! Resolution is strict: a path that does not exist fails the node with
//! `workflow_template_missing` unless a `default(…)` filter is present.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::error::WorkflowError;

/// Resolves `{{ secrets.<name> }}` at execution time.
///
/// Implementations live in `fergents-secrets`; the engine only needs a lookup
/// so the template layer stays free of keychain code and of any I/O.
pub trait SecretResolver {
    /// Resolve a secret by name. Secrets are never logged or persisted.
    fn resolve(&self, name: &str) -> Option<Value>;
}

/// A resolver that knows no secrets, used by validation and dry runs.
pub struct NoSecrets;

impl SecretResolver for NoSecrets {
    fn resolve(&self, _name: &str) -> Option<Value> {
        None
    }
}

/// The values a template resolves against.
#[derive(Clone)]
pub struct RenderContext {
    /// The run context (node outputs, trigger payload, variables).
    pub data: Value,
    /// The current node's result, exposed as `{{ result.… }}`.
    pub result: Option<Value>,
    /// The run id, exposed as `{{ run.id }}`.
    pub run_id: String,
    /// The workflow id, exposed as `{{ workflow.id }}`.
    pub workflow_id: String,
    /// The injected clock, exposed as `{{ now }}` and `{{ today }}`.
    pub now: DateTime<Utc>,
    /// The secret resolver.
    pub secrets: Arc<dyn SecretResolver + Send + Sync>,
}

impl RenderContext {
    /// A context with no result and no secrets, for validation and dry runs.
    pub fn for_validation(now: DateTime<Utc>) -> Self {
        RenderContext {
            data: Value::Object(Default::default()),
            result: None,
            run_id: String::new(),
            workflow_id: String::new(),
            now,
            secrets: Arc::new(NoSecrets),
        }
    }

    /// A context that only swapped the data, keeping the run identity, the
    /// clock, and the secret resolver. Used by `transform` and `loop`.
    pub fn with_data(&self, data: Value) -> Self {
        RenderContext {
            data,
            result: None,
            run_id: self.run_id.clone(),
            workflow_id: self.workflow_id.clone(),
            now: self.now,
            secrets: Arc::clone(&self.secrets),
        }
    }
}

/// A compiled template.
#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    segments: Vec<Segment>,
}

#[derive(Debug, Clone, PartialEq)]
enum Segment {
    Text(String),
    Expr(Expr),
}

/// A compiled expression: a rooted path plus filters.
#[derive(Debug, Clone, PartialEq)]
pub struct Expr {
    root: Root,
    path: Vec<PathSeg>,
    filters: Vec<Filter>,
}

#[derive(Debug, Clone, PartialEq)]
enum Root {
    Context(String),
    Result,
    Secret(String),
    RunId,
    WorkflowId,
    Now,
    Today,
}

#[derive(Debug, Clone, PartialEq)]
enum PathSeg {
    Key(String),
    Index(usize),
}

#[derive(Debug, Clone, PartialEq)]
enum Filter {
    Length,
    Upper,
    Lower,
    Json,
    Join(String),
    Default(Value),
    Date(String),
}

impl Template {
    /// Compile a template, rejecting syntax errors at save time.
    pub fn parse(source: &str) -> Result<Template, WorkflowError> {
        let mut segments = Vec::new();
        let mut cursor = 0usize;
        let mut text_start = 0usize;

        while let Some(offset) = source[cursor..].find("{{") {
            let open = cursor + offset;
            let body_start = open + 2;
            let close_rel = source[body_start..].find("}}").ok_or_else(|| {
                WorkflowError::TemplateInvalid(format!(
                    "unterminated `{{{{` at byte {open} in `{source}`"
                ))
            })?;
            let body_end = body_start + close_rel;

            if open > text_start {
                segments.push(Segment::Text(source[text_start..open].to_string()));
            }
            let inner = source[body_start..body_end].trim();
            segments.push(Segment::Expr(Expr::parse(inner)?));

            cursor = body_end + 2;
            text_start = cursor;
        }

        if text_start < source.len() {
            segments.push(Segment::Text(source[text_start..].to_string()));
        }

        if segments.is_empty() {
            segments.push(Segment::Text(String::new()));
        }

        Ok(Template { segments })
    }

    /// Whether the template is pure literal text.
    pub fn is_static(&self) -> bool {
        self.segments
            .iter()
            .all(|segment| matches!(segment, Segment::Text(_)))
    }

    /// Whether the template reads a secret. Callers use this to redact.
    pub fn reads_secret(&self) -> bool {
        self.segments.iter().any(|segment| match segment {
            Segment::Expr(expr) => matches!(expr.root, Root::Secret(_)),
            Segment::Text(_) => false,
        })
    }

    /// Every context key this template reads, as `a.b[0].c` strings.
    ///
    /// Only context-rooted paths are reported; `result`, `secrets`, and the
    /// built-ins are excluded.
    pub fn context_paths(&self) -> Vec<String> {
        let mut out = Vec::new();
        for segment in &self.segments {
            if let Segment::Expr(expr) = segment {
                if let Root::Context(root) = &expr.root {
                    let mut path = root.clone();
                    for seg in &expr.path {
                        match seg {
                            PathSeg::Key(key) => {
                                path.push('.');
                                path.push_str(key);
                            }
                            PathSeg::Index(index) => {
                                path.push('[');
                                path.push_str(&index.to_string());
                                path.push(']');
                            }
                        }
                    }
                    out.push(path);
                }
            }
        }
        out
    }

    /// Render the template.
    ///
    /// A template made of a single expression yields the native JSON value, so
    /// `{{ fetch.items }}` stays an array. Any template with surrounding text
    /// yields a string.
    pub fn render(&self, ctx: &RenderContext) -> Result<Value, WorkflowError> {
        if self.segments.len() == 1 {
            if let Segment::Expr(expr) = &self.segments[0] {
                return expr.eval(ctx)?.ok_or_else(|| expr.missing());
            }
        }

        let mut out = String::new();
        for segment in &self.segments {
            match segment {
                Segment::Text(text) => out.push_str(text),
                Segment::Expr(expr) => {
                    let value = expr.eval(ctx)?.ok_or_else(|| expr.missing())?;
                    out.push_str(&as_text(&value));
                }
            }
        }
        Ok(Value::String(out))
    }

    /// Render a template that must produce a string.
    pub fn render_string(&self, ctx: &RenderContext) -> Result<String, WorkflowError> {
        match self.render(ctx)? {
            Value::String(text) => Ok(text),
            other => Ok(as_text(&other)),
        }
    }
}

impl Expr {
    /// Compile an expression (without the surrounding braces).
    pub fn parse(source: &str) -> Result<Expr, WorkflowError> {
        let parts = split_filters(source)?;
        let head = parts
            .first()
            .map(|part| part.trim())
            .filter(|part| !part.is_empty())
            .ok_or_else(|| {
                WorkflowError::TemplateInvalid(format!("empty expression `{source}`"))
            })?;

        let (root, path) = parse_path(head)?;
        let mut filters = Vec::new();
        for raw in &parts[1..] {
            filters.push(parse_filter(raw.trim())?);
        }

        Ok(Expr {
            root,
            path,
            filters,
        })
    }

    /// The human-readable path, used in error messages.
    pub fn display_path(&self) -> String {
        let mut out = match &self.root {
            Root::Context(name) => name.clone(),
            Root::Result => "result".to_string(),
            Root::Secret(name) => format!("secrets.{name}"),
            Root::RunId => "run.id".to_string(),
            Root::WorkflowId => "workflow.id".to_string(),
            Root::Now => "now".to_string(),
            Root::Today => "today".to_string(),
        };
        for seg in &self.path {
            match seg {
                PathSeg::Key(key) => {
                    out.push('.');
                    out.push_str(key);
                }
                PathSeg::Index(index) => {
                    out.push('[');
                    out.push_str(&index.to_string());
                    out.push(']');
                }
            }
        }
        out
    }

    /// Evaluate to a value; `None` means "path missing".
    pub fn eval(&self, ctx: &RenderContext) -> Result<Option<Value>, WorkflowError> {
        let base = match &self.root {
            Root::Context(name) => ctx.data.get(name).cloned(),
            Root::Result => ctx.result.clone(),
            Root::Secret(name) => ctx.secrets.resolve(name),
            Root::RunId => Some(Value::String(ctx.run_id.clone())),
            Root::WorkflowId => Some(Value::String(ctx.workflow_id.clone())),
            Root::Now => Some(Value::String(ctx.now.to_rfc3339())),
            Root::Today => Some(Value::String(ctx.now.date_naive().to_string())),
        };

        let mut value = match base {
            Some(value) => value,
            None => return Ok(apply_defaults(&self.filters, None)),
        };

        for seg in &self.path {
            value = match descend(value, seg) {
                Some(next) => next,
                None => return Ok(apply_defaults(&self.filters, None)),
            };
        }

        let mut current = Some(value);
        for filter in &self.filters {
            current = match current {
                None => None,
                Some(value) => apply_filter(filter, value)?,
            };
        }
        Ok(current)
    }

    /// Evaluate and coerce to a boolean (used by `condition` and `filter`).
    pub fn eval_bool(&self, ctx: &RenderContext) -> Result<bool, WorkflowError> {
        match self.eval(ctx)? {
            Some(value) => Ok(truthy(&value)),
            None => Err(self.missing()),
        }
    }

    fn missing(&self) -> WorkflowError {
        WorkflowError::TemplateMissing(self.display_path())
    }
}

fn apply_defaults(filters: &[Filter], value: Option<Value>) -> Option<Value> {
    let mut current = value;
    for filter in filters {
        if let Filter::Default(default) = filter {
            if current.is_none() {
                current = Some(default.clone());
            }
        }
    }
    current
}

fn descend(value: Value, seg: &PathSeg) -> Option<Value> {
    match seg {
        PathSeg::Key(key) => match value {
            Value::Object(map) => map.get(key).cloned(),
            _ => None,
        },
        PathSeg::Index(index) => match value {
            Value::Array(items) => items.get(*index).cloned(),
            _ => None,
        },
    }
}

fn apply_filter(filter: &Filter, value: Value) -> Result<Option<Value>, WorkflowError> {
    let out = match filter {
        Filter::Length => match &value {
            Value::String(text) => Value::from(text.chars().count()),
            Value::Array(items) => Value::from(items.len()),
            Value::Object(map) => Value::from(map.len()),
            Value::Null => Value::from(0),
            other => return Err(type_error("length", "string, array, or object", other)),
        },
        Filter::Upper => Value::String(require_string("upper", &value)?.to_uppercase()),
        Filter::Lower => Value::String(require_string("lower", &value)?.to_lowercase()),
        Filter::Json => Value::String(
            serde_json::to_string(&value)
                .map_err(|err| WorkflowError::TemplateInvalid(format!("json filter: {err}")))?,
        ),
        Filter::Join(sep) => match &value {
            Value::Array(items) => {
                let joined: Vec<String> = items.iter().map(as_text).collect();
                Value::String(joined.join(sep))
            }
            other => return Err(type_error("join", "array", other)),
        },
        Filter::Default(_) => value.clone(),
        Filter::Date(fmt) => {
            let parsed = match &value {
                Value::String(text) => DateTime::parse_from_rfc3339(text)
                    .map(|dt| dt.with_timezone(&Utc))
                    .ok()
                    .or_else(|| {
                        text.parse::<i64>()
                            .ok()
                            .and_then(|epoch| DateTime::from_timestamp(epoch, 0))
                    }),
                Value::Number(number) => number
                    .as_i64()
                    .and_then(|epoch| DateTime::from_timestamp(epoch, 0)),
                other => return Err(type_error("date", "string or integer epoch", other)),
            }
            .ok_or_else(|| {
                let shown = match &value {
                    Value::String(text) => text.clone(),
                    other => other.to_string(),
                };
                WorkflowError::TemplateInvalid(format!(
                    "date filter: `{shown}` is not an RFC 3339 timestamp or an epoch"
                ))
            })?;
            Value::String(parsed.format(fmt).to_string())
        }
    };
    Ok(Some(out))
}

fn require_string<'a>(filter: &str, value: &'a Value) -> Result<&'a str, WorkflowError> {
    value
        .as_str()
        .ok_or_else(|| type_error(filter, "string", value))
}

fn type_error(filter: &str, expected: &str, actual: &Value) -> WorkflowError {
    WorkflowError::TemplateInvalid(format!(
        "filter `{filter}` expects {expected}, got {}",
        json_kind(actual)
    ))
}

fn json_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

/// Stringify a JSON value for text interpolation.
pub fn as_text(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(text) => text.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

/// JSON truthiness, used by conditions and filters.
pub fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => number.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Value::String(text) => !text.is_empty(),
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
    }
}

/// Render every string inside a JSON value, recursively. Object keys are left
/// untouched.
pub fn render_value(value: &Value, ctx: &RenderContext) -> Result<Value, WorkflowError> {
    match value {
        Value::String(text) => Template::parse(text)?.render(ctx),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(render_value(item, ctx)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(map) => {
            let mut out = serde_json::Map::with_capacity(map.len());
            for (key, item) in map {
                out.insert(key.clone(), render_value(item, ctx)?);
            }
            Ok(Value::Object(out))
        }
        other => Ok(other.clone()),
    }
}

/// Parse a path expression into a root and its trailing segments.
fn parse_path(source: &str) -> Result<(Root, Vec<PathSeg>), WorkflowError> {
    let (ident, rest) = take_ident(source)?;
    let mut segments = parse_segments(rest)?;

    let root = match ident.as_str() {
        "result" => Root::Result,
        "secrets" => {
            let name = match segments.first() {
                Some(PathSeg::Key(key)) => key.clone(),
                _ => {
                    return Err(WorkflowError::TemplateInvalid(
                        "`secrets` requires a secret name, for example `secrets.github_token`"
                            .to_string(),
                    ));
                }
            };
            segments.remove(0);
            Root::Secret(name)
        }
        "run" => {
            expect_single_key(&segments, "id", "run")?;
            segments.clear();
            Root::RunId
        }
        "workflow" => {
            expect_single_key(&segments, "id", "workflow")?;
            segments.clear();
            Root::WorkflowId
        }
        "now" => {
            expect_empty(&segments, "now")?;
            Root::Now
        }
        "today" => {
            expect_empty(&segments, "today")?;
            Root::Today
        }
        other => Root::Context(other.to_string()),
    };

    Ok((root, segments))
}

fn expect_single_key(segments: &[PathSeg], key: &str, root: &str) -> Result<(), WorkflowError> {
    match segments {
        [PathSeg::Key(found)] if found == key => Ok(()),
        _ => Err(WorkflowError::TemplateInvalid(format!(
            "`{root}` only supports `{root}.{key}`"
        ))),
    }
}

fn expect_empty(segments: &[PathSeg], root: &str) -> Result<(), WorkflowError> {
    if segments.is_empty() {
        Ok(())
    } else {
        Err(WorkflowError::TemplateInvalid(format!(
            "`{root}` takes no path"
        )))
    }
}

fn parse_segments(source: &str) -> Result<Vec<PathSeg>, WorkflowError> {
    let mut rest = source.trim();
    let mut segments = Vec::new();
    loop {
        if rest.is_empty() {
            return Ok(segments);
        }
        if let Some(stripped) = rest.strip_prefix('.') {
            let (key, tail) = take_ident(stripped)?;
            segments.push(PathSeg::Key(key));
            rest = tail.trim_start();
            continue;
        }
        if let Some(stripped) = rest.strip_prefix('[') {
            let end = stripped.find(']').ok_or_else(|| {
                WorkflowError::TemplateInvalid(format!("unterminated `[` in `{source}`"))
            })?;
            let index: usize = stripped[..end].trim().parse().map_err(|_| {
                WorkflowError::TemplateInvalid(format!(
                    "array index must be an integer in `{source}`"
                ))
            })?;
            segments.push(PathSeg::Index(index));
            rest = stripped[end + 1..].trim_start();
            continue;
        }
        return Err(WorkflowError::TemplateInvalid(format!(
            "unexpected `{rest}` in path `{source}`"
        )));
    }
}

fn take_ident(source: &str) -> Result<(String, &str), WorkflowError> {
    let mut chars = source.char_indices();
    let Some((_, first)) = chars.next() else {
        return Err(WorkflowError::TemplateInvalid(
            "expected an identifier".to_string(),
        ));
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return Err(WorkflowError::TemplateInvalid(format!(
            "expected an identifier, found `{first}`"
        )));
    }
    let mut end = source.len();
    for (index, ch) in chars {
        if !(ch.is_ascii_alphanumeric() || ch == '_') {
            end = index;
            break;
        }
    }
    Ok((source[..end].to_string(), &source[end..]))
}

/// Split an expression on `|`, respecting parentheses and quotes.
fn split_filters(source: &str) -> Result<Vec<String>, WorkflowError> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;

    for ch in source.chars() {
        match quote {
            Some(active) => {
                current.push(ch);
                if ch == active {
                    quote = None;
                }
            }
            None => match ch {
                '"' | '\'' => {
                    quote = Some(ch);
                    current.push(ch);
                }
                '(' => {
                    depth += 1;
                    current.push(ch);
                }
                ')' => {
                    depth = depth.saturating_sub(1);
                    current.push(ch);
                }
                '|' if depth == 0 => parts.push(std::mem::take(&mut current)),
                _ => current.push(ch),
            },
        }
    }

    if quote.is_some() {
        return Err(WorkflowError::TemplateInvalid(format!(
            "unterminated string in `{source}`"
        )));
    }
    parts.push(current);
    Ok(parts)
}

fn parse_filter(source: &str) -> Result<Filter, WorkflowError> {
    let (name, args) = match source.find('(') {
        Some(open) => {
            let close = source
                .rfind(')')
                .filter(|close| *close > open)
                .ok_or_else(|| {
                    WorkflowError::TemplateInvalid(format!("unterminated `(` in `{source}`"))
                })?;
            let args = source[open + 1..close].trim();
            (source[..open].trim(), Some(args))
        }
        None => (source.trim(), None),
    };

    match name {
        "length" if args.is_none() => Ok(Filter::Length),
        "upper" if args.is_none() => Ok(Filter::Upper),
        "lower" if args.is_none() => Ok(Filter::Lower),
        "json" if args.is_none() => Ok(Filter::Json),
        "join" => Ok(Filter::Join(parse_string_arg(name, args)?)),
        "default" => {
            let raw = args.ok_or_else(|| {
                WorkflowError::TemplateInvalid("`default` requires an argument".to_string())
            })?;
            Ok(Filter::Default(parse_literal(raw)?))
        }
        "date" => {
            let fmt = parse_string_arg(name, args)?;
            validate_date_format(&fmt)?;
            Ok(Filter::Date(fmt))
        }
        other => Err(WorkflowError::TemplateInvalid(format!(
            "unknown filter `{other}`"
        ))),
    }
}

fn parse_string_arg(name: &str, args: Option<&str>) -> Result<String, WorkflowError> {
    let raw = args
        .ok_or_else(|| WorkflowError::TemplateInvalid(format!("`{name}` requires an argument")))?;
    let raw = raw.trim();
    let unquoted = raw
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            raw.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
        .unwrap_or(raw);
    Ok(unquoted.to_string())
}

fn parse_literal(raw: &str) -> Result<Value, WorkflowError> {
    let raw = raw.trim();
    if let Some(text) = raw
        .strip_prefix('"')
        .and_then(|rest| rest.strip_suffix('"'))
        .or_else(|| {
            raw.strip_prefix('\'')
                .and_then(|rest| rest.strip_suffix('\''))
        })
    {
        return Ok(Value::String(text.to_string()));
    }
    match raw {
        "true" => return Ok(Value::Bool(true)),
        "false" => return Ok(Value::Bool(false)),
        "null" => return Ok(Value::Null),
        _ => {}
    }
    if let Ok(number) = raw.parse::<i64>() {
        return Ok(Value::from(number));
    }
    if let Ok(number) = raw.parse::<f64>() {
        return Ok(Value::from(number));
    }
    Err(WorkflowError::TemplateInvalid(format!(
        "`default({raw})` is not a literal"
    )))
}

fn validate_date_format(fmt: &str) -> Result<(), WorkflowError> {
    use chrono::format::{Item, StrftimeItems};
    if StrftimeItems::new(fmt).any(|item| matches!(item, Item::Error)) {
        return Err(WorkflowError::TemplateInvalid(format!(
            "invalid date format `{fmt}`"
        )));
    }
    Ok(())
}
