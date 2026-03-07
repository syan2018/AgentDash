use schemars::{JsonSchema, schema_for};
use serde_json::{Map, Value, json};

pub fn schema_value<T: JsonSchema>() -> Value {
    sanitize_tool_schema(serde_json::to_value(schema_for!(T)).expect("schema 应可序列化"))
}

pub fn sanitize_tool_schema(mut schema: Value) -> Value {
    sanitize_schema_in_place(&mut schema);
    schema
}

fn sanitize_schema_in_place(schema: &mut Value) {
    let Some(map) = schema.as_object_mut() else {
        return;
    };

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
    use serde::Deserialize;

    #[derive(Debug, Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct ExampleParams {
        required: String,
        optional_text: Option<String>,
        optional_flag: Option<bool>,
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
}
