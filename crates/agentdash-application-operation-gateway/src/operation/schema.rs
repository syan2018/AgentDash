use serde_json::Value;

/// Projects an externally supplied schema into the exact subset enforced by OperationGateway.
///
/// External providers such as MCP remain the final validator for constraints that are not part of
/// the Gateway subset. Keeping the projection here prevents one unsupported annotation or
/// validation keyword from invalidating the actor's entire dynamic Operation surface.
pub fn project_json_schema_to_supported_subset(schema: Value) -> Value {
    match schema {
        Value::Bool(_) => schema,
        Value::Object(mut object) => {
            if !object.contains_key("anyOf")
                && let Some(one_of) = object.remove("oneOf")
            {
                object.insert("anyOf".to_owned(), one_of);
            }
            let mut projected = serde_json::Map::new();
            for key in [
                "type",
                "required",
                "enum",
                "const",
                "minimum",
                "maximum",
                "description",
            ] {
                if let Some(value) = object.remove(key) {
                    projected.insert(key.to_owned(), value);
                }
            }
            if let Some(properties) = object
                .remove("properties")
                .and_then(|value| value.as_object().cloned())
            {
                projected.insert(
                    "properties".to_owned(),
                    Value::Object(
                        properties
                            .into_iter()
                            .map(|(key, schema)| {
                                (key, project_json_schema_to_supported_subset(schema))
                            })
                            .collect(),
                    ),
                );
            }
            if let Some(items) = object.remove("items") {
                projected.insert(
                    "items".to_owned(),
                    project_json_schema_to_supported_subset(items),
                );
            }
            if let Some(additional) = object.remove("additionalProperties") {
                projected.insert(
                    "additionalProperties".to_owned(),
                    project_json_schema_to_supported_subset(additional),
                );
            }
            if let Some(branches) = object
                .remove("anyOf")
                .and_then(|value| value.as_array().cloned())
            {
                projected.insert(
                    "anyOf".to_owned(),
                    Value::Array(
                        branches
                            .into_iter()
                            .map(project_json_schema_to_supported_subset)
                            .collect(),
                    ),
                );
            }
            let projected = Value::Object(projected);
            if validate_json_schema_definition(&projected).is_ok() {
                projected
            } else {
                Value::Bool(true)
            }
        }
        _ => Value::Bool(true),
    }
}

pub fn validate_json_schema_definition(schema: &Value) -> Result<(), String> {
    validate_schema_definition_at(schema, "$schema")
}

pub fn validate_json_schema_subset(schema: &Value, value: &Value) -> Result<(), String> {
    validate_json_schema_definition(schema)?;
    validate_schema_value(schema, value, "$")
}

