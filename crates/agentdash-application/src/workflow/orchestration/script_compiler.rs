use std::collections::{BTreeMap, BTreeSet};

use agentdash_domain::workflow::{
    ActivationRule, ActivityCompletionPolicy, ActivityIterationPolicy, ActivityJoinPolicy,
    AgentProcedureContract, AgentProcedureExecutionSpec, AgentReusePolicy, ApiRequestExecutorSpec,
    ArtifactAliasPolicy, BashExecExecutorSpec, ContextStrategy, ExecutorSpec,
    FunctionActivityExecutorSpec, GateStrategy, HumanActivityExecutorSpec,
    HumanApprovalExecutorSpec, InputPortDefinition, OrchestrationLimits, OrchestrationPlanSnapshot,
    OrchestrationSourceRef, OutputPortDefinition, PlanNode, PlanNodeKind, RuntimeSessionPolicy,
    StandaloneFulfillment, StateExchangeRule, TransitionCondition, ValidationSeverity,
    WorkflowInjectionSpec,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

use crate::workflow::script::{
    WorkflowScriptAgent, WorkflowScriptBuilderDocument, WorkflowScriptEffect,
    WorkflowScriptFunction, WorkflowScriptHumanGate, WorkflowScriptLocalEffect,
    WorkflowScriptPhase, WorkflowScriptPipeline, WorkflowScriptRequest, WorkflowScriptStatement,
};

pub const WORKFLOW_SCRIPT_COMPILER_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone)]
pub struct ScriptCompileInput<'a> {
    pub document: &'a WorkflowScriptBuilderDocument,
    pub source_ref: OrchestrationSourceRef,
    pub source_digest: String,
    pub source_path: String,
    pub args: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub target_schema_version: u32,
}

