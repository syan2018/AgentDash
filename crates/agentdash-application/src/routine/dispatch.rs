use agentdash_domain::routine::{DispatchStrategy, Routine, RoutineExecution};
use agentdash_domain::workflow::{
    AgentPolicy, CapabilityPolicy, ContextPolicy, ExecutionSource, RunPolicy, RuntimePolicy,
    SubjectExecutionIntent, SubjectRef,
};

use super::reuse_resolver::RoutineDispatchReuseTarget;

/// DispatchStrategy → dispatch policy 映射。
///
/// | DispatchStrategy | run_policy        | agent_policy |
/// |------------------|-------------------|--------------|
/// | Fresh            | CreateLinkedRun   | Create       |
/// | Reuse            | ReuseExisting     | Resume       |
/// | PerEntity        | CreateLinkedRun   | Create       |
fn map_dispatch_strategy(strategy: &DispatchStrategy) -> (RunPolicy, AgentPolicy) {
    match strategy {
        DispatchStrategy::Fresh => (RunPolicy::CreateLinkedRun, AgentPolicy::Create),
        DispatchStrategy::Reuse => (RunPolicy::ReuseExisting, AgentPolicy::Resume),
        DispatchStrategy::PerEntity { .. } => (RunPolicy::CreateLinkedRun, AgentPolicy::Create),
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
        workflow_graph_ref: None,
        run_policy,
        agent_policy,
        context_policy: ContextPolicy::Isolated,
        capability_policy: CapabilityPolicy::Baseline,
        runtime_policy: RuntimePolicy::CreateRuntimeSession,
    }
}

/// 为 Reuse / PerEntity 策略提供 resolver target 感知的 intent 构造。
///
/// 当 resolver 已找到稳定 run + agent anchor 时，使用 ReuseExisting + parent refs
/// 在同一个 LifecycleRun / LifecycleAgent 上追加执行。PerEntity 首次触发没有 target 时，
/// 这是新的 per-entity dispatch anchor，应创建新的 run + agent。
pub fn build_routine_execution_intent_with_reuse(
    routine: &Routine,
    execution: &RoutineExecution,
    reuse_target: Option<&RoutineDispatchReuseTarget>,
) -> SubjectExecutionIntent {
    let mut intent = build_routine_execution_intent(routine, execution);

    if let Some(target) = reuse_target {
        intent.parent_run_id = Some(target.run_id);
        intent.parent_agent_id = Some(target.agent_id);
        intent.run_policy = RunPolicy::ReuseExisting;
        intent.agent_policy = AgentPolicy::Resume;
    }

    intent
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::routine::{DispatchStrategy, RoutineTriggerConfig};
    use uuid::Uuid;

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
        let agent_id = Uuid::new_v4();
        let target = RoutineDispatchReuseTarget {
            run_id,
            agent_id,
            frame_id: Uuid::new_v4(),
            orchestration_id: None,
            node_path: None,
        };
        let intent = build_routine_execution_intent_with_reuse(&routine, &execution, Some(&target));

        assert_eq!(intent.run_policy, RunPolicy::ReuseExisting);
        assert_eq!(intent.agent_policy, AgentPolicy::Resume);
        assert_eq!(intent.parent_run_id, Some(run_id));
        assert_eq!(intent.parent_agent_id, Some(agent_id));
    }

    #[test]
    fn per_entity_without_reuse_target_creates_new_anchor() {
        let routine = test_routine(DispatchStrategy::PerEntity {
            entity_key_path: "issue.id".to_string(),
        });
        let execution = RoutineExecution::new(routine.id, "github:issues.opened");
        let intent = build_routine_execution_intent_with_reuse(&routine, &execution, None);

        assert_eq!(intent.run_policy, RunPolicy::CreateLinkedRun);
        assert_eq!(intent.agent_policy, AgentPolicy::Create);
        assert_eq!(intent.parent_run_id, None);
        assert_eq!(intent.parent_agent_id, None);
    }
}
