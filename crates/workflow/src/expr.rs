//! The closed condition and transform language (spec §1.6, Appendix D).
//!
//! Conditions are parsed into an AST and evaluated against a [`RenderContext`].
//! User text is never evaluated: there is no arithmetic, no function call, and
//! no way to reach the host. Transforms are declarative operation lists
//! (`pick`, `map`, `filter`, `merge`, `sort`) that walk JSON values.
//!
//! Both halves are pure, total, and deterministic: the only inputs are the
//! render context and the values being transformed, and evaluation never
//! mutates anything.

use std::cmp::Ordering;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::document::JsonMap;
use crate::error::WorkflowError;
use crate::template::{self, Expr as PathExpr, RenderContext, Template};

/// A parsed condition.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    root: Node,
}

/// A condition AST node.
#[derive(Debug, Clone, PartialEq)]
enum Node {
    /// A comparison between two operands.
    Compare {
        left: Operand,
        op: Comparison,
        right: Operand,
    },
    /// A bare operand, tested for JSON truthiness.
    Truthy(Operand),
    /// All children must hold.
    And(Vec<Node>),
    /// At least one child must hold.
    Or(Vec<Node>),
    /// The child must not hold.
    Not(Box<Node>),
}

/// A comparison operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Comparison {
    /// Equality.
    Eq,
    /// Inequality.
    Ne,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Gte,
    /// Less than.
    Lt,
    /// Less than or equal.
    Lte,
    /// Membership: the left value is in the right array, object, or string.
    In,
    /// Containment: the right value is in the left array, object, or string.
    Contains,
    /// Regular-expression match; the right operand is the pattern.
    Matches,
}

impl Comparison {
    /// The wire name used in the expression language.
    pub fn as_str(&self) -> &'static str {
        match self {
            Comparison::Eq => "eq",
            Comparison::Ne => "ne",
            Comparison::Gt => "gt",
            Comparison::Gte => "gte",
            Comparison::Lt => "lt",
            Comparison::Lte => "lte",
            Comparison::In => "in",
            Comparison::Contains => "contains",
            Comparison::Matches => "matches",
        }
    }

    fn from_name(name: &str) -> Option<Comparison> {
        match name {
            "eq" => Some(Comparison::Eq),
            "ne" => Some(Comparison::Ne),
            "gt" => Some(Comparison::Gt),
            "gte" => Some(Comparison::Gte),
            "lt" => Some(Comparison::Lt),
            "lte" => Some(Comparison::Lte),
            "in" => Some(Comparison::In),
            "contains" => Some(Comparison::Contains),
            "matches" => Some(Comparison::Matches),
            _ => None,
        }
    }
}

/// An operand: a context path or a literal.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    /// A path resolved through the template expression rules.
    Path(PathExpr),
    /// A literal value.
    Literal(Value),
}

impl Operand {
    fn eval(&self, ctx: &RenderContext) -> Result<Option<Value>, WorkflowError> {
        match self {
            Operand::Path(path) => path.eval(ctx),
            Operand::Literal(value) => Ok(Some(value.clone())),
        }
    }

    /// The value, with a missing path read as `null` so conditions stay total.
    fn value(&self, ctx: &RenderContext) -> Result<Value, WorkflowError> {
        Ok(self.eval(ctx)?.unwrap_or(Value::Null))
    }
}

impl Condition {
    /// Parse a condition expression.
    ///
    /// The grammar is:
    ///
    /// ```text
    /// expr       := or_expr
    /// or_expr    := and_expr ("or" and_expr)*
    /// and_expr   := not_expr ("and" not_expr)*
    /// not_expr   := "not" not_expr | primary
    /// primary    := "(" expr ")" | comparison
    /// comparison := operand (op operand)?
    /// op         := "eq" | "ne" | "gt" | "gte" | "lt" | "lte" | "in"
    ///             | "contains" | "matches" | "==" | "!=" | ">" | ">=" | "<" | "<="
    /// operand    := path | literal | "[" literal ("," literal)* "]"
    /// ```
    ///
    /// A bare operand is tested for truthiness.
    pub fn parse(source: &str) -> Result<Condition, WorkflowError> {
        let tokens = lex(source)?;
        if tokens.is_empty() {
            return Err(WorkflowError::ExpressionInvalid(
                "empty condition expression".to_string(),
            ));
        }
        let mut parser = Parser { tokens, pos: 0 };
        let root = parser.parse_or()?;
        if let Some(token) = parser.peek() {
            return Err(WorkflowError::ExpressionInvalid(format!(
                "unexpected `{}` in `{source}`",
                token.describe()
            )));
        }
        Ok(Condition { root })
    }

