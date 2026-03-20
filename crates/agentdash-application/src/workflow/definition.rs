use agentdash_domain::workflow::{
    WorkflowContextBinding, WorkflowDefinition, WorkflowPhaseCompletionMode,
    WorkflowPhaseDefinition, WorkflowTargetKind,
};

pub const TRELLIS_DEV_WORKFLOW_KEY: &str = "trellis_dev_workflow";

pub fn build_trellis_dev_workflow_definition(
    target_kind: WorkflowTargetKind,
) -> Result<WorkflowDefinition, String> {
    WorkflowDefinition::new(
        TRELLIS_DEV_WORKFLOW_KEY,
        "Trellis Dev Workflow",
        "把 Trellis 研发流程提炼为平台内可复用 workflow，覆盖 Start / Implement / Check / Record 四阶段。",
        target_kind,
        vec![
            WorkflowPhaseDefinition {
                key: "start".to_string(),
                title: "Start".to_string(),
                description: "识别目标对象、读取 workflow / PRD / spec，并生成初始上下文集。"
                    .to_string(),
                context_bindings: vec![
                    binding(".trellis/workflow.md", "workflow 总规则"),
                    binding(".trellis/spec/backend/index.md", "后端开发规范入口"),
                ],
                requires_session: false,
                completion_mode: WorkflowPhaseCompletionMode::Manual,
            },
            WorkflowPhaseDefinition {
                key: "implement".to_string(),
                title: "Implement".to_string(),
                description: "绑定 implement context，进入执行会话并推进开发。".to_string(),
                context_bindings: vec![binding(
                    "implement.jsonl",
                    "phase-specific implement context",
                )],
                requires_session: true,
                completion_mode: WorkflowPhaseCompletionMode::SessionEnded,
            },
            WorkflowPhaseDefinition {
                key: "check".to_string(),
                title: "Check".to_string(),
                description: "绑定 check context，执行 review、checklist 与质量确认。".to_string(),
                context_bindings: vec![binding("check.jsonl", "phase-specific check context")],
                requires_session: true,
                completion_mode: WorkflowPhaseCompletionMode::ChecklistPassed,
            },
            WorkflowPhaseDefinition {
                key: "record".to_string(),
                title: "Record".to_string(),
                description: "生成 session summary、journal suggestion 与 archive suggestion。"
                    .to_string(),
                context_bindings: vec![
                    binding(
                        ".trellis/workspace/<developer>/journal-*.md",
                        "历史记录沉淀位置",
                    ),
                    binding("task.py archive", "归档建议与归档动作"),
                ],
                requires_session: false,
                completion_mode: WorkflowPhaseCompletionMode::Manual,
            },
        ],
    )
}

fn binding(path: &str, reason: &str) -> WorkflowContextBinding {
    WorkflowContextBinding {
        path: path.to_string(),
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trellis_workflow_definition_contains_four_core_phases() {
        let workflow = build_trellis_dev_workflow_definition(WorkflowTargetKind::Story)
            .expect("build workflow");

        assert_eq!(workflow.key, TRELLIS_DEV_WORKFLOW_KEY);
        assert_eq!(workflow.phases.len(), 4);
        assert_eq!(workflow.phases[0].key, "start");
        assert_eq!(workflow.phases[1].key, "implement");
        assert_eq!(workflow.phases[2].key, "check");
        assert_eq!(workflow.phases[3].key, "record");
    }
}
