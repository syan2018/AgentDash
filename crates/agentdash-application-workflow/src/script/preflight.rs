use agentdash_domain::workflow::{
    OrchestrationPlanSnapshot, OrchestrationSourceRef, PlanNodeKind, ValidationSeverity,
    WorkflowScriptCapabilitySummary, WorkflowScriptProvenance,
};
use agentdash_platform_spi::WorkflowScriptEvaluator;
use serde_json::{Map, Value};

use super::capability_summary::extract_workflow_script_capability_summary;
use super::{
    WorkflowScriptBuilderDiagnostic, WorkflowScriptBuilderDocument,
    parse_workflow_script_builder_document,
};

pub struct WorkflowScriptPreflightInput<'a> {
    pub evaluator: &'a dyn WorkflowScriptEvaluator,
    pub compiler: &'a dyn WorkflowScriptCompiler,
    pub source_text: &'a str,
    pub ctx: Value,
    pub args: Option<Value>,
    pub source_ref: OrchestrationSourceRef,
    pub provenance: WorkflowScriptProvenance,
}

#[derive(Debug, Clone)]
pub struct WorkflowScriptCompileInput<'a> {
    pub builder_document: &'a WorkflowScriptBuilderDocument,
    pub raw_builder_document: &'a Value,
    pub source_text: &'a str,
    pub source_ref: &'a OrchestrationSourceRef,
    pub provenance: &'a WorkflowScriptProvenance,
    pub capability_summary: &'a WorkflowScriptCapabilitySummary,
}

#[derive(Debug, Clone, Default)]
pub struct WorkflowScriptCompileOutput {
    pub plan_snapshot: Option<OrchestrationPlanSnapshot>,
    pub diagnostics: Vec<WorkflowScriptCompileDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowScriptCompileDiagnostic {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
    pub source_path: String,
}

impl WorkflowScriptCompileDiagnostic {
    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        source_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: ValidationSeverity::Error,
            message: message.into(),
            source_path: source_path.into(),
        }
    }

    pub fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        source_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: ValidationSeverity::Warning,
            message: message.into(),
            source_path: source_path.into(),
        }
    }

    pub fn is_blocking(&self) -> bool {
        self.severity == ValidationSeverity::Error
    }
}

pub trait WorkflowScriptCompiler: Send + Sync {
    fn compile_workflow_script(
        &self,
        input: WorkflowScriptCompileInput<'_>,
    ) -> WorkflowScriptCompileOutput;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowScriptPreflightDiagnostic {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
    pub source_path: String,
}

impl WorkflowScriptPreflightDiagnostic {
    pub fn error(
        code: impl Into<String>,
        message: impl Into<String>,
        source_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: ValidationSeverity::Error,
            message: message.into(),
            source_path: source_path.into(),
        }
    }

    pub fn warning(
        code: impl Into<String>,
        message: impl Into<String>,
        source_path: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            severity: ValidationSeverity::Warning,
            message: message.into(),
            source_path: source_path.into(),
        }
    }

    pub fn is_blocking(&self) -> bool {
        self.severity == ValidationSeverity::Error
    }
}

impl From<WorkflowScriptBuilderDiagnostic> for WorkflowScriptPreflightDiagnostic {
    fn from(value: WorkflowScriptBuilderDiagnostic) -> Self {
        Self {
            code: value.code,
            severity: value.severity,
            message: value.message,
            source_path: value.source_path,
        }
    }
}