    /// Evaluate the condition.
    pub fn eval(&self, ctx: &RenderContext) -> Result<bool, WorkflowError> {
        eval_node(&self.root, ctx)
    }

    /// Every context path the condition reads, in `a.b[0]` form.
    pub fn context_paths(&self) -> Vec<String> {
        let mut out = Vec::new();
        collect_paths(&self.root, &mut out);
        out.retain(|path| !is_builtin_path(path));
        out
    }

    /// Whether the condition reads a secret.
    pub fn reads_secret(&self) -> bool {
        let mut paths = Vec::new();
        collect_paths(&self.root, &mut paths);
        paths.iter().any(|path| path.starts_with("secrets."))
    }
}

fn is_builtin_path(path: &str) -> bool {
    path == "result"
        || path.starts_with("result.")
        || path.starts_with("secrets.")
        || path == "run.id"
        || path == "workflow.id"
        || path == "now"
        || path == "today"
}

fn collect_paths(node: &Node, out: &mut Vec<String>) {
    match node {
        Node::Compare { left, op: _, right } => {
            collect_operand(left, out);
            collect_operand(right, out);
        }
        Node::Truthy(operand) => collect_operand(operand, out),
        Node::And(items) | Node::Or(items) => {
            for item in items {
                collect_paths(item, out);
            }
        }
        Node::Not(inner) => collect_paths(inner, out),
    }
}

fn collect_operand(operand: &Operand, out: &mut Vec<String>) {
    if let Operand::Path(path) = operand {
        out.push(path.display_path());
    }
}

