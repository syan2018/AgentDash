use agentdash_domain::workflow::ValidationSeverity;
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowScriptBuilderDocument {
    pub name: Option<String>,
    pub args_schema: Option<Value>,
    pub limits: Option<Value>,
    pub body: Vec<WorkflowScriptStatement>,
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowScriptStatement {
    Phase(WorkflowScriptPhase),
    Log(String),
    Agent(WorkflowScriptAgent),
    Parallel(WorkflowScriptParallel),
    Pipeline(WorkflowScriptPipeline),
    Function(WorkflowScriptFunction),
    LocalEffect(WorkflowScriptLocalEffect),
    HumanGate(WorkflowScriptHumanGate),
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowScriptPhase {
    pub name: String,
    pub body: Vec<WorkflowScriptStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowScriptAgent {
    pub name: String,
    pub procedure: Option<String>,
    pub prompt: Option<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub limits: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowScriptParallel {
    pub branches: Vec<WorkflowScriptStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowScriptPipeline {
    pub stages: Vec<WorkflowScriptStatement>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowScriptFunction {
    pub name: String,
    pub request: WorkflowScriptRequest,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowScriptLocalEffect {
    pub name: String,
    pub effect: WorkflowScriptEffect,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowScriptHumanGate {
    pub name: String,
    pub form_schema: String,
    pub decision_port: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowScriptRequest {
    ApiRequest {
        method: String,
        url: String,
        body: Option<Value>,
        headers: Option<Value>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkflowScriptEffect {
    BashExec {
        command: String,
        args: Vec<String>,
        working_directory: Option<String>,
    },
    CapabilityEffect {
        capability_key: String,
        input: Option<Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkflowScriptBuilderDiagnostic {
    pub code: String,
    pub severity: ValidationSeverity,
    pub message: String,
    pub source_path: String,
}

impl WorkflowScriptBuilderDiagnostic {
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
        }
    }

    pub fn is_blocking(&self) -> bool {
        self.severity == ValidationSeverity::Error
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct WorkflowScriptBuilderParseOutput {
    pub document: Option<WorkflowScriptBuilderDocument>,
    pub diagnostics: Vec<WorkflowScriptBuilderDiagnostic>,
}

impl WorkflowScriptBuilderParseOutput {
    pub fn has_blocking_diagnostics(&self) -> bool {
        self.diagnostics
            .iter()
            .any(WorkflowScriptBuilderDiagnostic::is_blocking)
    }
}

pub fn parse_workflow_script_builder_document(value: Value) -> WorkflowScriptBuilderParseOutput {
    let mut parser = Parser::default();
    let document = parser.parse_document(&value, "$");
    WorkflowScriptBuilderParseOutput {
        document,
        diagnostics: parser.diagnostics,
    }
}

#[derive(Default)]
struct Parser {
    diagnostics: Vec<WorkflowScriptBuilderDiagnostic>,
}

impl Parser {
    fn parse_document(
        &mut self,
        value: &Value,
        path: &str,
    ) -> Option<WorkflowScriptBuilderDocument> {
        let object = self.object(value, path)?;
        self.require_kind(object, "workflow", path)?;
        let body = self.required_statement_array(object, "body", path)?;

        Some(WorkflowScriptBuilderDocument {
            name: self.optional_string(object, "name", path),
            args_schema: object.get("args").cloned(),
            limits: object.get("limits").cloned(),
            body,
            metadata: object.get("metadata").cloned(),
        })
    }

    fn parse_statement(&mut self, value: &Value, path: &str) -> Option<WorkflowScriptStatement> {
        let object = self.object(value, path)?;
        let kind = self.required_string(object, "kind", path)?;
        match kind.as_str() {
            "phase" => self
                .parse_phase(object, path)
                .map(WorkflowScriptStatement::Phase),
            "log" => self
                .required_string(object, "message", path)
                .map(WorkflowScriptStatement::Log),
            "agent" => self
                .parse_agent(object, path)
                .map(WorkflowScriptStatement::Agent),
            "parallel" => self
                .required_statement_array(object, "branches", path)
                .map(|branches| {
                    WorkflowScriptStatement::Parallel(WorkflowScriptParallel { branches })
                }),
            "pipeline" => self
                .required_statement_array(object, "stages", path)
                .map(|stages| WorkflowScriptStatement::Pipeline(WorkflowScriptPipeline { stages })),
            "function" => self
                .parse_function(object, path)
                .map(WorkflowScriptStatement::Function),
            "local_effect" => self
                .parse_local_effect(object, path)
                .map(WorkflowScriptStatement::LocalEffect),
            "human_gate" => self
                .parse_human_gate(object, path)
                .map(WorkflowScriptStatement::HumanGate),
            _ => {
                self.push_error(
                    "unknown_primitive",
                    format!("Unknown workflow script primitive `{kind}`"),
                    format!("{path}.kind"),
                );
                None
            }
        }
    }

    fn parse_phase(
        &mut self,
        object: &Map<String, Value>,
        path: &str,
    ) -> Option<WorkflowScriptPhase> {
        Some(WorkflowScriptPhase {
            name: self.required_string(object, "name", path)?,
            body: self.required_statement_array(object, "body", path)?,
        })
    }

    fn parse_agent(
        &mut self,
        object: &Map<String, Value>,
        path: &str,
    ) -> Option<WorkflowScriptAgent> {
        Some(WorkflowScriptAgent {
            name: self.required_string(object, "name", path)?,
            procedure: self.optional_string(object, "procedure", path),
            prompt: self.optional_string(object, "prompt", path),
            inputs: self.optional_string_array(object, "inputs", path),
            outputs: self.optional_string_array(object, "outputs", path),
            limits: object.get("limits").cloned(),
        })
    }

    fn parse_function(
        &mut self,
        object: &Map<String, Value>,
        path: &str,
    ) -> Option<WorkflowScriptFunction> {
        let request_value = self.required_value(object, "request", path)?;
        Some(WorkflowScriptFunction {
            name: self.required_string(object, "name", path)?,
            request: self.parse_request(request_value, &format!("{path}.request"))?,
            inputs: self.optional_string_array(object, "inputs", path),
            outputs: self.optional_string_array(object, "outputs", path),
        })
    }

    fn parse_local_effect(
        &mut self,
        object: &Map<String, Value>,
        path: &str,
    ) -> Option<WorkflowScriptLocalEffect> {
        let effect_value = self.required_value(object, "effect", path)?;
        Some(WorkflowScriptLocalEffect {
            name: self.required_string(object, "name", path)?,
            effect: self.parse_effect(effect_value, &format!("{path}.effect"))?,
            inputs: self.optional_string_array(object, "inputs", path),
            outputs: self.optional_string_array(object, "outputs", path),
        })
    }

    fn parse_human_gate(
        &mut self,
        object: &Map<String, Value>,
        path: &str,
    ) -> Option<WorkflowScriptHumanGate> {
        Some(WorkflowScriptHumanGate {
            name: self.required_string(object, "name", path)?,
            form_schema: self.required_string(object, "form_schema", path)?,
            decision_port: self.required_string(object, "decision_port", path)?,
        })
    }

    fn parse_request(&mut self, value: &Value, path: &str) -> Option<WorkflowScriptRequest> {
        let object = self.object(value, path)?;
        let kind = self.required_string(object, "kind", path)?;
        match kind.as_str() {
            "api_request" => Some(WorkflowScriptRequest::ApiRequest {
                method: self.required_string(object, "method", path)?,
                url: self.required_string(object, "url", path)?,
                body: object.get("body").cloned(),
                headers: object.get("headers").cloned(),
            }),
            _ => {
                self.push_error(
                    "unknown_request_helper",
                    format!("Unknown workflow script request helper `{kind}`"),
                    format!("{path}.kind"),
                );
                None
            }
        }
    }

    fn parse_effect(&mut self, value: &Value, path: &str) -> Option<WorkflowScriptEffect> {
        let object = self.object(value, path)?;
        let kind = self.required_string(object, "kind", path)?;
        match kind.as_str() {
            "bash_exec" => Some(WorkflowScriptEffect::BashExec {
                command: self.required_string(object, "command", path)?,
                args: self.optional_string_array(object, "args", path),
                working_directory: self.optional_string(object, "working_directory", path),
            }),
            "capability_effect" => Some(WorkflowScriptEffect::CapabilityEffect {
                capability_key: self.required_string(object, "capability_key", path)?,
                input: object.get("input").cloned(),
            }),
            _ => {
                self.push_error(
                    "unknown_effect_helper",
                    format!("Unknown workflow script effect helper `{kind}`"),
                    format!("{path}.kind"),
                );
                None
            }
        }
    }

    fn require_kind(
        &mut self,
        object: &Map<String, Value>,
        expected: &str,
        path: &str,
    ) -> Option<()> {
        let kind = self.required_string(object, "kind", path)?;
        if kind == expected {
            return Some(());
        }
        self.push_error(
            "unexpected_builder_document_kind",
            format!("Expected builder document kind `{expected}`, got `{kind}`"),
            format!("{path}.kind"),
        );
        None
    }

    fn object<'a>(&mut self, value: &'a Value, path: &str) -> Option<&'a Map<String, Value>> {
        match value {
            Value::Object(object) => Some(object),
            _ => {
                self.push_error(
                    "expected_object",
                    "Expected workflow script builder object",
                    path,
                );
                None
            }
        }
    }

    fn required_statement_array(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Option<Vec<WorkflowScriptStatement>> {
        let field_path = format!("{path}.{key}");
        let value = self.required_value(object, key, path)?;
        let Some(array) = value.as_array() else {
            self.push_error(
                "invalid_field_type",
                format!("Field `{key}` must be an array"),
                field_path,
            );
            return None;
        };

        let mut statements = Vec::new();
        for (index, item) in array.iter().enumerate() {
            if let Some(statement) = self.parse_statement(item, &format!("{path}.{key}[{index}]")) {
                statements.push(statement);
            }
        }
        Some(statements)
    }

    fn required_value<'a>(
        &mut self,
        object: &'a Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Option<&'a Value> {
        object.get(key).or_else(|| {
            self.push_error(
                "missing_field",
                format!("Missing required field `{key}`"),
                format!("{path}.{key}"),
            );
            None
        })
    }

    fn required_string(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Option<String> {
        let value = self.required_value(object, key, path)?;
        self.string_value(value, key, &format!("{path}.{key}"))
    }

    fn optional_string(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Option<String> {
        let value = object.get(key)?;
        self.string_value(value, key, &format!("{path}.{key}"))
    }

    fn string_value(&mut self, value: &Value, key: &str, path: &str) -> Option<String> {
        match value {
            Value::String(value) => Some(value.clone()),
            _ => {
                self.push_error(
                    "invalid_field_type",
                    format!("Field `{key}` must be a string"),
                    path,
                );
                None
            }
        }
    }

    fn optional_string_array(
        &mut self,
        object: &Map<String, Value>,
        key: &str,
        path: &str,
    ) -> Vec<String> {
        let Some(value) = object.get(key) else {
            return Vec::new();
        };
        let field_path = format!("{path}.{key}");
        let Some(array) = value.as_array() else {
            self.push_error(
                "invalid_field_type",
                format!("Field `{key}` must be an array of strings"),
                field_path,
            );
            return Vec::new();
        };

        array
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                self.string_value(item, key, &format!("{path}.{key}[{index}]"))
            })
            .collect()
    }

    fn push_error(
        &mut self,
        code: impl Into<String>,
        message: impl Into<String>,
        source_path: impl Into<String>,
    ) {
        self.diagnostics
            .push(WorkflowScriptBuilderDiagnostic::error(
                code,
                message,
                source_path,
            ));
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn workflow_script_builder_document_parses_representative_primitives() {
        let output = parse_workflow_script_builder_document(json!({
            "kind": "workflow",
            "name": "research_review",
            "args": { "topic": "string" },
            "limits": { "max_agents": 6 },
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
                                    "procedure": "researcher",
                                    "prompt": "Scan docs",
                                    "outputs": ["notes"]
                                },
                                {
                                    "kind": "function",
                                    "name": "fetch_index",
                                    "request": {
                                        "kind": "api_request",
                                        "method": "GET",
                                        "url": "https://example.test/index"
                                    },
                                    "outputs": ["index"]
                                }
                            ]
                        },
                        {
                            "kind": "pipeline",
                            "stages": [
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
        }));

        assert!(
            !output.has_blocking_diagnostics(),
            "{:?}",
            output.diagnostics
        );
        let document = output.document.expect("typed builder document");
        assert_eq!(document.name.as_deref(), Some("research_review"));
        assert_eq!(document.body.len(), 1);

        let WorkflowScriptStatement::Phase(phase) = &document.body[0] else {
            panic!("expected phase");
        };
        assert_eq!(phase.name, "collect");

        let WorkflowScriptStatement::Parallel(parallel) = &phase.body[0] else {
            panic!("expected parallel");
        };
        assert_eq!(parallel.branches.len(), 2);
        let WorkflowScriptStatement::Agent(agent) = &parallel.branches[0] else {
            panic!("expected agent branch");
        };
        assert_eq!(agent.procedure.as_deref(), Some("researcher"));
        assert_eq!(agent.outputs, vec!["notes"]);

        let WorkflowScriptStatement::Function(function) = &parallel.branches[1] else {
            panic!("expected function branch");
        };
        assert!(matches!(
            function.request,
            WorkflowScriptRequest::ApiRequest { .. }
        ));

        let WorkflowScriptStatement::Pipeline(pipeline) = &phase.body[1] else {
            panic!("expected pipeline");
        };
        assert_eq!(pipeline.stages.len(), 2);
        assert!(matches!(
            pipeline.stages[0],
            WorkflowScriptStatement::LocalEffect(_)
        ));
        assert!(matches!(
            pipeline.stages[1],
            WorkflowScriptStatement::HumanGate(_)
        ));
    }

    #[test]
    fn workflow_script_builder_document_reports_pathful_diagnostics() {
        let output = parse_workflow_script_builder_document(json!({
            "kind": "workflow",
            "body": [
                {
                    "kind": "parallel",
                    "branches": [
                        { "kind": "unknown_node", "name": "x" }
                    ]
                }
            ]
        }));

        assert!(output.has_blocking_diagnostics());
        assert!(output.diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "unknown_primitive"
                && diagnostic.source_path == "$.body[0].branches[0].kind"
        }));
    }

    #[test]
    fn workflow_script_builder_document_reports_primitive_field_paths() {
        let output = parse_workflow_script_builder_document(json!({
            "kind": "workflow",
            "body": [
                { "kind": "phase", "body": [] },
                { "kind": "parallel" },
                { "kind": "agent" },
                { "kind": "pipeline" },
                { "kind": "function", "name": "fetch_index" },
                { "kind": "local_effect", "name": "format_notes" },
                { "kind": "human_gate", "name": "approve_notes" }
            ]
        }));

        assert!(output.has_blocking_diagnostics());
        for source_path in [
            "$.body[0].name",
            "$.body[1].branches",
            "$.body[2].name",
            "$.body[3].stages",
            "$.body[4].request",
            "$.body[5].effect",
            "$.body[6].form_schema",
        ] {
            assert!(
                output
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.source_path == source_path),
                "missing diagnostic for {source_path}: {:?}",
                output.diagnostics
            );
        }
    }
}
