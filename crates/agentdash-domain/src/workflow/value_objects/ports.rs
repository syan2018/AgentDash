use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Standalone 场景下 input port 的满足策略。
///
/// Lifecycle 内运行时由 edge wire 自动满足；standalone（如主 agent 给子 agent
/// 分配 workflow）时由此字段指示调用方如何提供输入。
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StandaloneFulfillment {
    /// 调用方必须在启动前通过 `lifecycle://artifacts/{key}` 写入
    #[default]
    Required,
    /// 可选输入，未提供时使用 default_value
    Optional {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default_value: Option<String>,
    },
}
/// 门禁策略：定义 output port 交付检查的严格程度。
/// 实际检查逻辑由对应的 Rhai Hook Preset 实现。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GateStrategy {
    #[default]
    Existence,
    Schema,
    LlmJudge,
}

/// Input port 上下文构建策略：控制前驱 output artifact 如何注入后继 session。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, TS, PartialEq, Eq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    #[default]
    Full,
    Summary,
    MetadataOnly,
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, JsonSchema)]
pub struct OutputPortDefinition {
    pub key: String,
    pub description: String,
    #[serde(default)]
    pub gate_strategy: GateStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gate_params: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, JsonSchema)]
pub struct InputPortDefinition {
    pub key: String,
    pub description: String,
    #[serde(default)]
    pub context_strategy: ContextStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_template: Option<String>,
    /// Standalone 运行时（非 lifecycle edge wire）如何满足此 input port。
    #[serde(default)]
    pub standalone_fulfillment: StandaloneFulfillment,
}
