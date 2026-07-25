use schemars::{JsonSchema, schema_for};
use serde_json::{Map, Value};

/// 从 `JsonSchema` derive 生成 OpenAI 兼容的 tool parameters schema。
pub fn schema_value<T: JsonSchema>() -> Value {
    sanitize_tool_schema(serde_json::to_value(schema_for!(T)).expect("schema 应可序列化"))
}

/// 清洗原始 JSON Schema，使其满足 OpenAI function-calling 约束：
/// - 移除装饰性关键字（title / default / format 等）
/// - `const` 转为 `enum`，`oneOf` 转为 `anyOf`
/// - 保留 derive 产出的必填性；可选字段保持可省略
/// - 内联 anyOf/allOf 中的本地 $ref
pub fn sanitize_tool_schema(mut schema: Value) -> Value {
    sanitize_schema_in_place(&mut schema);
    let snapshot = schema.clone();
    inline_local_refs(&mut schema, &snapshot, &mut Vec::new());
    remove_definition_tables(&mut schema);
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

    for property_schema in properties.values_mut() {
        sanitize_schema_in_place(property_schema);
    }

    map.insert("type".to_string(), Value::String("object".to_string()));
    map.insert(
        "required".to_string(),
        Value::Array(
            original_required
                .into_iter()
                .map(Value::String)
                .collect::<Vec<_>>(),
        ),
    );
    map.entry("additionalProperties".to_string())
        .or_insert(Value::Bool(false));
}

fn inline_local_refs(schema: &mut Value, root: &Value, ref_stack: &mut Vec<String>) {
    let Some(map) = schema.as_object_mut() else {
        if let Some(items) = schema.as_array_mut() {
            for item in items {
                inline_local_refs(item, root, ref_stack);
            }
        }
        return;
    };

    if let Some(reference) = map.get("$ref").and_then(Value::as_str).map(str::to_string)
        && reference.starts_with('#')
        && !ref_stack.contains(&reference)
        && let Some(resolved) = resolve_local_ref(root, &reference)
    {
        ref_stack.push(reference);
        *schema = resolved;
        inline_local_refs(schema, root, ref_stack);
        ref_stack.pop();
        return;
    }

    for key in ["anyOf", "allOf", "oneOf"] {
        if let Some(items) = map.get_mut(key).and_then(Value::as_array_mut) {
            for item in items.iter_mut() {
                inline_local_refs(item, root, ref_stack);
            }
        }
    }

    if let Some(properties) = map.get_mut("properties").and_then(Value::as_object_mut) {
        for property in properties.values_mut() {
            inline_local_refs(property, root, ref_stack);
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
            inline_local_refs(child, root, ref_stack);
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
                inline_local_refs(child, root, ref_stack);
            }
        }
    }

    if let Some(items) = map.get_mut("prefixItems").and_then(Value::as_array_mut) {
        for item in items {
            inline_local_refs(item, root, ref_stack);
        }
    }
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

fn remove_definition_tables(schema: &mut Value) {
    let Some(map) = schema.as_object_mut() else {
        if let Some(items) = schema.as_array_mut() {
            for item in items {
                remove_definition_tables(item);
            }
        }
        return;
    };

    map.remove("$defs");
    map.remove("definitions");

    for value in map.values_mut() {
        remove_definition_tables(value);
    }
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

    fn required_names(schema: &Value) -> Vec<&str> {
        schema
            .get("required")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .collect()
    }

    #[test]
    fn object_schema_preserves_optional_fields() {
        let schema = schema_value::<ExampleParams>();
        let required = required_names(&schema);

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["additionalProperties"], false);
        assert_eq!(required, vec!["required"]);
        assert!(!required.contains(&"optional_text"));
        assert!(!required.contains(&"optional_flag"));
    }

    #[test]
    fn nested_object_schema_preserves_optional_fields() {
        let schema = schema_value::<NestedBatchParams>();
        let nested = &schema["properties"]["tasks"]["items"];
        let required = required_names(nested);

        assert_eq!(required, vec!["title"]);
        assert!(!required.contains(&"workspace_id"));
    }
}
