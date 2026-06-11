use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::workflow::{
    ActivityDefinition, ActivityExecutorSpec, ActivityJoinPolicy, ActivityTransition,
    ActivityTransitionKind, AgentProcedureExecutionSpec, ExecutorSpec,
    FunctionActivityExecutorSpec, OrchestrationLimits, OrchestrationPlanSnapshot,
    OrchestrationSourceRef, PlanNode, PlanNodeKind, StateExchangeRule, TransitionCondition,
    ValidationSeverity, WorkflowGraph, validate_workflow_graph,
};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub const WORKFLOW_GRAPH_COMPILER_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowGraphCompileMode {
    Strict,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowGraphCompileSourceMetadata {
    pub source_ref: OrchestrationSourceRef,
    pub source_path: String,
}

impl WorkflowGraphCompileSourceMetadata {
    pub fn from_graph(graph: &WorkflowGraph) -> Self {
        Self {
            source_ref: OrchestrationSourceRef::WorkflowGraph {
                graph_id: graph.id,
                graph_version: Some(graph.version),
            },
            source_path: format!("workflow_graphs[{}]", graph.id),
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowGraphCompileInput<'a> {
    pub graph: &'a WorkflowGraph,
    pub source_metadata: WorkflowGraphCompileSourceMetadata,
    pub compile_mode: WorkflowGraphCompileMode,
    pub target_schema_version: u32,
}

impl<'a> WorkflowGraphCompileInput<'a> {
    pub fn strict(graph: &'a WorkflowGraph) -> Self {
        Self {
            graph,
            source_metadata: WorkflowGraphCompileSourceMetadata::from_graph(graph),
            compile_mode: WorkflowGraphCompileMode::Strict,
            target_schema_version: WORKFLOW_GRAPH_COMPILER_SCHEMA_VERSION,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowGraphCompileDiagnostic {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
    pub source_path: String,
    pub related_paths: Vec<String>,
}

impl WorkflowGraphCompileDiagnostic {
    fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        source_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: ValidationSeverity::Error,
            message: message.into(),
            source_path: source_path.into(),
            related_paths: Vec::new(),
        }
    }

    fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        source_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: ValidationSeverity::Warning,
            message: message.into(),
            source_path: source_path.into(),
            related_paths: Vec::new(),
        }
    }

    fn with_related_paths(mut self, related_paths: Vec<String>) -> Self {
        self.related_paths = related_paths;
        self
    }

    pub fn is_blocking(&self) -> bool {
        self.severity == ValidationSeverity::Error
    }
}

#[derive(Debug, Clone)]
pub struct WorkflowGraphCompileOutput {
    pub plan_snapshot: OrchestrationPlanSnapshot,
    pub diagnostics: Vec<WorkflowGraphCompileDiagnostic>,
}

impl WorkflowGraphCompileOutput {
    pub fn has_blocking_diagnostics(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|diagnostic| diagnostic.is_blocking())
    }
}

pub struct WorkflowGraphCompiler;

impl WorkflowGraphCompiler {
    pub fn compile(input: WorkflowGraphCompileInput<'_>) -> WorkflowGraphCompileOutput {
        compile_workflow_graph(input)
    }
}

pub fn compile_workflow_graph(input: WorkflowGraphCompileInput<'_>) -> WorkflowGraphCompileOutput {
    let mut compiler = Compiler::new(input);
    compiler.compile()
}

struct Compiler<'a> {
    input: WorkflowGraphCompileInput<'a>,
    diagnostics: Vec<WorkflowGraphCompileDiagnostic>,
}

#[derive(Debug, Clone, Copy)]
struct ActivityLookup<'a> {
    index: usize,
    activity: &'a ActivityDefinition,
}

impl<'a> Compiler<'a> {
    fn new(input: WorkflowGraphCompileInput<'a>) -> Self {
        Self {
            input,
            diagnostics: Vec::new(),
        }
    }