impl From<WorkflowScriptCompileDiagnostic> for WorkflowScriptPreflightDiagnostic {
    fn from(value: WorkflowScriptCompileDiagnostic) -> Self {
        Self {
            code: value.code,
            severity: value.severity,
            message: value.message,
            source_path: value.source_path,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowScriptPlanPreview {
    pub plan_digest: String,
    pub node_count: usize,
    pub entry_node_ids: Vec<String>,
    pub nodes: Vec<WorkflowScriptPlanPreviewNode>,
}

impl WorkflowScriptPlanPreview {
    pub fn from_plan_snapshot(plan_snapshot: &OrchestrationPlanSnapshot) -> Self {
        Self {
            plan_digest: plan_snapshot.plan_digest.clone(),
            node_count: plan_snapshot.nodes.len(),
            entry_node_ids: plan_snapshot.entry_node_ids.clone(),
            nodes: plan_snapshot
                .nodes
                .iter()
                .map(|node| WorkflowScriptPlanPreviewNode {
                    node_id: node.node_id.clone(),
                    node_path: node.node_path.clone(),
                    kind: plan_node_kind_wire_name(node.kind),
                    label: node.label.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowScriptPlanPreviewNode {
    pub node_id: String,
    pub node_path: String,
    pub kind: String,
    pub label: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WorkflowScriptPreflightOutput {
    pub source_text: String,
    pub args: Option<Value>,
    pub source_ref: OrchestrationSourceRef,
    pub provenance: WorkflowScriptProvenance,
    pub raw_builder_document: Option<Value>,
    pub builder_document: Option<WorkflowScriptBuilderDocument>,
    pub plan_snapshot: Option<OrchestrationPlanSnapshot>,
    pub plan_preview: Option<WorkflowScriptPlanPreview>,
    pub capability_summary: WorkflowScriptCapabilitySummary,
    pub diagnostics: Vec<WorkflowScriptPreflightDiagnostic>,
}

impl WorkflowScriptPreflightOutput {
    pub fn has_blocking_diagnostics(&self) -> bool {
        self.diagnostics
            .iter()
            .any(WorkflowScriptPreflightDiagnostic::is_blocking)
    }
}

pub struct WorkflowScriptPreflightService;

impl WorkflowScriptPreflightService {
    pub fn preflight(input: WorkflowScriptPreflightInput<'_>) -> WorkflowScriptPreflightOutput {
        preflight_workflow_script(input)
    }
}

pub fn preflight_workflow_script(
    input: WorkflowScriptPreflightInput<'_>,
) -> WorkflowScriptPreflightOutput {
    let mut diagnostics = Vec::new();

    if let Err(errors) = input.evaluator.validate_workflow_script(input.source_text) {
        diagnostics.extend(errors.into_iter().map(|message| {
            WorkflowScriptPreflightDiagnostic::error("rhai_syntax_error", message, "source_text")
        }));
        return output_without_builder(input, diagnostics);
    }

    let eval_ctx = evaluation_context(input.ctx.clone(), input.args.as_ref());
    let raw_builder_document = match input
        .evaluator
        .eval_workflow_script(input.source_text, &eval_ctx)
    {
        Ok(value) => value,
        Err(message) => {
            diagnostics.push(WorkflowScriptPreflightDiagnostic::error(
                "workflow_script_eval_failed",
                message,
                "source_text",
            ));
            return output_without_builder(input, diagnostics);
        }
    };

    let parse_output = parse_workflow_script_builder_document(raw_builder_document.clone());
    diagnostics.extend(
        parse_output
            .diagnostics
            .into_iter()
            .map(WorkflowScriptPreflightDiagnostic::from),
    );

    let Some(builder_document) = parse_output.document else {
        return WorkflowScriptPreflightOutput {
            source_text: input.source_text.to_string(),
            args: input.args,
            source_ref: input.source_ref,
            provenance: input.provenance,
            raw_builder_document: Some(raw_builder_document),
            builder_document: None,
            plan_snapshot: None,
            plan_preview: None,
            capability_summary: WorkflowScriptCapabilitySummary::default(),
            diagnostics,
        };
    };

    let capability_summary = extract_workflow_script_capability_summary(&builder_document);
    let compile_output = input
        .compiler
        .compile_workflow_script(WorkflowScriptCompileInput {
            builder_document: &builder_document,
            raw_builder_document: &raw_builder_document,
            source_text: input.source_text,
            source_ref: &input.source_ref,
            provenance: &input.provenance,
            capability_summary: &capability_summary,
        });
    diagnostics.extend(
        compile_output
            .diagnostics
            .into_iter()
            .map(WorkflowScriptPreflightDiagnostic::from),
    );
    let has_blocking_diagnostics = diagnostics
        .iter()
        .any(WorkflowScriptPreflightDiagnostic::is_blocking);
    let plan_snapshot = if has_blocking_diagnostics {
        None
    } else {
        compile_output.plan_snapshot
    };
    let plan_preview = plan_snapshot
        .as_ref()
        .map(WorkflowScriptPlanPreview::from_plan_snapshot);

    WorkflowScriptPreflightOutput {
        source_text: input.source_text.to_string(),
        args: input.args,
        source_ref: input.source_ref,
        provenance: input.provenance,
        raw_builder_document: Some(raw_builder_document),
        builder_document: Some(builder_document),
        plan_snapshot,
        plan_preview,
        capability_summary,
        diagnostics,
    }
}

fn output_without_builder(
    input: WorkflowScriptPreflightInput<'_>,
    diagnostics: Vec<WorkflowScriptPreflightDiagnostic>,
) -> WorkflowScriptPreflightOutput {
    WorkflowScriptPreflightOutput {
        source_text: input.source_text.to_string(),
        args: input.args,
        source_ref: input.source_ref,
        provenance: input.provenance,
        raw_builder_document: None,
        builder_document: None,
        plan_snapshot: None,
        plan_preview: None,
        capability_summary: WorkflowScriptCapabilitySummary::default(),
        diagnostics,
    }
}

fn evaluation_context(ctx: Value, args: Option<&Value>) -> Value {
    match ctx {
        Value::Object(mut object) => {
            if let Some(args) = args {
                object.insert("args".to_string(), args.clone());
            }
            Value::Object(object)
        }
        value => {
            let mut object = Map::new();
            object.insert("ctx".to_string(), value);
            if let Some(args) = args {
                object.insert("args".to_string(), args.clone());
            }
            Value::Object(object)
        }
    }
}

fn plan_node_kind_wire_name(kind: PlanNodeKind) -> String {
    match serde_json::to_value(kind) {
        Ok(Value::String(value)) => value,
        _ => format!("{kind:?}"),
    }
}

#[cfg(test)]
mod tests {
    use agentdash_domain::workflow::{
        ActivationRule, OrchestrationLimits, PlanNode, WorkflowScriptApiEndpoint,
        WorkflowScriptBashCommand, WorkflowScriptHumanGateCapability, WorkflowScriptProvenance,
    };
    use chrono::Utc;
    use serde_json::json;

    use super::*;

    struct FakeEvaluator {
        validate_result: Result<(), Vec<String>>,
        eval_result: Result<Value, String>,
    }

    impl WorkflowScriptEvaluator for FakeEvaluator {
        fn validate_workflow_script(&self, _script: &str) -> Result<(), Vec<String>> {
            self.validate_result.clone()
        }

        fn eval_workflow_script(&self, _script: &str, _ctx: &Value) -> Result<Value, String> {
            self.eval_result.clone()
        }
    }

    #[derive(Default)]
    struct FakeCompiler {
        output: WorkflowScriptCompileOutput,
    }

    impl WorkflowScriptCompiler for FakeCompiler {
        fn compile_workflow_script(
            &self,
            _input: WorkflowScriptCompileInput<'_>,
        ) -> WorkflowScriptCompileOutput {
            self.output.clone()
        }
    }

    fn source_ref() -> OrchestrationSourceRef {
        OrchestrationSourceRef::Inline {
            source_digest: "sha256:source".to_string(),
        }
    }

    fn preflight_input<'a>(
        evaluator: &'a dyn WorkflowScriptEvaluator,
        compiler: &'a dyn WorkflowScriptCompiler,
    ) -> WorkflowScriptPreflightInput<'a> {
        WorkflowScriptPreflightInput {
            evaluator,
            compiler,
            source_text: "workflow(#{ body: [] })",
            ctx: json!({"project_id": "project-1"}),
            args: Some(json!({"topic": "runtime"})),
            source_ref: source_ref(),
            provenance: WorkflowScriptProvenance::default(),
        }
    }

    fn plan_snapshot() -> OrchestrationPlanSnapshot {
        let source_ref = source_ref();
        OrchestrationPlanSnapshot {
            plan_digest: "sha256:script-plan".to_string(),
            plan_version: 1,
            source_ref,
            nodes: vec![PlanNode {
                node_id: "scan_docs".to_string(),
                node_path: "scan_docs".to_string(),
                parent_node_id: None,
                kind: PlanNodeKind::AgentCall,
                label: Some("scan_docs".to_string()),
                executor: None,
                input_ports: Vec::new(),
                output_ports: Vec::new(),
                completion_policy: None,
                iteration_policy: None,
                join_policy: None,
                result_contract: None,
                metadata: None,
            }],
            entry_node_ids: vec!["scan_docs".to_string()],
            activation_rules: vec![ActivationRule::Entry {
                node_id: "scan_docs".to_string(),
            }],
            state_exchange_rules: Vec::new(),
            limits: OrchestrationLimits::default(),
            metadata: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn preflight_returns_syntax_diagnostics_without_eval() {
        let evaluator = FakeEvaluator {
            validate_result: Err(vec!["Unexpected token".to_string()]),
            eval_result: Ok(json!({"kind": "workflow", "body": []})),
        };
        let compiler = FakeCompiler::default();

        let output = preflight_workflow_script(preflight_input(&evaluator, &compiler));

        assert!(output.has_blocking_diagnostics());
        assert!(output.raw_builder_document.is_none());
        assert_eq!(output.diagnostics[0].code, "rhai_syntax_error");
        assert_eq!(output.diagnostics[0].source_path, "source_text");
    }

    #[test]
    fn preflight_merges_builder_and_compiler_diagnostics() {
        let evaluator = FakeEvaluator {
            validate_result: Ok(()),
            eval_result: Ok(json!({
                "kind": "workflow",
                "body": [
                    {
                        "kind": "agent",
                        "name": "scan_docs",
                        "procedure": "researcher"
                    },
                    { "kind": "unknown_node" }
                ]
            })),
        };
        let compiler = FakeCompiler {
            output: WorkflowScriptCompileOutput {
                plan_snapshot: Some(plan_snapshot()),
                diagnostics: vec![WorkflowScriptCompileDiagnostic::warning(
                    "compiler_preview_warning",
                    "Preview compiler warning",
                    "$.body[0]",
                )],
            },
        };

        let output = preflight_workflow_script(preflight_input(&evaluator, &compiler));

        assert!(output.has_blocking_diagnostics());
        assert!(output.plan_snapshot.is_none());
        assert!(output.plan_preview.is_none());
        assert!(output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "unknown_primitive" && diagnostic.source_path == "$.body[1].kind"
        }));
        assert!(output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "compiler_preview_warning" && diagnostic.source_path == "$.body[0]"
        }));
    }

    #[test]
    fn preflight_extracts_capability_summary_from_builder_document() {
        let evaluator = FakeEvaluator {
            validate_result: Ok(()),
            eval_result: Ok(json!({
                "kind": "workflow",
                "body": [
                    {
                        "kind": "phase",
                        "name": "collect",
                        "body": [
                            {
                                "kind": "parallel",
                                "branches": [
                                    {
                                        "kind": "agent",
                                        "name": "scan_docs",
                                        "procedure": "researcher"
                                    },
                                    {
                                        "kind": "function",
                                        "name": "fetch_index",
                                        "request": {
                                            "kind": "api_request",
                                            "method": "GET",
                                            "url": "https://example.test/index"
                                        }
                                    },
                                    {
                                        "kind": "local_effect",
                                        "name": "format_notes",
                                        "effect": {
                                            "kind": "bash_exec",
                                            "command": "pnpm",
                                            "args": ["format"]
                                        }
                                    },
                                    {
                                        "kind": "local_effect",
                                        "name": "render_canvas",
                                        "effect": {
                                            "kind": "capability_effect",
                                            "capability_key": "canvas.render"
                                        }
                                    },
                                    {
                                        "kind": "human_gate",
                                        "name": "approve_notes",
                                        "form_schema": "workflow.approval",
                                        "decision_port": "decision"
                                    }
                                ]
                            }
                        ]
                    }
                ]
            })),
        };
        let compiler = FakeCompiler {
            output: WorkflowScriptCompileOutput {
                plan_snapshot: Some(plan_snapshot()),
                diagnostics: Vec::new(),
            },
        };

        let output = preflight_workflow_script(preflight_input(&evaluator, &compiler));

        assert!(!output.has_blocking_diagnostics());
        assert_eq!(
            output.capability_summary.agent_procedure_keys,
            vec!["researcher"]
        );
        assert_eq!(
            output.capability_summary.function_api_endpoints,
            vec![WorkflowScriptApiEndpoint {
                method: "GET".to_string(),
                url: "https://example.test/index".to_string(),
            }]
        );
        assert_eq!(
            output.capability_summary.local_effect_capabilities,
            vec!["canvas.render"]
        );
        assert_eq!(
            output.capability_summary.bash_commands,
            vec![WorkflowScriptBashCommand {
                command: "pnpm".to_string(),
                args: vec!["format".to_string()],
                working_directory: None,
            }]
        );
        assert_eq!(
            output.capability_summary.human_gates,
            vec![WorkflowScriptHumanGateCapability {
                name: "approve_notes".to_string(),
                form_schema: "workflow.approval".to_string(),
                decision_port: "decision".to_string(),
            }]
        );
        assert_eq!(
            output
                .plan_preview
                .as_ref()
                .map(|preview| preview.node_count),
            Some(1)
        );
    }
}
