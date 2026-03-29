pub use agentdash_spi::schema::{sanitize_tool_schema, schema_value};

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
