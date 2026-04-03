//! Companion 信道 Payload Type 注册表
//!
//! 每个 payload type 定义三件事：
//! - request_schema: request payload 需要哪些字段
//! - response_type: 对应 respond payload 应使用什么 type
//! - ui_hint: 前端应使用哪种 UI 组件渲染
//!
//! 校验在 companion_request / companion_respond 执行时按 type 进行。
//! 未识别的 payload 字段静默忽略，保证向前兼容。

use std::collections::HashMap;

/// 单个 payload type 的定义
#[derive(Debug, Clone)]
pub struct PayloadTypeDefinition {
    /// type 名称
    pub name: &'static str,
    /// 是否为 request type（companion_request 使用）
    pub is_request: bool,
    /// 是否为 response type（companion_respond 使用）
    pub is_response: bool,
    /// request payload 必填字段
    pub required_fields: &'static [&'static str],
    /// 对应的 respond type（仅 request type 有）
    pub response_type: Option<&'static str>,
    /// 前端 UI 组件提示
    pub ui_hint: &'static str,
}

/// Payload Type 注册表
pub struct PayloadTypeRegistry {
    types: HashMap<&'static str, PayloadTypeDefinition>,
}

impl PayloadTypeRegistry {
    /// 创建包含所有内置 type 的注册表
    pub fn with_builtins() -> Self {
        let mut registry = Self {
            types: HashMap::new(),
        };

        // ─── Request types ───────────────────────────────────
        registry.register(PayloadTypeDefinition {
            name: "task",
            is_request: true,
            is_response: false,
            required_fields: &["prompt"],
            response_type: Some("completion"),
            ui_hint: "task_dispatch_card",
        });
        registry.register(PayloadTypeDefinition {
            name: "review",
            is_request: true,
            is_response: false,
            required_fields: &["prompt"],
            response_type: Some("resolution"),
            ui_hint: "review_card",
        });
        registry.register(PayloadTypeDefinition {
            name: "approval",
            is_request: true,
            is_response: false,
            required_fields: &["prompt"],
            response_type: Some("decision"),
            ui_hint: "approval_card",
        });
        registry.register(PayloadTypeDefinition {
            name: "notification",
            is_request: true,
            is_response: false,
            required_fields: &["message"],
            response_type: None, // 不期望回复
            ui_hint: "notification_toast",
        });

        // ─── Response types ──────────────────────────────────
        registry.register(PayloadTypeDefinition {
            name: "completion",
            is_request: false,
            is_response: true,
            required_fields: &["status", "summary"],
            response_type: None,
            ui_hint: "completion_card",
        });
        registry.register(PayloadTypeDefinition {
            name: "resolution",
            is_request: false,
            is_response: true,
            required_fields: &["status", "summary"],
            response_type: None,
            ui_hint: "resolution_badge",
        });
        registry.register(PayloadTypeDefinition {
            name: "decision",
            is_request: false,
            is_response: true,
            required_fields: &["choice"],
            response_type: None,
            ui_hint: "decision_badge",
        });

        registry
    }

    fn register(&mut self, definition: PayloadTypeDefinition) {
        self.types.insert(definition.name, definition);
    }

    /// 查找 type 定义
    pub fn get(&self, name: &str) -> Option<&PayloadTypeDefinition> {
        self.types.get(name)
    }

    /// 校验 request payload。返回 None 表示校验通过，Some(error) 表示校验失败。
    pub fn validate_request(&self, payload: &serde_json::Value) -> Option<String> {
        let type_name = match payload.get("type").and_then(|v| v.as_str()) {
            Some(name) => name,
            None => return None, // 无 type 字段时跳过校验（向前兼容）
        };

        let definition = match self.types.get(type_name) {
            Some(def) => def,
            None => return None, // 未知 type 静默跳过（向前兼容）
        };

        if !definition.is_request {
            return Some(format!(
                "payload.type=`{type_name}` 是 response type，不能用于 companion_request"
            ));
        }

        for field in definition.required_fields {
            let value = payload.get(*field);
            let is_empty = value
                .map(|v| v.as_str().is_some_and(|s| s.trim().is_empty()) || v.is_null())
                .unwrap_or(true);
            if is_empty {
                return Some(format!("payload.type=`{type_name}` 要求必填 `{field}`"));
            }
        }

        None
    }

    /// 校验 respond payload。`request_type` 是原始 request 的 type（如果有）。
    /// 返回 None 表示校验通过，Some(error) 表示校验失败。
    pub fn validate_response(
        &self,
        payload: &serde_json::Value,
        request_type: Option<&str>,
    ) -> Option<String> {
        let type_name = match payload.get("type").and_then(|v| v.as_str()) {
            Some(name) => name,
            None => return None, // 无 type 字段时跳过校验
        };

        let definition = match self.types.get(type_name) {
            Some(def) => def,
            None => return None, // 未知 type 静默跳过
        };

        if !definition.is_response {
            return Some(format!(
                "payload.type=`{type_name}` 是 request type，不能用于 companion_respond"
            ));
        }

        // 如果知道原始 request type，校验 response type 是否匹配
        if let Some(req_type) = request_type {
            if let Some(req_def) = self.types.get(req_type) {
                if let Some(expected_response) = req_def.response_type {
                    if type_name != expected_response {
                        return Some(format!(
                            "request type=`{req_type}` 期望 response type=`{expected_response}`，收到 `{type_name}`"
                        ));
                    }
                }
            }
        }

        for field in definition.required_fields {
            let value = payload.get(*field);
            let is_empty = value
                .map(|v| v.as_str().is_some_and(|s| s.trim().is_empty()) || v.is_null())
                .unwrap_or(true);
            if is_empty {
                return Some(format!("payload.type=`{type_name}` 要求必填 `{field}`"));
            }
        }

        None
    }