impl<'a> ScriptCompileInput<'a> {
    pub fn new(
        document: &'a WorkflowScriptBuilderDocument,
        source_ref: OrchestrationSourceRef,
        source_digest: impl Into<String>,
        source_path: impl Into<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            document,
            source_ref,
            source_digest: source_digest.into(),
            source_path: source_path.into(),
            args: None,
            created_at,
            target_schema_version: WORKFLOW_SCRIPT_COMPILER_SCHEMA_VERSION,
        }
    }

    pub fn with_args(mut self, args: Value) -> Self {
        self.args = Some(args);
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptCompileDiagnostic {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
    pub source_path: String,
    pub related_paths: Vec<String>,
}

impl ScriptCompileDiagnostic {
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
pub struct ScriptCompileOutput {
    pub plan_snapshot: OrchestrationPlanSnapshot,
    pub diagnostics: Vec<ScriptCompileDiagnostic>,
    pub capability_summary: Value,
}

impl ScriptCompileOutput {
    pub fn has_blocking_diagnostics(&self) -> bool {
        self.diagnostics
            .iter()
            .any(ScriptCompileDiagnostic::is_blocking)
    }
}

pub struct ScriptCompiler;

impl ScriptCompiler {
    pub fn compile(input: ScriptCompileInput<'_>) -> ScriptCompileOutput {
        compile_workflow_script_builder_document(input)
    }
}

impl crate::workflow::script::WorkflowScriptCompiler for ScriptCompiler {
    fn compile_workflow_script(
        &self,
        input: crate::workflow::script::WorkflowScriptCompileInput<'_>,
    ) -> crate::workflow::script::WorkflowScriptCompileOutput {
        let output = Self::compile(ScriptCompileInput {
            document: input.builder_document,
            source_ref: input.source_ref.clone(),
            source_digest: source_text_digest(input.source_text),
            source_path: "source_text".to_string(),
            args: input.args.cloned(),
            created_at: input.provenance.created_at,
            target_schema_version: WORKFLOW_SCRIPT_COMPILER_SCHEMA_VERSION,
        });
        crate::workflow::script::WorkflowScriptCompileOutput {
            plan_snapshot: Some(output.plan_snapshot),
            diagnostics: output
                .diagnostics
                .into_iter()
                .map(
                    |diagnostic| crate::workflow::script::WorkflowScriptCompileDiagnostic {
                        code: diagnostic.code,
                        severity: diagnostic.severity,
                        message: diagnostic.message,
                        source_path: diagnostic.source_path,
                    },
                )
                .collect(),
        }
    }
}

pub fn compile_workflow_script_builder_document(
    input: ScriptCompileInput<'_>,
) -> ScriptCompileOutput {
    let mut compiler = Compiler::new(input);
    compiler.compile()
}

#[derive(Debug, Clone, Default)]
struct StatementFragment {
    entries: Vec<String>,
    exits: Vec<String>,
}

impl StatementFragment {
    fn single(node_id: String) -> Self {
        Self {
            entries: vec![node_id.clone()],
            exits: vec![node_id],
        }
    }

    fn is_executable(&self) -> bool {
        !self.entries.is_empty() && !self.exits.is_empty()
    }
}

#[derive(Debug, Clone, Copy)]
enum SequenceRequirement {
    Optional,
    PipelineStage,
}

#[derive(Debug, Default)]
struct CapabilitySummaryBuilder {
    node_counts: BTreeMap<String, u32>,
    agent_procedures: BTreeSet<String>,
    api_requests: Vec<Value>,
    bash_execs: Vec<Value>,
    local_effect_capabilities: BTreeSet<String>,
    human_gates: Vec<Value>,
}

impl CapabilitySummaryBuilder {
    fn increment_node_kind(&mut self, key: &str) {
        *self.node_counts.entry(key.to_string()).or_default() += 1;
    }

    fn record_agent(&mut self, procedure_key: &str) {
        self.increment_node_kind("agent_call");
        if !procedure_key.trim().is_empty() {
            self.agent_procedures.insert(procedure_key.to_string());
        }
    }

    fn record_api_request(&mut self, method: &str, url: &str) {
        self.increment_node_kind("function");
        self.api_requests.push(json!({
            "method": method,
            "url": url,
        }));
    }

    fn record_bash_exec(
        &mut self,
        command: &str,
        args: &[String],
        working_directory: Option<&str>,
    ) {
        self.increment_node_kind("local_effect");
        self.bash_execs.push(json!({
            "command": command,
            "args": args,
            "working_directory": working_directory,
        }));
    }

    fn record_capability_effect(&mut self, capability_key: &str) {
        self.increment_node_kind("local_effect");
        if !capability_key.trim().is_empty() {
            self.local_effect_capabilities
                .insert(capability_key.to_string());
        }
    }

    fn record_human_gate(&mut self, name: &str, form_schema: &str, decision_port: &str) {
        self.increment_node_kind("human_gate");
        self.human_gates.push(json!({
            "name": name,
            "form_schema": form_schema,
            "decision_port": decision_port,
        }));
    }

    fn record_phase(&mut self) {
        self.increment_node_kind("phase");
    }

    fn to_value(&self) -> Value {
        let mut api_requests = self.api_requests.clone();
        api_requests.sort_by_key(stable_json_sort_key);
        let mut bash_execs = self.bash_execs.clone();
        bash_execs.sort_by_key(stable_json_sort_key);
        let mut human_gates = self.human_gates.clone();
        human_gates.sort_by_key(stable_json_sort_key);

        json!({
            "node_counts": self.node_counts,
            "agent_procedures": self.agent_procedures.iter().cloned().collect::<Vec<_>>(),
            "api_requests": api_requests,
            "bash_execs": bash_execs,
            "local_effect_capabilities": self.local_effect_capabilities.iter().cloned().collect::<Vec<_>>(),
            "human_gates": human_gates,
        })
    }
}

struct Compiler<'a> {
    input: ScriptCompileInput<'a>,
    diagnostics: Vec<ScriptCompileDiagnostic>,
    nodes: Vec<PlanNode>,
    activation_rules: Vec<ActivationRule>,
    state_exchange_rules: Vec<StateExchangeRule>,
    node_source_paths: BTreeMap<String, String>,
    node_input_ports: BTreeMap<String, Vec<String>>,
    node_output_ports: BTreeMap<String, Vec<String>>,
    input_port_source_paths: BTreeMap<(String, String), String>,
    incoming_predecessors: BTreeMap<String, BTreeSet<String>>,
    transition_edges: BTreeSet<(String, String)>,
    transition_index: usize,
    state_exchange_index: usize,
    log_markers: Vec<Value>,
    root_input_bindings: Vec<Value>,
    capability_summary: CapabilitySummaryBuilder,
}

impl<'a> Compiler<'a> {
    fn new(input: ScriptCompileInput<'a>) -> Self {
        Self {
            input,
            diagnostics: Vec::new(),
            nodes: Vec::new(),
            activation_rules: Vec::new(),
            state_exchange_rules: Vec::new(),
            node_source_paths: BTreeMap::new(),
            node_input_ports: BTreeMap::new(),
            node_output_ports: BTreeMap::new(),
            input_port_source_paths: BTreeMap::new(),
            incoming_predecessors: BTreeMap::new(),
            transition_edges: BTreeSet::new(),
            transition_index: 0,
            state_exchange_index: 0,
            log_markers: Vec::new(),
            root_input_bindings: Vec::new(),
            capability_summary: CapabilitySummaryBuilder::default(),
        }
    }

    fn compile(&mut self) -> ScriptCompileOutput {
        if self.input.target_schema_version != WORKFLOW_SCRIPT_COMPILER_SCHEMA_VERSION {
            self.diagnostics.push(ScriptCompileDiagnostic::error(
                "unsupported_plan_schema_version",
                format!(
                    "Workflow script compiler supports plan schema version {}, got {}",
                    WORKFLOW_SCRIPT_COMPILER_SCHEMA_VERSION, self.input.target_schema_version
                ),
                "compiler.target_schema_version",
            ));
        }

        if self.input.document.body.is_empty() {
            self.diagnostics.push(ScriptCompileDiagnostic::error(
                "workflow_body_empty",
                "Workflow script body must contain at least one executable statement",
                "$.body",
            ));
        }

        let limits = self.compile_limits(self.input.document.limits.as_ref());
        let fragment = self.compile_sequence(
            &self.input.document.body,
            "$.body",
            None,
            &[],
            SequenceRequirement::Optional,
        );

        if self.executable_node_ids().is_empty() {
            self.diagnostics.push(ScriptCompileDiagnostic::error(
                "no_executable_nodes",
                "Workflow script does not compile to any executable orchestration node",
                "$.body",
            ));
        }

        let mut entry_node_ids = sorted_unique(fragment.entries);
        for node_id in &entry_node_ids {
            self.activation_rules.push(ActivationRule::Entry {
                node_id: node_id.clone(),
            });
        }

        self.compile_node_activation_policies();
        self.validate_input_bindings(&entry_node_ids);

        self.nodes
            .sort_by(|left, right| left.node_id.cmp(&right.node_id));
        entry_node_ids.sort();
        self.activation_rules.sort_by_key(activation_rule_sort_key);
        self.state_exchange_rules
            .sort_by(|left, right| left.rule_id.cmp(&right.rule_id));
        self.log_markers.sort_by_key(stable_json_sort_key);
        self.root_input_bindings.sort_by_key(stable_json_sort_key);

        let capability_summary = self.capability_summary.to_value();
        let metadata = Some(self.plan_metadata(&capability_summary));
        let plan_digest = plan_digest(PlanDigestContent {
            compiler_schema_version: self.input.target_schema_version,
            source_ref: &self.input.source_ref,
            source_digest: &self.input.source_digest,
            nodes: &self.nodes,
            entry_node_ids: &entry_node_ids,
            activation_rules: &self.activation_rules,
            state_exchange_rules: &self.state_exchange_rules,
            limits: &limits,
            metadata: &metadata,
        });

        ScriptCompileOutput {
            plan_snapshot: OrchestrationPlanSnapshot {
                plan_digest,
                plan_version: self.input.target_schema_version,
                source_ref: self.input.source_ref.clone(),
                nodes: std::mem::take(&mut self.nodes),
                entry_node_ids,
                activation_rules: std::mem::take(&mut self.activation_rules),
                state_exchange_rules: std::mem::take(&mut self.state_exchange_rules),
                limits,
                metadata,
                created_at: self.input.created_at,
            },
            diagnostics: std::mem::take(&mut self.diagnostics),
            capability_summary,
        }
    }

    fn compile_sequence(
        &mut self,
        statements: &[WorkflowScriptStatement],
        array_path: &str,
        parent_node_id: Option<&str>,
        phase_path: &[String],
        requirement: SequenceRequirement,
    ) -> StatementFragment {
        let mut entries = Vec::new();
        let mut previous_exits: Vec<String> = Vec::new();

        for (index, statement) in statements.iter().enumerate() {
            let source_path = format!("{array_path}[{index}]");
            let fragment =
                self.compile_statement(statement, &source_path, parent_node_id, phase_path);
            if !fragment.is_executable() {
                if matches!(requirement, SequenceRequirement::PipelineStage) {
                    self.diagnostics.push(ScriptCompileDiagnostic::error(
                        "pipeline_stage_not_executable",
                        "Pipeline stage must compile to at least one executable node",
                        source_path,
                    ));
                }
                continue;
            }

            if previous_exits.is_empty() {
                entries.extend(fragment.entries.clone());
            } else {
                for from_node_id in &previous_exits {
                    for to_node_id in &fragment.entries {
                        self.add_transition(
                            from_node_id,
                            to_node_id,
                            ActivityJoinPolicy::All,
                            &source_path,
                        );
                    }
                }
            }
            previous_exits = fragment.exits;
        }

        StatementFragment {
            entries: sorted_unique(entries),
            exits: sorted_unique(previous_exits),
        }
    }

    fn compile_statement(
        &mut self,
        statement: &WorkflowScriptStatement,
        source_path: &str,
        parent_node_id: Option<&str>,
        phase_path: &[String],
    ) -> StatementFragment {
        match statement {
            WorkflowScriptStatement::Phase(phase) => {
                self.compile_phase(phase, source_path, parent_node_id, phase_path)
            }
            WorkflowScriptStatement::Log(message) => self.compile_log(message, source_path),
            WorkflowScriptStatement::Agent(agent) => {
                self.compile_agent(agent, source_path, parent_node_id, phase_path)
            }
            WorkflowScriptStatement::Parallel(parallel) => {
                if parallel.branches.is_empty() {
                    self.diagnostics.push(ScriptCompileDiagnostic::error(
                        "parallel_empty_branches",
                        "Parallel block must contain at least one branch",
                        format!("{source_path}.branches"),
                    ));
                    return StatementFragment::default();
                }

                let mut entries = Vec::new();
                let mut exits = Vec::new();
                for (index, branch) in parallel.branches.iter().enumerate() {
                    let branch_path = format!("{source_path}.branches[{index}]");
                    let fragment =
                        self.compile_statement(branch, &branch_path, parent_node_id, phase_path);
                    if !fragment.is_executable() {
                        self.diagnostics.push(ScriptCompileDiagnostic::error(
                            "parallel_branch_not_executable",
                            "Parallel branch must compile to at least one executable node",
                            branch_path,
                        ));
                        continue;
                    }
                    entries.extend(fragment.entries);
                    exits.extend(fragment.exits);
                }
                StatementFragment {
                    entries: sorted_unique(entries),
                    exits: sorted_unique(exits),
                }
            }
            WorkflowScriptStatement::Pipeline(pipeline) => {
                self.compile_pipeline(pipeline, source_path, parent_node_id, phase_path)
            }
            WorkflowScriptStatement::Function(function) => {
                self.compile_function(function, source_path, parent_node_id, phase_path)
            }
            WorkflowScriptStatement::LocalEffect(effect) => {
                self.compile_local_effect(effect, source_path, parent_node_id, phase_path)
            }
            WorkflowScriptStatement::HumanGate(gate) => {
                self.compile_human_gate(gate, source_path, parent_node_id, phase_path)
            }
        }
    }

    fn compile_phase(
        &mut self,
        phase: &WorkflowScriptPhase,
        source_path: &str,
        parent_node_id: Option<&str>,
        parent_phase_path: &[String],
    ) -> StatementFragment {
        let node_id = node_id_for(parent_phase_path, &phase.name);
        if !self.register_node(&node_id, source_path) {
            return StatementFragment::default();
        }

        self.capability_summary.record_phase();
        let mut phase_path = parent_phase_path.to_vec();
        phase_path.push(phase.name.clone());
        self.nodes.push(PlanNode {
            node_id: node_id.clone(),
            node_path: node_id.clone(),
            parent_node_id: parent_node_id.map(ToOwned::to_owned),
            kind: PlanNodeKind::Phase,
            label: Some(phase.name.clone()),
            executor: None,
            input_ports: Vec::new(),
            output_ports: Vec::new(),
            completion_policy: None,
            iteration_policy: None,
            join_policy: None,
            result_contract: None,
            metadata: Some(json!({
                "source_path": source_path,
                "phase_path": phase_path,
                "metadata_only": true,
            })),
        });
        self.compile_sequence(
            &phase.body,
            &format!("{source_path}.body"),
            Some(&node_id),
            &phase_path,
            SequenceRequirement::Optional,
        )
    }

    fn compile_log(&mut self, message: &str, source_path: &str) -> StatementFragment {
        self.log_markers.push(json!({
            "message": message,
            "source_path": source_path,
            "runtime_mapping": "metadata_only",
        }));
        self.diagnostics.push(ScriptCompileDiagnostic::warning(
            "log_metadata_only",
            "log() is recorded in plan metadata and does not compile to a runtime node",
            source_path,
        ));
        StatementFragment::default()
    }

    fn compile_pipeline(
        &mut self,
        pipeline: &WorkflowScriptPipeline,
        source_path: &str,
        parent_node_id: Option<&str>,
        phase_path: &[String],
    ) -> StatementFragment {
        if pipeline.stages.is_empty() {
            self.diagnostics.push(ScriptCompileDiagnostic::error(
                "pipeline_empty_stages",
                "Pipeline block must contain at least one stage",
                format!("{source_path}.stages"),
            ));
            return StatementFragment::default();
        }
        self.compile_sequence(
            &pipeline.stages,
            &format!("{source_path}.stages"),
            parent_node_id,
            phase_path,
            SequenceRequirement::PipelineStage,
        )
    }

    fn compile_agent(
        &mut self,
        agent: &WorkflowScriptAgent,
        source_path: &str,
        parent_node_id: Option<&str>,
        phase_path: &[String],
    ) -> StatementFragment {
        let node_id = node_id_for(phase_path, &agent.name);
        if !self.register_node(&node_id, source_path) {
            return StatementFragment::default();
        }

        let procedure_key = agent
            .procedure
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        let inline_prompt = agent.prompt.as_deref().map(str::trim).unwrap_or_default();
        let executor = if !procedure_key.is_empty() {
            self.capability_summary.record_agent(procedure_key);
            Some(ExecutorSpec::AgentProcedure {
                procedure: AgentProcedureExecutionSpec::by_key(procedure_key.to_string()),
                agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
                runtime_session_policy: RuntimeSessionPolicy::CreateNew,
            })
        } else if !inline_prompt.is_empty() {
            Some(ExecutorSpec::AgentProcedure {
                procedure: AgentProcedureExecutionSpec::Snapshot {
                    procedure_key: None,
                    name: Some(agent.name.clone()),
                    contract: Box::new(inline_agent_contract(agent)),
                    source_ref: Some(self.input.source_ref.clone()),
                    contract_digest: Some(inline_agent_contract_digest(agent)),
                },
                agent_reuse_policy: AgentReusePolicy::CreateActivityAgent,
                runtime_session_policy: RuntimeSessionPolicy::CreateNew,
            })
        } else {
            self.diagnostics.push(ScriptCompileDiagnostic::error(
                "agent_missing_procedure",
                format!(
                    "Agent node `{}` must declare procedure or prompt",
                    agent.name
                ),
                format!("{source_path}.procedure"),
            ));
            None
        };

        self.push_executable_node(ExecutableNodeInput {
            node_id,
            label: agent.name.clone(),
            parent_node_id,
            kind: PlanNodeKind::AgentCall,
            executor,
            inputs: &agent.inputs,
            outputs: &agent.outputs,
            completion_policy: output_completion_policy(&agent.outputs),
            source_path,
            phase_path,
            metadata: json!({
                "source_path": source_path,
                "prompt": agent.prompt,
                "limits": agent.limits,
            }),
        })
    }

    fn compile_function(
        &mut self,
        function: &WorkflowScriptFunction,
        source_path: &str,
        parent_node_id: Option<&str>,
        phase_path: &[String],
    ) -> StatementFragment {
        let node_id = node_id_for(phase_path, &function.name);
        if !self.register_node(&node_id, source_path) {
            return StatementFragment::default();
        }

        let executor = match &function.request {
            WorkflowScriptRequest::ApiRequest {
                method,
                url,
                body,
                headers,
            } => {
                if method.trim().is_empty() {
                    self.diagnostics.push(ScriptCompileDiagnostic::error(
                        "function_request_missing",
                        "api_request method must not be empty",
                        format!("{source_path}.request.method"),
                    ));
                }
                if url.trim().is_empty() {
                    self.diagnostics.push(ScriptCompileDiagnostic::error(
                        "function_request_missing",
                        "api_request url must not be empty",
                        format!("{source_path}.request.url"),
                    ));
                }
                if headers.is_some() {
                    self.diagnostics.push(ScriptCompileDiagnostic::error(
                        "function_request_unsupported",
                        "api_request headers are not represented by the current Function executor spec",
                        format!("{source_path}.request.headers"),
                    ));
                }
                self.capability_summary.record_api_request(method, url);
                Some(ExecutorSpec::Function {
                    spec: FunctionActivityExecutorSpec::ApiRequest(ApiRequestExecutorSpec {
                        method: method.clone(),
                        url_template: url.clone(),
                        body_template: body.clone(),
                    }),
                })
            }
        };

        self.push_executable_node(ExecutableNodeInput {
            node_id,
            label: function.name.clone(),
            parent_node_id,
            kind: PlanNodeKind::Function,
            executor,
            inputs: &function.inputs,
            outputs: &function.outputs,
            completion_policy: output_completion_policy(&function.outputs),
            source_path,
            phase_path,
            metadata: json!({
                "source_path": source_path,
                "request_kind": "api_request",
            }),
        })
    }

    fn compile_local_effect(
        &mut self,
        effect: &WorkflowScriptLocalEffect,
        source_path: &str,
        parent_node_id: Option<&str>,
        phase_path: &[String],
    ) -> StatementFragment {
        let node_id = node_id_for(phase_path, &effect.name);
        if !self.register_node(&node_id, source_path) {
            return StatementFragment::default();
        }

        let executor = match &effect.effect {
            WorkflowScriptEffect::BashExec {
                command,
                args,
                working_directory,
            } => {
                if command.trim().is_empty() {
                    self.diagnostics.push(ScriptCompileDiagnostic::error(
                        "local_effect_bash_command_missing",
                        "bash_exec command must not be empty",
                        format!("{source_path}.effect.command"),
                    ));
                }
                self.capability_summary.record_bash_exec(
                    command,
                    args,
                    working_directory.as_deref(),
                );
                Some(ExecutorSpec::Function {
                    spec: FunctionActivityExecutorSpec::BashExec(BashExecExecutorSpec {
                        command: command.clone(),
                        args: args.clone(),
                        working_directory: working_directory.clone(),
                    }),
                })
            }
            WorkflowScriptEffect::CapabilityEffect {
                capability_key,
                input,
            } => {
                if capability_key.trim().is_empty() {
                    self.diagnostics.push(ScriptCompileDiagnostic::error(
                        "local_effect_capability_missing",
                        "capability_effect capability_key must not be empty",
                        format!("{source_path}.effect.capability_key"),
                    ));
                }
                self.capability_summary
                    .record_capability_effect(capability_key);
                Some(ExecutorSpec::LocalEffect {
                    capability_key: capability_key.clone(),
                    input: input.clone(),
                })
            }
        };

        self.push_executable_node(ExecutableNodeInput {
            node_id,
            label: effect.name.clone(),
            parent_node_id,
            kind: PlanNodeKind::LocalEffect,
            executor,
            inputs: &effect.inputs,
            outputs: &effect.outputs,
            completion_policy: output_completion_policy(&effect.outputs),
            source_path,
            phase_path,
            metadata: json!({
                "source_path": source_path,
                "effect_kind": match &effect.effect {
                    WorkflowScriptEffect::BashExec { .. } => "bash_exec",
                    WorkflowScriptEffect::CapabilityEffect { .. } => "capability_effect",
                },
            }),
        })
    }

    fn compile_human_gate(
        &mut self,
        gate: &WorkflowScriptHumanGate,
        source_path: &str,
        parent_node_id: Option<&str>,
        phase_path: &[String],
    ) -> StatementFragment {
        let node_id = node_id_for(phase_path, &gate.name);
        if !self.register_node(&node_id, source_path) {
            return StatementFragment::default();
        }

        if gate.form_schema.trim().is_empty() {
            self.diagnostics.push(ScriptCompileDiagnostic::error(
                "human_gate_form_schema_empty",
                "human_gate form_schema must not be empty",
                format!("{source_path}.form_schema"),
            ));
        }
        if gate.decision_port.trim().is_empty() {
            self.diagnostics.push(ScriptCompileDiagnostic::error(
                "human_gate_decision_port_empty",
                "human_gate decision_port must not be empty",
                format!("{source_path}.decision_port"),
            ));
        }

        let outputs = if gate.decision_port.trim().is_empty() {
            Vec::new()
        } else {
            vec![gate.decision_port.clone()]
        };
        if !gate.decision_port.trim().is_empty()
            && !outputs.iter().any(|port| port == &gate.decision_port)
        {
            self.diagnostics.push(ScriptCompileDiagnostic::error(
                "human_gate_decision_port_mismatch",
                "human_gate decision_port must be declared as an output port",
                format!("{source_path}.decision_port"),
            ));
        }
        self.capability_summary.record_human_gate(
            &gate.name,
            &gate.form_schema,
            &gate.decision_port,
        );

        self.push_executable_node(ExecutableNodeInput {
            node_id,
            label: gate.name.clone(),
            parent_node_id,
            kind: PlanNodeKind::HumanGate,
            executor: Some(ExecutorSpec::Human {
                spec: HumanActivityExecutorSpec::Approval(HumanApprovalExecutorSpec {
                    form_schema_key: gate.form_schema.clone(),
                    title: Some(gate.name.clone()),
                }),
            }),
            inputs: &[],
            outputs: &outputs,
            completion_policy: ActivityCompletionPolicy::HumanDecision {
                decision_port: gate.decision_port.clone(),
            },
            source_path,
            phase_path,
            metadata: json!({
                "source_path": source_path,
                "form_schema": gate.form_schema,
            }),
        })
    }

    fn push_executable_node(&mut self, input: ExecutableNodeInput<'_>) -> StatementFragment {
        let input_ports = input
            .inputs
            .iter()
            .map(|key| input_port(key))
            .collect::<Vec<_>>();
        let output_ports = input
            .outputs
            .iter()
            .map(|key| output_port(key))
            .collect::<Vec<_>>();

        self.node_input_ports
            .insert(input.node_id.clone(), input.inputs.to_vec());
        self.node_output_ports
            .insert(input.node_id.clone(), input.outputs.to_vec());
        for (index, key) in input.inputs.iter().enumerate() {
            self.input_port_source_paths.insert(
                (input.node_id.clone(), key.clone()),
                format!("{}.inputs[{index}]", input.source_path),
            );
        }

        let mut metadata = input.metadata;
        if let Value::Object(object) = &mut metadata {
            object.insert("phase_path".to_string(), json!(input.phase_path));
        }

        self.nodes.push(PlanNode {
            node_id: input.node_id.clone(),
            node_path: input.node_id.clone(),
            parent_node_id: input.parent_node_id.map(ToOwned::to_owned),
            kind: input.kind,
            label: Some(input.label),
            executor: input.executor,
            input_ports,
            output_ports: output_ports.clone(),
            completion_policy: Some(input.completion_policy.clone()),
            iteration_policy: Some(ActivityIterationPolicy::default()),
            join_policy: Some(ActivityJoinPolicy::All),
            result_contract: Some(json!({
                "completion_policy": input.completion_policy,
                "output_ports": output_ports,
            })),
            metadata: Some(metadata),
        });
        StatementFragment::single(input.node_id)
    }

    fn register_node(&mut self, node_id: &str, source_path: &str) -> bool {
        if let Some(first_path) = self.node_source_paths.get(node_id) {
            self.diagnostics.push(
                ScriptCompileDiagnostic::error(
                    "duplicate_node_path",
                    format!("Workflow script node path `{node_id}` is duplicated"),
                    source_path,
                )
                .with_related_paths(vec![first_path.clone()]),
            );
            return false;
        }
        self.node_source_paths
            .insert(node_id.to_string(), source_path.to_string());
        true
    }

    fn add_transition(
        &mut self,
        from_node_id: &str,
        to_node_id: &str,
        join_policy: ActivityJoinPolicy,
        source_path: &str,
    ) {
        if !self
            .transition_edges
            .insert((from_node_id.to_string(), to_node_id.to_string()))
        {
            return;
        }

        let transition_index = self.transition_index;
        self.transition_index += 1;
        let rule_id = format!("script_transition:{transition_index}:{from_node_id}->{to_node_id}");
        self.activation_rules.push(ActivationRule::Transition {
            rule_id: rule_id.clone(),
            from_node_id: from_node_id.to_string(),
            to_node_id: to_node_id.to_string(),
            condition: TransitionCondition::Always,
            join_policy,
            max_traversals: None,
            source_path: Some(source_path.to_string()),
        });
        self.incoming_predecessors
            .entry(to_node_id.to_string())
            .or_default()
            .insert(from_node_id.to_string());

        let source_outputs = self
            .node_output_ports
            .get(from_node_id)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect::<BTreeSet<_>>();
        let target_inputs = self
            .node_input_ports
            .get(to_node_id)
            .cloned()
            .unwrap_or_default();
        for input_port in target_inputs {
            if !source_outputs.contains(&input_port) {
                continue;
            }
            let state_exchange_index = self.state_exchange_index;
            self.state_exchange_index += 1;
            self.state_exchange_rules.push(StateExchangeRule {
                rule_id: format!(
                    "script_state:{state_exchange_index}:{from_node_id}.{input_port}->{to_node_id}.{input_port}"
                ),
                from_node_id: from_node_id.to_string(),
                from_port: input_port.clone(),
                to_node_id: to_node_id.to_string(),
                to_port: input_port,
                alias: ArtifactAliasPolicy::Latest,
                source_transition_id: Some(rule_id.clone()),
                source_path: Some(source_path.to_string()),
            });
        }
    }

    fn compile_node_activation_policies(&mut self) {
        let executable_node_ids = self.executable_node_ids();
        for node_id in executable_node_ids {
            self.activation_rules.push(ActivationRule::Retry {
                node_id: node_id.clone(),
                max_attempts: Some(1),
            });
            if self.incoming_predecessors.contains_key(&node_id) {
                self.activation_rules.push(ActivationRule::Join {
                    node_id,
                    policy: join_policy_label(ActivityJoinPolicy::All),
                });
            }
        }
    }

    fn validate_input_bindings(&mut self, entry_node_ids: &[String]) {
        let entry_node_ids = entry_node_ids.iter().cloned().collect::<BTreeSet<_>>();
        let arg_keys = self.root_arg_keys();
        let mut bindings_by_target: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
        for rule in &self.state_exchange_rules {
            bindings_by_target
                .entry((rule.to_node_id.clone(), rule.to_port.clone()))
                .or_default()
                .push(format!("{}.{}", rule.from_node_id, rule.from_port));
        }

        for (node_id, input_ports) in self.node_input_ports.clone() {
            for input_port in input_ports {
                let key = (node_id.clone(), input_port.clone());
                let bound_sources = bindings_by_target.get(&key).cloned().unwrap_or_default();
                if bound_sources.len() > 1 {
                    self.diagnostics.push(
                        ScriptCompileDiagnostic::error(
                            "ambiguous_input_binding",
                            format!(
                                "Input `{node_id}.{input_port}` has multiple predecessor output bindings"
                            ),
                            self.input_port_source_path(&node_id, &input_port),
                        )
                        .with_related_paths(bound_sources),
                    );
                    continue;
                }
                if bound_sources.len() == 1 {
                    continue;
                }

                if entry_node_ids.contains(&node_id) && arg_keys.contains(&input_port) {
                    self.root_input_bindings.push(json!({
                        "node_id": node_id,
                        "port": input_port,
                        "source": "args",
                    }));
                    continue;
                }

                let has_incoming = self
                    .incoming_predecessors
                    .get(&node_id)
                    .is_some_and(|items| !items.is_empty());
                let (code, message) = if has_incoming {
                    (
                        "missing_output_binding",
                        format!(
                            "Input `{node_id}.{input_port}` cannot be resolved from predecessor output ports"
                        ),
                    )
                } else {
                    (
                        "unresolvable_input",
                        format!(
                            "Entry input `{node_id}.{input_port}` is not supplied by args or state exchange"
                        ),
                    )
                };
                self.diagnostics.push(ScriptCompileDiagnostic::error(
                    code,
                    message,
                    self.input_port_source_path(&node_id, &input_port),
                ));
            }
        }
    }

    fn input_port_source_path(&self, node_id: &str, input_port: &str) -> String {
        self.input_port_source_paths
            .get(&(node_id.to_string(), input_port.to_string()))
            .cloned()
            .unwrap_or_else(|| {
                self.node_source_paths
                    .get(node_id)
                    .cloned()
                    .unwrap_or_else(|| "$".to_string())
            })
    }

    fn executable_node_ids(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    PlanNodeKind::AgentCall
                        | PlanNodeKind::Function
                        | PlanNodeKind::LocalEffect
                        | PlanNodeKind::HumanGate
                )
            })
            .map(|node| node.node_id.clone())
            .collect()
    }

    fn root_arg_keys(&self) -> BTreeSet<String> {
        let mut keys = BTreeSet::new();
        if let Some(Value::Object(args)) = &self.input.args {
            keys.extend(args.keys().cloned());
        }
        if let Some(Value::Object(schema)) = &self.input.document.args_schema {
            if let Some(Value::Object(properties)) = schema.get("properties") {
                keys.extend(properties.keys().cloned());
            } else {
                keys.extend(
                    schema
                        .keys()
                        .filter(|key| {
                            !matches!(
                                key.as_str(),
                                "type" | "required" | "properties" | "additionalProperties"
                            )
                        })
                        .cloned(),
                );
            }
        }
        keys
    }

    fn compile_limits(&mut self, value: Option<&Value>) -> OrchestrationLimits {
        let Some(value) = value else {
            return OrchestrationLimits::default();
        };
        let Some(object) = value.as_object() else {
            self.diagnostics.push(ScriptCompileDiagnostic::error(
                "invalid_limits",
                "Workflow script limits must be an object",
                "$.limits",
            ));
            return OrchestrationLimits::default();
        };
        OrchestrationLimits {
            max_concurrency: self.optional_u32_limit(object, "max_concurrency", "$.limits"),
            max_agent_runs: self
                .optional_u32_limit(object, "max_agent_runs", "$.limits")
                .or_else(|| self.optional_u32_limit(object, "max_agents", "$.limits")),
            max_effect_runs: self
                .optional_u32_limit(object, "max_effect_runs", "$.limits")
                .or_else(|| self.optional_u32_limit(object, "max_effects", "$.limits")),
            budget: object.get("budget").cloned(),
            timeout_ms: self.optional_u64_limit(object, "timeout_ms", "$.limits"),
            max_traversals: self.optional_u32_limit(object, "max_traversals", "$.limits"),
        }
    }

    fn optional_u32_limit(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Option<u32> {
        let value = object.get(key)?;
        match value.as_u64().and_then(|value| u32::try_from(value).ok()) {
            Some(value) => Some(value),
            None => {
                self.diagnostics.push(ScriptCompileDiagnostic::error(
                    "invalid_limit_value",
                    format!("Limit `{key}` must be an unsigned 32-bit integer"),
                    format!("{path}.{key}"),
                ));
                None
            }
        }
    }

    fn optional_u64_limit(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Option<u64> {
        let value = object.get(key)?;
        match value.as_u64() {
            Some(value) => Some(value),
            None => {
                self.diagnostics.push(ScriptCompileDiagnostic::error(
                    "invalid_limit_value",
                    format!("Limit `{key}` must be an unsigned integer"),
                    format!("{path}.{key}"),
                ));
                None
            }
        }
    }

    fn plan_metadata(&self, capability_summary: &Value) -> Value {
        json!({
            "compiler": {
                "name": "workflow_script_compiler",
                "schema_version": self.input.target_schema_version,
            },
            "source": {
                "source_ref": self.input.source_ref,
                "source_digest": self.input.source_digest,
                "source_path": self.input.source_path,
                "document_name": self.input.document.name,
            },
            "script": {
                "args_schema": self.input.document.args_schema,
                "args": self.input.args,
                "raw_limits": self.input.document.limits,
                "log_markers": self.log_markers,
                "root_input_bindings": self.root_input_bindings,
                "capability_summary": capability_summary,
            },
        })
    }
}

struct ExecutableNodeInput<'a> {
    node_id: String,
    label: String,
    parent_node_id: Option<&'a str>,
    kind: PlanNodeKind,
    executor: Option<ExecutorSpec>,
    inputs: &'a [String],
    outputs: &'a [String],
    completion_policy: ActivityCompletionPolicy,
    source_path: &'a str,
    phase_path: &'a [String],
    metadata: Value,
}

