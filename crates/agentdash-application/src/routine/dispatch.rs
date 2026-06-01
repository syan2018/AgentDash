use uuid::Uuid;

use agentdash_domain::routine::{DispatchStrategy, Routine, RoutineExecution};
use agentdash_domain::workflow::{
    AgentPolicy, CapabilityPolicy, ContextPolicy, ExecutionSource, RunPolicy, RuntimePolicy,
    SubjectExecutionIntent, SubjectRef, WorkflowGraphRef,
};

use crate::workflow::freeform::FREEFORM_LIFECYCLE_KEY;

/// DispatchStrategy → dispatch policy 映射。
///
/// | DispatchStrategy | run_policy        | agent_policy |
/// |------------------|-------------------|--------------|
/// | Fresh            | CreateLinkedRun   | Create       |
/// | Reuse            | ReuseExisting     | Resume       |
/// | PerEntity        | ReuseExisting     | Resume/Create|
fn map_dispatch_strategy(strategy: &DispatchStrategy) -> (RunPolicy, AgentPolicy) {
    match strategy {
        DispatchStrategy::Fresh => (RunPolicy::CreateLinkedRun, AgentPolicy::Create),
        DispatchStrategy::Reuse => (RunPolicy::ReuseExisting, AgentPolicy::Resume),
        DispatchStrategy::PerEntity { .. } => (RunPolicy::ReuseExisting, AgentPolicy::Resume),
    }
}

/// 从 Routine + RoutineExecution 构造 `SubjectExecutionIntent`。
///
/// prompt 通过上层 frame builder 注入，
/// 此处只负责 policy 映射和 subject ref 构造。
pub fn build_routine_execution_intent(
    routine: &Routine,
    execution: &RoutineExecution,
) -> SubjectExecutionIntent {
    let (run_policy, agent_policy) = map_dispatch_strategy(&routine.dispatch_strategy);

    SubjectExecutionIntent {
        project_id: routine.project_id,
        source: ExecutionSource::Routine,
        subject_ref: SubjectRef::new("routine_execution", execution.id),
        parent_run_id: None,
        parent_agent_id: None,
        workflow_graph_ref: WorkflowGraphRef::ByKey {
            project_id: routine.project_id,
            key: FREEFORM_LIFECYCLE_KEY.to_string(),
        },
        agent_procedure_ref: None,
        run_policy,
        agent_policy,
        context_policy: ContextPolicy::Isolated,
        capability_policy: CapabilityPolicy::Baseline,
        runtime_policy: RuntimePolicy::CreateRuntimeSession,
    }
}

/// 为 PerEntity 策略提供 entity_key 感知的 intent 构造。
///
/// 当 entity_key 已解析且存在关联的 run_id 时，使用 ReuseExisting + parent_run_id
/// 在同一个 LifecycleRun 内追加执行。
pub fn build_routine_execution_intent_with_reuse(
    routine: &Routine,
    execution: &RoutineExecution,
    reuse_run_id: Option<Uuid>,
) -> SubjectExecutionIntent {
    let mut intent = build_routine_execution_intent(routine, execution);

    if let Some(run_id) = reuse_run_id {
        intent.parent_run_id = Some(run_id);
        intent.run_policy = RunPolicy::ReuseExisting;
        intent.agent_policy = AgentPolicy::Resume;
    }

    intent
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::routine::{DispatchStrategy, RoutineTriggerConfig};

    fn test_routine(strategy: DispatchStrategy) -> Routine {
        Routine::new(
            Uuid::new_v4(),
            "test-routine",
            "test prompt {{trigger_source}}",
            Uuid::new_v4(),
            RoutineTriggerConfig::Scheduled {
                cron_expression: "0 * * * *".to_string(),
                timezone: None,
            },
            strategy,
        )
    }

    #[test]
    fn fresh_strategy_maps_to_create_linked_run() {
        let routine = test_routine(DispatchStrategy::Fresh);
        let execution = RoutineExecution::new(routine.id, "scheduled");
        let intent = build_routine_execution_intent(&routine, &execution);

        assert_eq!(intent.run_policy, RunPolicy::CreateLinkedRun);
        assert_eq!(intent.agent_policy, AgentPolicy::Create);
        assert_eq!(intent.source, ExecutionSource::Routine);
        assert_eq!(intent.subject_ref.kind, "routine_execution");
        assert_eq!(intent.subject_ref.id, execution.id);
    }

    #[test]
    fn reuse_strategy_maps_to_reuse_existing() {
        let routine = test_routine(DispatchStrategy::Reuse);
        let execution = RoutineExecution::new(routine.id, "webhook");
        let intent = build_routine_execution_intent(&routine, &execution);

        assert_eq!(intent.run_policy, RunPolicy::ReuseExisting);
        assert_eq!(intent.agent_policy, AgentPolicy::Resume);
    }

    #[test]
    fn per_entity_with_reuse_run_id_overrides_policy() {
        let routine = test_routine(DispatchStrategy::PerEntity {
            entity_key_path: "issue.id".to_string(),
        });
        let execution = RoutineExecution::new(routine.id, "github:issues.opened");
        let run_id = Uuid::new_v4();
        let intent = build_routine_execution_intent_with_reuse(&routine, &execution, Some(run_id));

        assert_eq!(intent.run_policy, RunPolicy::ReuseExisting);
        assert_eq!(intent.agent_policy, AgentPolicy::Resume);
        assert_eq!(intent.parent_run_id, Some(run_id));
    }
}
