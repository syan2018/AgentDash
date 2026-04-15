use std::sync::LazyLock;

use agentdash_domain::workflow::WorkflowHookTrigger;
use agentdash_spi::HookTrigger;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PresetSource {
    Builtin,
    UserDefined,
}

#[derive(Debug, Clone, Serialize)]
pub struct HookRulePreset {
    pub key: &'static str,
    pub trigger: WorkflowHookTrigger,
    pub label: &'static str,
    pub description: &'static str,
    pub param_schema: Option<serde_json::Value>,
    pub script: &'static str,
    pub source: PresetSource,
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
            script: include_str!("../../scripts/hook-presets/block_record_artifact.rhai"),
            source: PresetSource::Builtin,
        },
        HookRulePreset {
            key: "session_terminal_advance",
            trigger: WorkflowHookTrigger::BeforeStop,
            label: "Session 终态自动推进",
            description: "当 session 进入终态时自动推进 lifecycle step",
            param_schema: None,
            script: include_str!("../../scripts/hook-presets/session_terminal_advance.rhai"),
            source: PresetSource::Builtin,
        },
        HookRulePreset {
            key: "stop_gate_checks_pending",
            trigger: WorkflowHookTrigger::BeforeStop,
            label: "完成条件门禁",
            description: "在 completion checks 全部满足前，阻止 Agent 结束 session",
            param_schema: None,
            script: include_str!("../../scripts/hook-presets/stop_gate_checks_pending.rhai"),
            source: PresetSource::Builtin,
        },
        HookRulePreset {
            key: "manual_step_notice",
            trigger: WorkflowHookTrigger::BeforeStop,
            label: "手动推进通知",
            description: "通知 Agent 当前 step 使用手动推进策略，不会自动切换到下一步",
            param_schema: None,
            script: include_str!("../../scripts/hook-presets/manual_step_notice.rhai"),
            source: PresetSource::Builtin,
        },
        HookRulePreset {
            key: "subagent_inherit_context",
            trigger: WorkflowHookTrigger::BeforeSubagentDispatch,
            label: "子 Agent 继承上下文",
            description: "派发子 Agent 时自动继承当前 session 的 workflow 注入和约束",
            param_schema: None,
            script: include_str!("../../scripts/hook-presets/subagent_inherit_context.rhai"),
            source: PresetSource::Builtin,
        },
        HookRulePreset {
            key: "subagent_record_result",
            trigger: WorkflowHookTrigger::AfterSubagentDispatch,
            label: "记录子 Agent 派发结果",
            description: "子 Agent 派发完成后记录诊断信息",
            param_schema: None,
            script: include_str!("../../scripts/hook-presets/subagent_record_result.rhai"),
            source: PresetSource::Builtin,
        },
        HookRulePreset {
            key: "subagent_result_channel",
            trigger: WorkflowHookTrigger::SubagentResult,
            label: "子 Agent 回流处理",
            description: "处理子 Agent 回流结果，根据 adoption_mode 注入约束或 follow-up 要求",
            param_schema: None,
            script: include_str!("../../scripts/hook-presets/subagent_result_channel.rhai"),
            source: PresetSource::Builtin,
        },
        HookRulePreset {
            key: "supervised_tool_gate",
            trigger: WorkflowHookTrigger::BeforeTool,
            label: "受监管工具审批",
            description: "在 SUPERVISED 权限策略下，执行/编辑类工具需要用户审批。支持通过 params.allowlist 配置白名单",
            param_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "allowlist": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "不需要审批的工具白名单"
                    }
                }
            })),
            script: include_str!("../../scripts/hook-presets/supervised_tool_gate.rhai"),
            source: PresetSource::Builtin,
        },
        HookRulePreset {
            key: "context_compaction_trigger",
            trigger: WorkflowHookTrigger::BeforeCompact,
            label: "上下文压缩触发",
            description: "当 token 使用超过阈值时自动触发上下文压缩。可通过 params 调整 reserve_tokens 和 keep_last_n",
            param_schema: Some(serde_json::json!({
                "type": "object",
                "properties": {
                    "reserve_tokens": {
                        "type": "integer",
                        "description": "预留 token 数（默认 16384）"
                    },
                    "keep_last_n": {
                        "type": "integer",
                        "description": "保留最近 N 条消息不压缩（默认 20）"
                    }
                }
            })),
            script: include_str!("../../scripts/hook-presets/context_compaction_trigger.rhai"),
            source: PresetSource::Builtin,
        },
        HookRulePreset {
            key: "task_session_terminal",
            trigger: WorkflowHookTrigger::SessionTerminal,
            label: "Task 终态状态转换",
            description: "Task 伴生 session 进入终态时，根据 execution_mode 和 terminal_state 声明 task 状态变更/重试等副作用",
            param_schema: None,
            script: include_str!("../../scripts/hook-presets/task_session_terminal.rhai"),
            source: PresetSource::Builtin,
        },
    ]
});

pub fn hook_rule_preset_registry() -> &'static [HookRulePreset] {
    &PRESET_REGISTRY
}

/// 返回 preset key → script 源码的映射，用于初始化 HookScriptEngine
pub fn builtin_preset_scripts() -> Vec<(&'static str, &'static str)> {
    PRESET_REGISTRY.iter().map(|p| (p.key, p.script)).collect()
}

pub fn domain_trigger_to_spi(trigger: WorkflowHookTrigger) -> HookTrigger {
    match trigger {
        WorkflowHookTrigger::UserPromptSubmit => HookTrigger::UserPromptSubmit,
        WorkflowHookTrigger::BeforeTool => HookTrigger::BeforeTool,
        WorkflowHookTrigger::AfterTool => HookTrigger::AfterTool,
        WorkflowHookTrigger::AfterTurn => HookTrigger::AfterTurn,
        WorkflowHookTrigger::BeforeStop => HookTrigger::BeforeStop,
        WorkflowHookTrigger::SessionTerminal => HookTrigger::SessionTerminal,
        WorkflowHookTrigger::BeforeSubagentDispatch => HookTrigger::BeforeSubagentDispatch,
        WorkflowHookTrigger::AfterSubagentDispatch => HookTrigger::AfterSubagentDispatch,
        WorkflowHookTrigger::SubagentResult => HookTrigger::SubagentResult,
        WorkflowHookTrigger::BeforeCompact => HookTrigger::BeforeCompact,
        WorkflowHookTrigger::AfterCompact => HookTrigger::AfterCompact,
        WorkflowHookTrigger::BeforeProviderRequest => HookTrigger::BeforeProviderRequest,
    }
}
