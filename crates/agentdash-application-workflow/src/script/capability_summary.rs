use agentdash_domain::workflow::{
    WorkflowScriptApiEndpoint, WorkflowScriptBashCommand, WorkflowScriptCapabilitySummary,
    WorkflowScriptHumanGateCapability,
};

use super::{
    WorkflowScriptBuilderDocument, WorkflowScriptEffect, WorkflowScriptRequest,
    WorkflowScriptStatement,
};

pub fn extract_workflow_script_capability_summary(
    document: &WorkflowScriptBuilderDocument,
) -> WorkflowScriptCapabilitySummary {
    let mut extractor = CapabilitySummaryExtractor::default();
    extractor.visit_statements(&document.body);
    extractor.finish()
}

#[derive(Default)]
struct CapabilitySummaryExtractor {
    agent_procedure_keys: Vec<String>,
    function_api_endpoints: Vec<WorkflowScriptApiEndpoint>,
    local_effect_capabilities: Vec<String>,
    bash_commands: Vec<WorkflowScriptBashCommand>,
    human_gates: Vec<WorkflowScriptHumanGateCapability>,
}

impl CapabilitySummaryExtractor {
    fn visit_statements(&mut self, statements: &[WorkflowScriptStatement]) {
        for statement in statements {
            self.visit_statement(statement);
        }
    }

    fn visit_statement(&mut self, statement: &WorkflowScriptStatement) {
        match statement {
            WorkflowScriptStatement::Phase(phase) => self.visit_statements(&phase.body),
            WorkflowScriptStatement::Log(_) => {}
            WorkflowScriptStatement::Agent(agent) => {
                if let Some(procedure) = &agent.procedure {
                    push_unique(&mut self.agent_procedure_keys, procedure.clone());
                }
            }
            WorkflowScriptStatement::Parallel(parallel) => {
                self.visit_statements(&parallel.branches)
            }
            WorkflowScriptStatement::Pipeline(pipeline) => self.visit_statements(&pipeline.stages),
            WorkflowScriptStatement::Function(function) => match &function.request {
                WorkflowScriptRequest::ApiRequest { method, url, .. } => push_unique(
                    &mut self.function_api_endpoints,
                    WorkflowScriptApiEndpoint {
                        method: method.clone(),
                        url: url.clone(),
                    },
                ),
            },
            WorkflowScriptStatement::LocalEffect(local_effect) => match &local_effect.effect {
                WorkflowScriptEffect::BashExec {
                    command,
                    args,
                    working_directory,
                } => push_unique(
                    &mut self.bash_commands,
                    WorkflowScriptBashCommand {
                        command: command.clone(),
                        args: args.clone(),
                        working_directory: working_directory.clone(),
                    },
                ),
                WorkflowScriptEffect::CapabilityEffect { capability_key, .. } => {
                    push_unique(&mut self.local_effect_capabilities, capability_key.clone());
                }
            },
            WorkflowScriptStatement::HumanGate(human_gate) => push_unique(
                &mut self.human_gates,
                WorkflowScriptHumanGateCapability {
                    name: human_gate.name.clone(),
                    form_schema: human_gate.form_schema.clone(),
                    decision_port: human_gate.decision_port.clone(),
                },
            ),
        }
    }

    fn finish(mut self) -> WorkflowScriptCapabilitySummary {
        self.agent_procedure_keys.sort();
        self.function_api_endpoints.sort();
        self.local_effect_capabilities.sort();
        self.bash_commands.sort();
        self.human_gates.sort();

        WorkflowScriptCapabilitySummary {
            agent_procedure_keys: self.agent_procedure_keys,
            function_api_endpoints: self.function_api_endpoints,
            local_effect_capabilities: self.local_effect_capabilities,
            bash_commands: self.bash_commands,
            human_gates: self.human_gates,
        }
    }
}

fn push_unique<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}