fn validate_schema_definition_at(schema: &Value, path: &str) -> Result<(), String> {
    match schema {
        Value::Bool(_) => return Ok(()),
        Value::Object(object) => {
            const VALIDATION_KEYS: [&str; 10] = [
                "type",
                "required",
                "properties",
                "additionalProperties",
                "items",
                "enum",
                "const",
                "minimum",
                "maximum",
                "anyOf",
            ];
            const ANNOTATION_KEYS: [&str; 1] = ["description"];
            if let Some(key) = object.keys().find(|key| {
                !VALIDATION_KEYS.contains(&key.as_str()) && !ANNOTATION_KEYS.contains(&key.as_str())
            }) {
                return Err(format!(
                    "{path}.{key} 不属于 Gateway 支持的 JSON Schema 子集"
                ));
            }
        }
        _ => return Err(format!("{path} 必须是对象或布尔值")),
    }

    if schema
        .get("description")
        .is_some_and(|value| !value.is_string())
    {
        return Err(format!("{path}.description 必须是字符串"));
    }

    for keyword in ["minimum", "maximum"] {
        if schema.get(keyword).is_some_and(|value| !value.is_number()) {
            return Err(format!("{path}.{keyword} 必须是数字"));
        }
    }

    if let Some(any_of) = schema.get("anyOf") {
        let branches = any_of
            .as_array()
            .filter(|branches| !branches.is_empty())
            .ok_or_else(|| format!("{path}.anyOf 必须是非空 schema 数组"))?;
        for (index, branch) in branches.iter().enumerate() {
            validate_schema_definition_at(branch, &format!("{path}.anyOf[{index}]"))?;
        }
    }

    if let Some(type_schema) = schema.get("type") {
        let validate_type_name = |value: &Value| -> Result<(), String> {
            let name = value
                .as_str()
                .ok_or_else(|| format!("{path}.type 必须是字符串或字符串数组"))?;
            if matches!(
                name,
                "array" | "boolean" | "integer" | "null" | "number" | "object" | "string"
            ) {
                Ok(())
            } else {
                Err(format!("{path}.type 包含未知 JSON 类型: {name}"))
            }
        };
        match type_schema {
            Value::String(_) => validate_type_name(type_schema)?,
            Value::Array(items) if !items.is_empty() => {
                for item in items {
                    validate_type_name(item)?;
                }
            }
            _ => return Err(format!("{path}.type 必须是字符串或非空字符串数组")),
        }
    }

    if let Some(required) = schema.get("required") {
        let items = required
            .as_array()
            .ok_or_else(|| format!("{path}.required 必须是字符串数组"))?;
        if items.iter().any(|item| item.as_str().is_none()) {
            return Err(format!("{path}.required 必须是字符串数组"));
        }
    }

    if let Some(properties) = schema.get("properties") {
        let properties = properties
            .as_object()
            .ok_or_else(|| format!("{path}.properties 必须是对象"))?;
        for (key, property_schema) in properties {
            validate_schema_definition_at(property_schema, &format!("{path}.properties.{key}"))?;
        }
    }

    if let Some(additional) = schema.get("additionalProperties")
        && !additional.is_boolean()
    {
        return Err(format!("{path}.additionalProperties 暂只支持布尔值"));
    }

    if let Some(items) = schema.get("items") {
        if items.is_array() {
            return Err(format!("{path}.items 暂不支持 tuple schema"));
        }
        validate_schema_definition_at(items, &format!("{path}.items"))?;
    }

    if let Some(values) = schema.get("enum")
        && !values.is_array()
    {
        return Err(format!("{path}.enum 必须是数组"));
    }

    Ok(())
}

fn validate_schema_value(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    match schema {
        Value::Bool(true) => return Ok(()),
        Value::Bool(false) => return Err(format!("{path} 被 false schema 拒绝")),
        Value::Object(_) => {}
        _ => return Err("schema 必须是对象或布尔值".to_string()),
    }

    validate_const(schema, value, path)?;
    validate_enum(schema, value, path)?;
    validate_type(schema, value, path)?;
    validate_number_bounds(schema, value, path)?;
    validate_any_of(schema, value, path)?;

    if let Some(object) = value.as_object() {
        validate_required(schema, object, path)?;
        validate_properties(schema, object, path)?;
        validate_additional_properties(schema, object)?;
    }

    if let Some(array) = value.as_array() {
        validate_items(schema, array, path)?;
    }

    Ok(())
}

fn validate_number_bounds(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    let Some(value) = value.as_f64() else {
        return Ok(());
    };
    if let Some(minimum) = schema.get("minimum").and_then(Value::as_f64)
        && value < minimum
    {
        return Err(format!("{path} 必须大于或等于 minimum {minimum}"));
    }
    if let Some(maximum) = schema.get("maximum").and_then(Value::as_f64)
        && value > maximum
    {
        return Err(format!("{path} 必须小于或等于 maximum {maximum}"));
    }
    Ok(())
}

fn validate_any_of(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    let Some(branches) = schema.get("anyOf").and_then(Value::as_array) else {
        return Ok(());
    };
    if branches
        .iter()
        .any(|branch| validate_schema_value(branch, value, path).is_ok())
    {
        Ok(())
    } else {
        Err(format!("{path} 不匹配 anyOf 中的任何 schema"))
    }
}

