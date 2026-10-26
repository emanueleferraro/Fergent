//! A small, closed-by-default JSON Schema validator (`spec.md` D12).

use serde_json::Value;

/// Validate `value` against `schema`, returning a path-qualified error.
pub fn validate(schema: &Value, value: &Value) -> Result<(), String> {
    validate_at(schema, value, "$")
}

fn validate_at(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    if schema == &Value::Bool(true) || schema.is_null() {
        return Ok(());
    }
    if schema == &Value::Bool(false) {
        return Err(format!("{path}: no value is allowed"));
    }
    let Some(object) = schema.as_object() else {
        return Ok(());
    };

    if let Some(expected) = object.get("type") {
        let matches = match expected {
            Value::String(kind) => type_matches(kind, value),
            Value::Array(kinds) => kinds
                .iter()
                .filter_map(Value::as_str)
                .any(|kind| type_matches(kind, value)),
            _ => true,
        };
        if !matches {
            return Err(format!(
                "{path}: expected {expected}, got {}",
                type_name(value)
            ));
        }
    }

    if let Some(Value::Array(options)) = object.get("enum") {
        if !options.contains(value) {
            return Err(format!("{path}: value is not one of the allowed options"));
        }
    }

    if let Some(Value::Object(properties)) = object.get("properties") {
        let Some(value_object) = value.as_object() else {
            return Ok(());
        };
        if let Some(Value::Array(required)) = object.get("required") {
            for key in required.iter().filter_map(Value::as_str) {
                if !value_object.contains_key(key) {
                    return Err(format!("{path}: missing required property {key}"));
                }
            }
        }
        let closed = object
            .get("additionalProperties")
            .map(|value| value == &Value::Bool(false))
            .unwrap_or(true);
        for (key, item) in value_object {
            match properties.get(key) {
                Some(property) => validate_at(property, item, &format!("{path}.{key}"))?,
                None if closed => {
                    return Err(format!("{path}: unknown property {key}"));
                }
                None => {}
            }
        }
    }

    if let (Some(items), Some(array)) = (object.get("items"), value.as_array()) {
        for (index, item) in array.iter().enumerate() {
            validate_at(items, item, &format!("{path}[{index}]"))?;
        }
    }

    if let Some(minimum) = object.get("minimum").and_then(Value::as_f64) {
        if value.as_f64().is_some_and(|number| number < minimum) {
            return Err(format!("{path}: value is below the minimum"));
        }
    }
    if let Some(maximum) = object.get("maximum").and_then(Value::as_f64) {
        if value.as_f64().is_some_and(|number| number > maximum) {
            return Err(format!("{path}: value is above the maximum"));
        }
    }
    if let Some(min) = object.get("minLength").and_then(Value::as_u64) {
        if value.as_str().is_some_and(|text| (text.len() as u64) < min) {
            return Err(format!("{path}: string is too short"));
        }
    }
    if let Some(max) = object.get("maxLength").and_then(Value::as_u64) {
        if value.as_str().is_some_and(|text| (text.len() as u64) > max) {
            return Err(format!("{path}: string is too long"));
        }
    }
    Ok(())
}

fn type_matches(kind: &str, value: &Value) -> bool {
    match kind {
        "object" => value.is_object(),
        "array" => value.is_array(),
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "boolean" => value.is_boolean(),
        "null" => value.is_null(),
        _ => true,
    }
}

fn type_name(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn enforces_required_and_closed_objects() {
        let schema = json!({
            "type": "object",
            "properties": { "query": { "type": "string" } },
            "required": ["query"],
            "additionalProperties": false
        });
        assert!(validate(&schema, &json!({ "query": "x" })).is_ok());
        assert!(validate(&schema, &json!({})).is_err());
        assert!(validate(&schema, &json!({ "query": "x", "extra": 1 })).is_err());
    }

    #[test]
    fn checks_types_and_enums() {
        assert!(validate(&json!({ "type": "integer" }), &json!(1)).is_ok());
        assert!(validate(&json!({ "type": "integer" }), &json!(1.5)).is_err());
        assert!(validate(&json!({ "enum": ["a"] }), &json!("b")).is_err());
    }
}