    fn compile(&mut self) -> WorkflowGraphCompileOutput {
        let graph = self.input.graph;
        if self.input.target_schema_version != WORKFLOW_GRAPH_COMPILER_SCHEMA_VERSION {
            self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                "unsupported_plan_schema_version",
                format!(
                    "WorkflowGraph compiler supports plan schema version {}, got {}",
                    WORKFLOW_GRAPH_COMPILER_SCHEMA_VERSION, self.input.target_schema_version
                ),
                "compiler.target_schema_version",
            ));
        }

        if let Err(error) = validate_workflow_graph(
            &graph.key,
            &graph.name,
            &graph.entry_activity_key,
            &graph.activities,
            &graph.transitions,
        ) {
            self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                "invalid_workflow_graph",
                format!("WorkflowGraph validation failed: {error}"),
                "graph",
            ));
        }

        let activity_lookup = self.build_activity_lookup(graph);
        if !activity_lookup.contains_key(&graph.entry_activity_key) {
            self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                "entry_activity_missing",
                format!(
                    "Entry activity `{}` does not resolve to an activity",
                    graph.entry_activity_key
                ),
                "entry_activity_key",
            ));
        }

        let mut nodes = self.compile_nodes(&activity_lookup);
        nodes.sort_by(|left, right| left.node_id.cmp(&right.node_id));

        let incoming_counts = incoming_counts(graph, &activity_lookup);
        self.validate_join_policy_reachability(&activity_lookup, &incoming_counts);

        let mut activation_rules = Vec::new();
        let mut state_exchange_rules = Vec::new();
        if activity_lookup.contains_key(&graph.entry_activity_key) {
            activation_rules.push(agentdash_domain::workflow::ActivationRule::Entry {
                node_id: graph.entry_activity_key.clone(),
            });
        }
        self.compile_transitions(
            graph,
            &activity_lookup,
            &mut activation_rules,
            &mut state_exchange_rules,
        );
        self.compile_node_activation_policies(
            &activity_lookup,
            &incoming_counts,
            &mut activation_rules,
        );
        self.detect_unbounded_cycles(graph, &activity_lookup);

        activation_rules.sort_by_key(activation_rule_sort_key);
        state_exchange_rules.sort_by(|left, right| left.rule_id.cmp(&right.rule_id));

        let entry_node_ids = if activity_lookup.contains_key(&graph.entry_activity_key) {
            vec![graph.entry_activity_key.clone()]
        } else {
            Vec::new()
        };
        let limits = OrchestrationLimits::default();
        let metadata = Some(plan_metadata(
            graph,
            &self.input.source_metadata,
            self.input.target_schema_version,
        ));
        let plan_digest = plan_digest(PlanDigestContent {
            plan_version: self.input.target_schema_version,
            source_ref: &self.input.source_metadata.source_ref,
            nodes: &nodes,
            entry_node_ids: &entry_node_ids,
            activation_rules: &activation_rules,
            state_exchange_rules: &state_exchange_rules,
            limits: &limits,
            metadata: &metadata,
        });

        WorkflowGraphCompileOutput {
            plan_snapshot: OrchestrationPlanSnapshot {
                plan_digest,
                plan_version: self.input.target_schema_version,
                source_ref: self.input.source_metadata.source_ref.clone(),
                nodes,
                entry_node_ids,
                activation_rules,
                state_exchange_rules,
                limits,
                metadata,
                created_at: graph.updated_at,
            },
            diagnostics: std::mem::take(&mut self.diagnostics),
        }
    }

    fn build_activity_lookup(
        &mut self,
        graph: &'a WorkflowGraph,
    ) -> BTreeMap<String, ActivityLookup<'a>> {
        let mut lookup: BTreeMap<String, ActivityLookup<'a>> = BTreeMap::new();
        for (index, activity) in graph.activities.iter().enumerate() {
            let source_path = activity_source_path(index);
            if let Some(first) = lookup.get(&activity.key) {
                self.diagnostics.push(
                    WorkflowGraphCompileDiagnostic::error(
                        "duplicate_node_id",
                        format!(
                            "Activity key `{}` canonicalizes to a duplicate plan node id",
                            activity.key
                        ),
                        format!("{source_path}.key"),
                    )
                    .with_related_paths(vec![format!("{}.key", activity_source_path(first.index))]),
                );
                continue;
            }
            lookup.insert(activity.key.clone(), ActivityLookup { index, activity });
        }
        lookup
    }

    fn compile_nodes(&mut self, lookup: &BTreeMap<String, ActivityLookup<'a>>) -> Vec<PlanNode> {
        lookup
            .values()
            .map(|activity_ref| {
                let activity = activity_ref.activity;
                let source_path = activity_source_path(activity_ref.index);
                let (kind, executor) =
                    self.compile_executor(activity, &format!("{source_path}.executor"));
                PlanNode {
                    node_id: activity.key.clone(),
                    node_path: activity.key.clone(),
                    parent_node_id: None,
                    kind,
                    label: Some(activity.key.clone()),
                    executor: Some(executor),
                    input_ports: activity.input_ports.clone(),
                    output_ports: activity.output_ports.clone(),
                    completion_policy: Some(activity.completion_policy.clone()),
                    iteration_policy: Some(activity.iteration_policy.clone()),
                    join_policy: Some(activity.join_policy),
                    result_contract: Some(json!({
                        "completion_policy": &activity.completion_policy,
                        "output_ports": &activity.output_ports,
                    })),
                    metadata: Some(json!({
                        "source_activity_key": activity.key,
                        "source_path": source_path,
                        "description": activity.description,
                        "source_executor_kind": activity.executor.kind(),
                    })),
                }
            })
            .collect()
    }

    fn compile_executor(
        &mut self,
        activity: &ActivityDefinition,
        source_path: &str,
    ) -> (PlanNodeKind, ExecutorSpec) {
        match &activity.executor {
            ActivityExecutorSpec::Agent(spec) => {
                if !spec.creates_activity_agent() && !spec.continues_current_agent() {
                    self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                        "unsupported_agent_executor_policy",
                        format!(
                            "Agent activity `{}` uses unsupported policy pair `{:?}` + `{:?}`",
                            activity.key, spec.agent_reuse_policy, spec.runtime_session_policy
                        ),
                        source_path,
                    ));
                }
                (
                    PlanNodeKind::AgentCall,
                    ExecutorSpec::AgentProcedure {
                        procedure: AgentProcedureExecutionSpec::by_key(spec.procedure_key.clone()),
                        agent_reuse_policy: spec.agent_reuse_policy,
                        runtime_session_policy: spec.runtime_session_policy,
                    },
                )
            }
            ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::ApiRequest(spec)) => (
                PlanNodeKind::Function,
                ExecutorSpec::Function {
                    spec: FunctionActivityExecutorSpec::ApiRequest(spec.clone()),
                },
            ),
            ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::BashExec(spec)) => (
                PlanNodeKind::LocalEffect,
                ExecutorSpec::Function {
                    spec: FunctionActivityExecutorSpec::BashExec(spec.clone()),
                },
            ),
            ActivityExecutorSpec::Human(spec) => (
                PlanNodeKind::HumanGate,
                ExecutorSpec::Human { spec: spec.clone() },
            ),
        }
    }

    fn compile_transitions(
        &mut self,
        graph: &WorkflowGraph,
        activity_lookup: &BTreeMap<String, ActivityLookup<'a>>,
        activation_rules: &mut Vec<agentdash_domain::workflow::ActivationRule>,
        state_exchange_rules: &mut Vec<StateExchangeRule>,
    ) {
        for (index, transition) in graph.transitions.iter().enumerate() {
            let source_path = transition_source_path(index);
            let from_lookup = activity_lookup.get(&transition.from);
            let to_lookup = activity_lookup.get(&transition.to);

            if from_lookup.is_none() {
                self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                    "dangling_transition_source",
                    format!(
                        "Transition source activity `{}` does not exist",
                        transition.from
                    ),
                    format!("{source_path}.from"),
                ));
            }
            if to_lookup.is_none() {
                self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                    "dangling_transition_target",
                    format!(
                        "Transition target activity `{}` does not exist",
                        transition.to
                    ),
                    format!("{source_path}.to"),
                ));
            }

            self.validate_transition_condition(
                &transition.condition,
                &format!("{source_path}.condition"),
                activity_lookup,
            );

            if transition.kind == ActivityTransitionKind::Artifact
                && transition.artifact_bindings.is_empty()
            {
                self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                    "artifact_edge_missing_state_exchange",
                    "Artifact transition must declare at least one state exchange binding",
                    &source_path,
                ));
            }

            if let Some(target) = to_lookup {
                if from_lookup.is_some() {
                    activation_rules.push(agentdash_domain::workflow::ActivationRule::Transition {
                        rule_id: transition_rule_id(index, transition),
                        from_node_id: transition.from.clone(),
                        to_node_id: transition.to.clone(),
                        condition: transition.condition.clone(),
                        join_policy: target.activity.join_policy,
                        max_traversals: transition.max_traversals,
                        source_path: Some(source_path.clone()),
                    });
                }
            }

            self.compile_artifact_bindings(
                index,
                transition,
                &source_path,
                activity_lookup,
                state_exchange_rules,
            );
        }
    }

    fn validate_transition_condition(
        &mut self,
        condition: &TransitionCondition,
        source_path: &str,
        activity_lookup: &BTreeMap<String, ActivityLookup<'a>>,
    ) {
        match condition {
            TransitionCondition::Always => {}
            TransitionCondition::ArtifactFieldEquals {
                activity,
                port,
                path,
                value: _,
            } => {
                if path.trim().is_empty() {
                    self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                        "dangling_condition_ref",
                        "Artifact condition path is empty",
                        format!("{source_path}.path"),
                    ));
                }
                self.validate_output_port_ref(
                    activity,
                    port,
                    "dangling_condition_ref",
                    source_path,
                );
            }
            TransitionCondition::HumanDecisionEquals {
                activity,
                decision_port,
                value: _,
            } => {
                self.validate_output_port_ref(
                    activity,
                    decision_port,
                    "dangling_condition_ref",
                    source_path,
                );
            }
            TransitionCondition::AgentSignalEquals {
                activity,
                signal_key,
                value: _,
            } => {
                if !activity_lookup.contains_key(activity) {
                    self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                        "dangling_condition_ref",
                        format!("Condition references missing activity `{activity}`"),
                        format!("{source_path}.activity"),
                    ));
                }
                if signal_key.trim().is_empty() {
                    self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                        "dangling_condition_ref",
                        "Agent signal condition references an empty signal key",
                        format!("{source_path}.signal_key"),
                    ));
                }
            }
        }
    }

    fn validate_output_port_ref(
        &mut self,
        activity_key: &str,
        port_key: &str,
        code: &str,
        source_path: &str,
    ) {
        let Some(activity_ref) = self.activity_lookup(activity_key) else {
            self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                code,
                format!("Reference points to missing activity `{activity_key}`"),
                format!("{source_path}.activity"),
            ));
            return;
        };
        if !activity_ref
            .activity
            .output_ports
            .iter()
            .any(|port| port.key == port_key)
        {
            self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                code,
                format!("Reference points to missing output port `{activity_key}.{port_key}`"),
                format!("{source_path}.port"),
            ));
        }
    }

    fn activity_lookup(&self, activity_key: &str) -> Option<ActivityLookup<'a>> {
        self.input
            .graph
            .activities
            .iter()
            .enumerate()
            .find(|(_, activity)| activity.key == activity_key)
            .map(|(index, activity)| ActivityLookup { index, activity })
    }

    fn compile_artifact_bindings(
        &mut self,
        transition_index: usize,
        transition: &ActivityTransition,
        transition_path: &str,
        activity_lookup: &BTreeMap<String, ActivityLookup<'a>>,
        state_exchange_rules: &mut Vec<StateExchangeRule>,
    ) {
        for (binding_index, binding) in transition.artifact_bindings.iter().enumerate() {
            let binding_path = format!("{transition_path}.artifact_bindings[{binding_index}]");
            let from_activity_key = binding
                .from_activity
                .as_deref()
                .unwrap_or(transition.from.as_str());
            let source = activity_lookup.get(from_activity_key);
            let target = activity_lookup.get(&transition.to);
            let mut valid = true;

            match source {
                Some(source) => {
                    if !source
                        .activity
                        .output_ports
                        .iter()
                        .any(|port| port.key == binding.from_port)
                    {
                        valid = false;
                        self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                            "dangling_artifact_binding_ref",
                            format!(
                                "Artifact binding references missing output port `{}.{}`",
                                from_activity_key, binding.from_port
                            ),
                            format!("{binding_path}.from_port"),
                        ));
                    }
                }
                None => {
                    valid = false;
                    self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                        "dangling_artifact_binding_ref",
                        format!(
                            "Artifact binding references missing source activity `{from_activity_key}`"
                        ),
                        format!("{binding_path}.from_activity"),
                    ));
                }
            }

            match target {
                Some(target) => {
                    if !target
                        .activity
                        .input_ports
                        .iter()
                        .any(|port| port.key == binding.to_port)
                    {
                        valid = false;
                        self.diagnostics.push(WorkflowGraphCompileDiagnostic::error(
                            "dangling_artifact_binding_ref",
                            format!(
                                "Artifact binding references missing input port `{}.{}`",
                                transition.to, binding.to_port
                            ),
                            format!("{binding_path}.to_port"),
                        ));
                    }
                }
                None => {
                    valid = false;
                }
            }

            if valid {
                state_exchange_rules.push(StateExchangeRule {
                    rule_id: artifact_rule_id(transition_index, binding_index, transition),
                    from_node_id: from_activity_key.to_string(),
                    from_port: binding.from_port.clone(),
                    to_node_id: transition.to.clone(),
                    to_port: binding.to_port.clone(),
                    alias: binding.alias,
                    source_transition_id: Some(transition_rule_id(transition_index, transition)),
                    source_path: Some(binding_path),
                });
            }
        }
    }

    fn compile_node_activation_policies(
        &self,
        activity_lookup: &BTreeMap<String, ActivityLookup<'a>>,
        incoming_counts: &BTreeMap<String, usize>,
        activation_rules: &mut Vec<agentdash_domain::workflow::ActivationRule>,
    ) {
        for activity_ref in activity_lookup.values() {
            let activity = activity_ref.activity;
            activation_rules.push(agentdash_domain::workflow::ActivationRule::Retry {
                node_id: activity.key.clone(),
                max_attempts: activity.iteration_policy.max_attempts,
            });

            if incoming_counts
                .get(&activity.key)
                .copied()
                .unwrap_or_default()
                > 0
            {
                activation_rules.push(agentdash_domain::workflow::ActivationRule::Join {
                    node_id: activity.key.clone(),
                    policy: join_policy_label(activity.join_policy),
                });
            }
        }
    }

    fn validate_join_policy_reachability(
        &mut self,
        activity_lookup: &BTreeMap<String, ActivityLookup<'a>>,
        incoming_counts: &BTreeMap<String, usize>,
    ) {
        for activity_ref in activity_lookup.values() {
            if let ActivityJoinPolicy::NOfM { n } = activity_ref.activity.join_policy {
                let incoming = incoming_counts
                    .get(&activity_ref.activity.key)
                    .copied()
                    .unwrap_or_default();
                if n as usize > incoming {
                    self.diagnostics.push(WorkflowGraphCompileDiagnostic::warning(
                        "n_of_m_exceeds_incoming_count",
                        format!(
                            "Activity `{}` requires {n} incoming transitions but only {incoming} exist",
                            activity_ref.activity.key
                        ),
                        format!("{}.join_policy", activity_source_path(activity_ref.index)),
                    ));
                }
            }
        }
    }

    fn detect_unbounded_cycles(
        &mut self,
        graph: &WorkflowGraph,
        activity_lookup: &BTreeMap<String, ActivityLookup<'a>>,
    ) {
        let mut adjacency: BTreeMap<String, Vec<(usize, String)>> = BTreeMap::new();
        for (index, transition) in graph.transitions.iter().enumerate() {
            if activity_lookup.contains_key(&transition.from)
                && activity_lookup.contains_key(&transition.to)
            {
                adjacency
                    .entry(transition.from.clone())
                    .or_default()
                    .push((index, transition.to.clone()));
            }
        }

        let mut seen_cycles = BTreeSet::new();
        for start in activity_lookup.keys() {
            let mut path_nodes = vec![start.clone()];
            let mut path_edges = Vec::new();
            self.walk_cycles(
                graph,
                activity_lookup,
                &adjacency,
                &mut seen_cycles,
                &mut path_nodes,
                &mut path_edges,
            );
        }
    }

    fn walk_cycles(
        &mut self,
        graph: &WorkflowGraph,
        activity_lookup: &BTreeMap<String, ActivityLookup<'a>>,
        adjacency: &BTreeMap<String, Vec<(usize, String)>>,
        seen_cycles: &mut BTreeSet<String>,
        path_nodes: &mut Vec<String>,
        path_edges: &mut Vec<usize>,
    ) {
        if path_edges.len() > graph.transitions.len() {
            return;
        }
        let Some(current) = path_nodes.last().cloned() else {
            return;
        };
        let Some(edges) = adjacency.get(&current) else {
            return;
        };

        for (edge_index, next_node) in edges {
            if let Some(position) = path_nodes.iter().position(|node| node == next_node) {
                let mut cycle_edges = path_edges[position..].to_vec();
                cycle_edges.push(*edge_index);
                let mut key_edges = cycle_edges.clone();
                key_edges.sort_unstable();
                let cycle_key = key_edges
                    .iter()
                    .map(|index| index.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                if seen_cycles.insert(cycle_key)
                    && !cycle_edges.iter().any(|index| {
                        transition_is_bounded(&graph.transitions[*index], activity_lookup)
                    })
                {
                    let first_edge = cycle_edges[0];
                    let related_paths = cycle_edges
                        .iter()
                        .map(|index| transition_source_path(*index))
                        .collect::<Vec<_>>();
                    self.diagnostics.push(
                        WorkflowGraphCompileDiagnostic::error(
                            "unbounded_cycle",
                            "Cycle has no max_attempts, max_traversals, or structured condition bound",
                            transition_source_path(first_edge),
                        )
                        .with_related_paths(related_paths),
                    );
                }
                continue;
            }

            path_nodes.push(next_node.clone());
            path_edges.push(*edge_index);
            self.walk_cycles(
                graph,
                activity_lookup,
                adjacency,
                seen_cycles,
                path_nodes,
                path_edges,
            );
            path_edges.pop();
            path_nodes.pop();
        }
    }
}