fn validate_const(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    let Some(expected) = schema.get("const") else {
        return Ok(());
    };
    if value == expected {
        Ok(())
    } else {
        Err(format!("{path} 必须等于 const"))
    }
}

fn validate_enum(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    let Some(items) = schema.get("enum") else {
        return Ok(());
    };
    let Some(items) = items.as_array() else {
        return Err("schema.enum 必须是数组".to_string());
    };
    if items.iter().any(|item| item == value) {
        Ok(())
    } else {
        Err(format!("{path} 不在 enum 允许值内"))
    }
}

fn validate_type(schema: &Value, value: &Value, path: &str) -> Result<(), String> {
    let Some(type_schema) = schema.get("type") else {
        return Ok(());
    };
    let allowed = match type_schema {
        Value::String(item) => vec![item.as_str()],
        Value::Array(items) => items
            .iter()
            .map(|item| {
                item.as_str()
                    .ok_or_else(|| "schema.type 数组元素必须是字符串".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err("schema.type 必须是字符串或字符串数组".to_string()),
    };
    if allowed
        .iter()
        .any(|expected| json_value_matches_type(value, expected))
    {
        Ok(())
    } else {
        Err(format!("{path} 类型不匹配，期望 {}", allowed.join(" 或 ")))
    }
}

fn validate_required(
    schema: &Value,
    object: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), String> {
    let Some(required) = schema.get("required") else {
        return Ok(());
    };
    let Some(required) = required.as_array() else {
        return Err("schema.required 必须是字符串数组".to_string());
    };
    for item in required {
        let Some(key) = item.as_str() else {
            return Err("schema.required 必须是字符串数组".to_string());
        };
        if !object.contains_key(key) {
            return Err(format!("{path}.{key} 是必填字段"));
        }
    }
    Ok(())
}

fn validate_properties(
    schema: &Value,
    object: &serde_json::Map<String, Value>,
    path: &str,
) -> Result<(), String> {
    let Some(properties) = schema.get("properties") else {
        return Ok(());
    };
    let Some(properties) = properties.as_object() else {
        return Err("schema.properties 必须是对象".to_string());
    };
    for (key, property_schema) in properties {
        if let Some(property_value) = object.get(key) {
            validate_schema_value(property_schema, property_value, &format!("{path}.{key}"))?;
        }
    }
    Ok(())
}

fn validate_additional_properties(
    schema: &Value,
    object: &serde_json::Map<String, Value>,
) -> Result<(), String> {
    if schema.get("additionalProperties") != Some(&Value::Bool(false)) {
        return Ok(());
    }
    let declared = schema
        .get("properties")
        .and_then(Value::as_object)
        .map(|properties| properties.keys().collect::<Vec<_>>())
        .unwrap_or_default();
    for key in object.keys() {
        if !declared
            .iter()
            .any(|declared_key| declared_key.as_str() == key)
        {
            return Err(format!("$.{key} 未在 schema.properties 中声明"));
        }
    }
    Ok(())
}

fn validate_items(schema: &Value, array: &[Value], path: &str) -> Result<(), String> {
    let Some(item_schema) = schema.get("items") else {
        return Ok(());
    };
    if item_schema.is_array() {
        return Err("schema.items 暂只支持单一 schema 对象或布尔值".to_string());
    }
    for (index, item) in array.iter().enumerate() {
        validate_schema_value(item_schema, item, &format!("{path}[{index}]"))?;
    }
    Ok(())
}

fn json_value_matches_type(value: &Value, expected: &str) -> bool {
    match expected {
        "array" => value.is_array(),
        "boolean" => value.is_boolean(),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "null" => value.is_null(),
        "number" => value.is_number(),
        "object" => value.is_object(),
        "string" => value.is_string(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::{
        project_json_schema_to_supported_subset, validate_json_schema_definition,
        validate_json_schema_subset,
    };

    #[test]
    fn rejects_additional_properties_and_enum_mismatch() {
        let schema = json!({
            "type": "object",
            "required": ["mode"],
            "properties": {
                "mode": { "type": "string", "enum": ["read", "write"] }
            },
            "additionalProperties": false
        });

        assert!(validate_json_schema_subset(&schema, &json!({"mode": "read"})).is_ok());
        assert!(validate_json_schema_subset(&schema, &json!({"mode": "admin"})).is_err());
        assert!(
            validate_json_schema_subset(&schema, &json!({"mode": "read", "extra": true})).is_err()
        );
    }

    #[test]
    fn rejects_unsupported_or_malformed_schema_definitions() {
        assert!(validate_json_schema_definition(&json!({ "oneOf": [] })).is_err());
        assert!(validate_json_schema_definition(&json!({ "type": "timestamp" })).is_err());
        assert!(validate_json_schema_definition(&json!({ "description": true })).is_err());
        assert!(validate_json_schema_definition(&json!({ "minimum": "zero" })).is_err());
        assert!(validate_json_schema_definition(&json!({ "anyOf": [] })).is_err());
        assert!(
            validate_json_schema_definition(&json!({
                "type": "object",
                "properties": { "nested": { "items": [] } }
            }))
            .is_err()
        );
    }

    #[test]
    fn accepts_description_annotations_on_nested_properties() {
        let schema = json!({
            "type": "object",
            "required": ["patch"],
            "properties": {
                "patch": {
                    "type": "string",
                    "description": "The patch text in Codex apply_patch format."
                }
            },
            "additionalProperties": false
        });

        assert!(validate_json_schema_definition(&schema).is_ok());
        assert!(
            validate_json_schema_subset(&schema, &json!({ "patch": "*** Begin Patch" })).is_ok()
        );
    }

    #[test]
    fn validates_numeric_bounds_and_any_of_branches() {
        let schema = json!({
            "anyOf": [
                {
                    "type": "object",
                    "required": ["offset"],
                    "properties": {
                        "offset": {
                            "type": "integer",
                            "minimum": 0,
                            "maximum": 10
                        }
                    }
                },
                { "type": "null" }
            ]
        });

        assert!(validate_json_schema_subset(&schema, &json!({ "offset": 0 })).is_ok());
        assert!(validate_json_schema_subset(&schema, &json!({ "offset": 10 })).is_ok());
        assert!(validate_json_schema_subset(&schema, &Value::Null).is_ok());
        assert!(validate_json_schema_subset(&schema, &json!({ "offset": -1 })).is_err());
        assert!(validate_json_schema_subset(&schema, &json!({ "offset": 11 })).is_err());
        assert!(validate_json_schema_subset(&schema, &json!("invalid")).is_err());
    }

    #[test]
    fn external_schema_projection_removes_unsupported_keywords_recursively() {
        let projected = project_json_schema_to_supported_subset(json!({
            "type": "object",
            "title": "MCP input",
            "properties": {
                "query": {
                    "type": "string",
                    "minLength": 1,
                    "pattern": "\\S+",
                    "description": "Search query"
                }
            },
            "required": ["query"],
            "additionalProperties": false
        }));

        assert!(validate_json_schema_definition(&projected).is_ok());
        assert_eq!(projected["properties"]["query"]["type"], "string");
        assert_eq!(
            projected["properties"]["query"]["description"],
            "Search query"
        );
        assert!(projected["properties"]["query"].get("minLength").is_none());
        assert!(projected["properties"]["query"].get("pattern").is_none());
    }

    #[test]
    fn malformed_external_schema_projects_to_an_unconstrained_valid_schema() {
        let projected = project_json_schema_to_supported_subset(json!({
            "type": "timestamp",
            "required": "query",
            "anyOf": []
        }));

        assert_eq!(projected, Value::Bool(true));
        assert!(validate_json_schema_definition(&projected).is_ok());
    }
}
