use schemars::{JsonSchema, schema_for};
use serde_json::{Map, Value, json};

pub fn schema_value<T: JsonSchema>() -> Value {
    sanitize_tool_schema(serde_json::to_value(schema_for!(T)).expect("schema 应可序列化"))
}

/// 清洗原始 JSON Schema，使其满足 OpenAI function-calling 约束：
/// - 移除装饰性关键字（title / default / format 等）
/// - `const` 转为 `enum`，`oneOf` 转为 `anyOf`
/// - 所有 property 列入 required，可选项标记 nullable
/// - 内联 anyOf/allOf 中的本地 $ref
pub fn sanitize_tool_schema(mut schema: Value) -> Value {
    sanitize_schema_in_place(&mut schema);
    let snapshot = schema.clone();
    inline_local_refs_in_combinators(&mut schema, &snapshot);
    schema
}

fn sanitize_schema_in_place(schema: &mut Value) {
    let Some(map) = schema.as_object_mut() else {
        return;
    };

    for keyword in [
        "$schema",
        "title",
        "default",
        "examples",
        "deprecated",
        "readOnly",
        "writeOnly",
        "format",
    ] {
        map.remove(keyword);
    }

    if let Some(const_value) = map.remove("const") {
        map.insert("enum".to_string(), Value::Array(vec![const_value]));
    }

    if let Some(one_of) = map.remove("oneOf") {
        map.entry("anyOf".to_string()).or_insert(one_of);
    }

    for keyword in [
        "items",
        "additionalProperties",
        "contains",
        "if",
        "then",
        "else",
    ] {
        if let Some(value) = map.get_mut(keyword) {
            sanitize_schema_in_place(value);
        }
    }

    for keyword in [
        "$defs",
        "definitions",
        "dependentSchemas",
        "patternProperties",
    ] {
        if let Some(values) = map.get_mut(keyword).and_then(Value::as_object_mut) {
            for value in values.values_mut() {
                sanitize_schema_in_place(value);
            }
        }
    }

    for keyword in ["anyOf", "allOf", "oneOf", "prefixItems"] {
        if let Some(values) = map.get_mut(keyword).and_then(Value::as_array_mut) {
            for value in values {
                sanitize_schema_in_place(value);
            }
        }
    }

    sanitize_object_schema(map);
}

fn sanitize_object_schema(map: &mut Map<String, Value>) {
    let original_required = map
        .get("required")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<std::collections::BTreeSet<_>>()
        })
        .unwrap_or_default();

    let Some(properties) = map.get_mut("properties").and_then(Value::as_object_mut) else {
        return;
    };

    let property_names = properties.keys().cloned().collect::<Vec<_>>();

    for (name, property_schema) in properties.iter_mut() {
        sanitize_schema_in_place(property_schema);
        if !original_required.contains(name) {
            make_nullable(property_schema);
        }
    }

    map.insert("type".to_string(), Value::String("object".to_string()));
    map.insert(
        "required".to_string(),
        Value::Array(
            property_names
                .into_iter()
                .map(Value::String)
                .collect::<Vec<_>>(),
        ),
    );
    map.entry("additionalProperties".to_string())
        .or_insert(Value::Bool(false));
}

fn inline_local_refs_in_combinators(schema: &mut Value, root: &Value) {
    let Some(map) = schema.as_object_mut() else {
        if let Some(items) = schema.as_array_mut() {
            for item in items {
                inline_local_refs_in_combinators(item, root);
            }
        }
        return;
    };

    for key in ["anyOf", "allOf", "oneOf"] {
        if let Some(items) = map.get_mut(key).and_then(Value::as_array_mut) {
            for item in items.iter_mut() {
                if let Some(reference) = extract_local_ref(item)
                    && let Some(resolved) = resolve_local_ref(root, reference)
                {
                    *item = resolved;
                }
                inline_local_refs_in_combinators(item, root);
            }
        }
    }

    if let Some(properties) = map.get_mut("properties").and_then(Value::as_object_mut) {
        for property in properties.values_mut() {
            inline_local_refs_in_combinators(property, root);
        }
    }

    for key in [
        "items",
        "additionalProperties",
        "contains",
        "if",
        "then",
        "else",
    ] {
        if let Some(child) = map.get_mut(key) {
            inline_local_refs_in_combinators(child, root);
        }
    }

    for key in [
        "$defs",
        "definitions",
        "dependentSchemas",
        "patternProperties",
    ] {
        if let Some(children) = map.get_mut(key).and_then(Value::as_object_mut) {
            for child in children.values_mut() {
                inline_local_refs_in_combinators(child, root);
            }
        }
    }

    if let Some(items) = map.get_mut("prefixItems").and_then(Value::as_array_mut) {
        for item in items {
            inline_local_refs_in_combinators(item, root);
        }
    }
}

fn extract_local_ref(value: &Value) -> Option<&str> {
    let object = value.as_object()?;
    if object.len() != 1 {
        return None;
    }
    object
        .get("$ref")?
        .as_str()
        .filter(|reference| reference.starts_with('#'))
}