#[derive(Serialize)]
struct PlanDigestContent<'a> {
    plan_version: u32,
    source_ref: &'a OrchestrationSourceRef,
    nodes: &'a [PlanNode],
    entry_node_ids: &'a [String],
    activation_rules: &'a [agentdash_domain::workflow::ActivationRule],
    state_exchange_rules: &'a [StateExchangeRule],
    limits: &'a OrchestrationLimits,
    metadata: &'a Option<Value>,
}

fn plan_digest(content: PlanDigestContent<'_>) -> String {
    let bytes = serde_json::to_vec(&content).expect("plan digest content should serialize");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn plan_metadata(
    graph: &WorkflowGraph,
    source_metadata: &WorkflowGraphCompileSourceMetadata,
    schema_version: u32,
) -> Value {
    json!({
        "compiler": {
            "name": "workflow_graph_compiler",
            "schema_version": schema_version,
        },
        "source": {
            "project_id": graph.project_id,
            "graph_id": graph.id,
            "key": graph.key,
            "name": graph.name,
            "version": graph.version,
            "source_path": source_metadata.source_path,
        },
    })
}

fn incoming_counts(
    graph: &WorkflowGraph,
    activity_lookup: &BTreeMap<String, ActivityLookup<'_>>,
) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for transition in &graph.transitions {
        if activity_lookup.contains_key(&transition.from)
            && activity_lookup.contains_key(&transition.to)
        {
            *counts.entry(transition.to.clone()).or_insert(0) += 1;
        }
    }
    counts
}