#[derive(Serialize)]
struct PlanDigestContent<'a> {
    compiler_schema_version: u32,
    source_ref: &'a OrchestrationSourceRef,
    source_digest: &'a str,
    nodes: &'a [PlanNode],
    entry_node_ids: &'a [String],
    activation_rules: &'a [ActivationRule],
    state_exchange_rules: &'a [StateExchangeRule],
    limits: &'a OrchestrationLimits,
    metadata: &'a Option<Value>,
}

fn plan_digest(content: PlanDigestContent<'_>) -> String {
    let value = serde_json::to_value(&content).expect("plan digest content should serialize");
    let canonical = canonicalize_value(value);
    let bytes = serde_json::to_vec(&canonical).expect("canonical plan digest should serialize");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn source_text_digest(source_text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_text.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

fn canonicalize_value(value: Value) -> Value {
    match value {
        Value::Array(items) => Value::Array(items.into_iter().map(canonicalize_value).collect()),
        Value::Object(object) => {
            let mut sorted = BTreeMap::new();
            for (key, value) in object {
                sorted.insert(key, canonicalize_value(value));
            }
            let mut object = Map::new();
            for (key, value) in sorted {
                object.insert(key, value);
            }
            Value::Object(object)
        }
        value => value,
    }
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

fn output_completion_policy(outputs: &[String]) -> ActivityCompletionPolicy {
    if outputs.is_empty() {
        ActivityCompletionPolicy::ExecutorTerminal
    } else {
        ActivityCompletionPolicy::OutputPorts {
            required_ports: outputs.to_vec(),
        }
    }
}

fn inline_agent_contract(agent: &WorkflowScriptAgent) -> AgentProcedureContract {
    AgentProcedureContract {
        injection: WorkflowInjectionSpec {
            guidance: agent
                .prompt
                .as_deref()
                .map(str::trim)
                .filter(|prompt| !prompt.is_empty())
                .map(str::to_string),
            context_bindings: Vec::new(),
        },
        input_ports: agent.inputs.iter().map(|key| input_port(key)).collect(),
        output_ports: agent.outputs.iter().map(|key| output_port(key)).collect(),
        ..AgentProcedureContract::default()
    }
}

fn inline_agent_contract_digest(agent: &WorkflowScriptAgent) -> String {
    let value = serde_json::to_value(inline_agent_contract(agent))
        .expect("inline agent contract should serialize");
    let canonical = canonicalize_value(value);
    let bytes = serde_json::to_vec(&canonical).expect("canonical contract should serialize");
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("sha256:{:x}", hasher.finalize())
}

fn node_id_for(phase_path: &[String], name: &str) -> String {
    let mut segments = phase_path
        .iter()
        .map(|segment| canonical_segment(segment))
        .collect::<Vec<_>>();
    segments.push(canonical_segment(name));
    segments
        .into_iter()
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(".")
}

fn canonical_segment(value: &str) -> String {
    let mut output = String::new();
    let mut previous_was_separator = false;
    for character in value.trim().chars() {
        let mapped = if character.is_ascii_alphanumeric() {
            character.to_ascii_lowercase()
        } else {
            '_'
        };
        if mapped == '_' {
            if !previous_was_separator && !output.is_empty() {
                output.push('_');
            }
            previous_was_separator = true;
        } else {
            output.push(mapped);
            previous_was_separator = false;
        }
    }
    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() {
        "node".to_string()
    } else {
        output
    }
}

fn sorted_unique(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn join_policy_label(policy: ActivityJoinPolicy) -> String {
    match policy {
        ActivityJoinPolicy::All => "all".to_string(),
        ActivityJoinPolicy::Any => "any".to_string(),
        ActivityJoinPolicy::First => "first".to_string(),
        ActivityJoinPolicy::NOfM { n } => format!("n_of_m:{n}"),
    }
}

fn activation_rule_sort_key(rule: &ActivationRule) -> String {
    match rule {
        ActivationRule::Entry { node_id } => format!("00:entry:{node_id}"),
        ActivationRule::Transition {
            rule_id,
            from_node_id,
            to_node_id,
            ..
        } => format!("10:transition:{rule_id}:{from_node_id}:{to_node_id}"),
        ActivationRule::Dependency {
            node_id,
            depends_on_node_ids,
        } => format!("20:dependency:{node_id}:{depends_on_node_ids:?}"),
        ActivationRule::Condition { node_id, .. } => format!("30:condition:{node_id}"),
        ActivationRule::ArtifactBinding {
            from_node_id,
            from_port,
            to_node_id,
            to_port,
        } => format!("40:artifact:{from_node_id}:{from_port}:{to_node_id}:{to_port}"),
        ActivationRule::Join { node_id, policy } => format!("50:join:{node_id}:{policy}"),
        ActivationRule::Retry {
            node_id,
            max_attempts,
        } => format!("60:retry:{node_id}:{max_attempts:?}"),
        ActivationRule::Iteration {
            node_id,
            max_traversals,
        } => format!("70:iteration:{node_id}:{max_traversals:?}"),
    }
}

fn stable_json_sort_key(value: &Value) -> String {
    serde_json::to_string(&canonicalize_value(value.clone())).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentdash_domain::workflow::RuntimeNodeStatus;
    use chrono::TimeZone;
    use uuid::Uuid;

    fn fixed_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 6, 6, 0, 0, 0)
            .single()
            .expect("fixed time")
    }

    fn source_ref() -> OrchestrationSourceRef {
        OrchestrationSourceRef::Inline {
            source_digest: "sha256:script-source".to_string(),
        }
    }

    fn doc(body: Vec<WorkflowScriptStatement>) -> WorkflowScriptBuilderDocument {
        WorkflowScriptBuilderDocument {
            name: Some("script_fixture".to_string()),
            args_schema: Some(json!({"topic": "string"})),
            limits: Some(json!({"max_agents": 8, "max_effects": 4, "max_concurrency": 3})),
            body,
            metadata: None,
        }
    }

    fn compile(document: &WorkflowScriptBuilderDocument) -> ScriptCompileOutput {
        ScriptCompiler::compile(
            ScriptCompileInput::new(
                document,
                source_ref(),
                "sha256:script-source",
                "scripts/fixture.rhai",
                fixed_time(),
            )
            .with_args(json!({"topic": "orchestration"})),
        )
    }

    fn agent(name: &str, outputs: &[&str]) -> WorkflowScriptStatement {
        WorkflowScriptStatement::Agent(WorkflowScriptAgent {
            name: name.to_string(),
            procedure: Some(format!("workflow.{name}")),
            prompt: Some(format!("Run {name}")),
            inputs: Vec::new(),
            outputs: outputs.iter().map(|value| value.to_string()).collect(),
            limits: None,
        })
    }

    fn agent_with_inputs(name: &str, inputs: &[&str], outputs: &[&str]) -> WorkflowScriptStatement {
        WorkflowScriptStatement::Agent(WorkflowScriptAgent {
            name: name.to_string(),
            procedure: Some(format!("workflow.{name}")),
            prompt: None,
            inputs: inputs.iter().map(|value| value.to_string()).collect(),
            outputs: outputs.iter().map(|value| value.to_string()).collect(),
            limits: None,
        })
    }

    fn inline_agent(name: &str, inputs: &[&str], outputs: &[&str]) -> WorkflowScriptStatement {
        WorkflowScriptStatement::Agent(WorkflowScriptAgent {
            name: name.to_string(),
            procedure: None,
            prompt: Some(format!("Run {name} inline")),
            inputs: inputs.iter().map(|value| value.to_string()).collect(),
            outputs: outputs.iter().map(|value| value.to_string()).collect(),
            limits: None,
        })
    }

    fn api_function(name: &str, inputs: &[&str], outputs: &[&str]) -> WorkflowScriptStatement {
        WorkflowScriptStatement::Function(WorkflowScriptFunction {
            name: name.to_string(),
            request: WorkflowScriptRequest::ApiRequest {
                method: "POST".to_string(),
                url: "https://example.test/api".to_string(),
                body: Some(json!({"ok": true})),
                headers: None,
            },
            inputs: inputs.iter().map(|value| value.to_string()).collect(),
            outputs: outputs.iter().map(|value| value.to_string()).collect(),
        })
    }

    fn human_gate(name: &str, decision_port: &str) -> WorkflowScriptStatement {
        WorkflowScriptStatement::HumanGate(WorkflowScriptHumanGate {
            name: name.to_string(),
            form_schema: "workflow.approval".to_string(),
            decision_port: decision_port.to_string(),
        })
    }

    fn diagnostic_codes(output: &ScriptCompileOutput) -> Vec<&str> {
        output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.code.as_str())
            .collect()
    }

    #[test]
    fn script_compiler_maps_single_agent() {
        let document = doc(vec![agent_with_inputs("research", &["topic"], &["result"])]);

        let output = compile(&document);

        assert!(
            !output.has_blocking_diagnostics(),
            "{:?}",
            output.diagnostics
        );
        assert_eq!(output.plan_snapshot.entry_node_ids, vec!["research"]);
        let node = output
            .plan_snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == "research")
            .expect("research node");
        assert_eq!(node.kind, PlanNodeKind::AgentCall);
        assert!(matches!(
            node.executor,
            Some(ExecutorSpec::AgentProcedure {
                procedure: AgentProcedureExecutionSpec::ByKey {
                    ref procedure_key
                },
                ..
            }) if procedure_key == "workflow.research"
        ));
        assert_eq!(node.input_ports[0].key, "topic");
        assert_eq!(node.output_ports[0].key, "result");
        assert_eq!(
            output.plan_snapshot.metadata.as_ref().expect("metadata")["script"]["root_input_bindings"]
                [0]["port"],
            "topic"
        );
    }

    #[test]
    fn script_compiler_embeds_prompt_agent_as_snapshot_procedure() {
        let document = doc(vec![inline_agent("research", &["topic"], &["result"])]);

        let output = compile(&document);

        assert!(
            !output.has_blocking_diagnostics(),
            "{:?}",
            output.diagnostics
        );
        let node = output
            .plan_snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == "research")
            .expect("research node");
        match node.executor.as_ref().expect("executor") {
            ExecutorSpec::AgentProcedure {
                procedure:
                    AgentProcedureExecutionSpec::Snapshot {
                        procedure_key,
                        name,
                        contract,
                        contract_digest,
                        ..
                    },
                ..
            } => {
                assert_eq!(procedure_key, &None);
                assert_eq!(name.as_deref(), Some("research"));
                assert_eq!(
                    contract.injection.guidance.as_deref(),
                    Some("Run research inline")
                );
                assert_eq!(contract.input_ports[0].key, "topic");
                assert_eq!(contract.output_ports[0].key, "result");
                assert!(
                    contract_digest
                        .as_deref()
                        .is_some_and(|value| value.starts_with("sha256:"))
                );
            }
            other => panic!("unexpected executor: {other:?}"),
        }
    }

    #[test]
    fn script_compiler_maps_phase_pipeline_agent_function_human_gate() {
        let document = doc(vec![WorkflowScriptStatement::Phase(WorkflowScriptPhase {
            name: "review".to_string(),
            body: vec![WorkflowScriptStatement::Pipeline(WorkflowScriptPipeline {
                stages: vec![
                    agent("collect", &["payload"]),
                    api_function("enrich", &["payload"], &["api_result"]),
                    human_gate("approve", "decision"),
                ],
            })],
        })]);

        let output = compile(&document);

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
        assert_eq!(kinds["review"], PlanNodeKind::Phase);
        assert_eq!(kinds["review.collect"], PlanNodeKind::AgentCall);
        assert_eq!(kinds["review.enrich"], PlanNodeKind::Function);
        assert_eq!(kinds["review.approve"], PlanNodeKind::HumanGate);
        assert_eq!(
            output.plan_snapshot.entry_node_ids,
            vec!["review.collect".to_string()]
        );
        assert!(output.plan_snapshot.activation_rules.iter().any(|rule| {
            matches!(
                rule,
                ActivationRule::Transition {
                    from_node_id,
                    to_node_id,
                    ..
                } if from_node_id == "review.collect" && to_node_id == "review.enrich"
            )
        }));
        assert!(output.plan_snapshot.activation_rules.iter().any(|rule| {
            matches!(
                rule,
                ActivationRule::Transition {
                    from_node_id,
                    to_node_id,
                    ..
                } if from_node_id == "review.enrich" && to_node_id == "review.approve"
            )
        }));
        let human = output
            .plan_snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == "review.approve")
            .expect("human node");
        assert!(matches!(
            human.executor,
            Some(ExecutorSpec::Human {
                spec: HumanActivityExecutorSpec::Approval(_)
            })
        ));
        assert!(matches!(
            human.completion_policy,
            Some(ActivityCompletionPolicy::HumanDecision {
                ref decision_port
            }) if decision_port == "decision"
        ));
    }

    #[test]
    fn script_compiler_maps_parallel_fanout_join_then_summary_stage() {
        let document = doc(vec![
            WorkflowScriptStatement::Parallel(crate::workflow::script::WorkflowScriptParallel {
                branches: vec![
                    agent("alpha", &["alpha_notes"]),
                    agent("beta", &["beta_notes"]),
                    agent("gamma", &["gamma_notes"]),
                ],
            }),
            agent_with_inputs(
                "summary",
                &["alpha_notes", "beta_notes", "gamma_notes"],
                &["summary"],
            ),
        ]);

        let output = compile(&document);

        assert!(
            !output.has_blocking_diagnostics(),
            "{:?}",
            output.diagnostics
        );
        assert_eq!(
            output.plan_snapshot.entry_node_ids,
            vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()]
        );
        let incoming_to_summary = output
            .plan_snapshot
            .activation_rules
            .iter()
            .filter(|rule| {
                matches!(
                    rule,
                    ActivationRule::Transition { to_node_id, .. } if to_node_id == "summary"
                )
            })
            .count();
        assert_eq!(incoming_to_summary, 3);
        assert_eq!(output.plan_snapshot.state_exchange_rules.len(), 3);
        for port in ["alpha_notes", "beta_notes", "gamma_notes"] {
            assert!(
                output
                    .plan_snapshot
                    .state_exchange_rules
                    .iter()
                    .any(|rule| rule.to_node_id == "summary" && rule.to_port == port),
                "missing binding for {port}"
            );
        }
    }

    #[test]
    fn script_compiler_maps_local_effect_bash_and_capability() {
        let document = doc(vec![WorkflowScriptStatement::Pipeline(
            WorkflowScriptPipeline {
                stages: vec![
                    WorkflowScriptStatement::LocalEffect(WorkflowScriptLocalEffect {
                        name: "run_bash".to_string(),
                        effect: WorkflowScriptEffect::BashExec {
                            command: "pnpm".to_string(),
                            args: vec!["test".to_string()],
                            working_directory: Some(".".to_string()),
                        },
                        inputs: Vec::new(),
                        outputs: vec!["stdout".to_string()],
                    }),
                    WorkflowScriptStatement::LocalEffect(WorkflowScriptLocalEffect {
                        name: "capability".to_string(),
                        effect: WorkflowScriptEffect::CapabilityEffect {
                            capability_key: "workspace.write".to_string(),
                            input: Some(json!({"path": "report.md"})),
                        },
                        inputs: vec!["stdout".to_string()],
                        outputs: vec!["result".to_string()],
                    }),
                ],
            },
        )]);

        let output = compile(&document);

        assert!(
            !output.has_blocking_diagnostics(),
            "{:?}",
            output.diagnostics
        );
        let bash = output
            .plan_snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == "run_bash")
            .expect("bash node");
        assert_eq!(bash.kind, PlanNodeKind::LocalEffect);
        assert!(matches!(
            bash.executor,
            Some(ExecutorSpec::Function {
                spec: FunctionActivityExecutorSpec::BashExec(_)
            })
        ));
        let capability = output
            .plan_snapshot
            .nodes
            .iter()
            .find(|node| node.node_id == "capability")
            .expect("capability node");
        assert!(matches!(
            capability.executor,
            Some(ExecutorSpec::LocalEffect {
                ref capability_key,
                ..
            }) if capability_key == "workspace.write"
        ));
        assert_eq!(
            output.capability_summary["local_effect_capabilities"][0],
            "workspace.write"
        );
    }

    #[test]
    fn script_compiler_materializes_state_exchange_for_matching_ports() {
        let document = doc(vec![WorkflowScriptStatement::Pipeline(
            WorkflowScriptPipeline {
                stages: vec![
                    agent("scan", &["notes"]),
                    api_function("fetch", &["notes"], &["response"]),
                ],
            },
        )]);

        let output = compile(&document);

        assert!(
            !output.has_blocking_diagnostics(),
            "{:?}",
            output.diagnostics
        );
        assert_eq!(output.plan_snapshot.state_exchange_rules.len(), 1);
        let rule = &output.plan_snapshot.state_exchange_rules[0];
        assert_eq!(rule.from_node_id, "scan");
        assert_eq!(rule.from_port, "notes");
        assert_eq!(rule.to_node_id, "fetch");
        assert_eq!(rule.to_port, "notes");
    }

    #[test]
    fn script_compiler_digest_is_stable() {
        let document = doc(vec![agent("research", &["result"])]);

        let first = compile(&document);
        let second = compile(&document);

        assert_eq!(
            first.plan_snapshot.plan_digest,
            second.plan_snapshot.plan_digest
        );
        assert_eq!(
            serde_json::to_value(&first.plan_snapshot).expect("first"),
            serde_json::to_value(&second.plan_snapshot).expect("second")
        );
        assert!(first.plan_snapshot.plan_digest.starts_with("sha256:"));
    }

    #[test]
    fn script_compiler_records_log_as_metadata_only_with_diagnostic() {
        let document = doc(vec![
            WorkflowScriptStatement::Log("collecting context".to_string()),
            agent("research", &["result"]),
        ]);

        let output = compile(&document);

        assert!(
            !output.has_blocking_diagnostics(),
            "{:?}",
            output.diagnostics
        );
        assert!(diagnostic_codes(&output).contains(&"log_metadata_only"));
        assert!(
            output
                .plan_snapshot
                .nodes
                .iter()
                .all(|node| node.kind != PlanNodeKind::Function || node.node_id != "log")
        );
        assert_eq!(
            output.plan_snapshot.metadata.as_ref().expect("metadata")["script"]["log_markers"][0]["runtime_mapping"],
            "metadata_only"
        );
    }

    #[test]
    fn script_compiler_reports_blocking_diagnostics() {
        let document = doc(vec![
            agent("duplicate", &["one"]),
            agent("duplicate", &["two"]),
            WorkflowScriptStatement::Agent(WorkflowScriptAgent {
                name: "missing_procedure".to_string(),
                procedure: None,
                prompt: None,
                inputs: Vec::new(),
                outputs: vec!["missing".to_string()],
                limits: None,
            }),
            WorkflowScriptStatement::Function(WorkflowScriptFunction {
                name: "bad_request".to_string(),
                request: WorkflowScriptRequest::ApiRequest {
                    method: String::new(),
                    url: String::new(),
                    body: None,
                    headers: Some(json!({"authorization": "secret"})),
                },
                inputs: vec!["not_produced".to_string()],
                outputs: vec!["bad".to_string()],
            }),
            WorkflowScriptStatement::HumanGate(WorkflowScriptHumanGate {
                name: "bad_gate".to_string(),
                form_schema: "workflow.approval".to_string(),
                decision_port: String::new(),
            }),
            WorkflowScriptStatement::Parallel(crate::workflow::script::WorkflowScriptParallel {
                branches: Vec::new(),
            }),
            WorkflowScriptStatement::Pipeline(WorkflowScriptPipeline { stages: Vec::new() }),
        ]);

        let output = compile(&document);
        let codes = diagnostic_codes(&output);

        assert!(output.has_blocking_diagnostics());
        for code in [
            "duplicate_node_path",
            "agent_missing_procedure",
            "function_request_missing",
            "function_request_unsupported",
            "human_gate_decision_port_empty",
            "parallel_empty_branches",
            "pipeline_empty_stages",
            "missing_output_binding",
        ] {
            assert!(codes.contains(&code), "missing {code}: {:?}", codes);
        }
        assert!(
            output
                .diagnostics
                .iter()
                .all(|diagnostic| !diagnostic.source_path.is_empty())
        );
    }

    #[test]
    fn phase_nodes_do_not_block_runtime_completion_when_activated() {
        let document = doc(vec![WorkflowScriptStatement::Phase(WorkflowScriptPhase {
            name: "metadata".to_string(),
            body: vec![agent("finish", &["result"])],
        })]);
        let output = compile(&document);
        let orchestration = super::super::runtime::activate_orchestration(
            "dynamic_script",
            output.plan_snapshot.source_ref.clone(),
            output.plan_snapshot,
        );

        let phase = orchestration
            .node_tree
            .iter()
            .find(|node| node.node_id == "metadata")
            .expect("phase runtime node");
        assert_eq!(phase.status, RuntimeNodeStatus::Skipped);
        let executable = orchestration
            .node_tree
            .iter()
            .find(|node| node.node_id == "metadata.finish")
            .expect("finish runtime node");
        assert_eq!(executable.status, RuntimeNodeStatus::Ready);
    }

    #[test]
    fn activation_materializes_root_args_into_entry_node_inputs() {
        let document = doc(vec![agent_with_inputs("research", &["topic"], &["result"])]);
        let output = compile(&document);
        let orchestration = super::super::runtime::activate_orchestration(
            "dynamic_script",
            output.plan_snapshot.source_ref.clone(),
            output.plan_snapshot,
        );

        let node = orchestration
            .node_tree
            .iter()
            .find(|node| node.node_id == "research")
            .expect("research node");
        assert_eq!(node.status, RuntimeNodeStatus::Ready);
        assert_eq!(node.inputs.len(), 1);
        assert_eq!(node.inputs[0].port_key, "topic");
        assert_eq!(node.inputs[0].value, json!("orchestration"));
    }

    #[test]
    fn source_ref_accepts_run_script_artifact() {
        let document = doc(vec![agent("research", &["result"])]);
        let artifact_id = Uuid::from_u128(42);
        let output = ScriptCompiler::compile(ScriptCompileInput::new(
            &document,
            OrchestrationSourceRef::RunScriptArtifact {
                artifact_id,
                revision: 3,
                source_digest: "sha256:source".to_string(),
            },
            "sha256:source",
            "run_script_artifacts[42]",
            fixed_time(),
        ));

        assert_eq!(
            output.plan_snapshot.source_ref,
            OrchestrationSourceRef::RunScriptArtifact {
                artifact_id,
                revision: 3,
                source_digest: "sha256:source".to_string(),
            }
        );
    }
}
