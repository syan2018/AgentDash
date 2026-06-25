//! AgentRun lifecycle read-model facade.
//!
//! Lifecycle read-model projection 由 `agentdash-application-lifecycle` 拥有；
//! AgentRun 只负责把自身 repository set 投影为 Lifecycle builder 所需的 query facade。

#[allow(unused_imports)]
pub use agentdash_application_ports::lifecycle_read_model::{
    ActiveRuntimeNodeRefView, AgentRunRefView, AgentRunView, ExecutorRunRefView,
    LifecycleExecutionEntryView, LifecycleExecutionEventKindView, LifecycleReadModelQueryPort,
    LifecycleRunRefView, LifecycleRunStatusView, LifecycleRunTopologyView, LifecycleRunView,
    LifecycleSubjectAssociationView, OrchestrationInstanceView, ProjectActiveAgentsView,
    RuntimeNodeView, RuntimeSessionRefView, SubjectExecutionView, SubjectRefView,
    SubjectRuntimeAttemptView,
};