fn transition_is_bounded(
    transition: &ActivityTransition,
    activity_lookup: &BTreeMap<String, ActivityLookup<'_>>,
) -> bool {
    transition.max_traversals.is_some()
        || !matches!(transition.condition, TransitionCondition::Always)
        || activity_lookup
            .get(&transition.to)
            .and_then(|activity| activity.activity.iteration_policy.max_attempts)
            .is_some()
}

fn join_policy_label(policy: ActivityJoinPolicy) -> String {
    match policy {
        ActivityJoinPolicy::All => "all".to_string(),
        ActivityJoinPolicy::Any => "any".to_string(),
        ActivityJoinPolicy::First => "first".to_string(),
        ActivityJoinPolicy::NOfM { n } => format!("n_of_m:{n}"),
    }
}

fn activity_source_path(index: usize) -> String {
    format!("activities[{index}]")
}

fn transition_source_path(index: usize) -> String {
    format!("transitions[{index}]")
}

fn transition_rule_id(index: usize, transition: &ActivityTransition) -> String {
    format!("transition:{index}:{}->{}", transition.from, transition.to)
}

fn artifact_rule_id(
    transition_index: usize,
    binding_index: usize,
    transition: &ActivityTransition,
) -> String {
    format!(
        "artifact:{transition_index}:{binding_index}:{}->{}",
        transition.from, transition.to
    )
}