fn resolve_local_ref(root: &Value, reference: &str) -> Option<Value> {
    let pointer = reference.strip_prefix("#/")?;
    let mut current = root;

    for segment in pointer.split('/') {
        let decoded = segment.replace("~1", "/").replace("~0", "~");
        current = current.as_object()?.get(decoded.as_str())?;
    }

    Some(current.clone())
}

fn make_nullable(schema: &mut Value) {
    let Some(map) = schema.as_object_mut() else {
        let original = schema.take();
        *schema = json!({
            "anyOf": [original, { "type": "null" }]
        });
        return;
    };

    if let Some(type_value) = map.get_mut("type") {
        match type_value {
            Value::String(current) if current != "null" => {
                *type_value = Value::Array(vec![
                    Value::String(current.clone()),
                    Value::String("null".to_string()),
                ]);
            }
            Value::String(_) => {}
            Value::Array(items) => {
                let has_null = items.iter().any(|item| item.as_str() == Some("null"));
                if !has_null {
                    items.push(Value::String("null".to_string()));
                }
            }
            _ => {}
        }
        return;
    }

    for keyword in ["anyOf", "oneOf"] {
        if let Some(items) = map.get_mut(keyword).and_then(Value::as_array_mut) {
            let has_null = items.iter().any(|item| {
                item.as_object()
                    .and_then(|entry| entry.get("type"))
                    .and_then(Value::as_str)
                    == Some("null")
            });
            if !has_null {
                items.push(json!({ "type": "null" }));
            }
            return;
        }
    }

    let original = schema.take();
    *schema = json!({
        "anyOf": [original, { "type": "null" }]
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde_json::Value;

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct ExampleParams {
        required: String,
        optional_text: Option<String>,
        optional_flag: Option<bool>,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct NestedTaskInput {
        title: String,
        workspace_id: Option<String>,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct NestedBatchParams {
        tasks: Vec<NestedTaskInput>,
    }

    #[test]
    fn object_schema_is_openai_compatible() {
        let schema = schema_value::<ExampleParams>();
        let required = schema["required"].as_array().unwrap();
        let required_names = required
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert!(required_names.contains(&"required"));
        assert!(required_names.contains(&"optional_text"));
        assert!(required_names.contains(&"optional_flag"));

        let optional_text_type = schema["properties"]["optional_text"]["type"]
            .as_array()
            .unwrap();
        assert!(optional_text_type.iter().any(|value| value == "string"));
        assert!(optional_text_type.iter().any(|value| value == "null"));

        let optional_flag_type = schema["properties"]["optional_flag"]["type"]
            .as_array()
            .unwrap();
        assert!(optional_flag_type.iter().any(|value| value == "boolean"));
        assert!(optional_flag_type.iter().any(|value| value == "null"));
    }

    #[test]
    fn nested_defs_are_also_sanitized_for_openai() {
        let schema = schema_value::<NestedBatchParams>();
        let defs = schema["$defs"].as_object().expect("should contain defs");
        let nested = defs
            .get("NestedTaskInput")
            .expect("nested task input schema should exist");

        let required = nested["required"]
            .as_array()
            .expect("required should be array")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();

        assert!(required.contains(&"title"));
        assert!(required.contains(&"workspace_id"));

        let workspace_id_type = nested["properties"]["workspace_id"]["type"]
            .as_array()
            .expect("workspace_id should be nullable");
        assert!(workspace_id_type.iter().any(|value| value == "string"));
        assert!(workspace_id_type.iter().any(|value| value == "null"));
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum TaggedMode {
        Inline { path: String },
        External { service_id: String },
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct SchemaKeywordParams {
        count: Option<u32>,
        mode: TaggedMode,
    }

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct OptionalTaggedWrapper {
        item: Option<TaggedMode>,
    }

    #[test]
    fn decorative_keywords_are_removed_and_const_becomes_enum() {
        let schema = schema_value::<SchemaKeywordParams>();
        let defs = schema["$defs"].as_object().expect("should contain defs");
        let tagged_mode = defs.get("TaggedMode").expect("tagged enum should exist");
        let any_of = tagged_mode["anyOf"].as_array().expect("anyOf should exist");
        let first_branch = any_of.first().expect("first branch should exist");
        let kind_schema = &first_branch["properties"]["kind"];

        assert!(schema.get("$schema").is_none());
        assert!(schema.get("title").is_none());
        assert!(
            defs.values()
                .all(|value| value.get("title").is_none() && value.get("default").is_none())
        );
        assert!(schema["properties"]["count"].get("format").is_none());
        assert_eq!(kind_schema["enum"], serde_json::json!(["inline"]));
        assert!(kind_schema.get("const").is_none());
        assert!(tagged_mode.get("oneOf").is_none());
    }

    #[test]
    fn local_refs_inside_anyof_are_inlined() {
        let schema = schema_value::<OptionalTaggedWrapper>();
        let any_of = schema["properties"]["item"]["anyOf"]
            .as_array()
            .expect("optional tagged wrapper should use anyOf");
        let first_branch = any_of.first().expect("anyOf should have first branch");

        assert!(first_branch.get("$ref").is_none());
        assert_eq!(first_branch["anyOf"].as_array().map(Vec::len), Some(2));
        assert!(first_branch.get("oneOf").is_none());
    }
}