    /// 获取 request type 对应的 response type 名称
    pub fn expected_response_type(&self, request_type: &str) -> Option<&'static str> {
        self.types
            .get(request_type)
            .and_then(|def| def.response_type)
    }

    /// 获取 type 对应的 ui_hint
    pub fn ui_hint(&self, type_name: &str) -> Option<&'static str> {
        self.types.get(type_name).map(|def| def.ui_hint)
    }

    /// 构造 hook 注入消息尾部的回复约束提示
    pub fn response_hint(&self, request_type: &str) -> Option<String> {
        let req_def = self.types.get(request_type)?;
        let response_type = req_def.response_type?;
        let resp_def = self.types.get(response_type)?;

        let required = resp_def
            .required_fields
            .iter()
            .map(|f| format!("`{f}`"))
            .collect::<Vec<_>>()
            .join("、");

        Some(format!(
            "回复要求：payload.type 必须为 `{response_type}`，必填 {required}。"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_types_are_registered() {
        let registry = PayloadTypeRegistry::with_builtins();
        assert!(registry.get("task").is_some());
        assert!(registry.get("completion").is_some());
        assert!(registry.get("review").is_some());
        assert!(registry.get("resolution").is_some());
        assert!(registry.get("approval").is_some());
        assert!(registry.get("decision").is_some());
        assert!(registry.get("notification").is_some());
    }

    #[test]
    fn validate_request_passes_for_valid_task() {
        let registry = PayloadTypeRegistry::with_builtins();
        let payload = serde_json::json!({"type": "task", "prompt": "审阅代码"});
        assert!(registry.validate_request(&payload).is_none());
    }

    #[test]
    fn validate_request_fails_for_missing_required_field() {
        let registry = PayloadTypeRegistry::with_builtins();
        let payload = serde_json::json!({"type": "task"});
        let error = registry.validate_request(&payload);
        assert!(error.is_some());
        assert!(error.unwrap().contains("prompt"));
    }

    #[test]
    fn validate_request_fails_for_response_type_used_as_request() {
        let registry = PayloadTypeRegistry::with_builtins();
        let payload =
            serde_json::json!({"type": "completion", "status": "completed", "summary": "done"});
        let error = registry.validate_request(&payload);
        assert!(error.is_some());
        assert!(error.unwrap().contains("response type"));
    }

    #[test]
    fn validate_request_skips_unknown_type() {
        let registry = PayloadTypeRegistry::with_builtins();
        let payload = serde_json::json!({"type": "future_type", "anything": "goes"});
        assert!(registry.validate_request(&payload).is_none());
    }

    #[test]
    fn validate_request_skips_missing_type() {
        let registry = PayloadTypeRegistry::with_builtins();
        let payload = serde_json::json!({"prompt": "no type field"});
        assert!(registry.validate_request(&payload).is_none());
    }

    #[test]
    fn validate_response_passes_for_valid_resolution() {
        let registry = PayloadTypeRegistry::with_builtins();
        let payload =
            serde_json::json!({"type": "resolution", "status": "approved", "summary": "ok"});
        assert!(
            registry
                .validate_response(&payload, Some("review"))
                .is_none()
        );
    }

    #[test]
    fn validate_response_fails_for_type_mismatch() {
        let registry = PayloadTypeRegistry::with_builtins();
        let payload =
            serde_json::json!({"type": "completion", "status": "completed", "summary": "done"});
        let error = registry.validate_response(&payload, Some("review"));
        assert!(error.is_some());
        assert!(error.unwrap().contains("resolution"));
    }

    #[test]
    fn validate_response_fails_for_missing_required_field() {
        let registry = PayloadTypeRegistry::with_builtins();
        let payload = serde_json::json!({"type": "decision"});
        let error = registry.validate_response(&payload, Some("approval"));
        assert!(error.is_some());
        assert!(error.unwrap().contains("choice"));
    }

    #[test]
    fn response_hint_includes_required_fields() {
        let registry = PayloadTypeRegistry::with_builtins();
        let hint = registry.response_hint("review").unwrap();
        assert!(hint.contains("resolution"));
        assert!(hint.contains("status"));
        assert!(hint.contains("summary"));
    }

    #[test]
    fn notification_has_no_response_type() {
        let registry = PayloadTypeRegistry::with_builtins();
        assert!(registry.expected_response_type("notification").is_none());
        assert!(registry.response_hint("notification").is_none());
    }
}
