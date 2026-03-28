use schemars::{JsonSchema, schema_for};
use serde_json::{Map, Value, json};

/// 从 `JsonSchema` derive 生成 OpenAI 兼容的 tool parameters schema。
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
