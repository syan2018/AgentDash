use std::sync::OnceLock;

use regex::Regex;
use serde_json::Value;
use tera::{Context, Tera};

/// Routine prompt 模板渲染器
///
/// 使用 Tera (Jinja2 语法) 对 prompt_template 进行插值。
/// 每次渲染创建临时 Tera 实例（Routine 数量有限，无需全局缓存）。
pub fn render_prompt_template(
    template: &str,
    trigger_source: &str,
    routine_name: &str,
    project_id: &str,
    payload: Option<&Value>,
) -> Result<String, String> {
    let mut tera = Tera::default();
    tera.add_raw_template("prompt", template)
        .map_err(|e| format!("模板解析失败: {e}"))?;

    let mut trigger = serde_json::Map::new();
    trigger.insert(
        "source".to_string(),
        Value::String(trigger_source.to_string()),
    );
    trigger.insert(
        "timestamp".to_string(),
        Value::String(chrono::Utc::now().to_rfc3339()),
    );
    if let Some(payload) = payload {
        trigger.insert("payload".to_string(), payload.clone());
    } else {
        trigger.insert("payload".to_string(), Value::Object(serde_json::Map::new()));
    }
    let mut routine = serde_json::Map::new();
    routine.insert("name".to_string(), Value::String(routine_name.to_string()));
    routine.insert(
        "project_id".to_string(),
        Value::String(project_id.to_string()),
    );

    let mut root = serde_json::Map::new();
    root.insert("trigger".to_string(), Value::Object(trigger));
    root.insert("routine".to_string(), Value::Object(routine));
    let context_value = Value::Object(root);

    validate_required_variables(template, &context_value)?;

    let context =
        Context::from_value(context_value).map_err(|e| format!("模板上下文构造失败: {e}"))?;

    tera.render("prompt", &context)
        .map_err(|e| format!("模板渲染失败: {e}"))
}

fn validate_required_variables(template: &str, context: &Value) -> Result<(), String> {
    for captures in variable_block_regex().captures_iter(template) {
        let Some(expression) = captures.name("expr") else {
            continue;
        };
        let expression = expression.as_str().trim();
        if expression.is_empty() || expression.contains("default(") {
            continue;
        }

        let primary = expression.split('|').next().unwrap_or_default().trim();
        if !looks_like_variable_path(primary) {
            continue;
        }

        if resolve_value_path(context, primary).is_none() {
            return Err(format!("模板变量 `{primary}` 缺失，且未提供 default 兜底"));
        }
    }

    Ok(())
}

fn variable_block_regex() -> &'static Regex {
    static REGEX: OnceLock<Regex> = OnceLock::new();
    REGEX.get_or_init(|| Regex::new(r"\{\{\s*(?P<expr>.*?)\s*\}\}").expect("regex 应合法"))
}

fn looks_like_variable_path(expression: &str) -> bool {
    expression.split('.').all(|segment| {
        !segment.is_empty()
            && segment
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_')
    })
}

fn resolve_value_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_simple_template() {
        let result = render_prompt_template(
            "Hello {{ routine.name }}, source is {{ trigger.source }}",
            "scheduled",
            "my-routine",
            "proj-123",
            None,
        )
        .unwrap();
        assert!(result.contains("my-routine"));
        assert!(result.contains("scheduled"));
    }

    #[test]
    fn test_payload_access() {
        let payload = json!({"alert_id": "SEN-4521", "severity": "critical"});
        let result = render_prompt_template(
            "Alert {{ trigger.payload.alert_id }} severity {{ trigger.payload.severity }}",
            "webhook",
            "sentry-routine",
            "proj-123",
            Some(&payload),
        )
        .unwrap();
        assert!(result.contains("SEN-4521"));
        assert!(result.contains("critical"));
    }

    #[test]
    fn test_conditional_template() {
        let payload = json!({"stacktrace": "at line 42"});
        let result = render_prompt_template(
            "Error report{% if trigger.payload.stacktrace %}\nStack: {{ trigger.payload.stacktrace }}{% endif %}",
            "webhook",
            "test",
            "proj-123",
            Some(&payload),
        )
        .unwrap();
        assert!(result.contains("Stack: at line 42"));
    }

    #[test]
    fn test_missing_variable_without_default_fails() {
        let result = render_prompt_template(
            "{{ trigger.payload.nonexistent_required_field }}",
            "webhook",
            "test",
            "proj-123",
            None,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_variable_with_default_is_allowed() {
        let result = render_prompt_template(
            "{{ trigger.payload.nonexistent_required_field | default(value=\"N/A\") }}",
            "webhook",
            "test",
            "proj-123",
            None,
        )
        .unwrap();
        assert_eq!(result, "N/A");
    }
}