fn eval_node(node: &Node, ctx: &RenderContext) -> Result<bool, WorkflowError> {
    match node {
        Node::Compare { left, op, right } => {
            let left = left.value(ctx)?;
            let right = right.value(ctx)?;
            compare(&left, *op, &right)
        }
        Node::Truthy(operand) => Ok(operand
            .eval(ctx)?
            .map(|value| template::truthy(&value))
            .unwrap_or(false)),
        Node::And(items) => {
            for item in items {
                if !eval_node(item, ctx)? {
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Node::Or(items) => {
            for item in items {
                if eval_node(item, ctx)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        Node::Not(inner) => Ok(!eval_node(inner, ctx)?),
    }
}

fn compare(left: &Value, op: Comparison, right: &Value) -> Result<bool, WorkflowError> {
    let out = match op {
        Comparison::Eq => values_equal(left, right),
        Comparison::Ne => !values_equal(left, right),
        Comparison::Gt => order(left, right) == Some(Ordering::Greater),
        Comparison::Gte => matches!(
            order(left, right),
            Some(Ordering::Greater | Ordering::Equal)
        ),
        Comparison::Lt => order(left, right) == Some(Ordering::Less),
        Comparison::Lte => matches!(order(left, right), Some(Ordering::Less | Ordering::Equal)),
        Comparison::In => membership(right, left),
        Comparison::Contains => containment(left, right),
        Comparison::Matches => {
            let pattern = match right {
                Value::String(text) => text,
                other => {
                    return Err(WorkflowError::ExpressionInvalid(format!(
                        "`matches` expects a pattern string, got {other}"
                    )))
                }
            };
            let regex = Regex::new(pattern).map_err(|err| {
                WorkflowError::ExpressionInvalid(format!("invalid pattern `{pattern}`: {err}"))
            })?;
            regex.is_match(&template::as_text(left))
        }
    };
    Ok(out)
}

fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => match (a.as_f64(), b.as_f64()) {
            (Some(a), Some(b)) => a == b,
            _ => a == b,
        },
        _ => left == right,
    }
}

fn order(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => a.as_f64()?.partial_cmp(&b.as_f64()?),
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        _ => None,
    }
}

/// Whether `needle` is a member of `haystack`.
fn membership(haystack: &Value, needle: &Value) -> bool {
    match haystack {
        Value::Array(items) => items.iter().any(|item| values_equal(item, needle)),
        Value::Object(map) => map.keys().any(|key| key == &template::as_text(needle)),
        Value::String(text) => text.contains(&template::as_text(needle)),
        _ => false,
    }
}

/// Whether `haystack` contains `needle` (the operands swapped relative to
/// [`membership`]).
fn containment(haystack: &Value, needle: &Value) -> bool {
    match haystack {
        Value::Array(items) => items.iter().any(|item| values_equal(item, needle)),
        Value::Object(map) => map.keys().any(|key| key == &template::as_text(needle)),
        Value::String(text) => text.contains(&template::as_text(needle)),
        _ => false,
    }
}

/// Bind names into the render context, for `map`/`filter` iteration scopes.
pub fn with_bindings(ctx: &RenderContext, bindings: &[(&str, Value)]) -> RenderContext {
    let mut data = match &ctx.data {
        Value::Object(map) => map.clone(),
        _ => serde_json::Map::new(),
    };
    for (name, value) in bindings {
        data.insert((*name).to_string(), value.clone());
    }
    RenderContext {
        data: Value::Object(data),
        result: ctx.result.clone(),
        run_id: ctx.run_id.clone(),
        workflow_id: ctx.workflow_id.clone(),
        now: ctx.now,
        secrets: std::sync::Arc::clone(&ctx.secrets),
    }
}

// ---------------------------------------------------------------------------
// Lexer
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Open,
    Close,
    BracketOpen,
    BracketClose,
    Comma,
    Op(Comparison),
    And,
    Or,
    Not,
    Path(String),
    Literal(Value),
    Template(String),
}

impl Token {
    fn describe(&self) -> String {
        match self {
            Token::Open => "(".to_string(),
            Token::Close => ")".to_string(),
            Token::BracketOpen => "[".to_string(),
            Token::BracketClose => "]".to_string(),
            Token::Comma => ",".to_string(),
            Token::Op(op) => op.as_str().to_string(),
            Token::And => "and".to_string(),
            Token::Or => "or".to_string(),
            Token::Not => "not".to_string(),
            Token::Path(path) => path.clone(),
            Token::Literal(value) => value.to_string(),
            Token::Template(source) => format!("{{{{ {source} }}}}"),
        }
    }
}

fn lex(source: &str) -> Result<Vec<Token>, WorkflowError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0usize;

    while index < chars.len() {
        let ch = chars[index];
        if ch.is_whitespace() {
            index += 1;
            continue;
        }
        match ch {
            '(' => {
                tokens.push(Token::Open);
                index += 1;
            }
            ')' => {
                tokens.push(Token::Close);
                index += 1;
            }
            '[' => {
                tokens.push(Token::BracketOpen);
                index += 1;
            }
            ']' => {
                tokens.push(Token::BracketClose);
                index += 1;
            }
            ',' => {
                tokens.push(Token::Comma);
                index += 1;
            }
            '"' | '\'' => {
                let (text, next) = lex_string(&chars, index)?;
                tokens.push(Token::Literal(Value::String(text)));
                index = next;
            }
            '=' | '!' | '>' | '<' => {
                let (op, next) = lex_symbolic_op(&chars, index)?;
                tokens.push(Token::Op(op));
                index = next;
            }
            '{' => {
                let (inner, next) = lex_braced(&chars, index)?;
                tokens.push(Token::Template(inner));
                index = next;
            }
            ch if ch.is_ascii_digit() || ch == '-' || ch == '+' => {
                let (value, next) = lex_number(&chars, index)?;
                tokens.push(Token::Literal(value));
                index = next;
            }
            ch if ch.is_ascii_alphabetic() || ch == '_' => {
                let (raw, next) = lex_word(&chars, index)?;
                let token = match raw.as_str() {
                    "and" => Token::And,
                    "or" => Token::Or,
                    "not" => Token::Not,
                    "true" => Token::Literal(Value::Bool(true)),
                    "false" => Token::Literal(Value::Bool(false)),
                    "null" => Token::Literal(Value::Null),
                    name => match Comparison::from_name(name) {
                        Some(op) => Token::Op(op),
                        None => Token::Path(raw),
                    },
                };
                tokens.push(token);
                index = next;
            }
            other => {
                return Err(WorkflowError::ExpressionInvalid(format!(
                    "unexpected character `{other}` in `{source}`"
                )))
            }
        }
    }

    Ok(tokens)
}

