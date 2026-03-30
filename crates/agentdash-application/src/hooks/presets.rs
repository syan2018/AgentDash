use std::sync::LazyLock;

use agentdash_domain::workflow::WorkflowHookTrigger;
use agentdash_spi::HookTrigger;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct HookRulePreset {
    pub key: &'static str,
    pub trigger: WorkflowHookTrigger,
    pub label: &'static str,
    pub description: &'static str,
    pub param_schema: Option<serde_json::Value>,
}

static PRESET_REGISTRY: LazyLock<Vec<HookRulePreset>> = LazyLock::new(|| {
    vec![
        HookRulePreset {
            key: "block_record_artifact",
            trigger: WorkflowHookTrigger::BeforeTool,
            label: "禁止上报特定产物",
            description: "在当前 step 期间禁止 Agent 上报指定类型的 workflow artifact",
            param_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "artifact_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "被禁止的 artifact type 列表（如 session_summary）"
                    }
                },
                "required": ["artifact_types"]
            })),
        },
        HookRulePreset {
            key: "session_terminal_advance",
            trigger: WorkflowHookTrigger::BeforeStop,
            label: "Session 终态自动推进",
            description: "当 session 进入终态时自动推进 lifecycle step",
            param_schema: None,
        },
        HookRulePreset {
            key: "stop_gate_checks_pending",
            trigger: WorkflowHookTrigger::BeforeStop,
            label: "完成条件门禁",
            description: "在 completion checks 全部满足前，阻止 Agent 结束 session",
            param_schema: None,
        },
        HookRulePreset {
            key: "manual_step_notice",
            trigger: WorkflowHookTrigger::BeforeStop,
            label: "手动推进通知",
            description: "通知 Agent 当前 step 使用手动推进策略，不会自动切换到下一步",
            param_schema: None,
        },
        HookRulePreset {
            key: "subagent_inherit_context",
            trigger: WorkflowHookTrigger::BeforeSubagentDispatch,
            label: "子 Agent 继承上下文",
            description: "派发子 Agent 时自动继承当前 session 的 workflow 注入和约束",
            param_schema: None,
        },
        HookRulePreset {
            key: "subagent_record_result",
            trigger: WorkflowHookTrigger::AfterSubagentDispatch,
            label: "记录子 Agent 派发结果",
            description: "子 Agent 派发完成后记录诊断信息",
            param_schema: None,
        },
        HookRulePreset {
            key: "subagent_result_channel",
            trigger: WorkflowHookTrigger::SubagentResult,
            label: "子 Agent 回流处理",
            description: "处理子 Agent 回流结果，根据 adoption_mode 注入约束或 follow-up 要求",
            param_schema: None,
        },
    ]
});

pub fn hook_rule_preset_registry() -> &'static [HookRulePreset] {
    &PRESET_REGISTRY
}

pub fn domain_trigger_to_spi(trigger: WorkflowHookTrigger) -> HookTrigger {
    match trigger {
        WorkflowHookTrigger::BeforeTool => HookTrigger::BeforeTool,
        WorkflowHookTrigger::AfterTool => HookTrigger::AfterTool,
        WorkflowHookTrigger::AfterTurn => HookTrigger::AfterTurn,
        WorkflowHookTrigger::BeforeStop => HookTrigger::BeforeStop,
        WorkflowHookTrigger::SessionTerminal => HookTrigger::SessionTerminal,
        WorkflowHookTrigger::BeforeSubagentDispatch => HookTrigger::BeforeSubagentDispatch,
        WorkflowHookTrigger::AfterSubagentDispatch => HookTrigger::AfterSubagentDispatch,
        WorkflowHookTrigger::SubagentResult => HookTrigger::SubagentResult,
    }
}