fn activation_rule_sort_key(rule: &agentdash_domain::workflow::ActivationRule) -> String {
    match rule {
        agentdash_domain::workflow::ActivationRule::Entry { node_id } => {
            format!("00:entry:{node_id}")
        }
        agentdash_domain::workflow::ActivationRule::Transition {
            rule_id,
            from_node_id,
            to_node_id,
            ..
        } => format!("10:transition:{rule_id}:{from_node_id}:{to_node_id}"),
        agentdash_domain::workflow::ActivationRule::Dependency {
            node_id,
            depends_on_node_ids,
        } => format!("20:dependency:{node_id}:{depends_on_node_ids:?}"),
        agentdash_domain::workflow::ActivationRule::Condition { node_id, .. } => {
            format!("30:condition:{node_id}")
        }
        agentdash_domain::workflow::ActivationRule::ArtifactBinding {
            from_node_id,
            from_port,
            to_node_id,
            to_port,
        } => format!("40:artifact:{from_node_id}:{from_port}:{to_node_id}:{to_port}"),
        agentdash_domain::workflow::ActivationRule::Join { node_id, policy } => {
            format!("50:join:{node_id}:{policy}")
        }
        agentdash_domain::workflow::ActivationRule::Retry {
            node_id,
            max_attempts,
        } => format!("60:retry:{node_id}:{max_attempts:?}"),
        agentdash_domain::workflow::ActivationRule::Iteration {
            node_id,
            max_traversals,
        } => format!("70:iteration:{node_id}:{max_traversals:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::{
        ActivityCompletionPolicy, ActivityIterationPolicy, AgentActivityExecutorSpec,
        AgentReusePolicy, ApiRequestExecutorSpec, ArtifactAliasPolicy, BashExecExecutorSpec,
        ContextStrategy, DefinitionSource, GateStrategy, HumanActivityExecutorSpec,
        HumanApprovalExecutorSpec, InputPortDefinition, OutputPortDefinition, RuntimeSessionPolicy,
        StandaloneFulfillment,
    };
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    fn fixed_time() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 6, 0, 0, 0)
            .single()
            .expect("fixed test time")
    }

    fn input_port(key: &str) -> InputPortDefinition {
        InputPortDefinition {
            key: key.to_string(),
            description: format!("{key} input"),
            context_strategy: ContextStrategy::Full,
            context_template: None,
            standalone_fulfillment: StandaloneFulfillment::Required,
        }
    }

    fn output_port(key: &str) -> OutputPortDefinition {
        OutputPortDefinition {
            key: key.to_string(),
            description: format!("{key} output"),
            gate_strategy: GateStrategy::Existence,
            gate_params: None,
        }
    }

    fn agent_activity(key: &str) -> ActivityDefinition {
        ActivityDefinition {
            key: key.to_string(),
            description: String::new(),
            executor: ActivityExecutorSpec::Agent(
                AgentActivityExecutorSpec::create_activity_agent(format!("workflow.{key}")),
            ),
            input_ports: vec![input_port("input")],
            output_ports: vec![output_port("output")],
            completion_policy: ActivityCompletionPolicy::ExecutorTerminal,
            iteration_policy: ActivityIterationPolicy::default(),
            join_policy: ActivityJoinPolicy::All,
        }
    }

    fn graph(
        entry_activity_key: &str,
        activities: Vec<ActivityDefinition>,
        transitions: Vec<ActivityTransition>,
    ) -> WorkflowGraph {
        let now = fixed_time();
        WorkflowGraph {
            id: Uuid::from_u128(1),
            project_id: Uuid::from_u128(2),
            key: "workflow_graph_compiler_fixture".to_string(),
            name: "Workflow Graph Compiler Fixture".to_string(),
            description: String::new(),
            source: DefinitionSource::UserAuthored,
            installed_source: None,
            version: 7,
            entry_activity_key: entry_activity_key.to_string(),
            activities,
            transitions,
            created_at: now,
            updated_at: now,
        }
    }

    fn transition(from: &str, to: &str) -> ActivityTransition {
        ActivityTransition {
            from: from.to_string(),
            to: to.to_string(),
            kind: ActivityTransitionKind::Flow,
            condition: TransitionCondition::Always,
            artifact_bindings: Vec::new(),
            max_traversals: None,
        }
    }

    fn compile(graph: &WorkflowGraph) -> WorkflowGraphCompileOutput {
        WorkflowGraphCompiler::compile(WorkflowGraphCompileInput::strict(graph))
    }

    fn diagnostic_codes(output: &WorkflowGraphCompileOutput) -> Vec<&str> {
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect()
    }

    #[test]
    fn workflow_graph_compiler_maps_executor_semantic_node_kinds() {
        let mut api = agent_activity("api");
        api.executor = ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::ApiRequest(
            ApiRequestExecutorSpec {
                method: "POST".to_string(),
                url_template: "https://example.test".to_string(),
                body_template: Some(json!({"ok": true})),
            },
        ));
        let mut bash = agent_activity("bash");
        bash.executor = ActivityExecutorSpec::Function(FunctionActivityExecutorSpec::BashExec(
            BashExecExecutorSpec {
                command: "pnpm".to_string(),
                args: vec!["test".to_string()],
                working_directory: Some(".".to_string()),
            },
        ));
        let mut human = agent_activity("human");
        human.executor = ActivityExecutorSpec::Human(HumanActivityExecutorSpec::Approval(
            HumanApprovalExecutorSpec {
                form_schema_key: "approval.plan_review".to_string(),
                title: Some("Review".to_string()),
            },
        ));
        human.completion_policy = ActivityCompletionPolicy::HumanDecision {
            decision_port: "output".to_string(),
        };

        let graph = graph(
            "agent",
            vec![agent_activity("agent"), api, bash, human],
            vec![
                transition("agent", "api"),
                transition("api", "bash"),
                transition("bash", "human"),
            ],
        );

        let output = compile(&graph);
        assert!(
            !output.has_blocking_diagnostics(),
            "{:?}",
            output.diagnostics
        );
        let kinds = output
            .plan_snapshot
            .nodes
            .iter()
            .map(|node| (node.node_id.as_str(), node.kind))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(kinds["agent"], PlanNodeKind::AgentCall);
        assert_eq!(kinds["api"], PlanNodeKind::Function);
        assert_eq!(kinds["bash"], PlanNodeKind::LocalEffect);
        assert_eq!(kinds["human"], PlanNodeKind::HumanGate);

        let bash_node = output
            .plan_snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == "bash")
            .expect("bash node");
        assert!(matches!(
            bash_node.executor,
            Some(ExecutorSpec::Function {
                spec: FunctionActivityExecutorSpec::BashExec(_)
            })
        ));
    }

    #[test]
    fn workflow_graph_compiler_preserves_conditions_on_transition_rules() {
        let mut source = agent_activity("source");
        source.output_ports.push(output_port("decision"));
        source.output_ports.push(output_port("signal"));
        let graph = graph(
            "source",
            vec![
                source,
                agent_activity("a"),
                agent_activity("b"),
                agent_activity("c"),
            ],
            vec![
                transition("source", "a"),
                ActivityTransition {
                    from: "source".to_string(),
                    to: "b".to_string(),
                    kind: ActivityTransitionKind::Flow,
                    condition: TransitionCondition::ArtifactFieldEquals {
                        activity: "source".to_string(),
                        port: "output".to_string(),
                        path: "/approved".to_string(),
                        value: json!(true),
                    },
                    artifact_bindings: Vec::new(),
                    max_traversals: None,
                },
                ActivityTransition {
                    from: "source".to_string(),
                    to: "c".to_string(),
                    kind: ActivityTransitionKind::Flow,
                    condition: TransitionCondition::AgentSignalEquals {
                        activity: "source".to_string(),
                        signal_key: "done".to_string(),
                        value: json!("yes"),
                    },
                    artifact_bindings: Vec::new(),
                    max_traversals: None,
                },
            ],
        );

        let output = compile(&graph);
        assert!(
            !output.has_blocking_diagnostics(),
            "{:?}",
            output.diagnostics
        );
        let transition_conditions = output
            .plan_snapshot
            .activation_rules
            .iter()
            .filter_map(|rule| match rule {
                agentdash_domain::workflow::ActivationRule::Transition {
                    to_node_id,
                    condition,
                    ..
                } => Some((to_node_id.as_str(), condition)),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        assert!(matches!(
            transition_conditions["a"],
            TransitionCondition::Always
        ));
        assert!(matches!(
            transition_conditions["b"],
            TransitionCondition::ArtifactFieldEquals { .. }
        ));
        assert!(matches!(
            transition_conditions["c"],
            TransitionCondition::AgentSignalEquals { .. }
        ));
    }

    #[test]
    fn workflow_graph_compiler_normalizes_artifact_bindings_to_state_exchange_rules() {
        let mut source = agent_activity("source");
        source.output_ports.push(output_port("proposal"));
        let mut target = agent_activity("target");
        target.input_ports.push(input_port("proposal_in"));

        let flow_transition = ActivityTransition {
            from: "source".to_string(),
            to: "target".to_string(),
            kind: ActivityTransitionKind::Flow,
            condition: TransitionCondition::Always,
            artifact_bindings: vec![agentdash_domain::workflow::ArtifactBinding {
                from_activity: None,
                from_port: "proposal".to_string(),
                to_port: "proposal_in".to_string(),
                alias: ArtifactAliasPolicy::LatestAndHistory,
            }],
            max_traversals: None,
        };
        let graph = graph("source", vec![source, target], vec![flow_transition]);

        let output = compile(&graph);
        assert!(
            !output.has_blocking_diagnostics(),
            "{:?}",
            output.diagnostics
        );
        assert_eq!(output.plan_snapshot.state_exchange_rules.len(), 1);
        let rule = &output.plan_snapshot.state_exchange_rules[0];
        assert_eq!(rule.from_node_id, "source");
        assert_eq!(rule.from_port, "proposal");
        assert_eq!(rule.to_node_id, "target");
        assert_eq!(rule.to_port, "proposal_in");
        assert_eq!(rule.alias, ArtifactAliasPolicy::LatestAndHistory);
        assert!(output.plan_snapshot.activation_rules.iter().any(|rule| {
            matches!(
                rule,
                agentdash_domain::workflow::ActivationRule::Transition {
                    from_node_id,
                    to_node_id,
                    ..
                } if from_node_id == "source" && to_node_id == "target"
            )
        }));
    }

    #[test]
    fn workflow_graph_compiler_preserves_join_iteration_and_traversal_limits() {
        let mut entry = agent_activity("entry");
        entry.iteration_policy = ActivityIterationPolicy {
            max_attempts: Some(3),
            artifact_alias: ArtifactAliasPolicy::PerAttempt,
        };
        let mut target = agent_activity("target");
        target.join_policy = ActivityJoinPolicy::NOfM { n: 1 };
        let mut edge = transition("entry", "target");
        edge.max_traversals = Some(2);
        let graph = graph("entry", vec![entry, target], vec![edge]);

        let output = compile(&graph);
        assert!(
            !output.has_blocking_diagnostics(),
            "{:?}",
            output.diagnostics
        );
        let entry_node = output
            .plan_snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == "entry")
            .expect("entry node");
        assert_eq!(
            entry_node
                .iteration_policy
                .as_ref()
                .expect("iteration policy")
                .artifact_alias,
            ArtifactAliasPolicy::PerAttempt
        );

        let transition_rule = output
            .plan_snapshot
            .activation_rules
            .iter()
            .find_map(|rule| match rule {
                agentdash_domain::workflow::ActivationRule::Transition {
                    to_node_id,
                    join_policy,
                    max_traversals,
                    ..
                } if to_node_id == "target" => Some((join_policy, max_traversals)),
                _ => None,
            })
            .expect("transition rule");
        assert_eq!(*transition_rule.0, ActivityJoinPolicy::NOfM { n: 1 });
        assert_eq!(*transition_rule.1, Some(2));
    }

    #[test]
    fn workflow_graph_compiler_digest_is_stable_for_same_graph() {
        let graph = graph("entry", vec![agent_activity("entry")], Vec::new());

        let first = compile(&graph);
        let second = compile(&graph);

        assert_eq!(
            first.plan_snapshot.plan_digest,
            second.plan_snapshot.plan_digest
        );
        assert_eq!(
            serde_json::to_value(&first.plan_snapshot).expect("serialize first"),
            serde_json::to_value(&second.plan_snapshot).expect("serialize second")
        );
        assert!(first.plan_snapshot.plan_digest.starts_with("sha256:"));
    }

    #[test]
    fn workflow_graph_compiler_reports_blocking_diagnostics() {
        let mut unsupported = agent_activity("unsupported");
        unsupported.executor = ActivityExecutorSpec::Agent(AgentActivityExecutorSpec {
            procedure_key: "workflow.unsupported".to_string(),
            agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
            runtime_session_policy: RuntimeSessionPolicy::DeliverToCurrentTrace,
        });
        unsupported.iteration_policy.max_attempts = None;
        let mut entry = agent_activity("entry");
        entry.iteration_policy.max_attempts = None;
        let graph = graph(
            "missing_entry",
            vec![entry, unsupported],
            vec![
                ActivityTransition {
                    from: "entry".to_string(),
                    to: "missing".to_string(),
                    kind: ActivityTransitionKind::Flow,
                    condition: TransitionCondition::ArtifactFieldEquals {
                        activity: "entry".to_string(),
                        port: "missing_port".to_string(),
                        path: "/ok".to_string(),
                        value: json!(true),
                    },
                    artifact_bindings: Vec::new(),
                    max_traversals: None,
                },
                ActivityTransition {
                    from: "missing".to_string(),
                    to: "unsupported".to_string(),
                    kind: ActivityTransitionKind::Artifact,
                    condition: TransitionCondition::Always,
                    artifact_bindings: vec![agentdash_domain::workflow::ArtifactBinding {
                        from_activity: Some("entry".to_string()),
                        from_port: "missing_port".to_string(),
                        to_port: "missing_input".to_string(),
                        alias: ArtifactAliasPolicy::Latest,
                    }],
                    max_traversals: None,
                },
                ActivityTransition {
                    from: "entry".to_string(),
                    to: "unsupported".to_string(),
                    kind: ActivityTransitionKind::Artifact,
                    condition: TransitionCondition::Always,
                    artifact_bindings: Vec::new(),
                    max_traversals: None,
                },
                transition("entry", "unsupported"),
                transition("unsupported", "entry"),
            ],
        );

        let output = compile(&graph);
        let codes = diagnostic_codes(&output);
        assert!(output.has_blocking_diagnostics());
        assert!(codes.contains(&"entry_activity_missing"));
        assert!(codes.contains(&"dangling_transition_target"));
        assert!(codes.contains(&"dangling_transition_source"));
        assert!(codes.contains(&"dangling_condition_ref"));
        assert!(codes.contains(&"dangling_artifact_binding_ref"));
        assert!(codes.contains(&"unsupported_agent_executor_policy"));
        assert!(codes.contains(&"artifact_edge_missing_state_exchange"));
        assert!(codes.contains(&"unbounded_cycle"));
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.source_path.is_empty())
        );
    }
}