fn lex_symbolic_op(chars: &[char], start: usize) -> Result<(Comparison, usize), WorkflowError> {
    let next = chars.get(start + 1).copied();
    let (op, len) = match (chars[start], next) {
        ('=', Some('=')) => (Comparison::Eq, 2),
        ('!', Some('=')) => (Comparison::Ne, 2),
        ('>', Some('=')) => (Comparison::Gte, 2),
        ('<', Some('=')) => (Comparison::Lte, 2),
        ('>', _) => (Comparison::Gt, 1),
        ('<', _) => (Comparison::Lt, 1),
        ('=', _) => (Comparison::Eq, 1),
        (other, _) => {
            return Err(WorkflowError::ExpressionInvalid(format!(
                "unexpected operator `{other}`"
            )))
        }
    };
    Ok((op, start + len))
}

fn lex_string(chars: &[char], start: usize) -> Result<(String, usize), WorkflowError> {
    let quote = chars[start];
    let mut out = String::new();
    let mut index = start + 1;
    while index < chars.len() {
        let ch = chars[index];
        if ch == '\\' {
            if let Some(escaped) = chars.get(index + 1) {
                out.push(*escaped);
                index += 2;
                continue;
            }
            break;
        }
        if ch == quote {
            return Ok((out, index + 1));
        }
        out.push(ch);
        index += 1;
    }
    Err(WorkflowError::ExpressionInvalid(format!(
        "unterminated string literal starting at {start}"
    )))
}

fn lex_braced(chars: &[char], start: usize) -> Result<(String, usize), WorkflowError> {
    let mut index = start + 1;
    let mut out = String::new();
    if chars.get(index) != Some(&'{') {
        return Err(WorkflowError::ExpressionInvalid(
            "expected `{{` to open a template operand".to_string(),
        ));
    }
    index += 1;
    while index + 1 < chars.len() {
        if chars[index] == '}' && chars[index + 1] == '}' {
            return Ok((out.trim().to_string(), index + 2));
        }
        out.push(chars[index]);
        index += 1;
    }
    Err(WorkflowError::ExpressionInvalid(
        "unterminated `{{` in condition".to_string(),
    ))
}

fn lex_number(chars: &[char], start: usize) -> Result<(Value, usize), WorkflowError> {
    let mut index = start;
    if matches!(chars.get(index), Some('-') | Some('+')) {
        index += 1;
    }
    while matches!(chars.get(index), Some(ch) if ch.is_ascii_digit()) {
        index += 1;
    }
    if chars.get(index) == Some(&'.') {
        index += 1;
        while matches!(chars.get(index), Some(ch) if ch.is_ascii_digit()) {
            index += 1;
        }
    }
    let raw: String = chars[start..index].iter().collect();
    if let Ok(value) = raw.parse::<i64>() {
        return Ok((Value::from(value), index));
    }
    let value: f64 = raw
        .parse()
        .map_err(|_| WorkflowError::ExpressionInvalid(format!("`{raw}` is not a number")))?;
    Ok((Value::from(value), index))
}

