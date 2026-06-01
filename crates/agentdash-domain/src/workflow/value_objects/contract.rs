use serde::{Deserialize, Serialize};

use super::{
    CapabilityConfig, InputPortDefinition, OutputPortDefinition, WorkflowHookRuleSpec,
    WorkflowInjectionSpec,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WorkflowContract {
    #[serde(default)]
    pub injection: WorkflowInjectionSpec,
    #[serde(default)]
    pub hook_rules: Vec<WorkflowHookRuleSpec>,
    /// Workflow 级顶层能力配置，包含工具能力、mount/context/policy 等能力维度。
    #[serde(default, skip_serializing_if = "CapabilityConfig::is_empty")]
    pub capability_config: CapabilityConfig,
    /// Workflow 产出声明 — 同时作为完成条件：port gate 门禁根据 `gate_strategy` 检查交付。
    ///
    /// Lifecycle step 绑定 workflow 时自动继承这些 ports 作为默认值，step 编辑器可 override。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub output_ports: Vec<OutputPortDefinition>,
    /// Workflow 输入声明 — 同时作为运行约束：lifecycle 内由 edge wire 满足，standalone 由调用方写入。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_ports: Vec<InputPortDefinition>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowSessionTerminalState {
    Completed,
    Failed,
    Interrupted,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct EffectiveSessionContract {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lifecycle_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_activity_key: Option<String>,
    #[serde(default)]
    pub injection: WorkflowInjectionSpec,
    #[serde(default)]
    pub hook_rules: Vec<WorkflowHookRuleSpec>,
}