/// Lex an identifier path: `ident` followed by `.ident` and `[index]` parts.
fn lex_word(chars: &[char], start: usize) -> Result<(String, usize), WorkflowError> {
    let mut index = start;
    while matches!(chars.get(index), Some(ch) if ch.is_ascii_alphanumeric() || *ch == '_') {
        index += 1;
    }

    loop {
        match chars.get(index) {
            Some('.') if matches!(chars.get(index + 1), Some(ch) if ch.is_ascii_alphabetic() || *ch == '_') =>
            {
                index += 1;
                while matches!(chars.get(index), Some(ch) if ch.is_ascii_alphanumeric() || *ch == '_')
                {
                    index += 1;
                }
            }
            Some('[') => {
                let mut cursor = index + 1;
                while matches!(chars.get(cursor), Some(ch) if ch.is_ascii_digit()) {
                    cursor += 1;
                }
                if chars.get(cursor) == Some(&']') && cursor > index + 1 {
                    index = cursor + 1;
                } else {
                    break;
                }
            }
            _ => break,
        }
    }

    Ok((chars[start..index].iter().collect(), index))
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn take_keyword(&mut self, expected: fn(&Token) -> bool) -> bool {
        match self.peek() {
            Some(token) if expected(token) => {
                self.pos += 1;
                true
            }
            _ => false,
        }
    }

    fn parse_or(&mut self) -> Result<Node, WorkflowError> {
        let mut items = vec![self.parse_and()?];
        while self.take_keyword(|token| matches!(token, Token::Or)) {
            items.push(self.parse_and()?);
        }
        Ok(if items.len() == 1 {
            items
                .pop()
                .unwrap_or(Node::Truthy(Operand::Literal(Value::Bool(false))))
        } else {
            Node::Or(items)
        })
    }

    fn parse_and(&mut self) -> Result<Node, WorkflowError> {
        let mut items = vec![self.parse_not()?];
        while self.take_keyword(|token| matches!(token, Token::And)) {
            items.push(self.parse_not()?);
        }
        Ok(if items.len() == 1 {
            items
                .pop()
                .unwrap_or(Node::Truthy(Operand::Literal(Value::Bool(false))))
        } else {
            Node::And(items)
        })
    }

    fn parse_not(&mut self) -> Result<Node, WorkflowError> {
        if self.take_keyword(|token| matches!(token, Token::Not)) {
            return Ok(Node::Not(Box::new(self.parse_not()?)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Node, WorkflowError> {
        if self.take_keyword(|token| matches!(token, Token::Open)) {
            let inner = self.parse_or()?;
            if !self.take_keyword(|token| matches!(token, Token::Close)) {
                return Err(WorkflowError::ExpressionInvalid(
                    "expected `)` to close the group".to_string(),
                ));
            }
            return Ok(inner);
        }

        let left = self.parse_operand()?;
        match self.peek().cloned() {
            Some(Token::Op(op)) => {
                self.pos += 1;
                let right = self.parse_operand()?;
                Ok(Node::Compare { left, op, right })
            }
            _ => Ok(Node::Truthy(left)),
        }
    }

    fn parse_operand(&mut self) -> Result<Operand, WorkflowError> {
        if self.take_keyword(|token| matches!(token, Token::BracketOpen)) {
            let mut items = Vec::new();
            loop {
                if self.take_keyword(|token| matches!(token, Token::BracketClose)) {
                    break;
                }
                match self.advance() {
                    Some(Token::Literal(value)) => items.push(value),
                    other => {
                        return Err(WorkflowError::ExpressionInvalid(format!(
                            "array literals hold literals only, found `{}`",
                            other.map(|token| token.describe()).unwrap_or_default()
                        )))
                    }
                }
                if self.take_keyword(|token| matches!(token, Token::Comma)) {
                    continue;
                }
                if self.take_keyword(|token| matches!(token, Token::BracketClose)) {
                    break;
                }
                return Err(WorkflowError::ExpressionInvalid(
                    "expected `,` or `]` in array literal".to_string(),
                ));
            }
            return Ok(Operand::Literal(Value::Array(items)));
        }

        match self.advance() {
            Some(Token::Literal(value)) => Ok(Operand::Literal(value)),
            Some(Token::Path(path)) => Ok(Operand::Path(parse_path_operand(&path)?)),
            Some(Token::Template(path)) => Ok(Operand::Path(parse_path_operand(&path)?)),
            other => Err(WorkflowError::ExpressionInvalid(format!(
                "expected an operand, found `{}`",
                other.map(|token| token.describe()).unwrap_or_default()
            ))),
        }
    }
}

fn parse_path_operand(raw: &str) -> Result<PathExpr, WorkflowError> {
    PathExpr::parse(raw).map_err(|err| WorkflowError::ExpressionInvalid(err.to_string()))
}

// ---------------------------------------------------------------------------
// Transform operations
// ---------------------------------------------------------------------------

/// A declarative transform operation (spec §1.6).
///
/// Operations apply in order: each one receives the previous result and returns
/// a new value. `from` is a template for the value to operate on; when absent
/// the running value is used. `map` and `filter` bind `item` and `index` for
/// their expressions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum TransformOp {
    /// Keep only the listed keys of an object.
    Pick {
        /// A template selecting the object; defaults to the running value.
        #[serde(default)]
        from: Option<String>,
        /// Keys to keep, in order.
        keys: Vec<String>,
    },
    /// Project each item of an array through a template object.
    Map {
        /// A template selecting the array; defaults to the running value.
        #[serde(default)]
        from: Option<String>,
        /// The output object; values are templates with `item` and `index`.
        to: JsonMap,
    },
    /// Keep the array items for which a condition holds.
    Filter {
        /// A template selecting the array; defaults to the running value.
        #[serde(default)]
        from: Option<String>,
        /// A condition evaluated with `item` and `index` bound.
        when: String,
    },
    /// Merge an array of objects into one object; later values win.
    Merge {
        /// A template selecting the array; defaults to the running value.
        #[serde(default)]
        from: Option<String>,
        /// Merge nested objects recursively instead of replacing them.
        #[serde(default)]
        deep: bool,
    },
    /// Sort an array of objects by a key path.
    Sort {
        /// A template selecting the array; defaults to the running value.
        #[serde(default)]
        from: Option<String>,
        /// A dotted key path relative to each item, for example `meta.rank`.
        by: String,
        /// Sort descending.
        #[serde(default)]
        desc: bool,
    },
}

impl TransformOp {
    /// Check the operation's templates and conditions at save time.
    pub fn validate(&self) -> Result<(), WorkflowError> {
        let source = match self {
            TransformOp::Pick { from, keys } => {
                if keys.is_empty() {
                    return Err(WorkflowError::ExpressionInvalid(
                        "`pick` requires at least one key".to_string(),
                    ));
                }
                from
            }
            TransformOp::Map { from, to } => {
                for value in to.values() {
                    validate_templates(value)?;
                }
                from
            }
            TransformOp::Filter { from, when } => {
                Condition::parse(when)?;
                from
            }
            TransformOp::Merge { from, deep: _ } => from,
            TransformOp::Sort { from, by, desc: _ } => {
                if by.is_empty() || !by.split('.').all(is_identifier) {
                    return Err(WorkflowError::ExpressionInvalid(format!(
                        "`sort` key `{by}` must be a dotted identifier path"
                    )));
                }
                from
            }
        };
        if let Some(source) = source {
            Template::parse(source)?;
        }
        Ok(())
    }

    /// Apply the operation to a value.
    pub fn apply(&self, input: Value, ctx: &RenderContext) -> Result<Value, WorkflowError> {
        let value = match self.source() {
            Some(source) => Template::parse(source)?.render(ctx)?,
            None => input,
        };

        match self {
            TransformOp::Pick { keys, .. } => {
                let object = expect_object("pick", &value)?;
                let mut out = serde_json::Map::with_capacity(keys.len());
                for key in keys {
                    if let Some(found) = object.get(key) {
                        out.insert(key.clone(), found.clone());
                    }
                }
                Ok(Value::Object(out))
            }
            TransformOp::Map { to, .. } => {
                let items = expect_array("map", &value)?;
                let mut out = Vec::with_capacity(items.len());
                for (index, item) in items.iter().enumerate() {
                    let bound = with_bindings(
                        ctx,
                        &[("item", item.clone()), ("index", Value::from(index as u64))],
                    );
                    let target =
                        Value::Object(to.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
                    out.push(template::render_value(&target, &bound)?);
                }
                Ok(Value::Array(out))
            }
            TransformOp::Filter { when, .. } => {
                let items = expect_array("filter", &value)?;
                let condition = Condition::parse(when)?;
                let mut out = Vec::new();
                for (index, item) in items.iter().enumerate() {
                    let bound = with_bindings(
                        ctx,
                        &[("item", item.clone()), ("index", Value::from(index as u64))],
                    );
                    if condition.eval(&bound)? {
                        out.push(item.clone());
                    }
                }
                Ok(Value::Array(out))
            }
            TransformOp::Merge { deep, .. } => {
                let items = expect_array("merge", &value)?;
                let mut out = serde_json::Map::new();
                for item in items {
                    let object = expect_object("merge", &item)?;
                    for (key, value) in object {
                        if *deep {
                            if let (Some(Value::Object(existing)), Value::Object(next)) =
                                (out.get(key), value)
                            {
                                let mut merged = existing.clone();
                                deep_merge(&mut merged, next);
                                out.insert(key.clone(), Value::Object(merged));
                                continue;
                            }
                        }
                        out.insert(key.clone(), value.clone());
                    }
                }
                Ok(Value::Object(out))
            }
            TransformOp::Sort { by, desc, .. } => {
                let mut items = expect_array("sort", &value)?;
                items.sort_by(|left, right| {
                    let left = lookup(left, by);
                    let right = lookup(right, by);
                    let ordering = match (&left, &right) {
                        (Some(a), Some(b)) => compare_values(a, b).unwrap_or(Ordering::Equal),
                        (Some(_), None) => Ordering::Less,
                        (None, Some(_)) => Ordering::Greater,
                        (None, None) => Ordering::Equal,
                    };
                    if *desc {
                        ordering.reverse()
                    } else {
                        ordering
                    }
                });
                Ok(Value::Array(items))
            }
        }
    }

    fn source(&self) -> Option<&str> {
        match self {
            TransformOp::Pick { from, .. }
            | TransformOp::Map { from, .. }
            | TransformOp::Filter { from, .. }
            | TransformOp::Merge { from, .. }
            | TransformOp::Sort { from, .. } => from.as_deref(),
        }
    }
}

/// Apply a transform pipeline.
pub fn apply_ops(
    ops: &[TransformOp],
    input: Value,
    ctx: &RenderContext,
) -> Result<Value, WorkflowError> {
    let mut current = input;
    for op in ops {
        op.validate()?;
        current = op.apply(current, ctx)?;
    }
    Ok(current)
}

/// Parse every string inside a JSON value as a template, without rendering.
pub fn validate_templates(value: &Value) -> Result<(), WorkflowError> {
    match value {
        Value::String(text) => {
            Template::parse(text)?;
            Ok(())
        }
        Value::Array(items) => items.iter().try_for_each(validate_templates),
        Value::Object(map) => map.values().try_for_each(validate_templates),
        _ => Ok(()),
    }
}

fn expect_object<'a>(
    op: &str,
    value: &'a Value,
) -> Result<&'a serde_json::Map<String, Value>, WorkflowError> {
    value.as_object().ok_or_else(|| {
        WorkflowError::ExpressionInvalid(format!("`{op}` expects an object, got {value}"))
    })
}

fn expect_array(op: &str, value: &Value) -> Result<Vec<Value>, WorkflowError> {
    value.as_array().cloned().ok_or_else(|| {
        WorkflowError::ExpressionInvalid(format!("`{op}` expects an array, got {value}"))
    })
}

fn deep_merge(
    target: &mut serde_json::Map<String, Value>,
    source: &serde_json::Map<String, Value>,
) {
    for (key, value) in source {
        match (target.get_mut(key), value) {
            (Some(Value::Object(existing)), Value::Object(next)) => deep_merge(existing, next),
            _ => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

fn compare_values(left: &Value, right: &Value) -> Option<Ordering> {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => a.as_f64()?.partial_cmp(&b.as_f64()?),
        (Value::String(a), Value::String(b)) => Some(a.cmp(b)),
        (Value::Bool(a), Value::Bool(b)) => Some(a.cmp(b)),
        _ => Some(template::as_text(left).cmp(&template::as_text(right))),
    }
}

/// Look a dotted key path up inside a value.
fn lookup(value: &Value, path: &str) -> Option<Value> {
    let mut current = value.clone();
    for segment in path.split('.') {
        current = match current {
            Value::Object(map) => map.get(segment)?.clone(),
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?.clone(),
            _ => return None,
        };
    }
    Some(current)
}

fn is_identifier(text: &str) -> bool {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() || first == '_' => {}
        _ => return false,
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}
