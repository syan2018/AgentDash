import type {
  ActivityDefinition,
  ActivityTransition,
  InputPortDefinition,
  OutputPortDefinition,
  AgentProcedure,
} from "../../../types";

function cloneOutputPort(port: OutputPortDefinition): OutputPortDefinition {
  return { ...port };
}

function cloneInputPort(port: InputPortDefinition): InputPortDefinition {
  return { ...port };
}

function fallbackOutputPort(key: string): OutputPortDefinition {
  return { key, description: "", gate_strategy: "existence" };
}

function fallbackInputPort(key: string): InputPortDefinition {
  return {
    key,
    description: "",
    context_strategy: "full",
    standalone_fulfillment: "required",
  };
}

function workflowForStep(
  step: ActivityDefinition,
  workflowByKey: Map<string, AgentProcedure>,
): AgentProcedure | null {
  const ProcedureKey = step.executor.kind === "agent" ? step.executor.procedure_key.trim() : "";
  return ProcedureKey ? workflowByKey.get(ProcedureKey) ?? null : null;
}

export function mergeWorkflowPortsIntoLifecycleStep(
  step: ActivityDefinition,
  workflow: AgentProcedure,
): ActivityDefinition {
  const existingOutputKeys = new Set(step.output_ports.map((port) => port.key));
  const existingInputKeys = new Set(step.input_ports.map((port) => port.key));
  const missingOutputPorts = workflow.contract.output_ports
    .filter((port) => !existingOutputKeys.has(port.key))
    .map(cloneOutputPort);
  const missingInputPorts = workflow.contract.input_ports
    .filter((port) => !existingInputKeys.has(port.key))
    .map(cloneInputPort);

  if (missingOutputPorts.length === 0 && missingInputPorts.length === 0) {
    return step;
  }

  const outputPorts = [
    ...step.output_ports,
    ...missingOutputPorts,
  ];
  const inputPorts = [
    ...step.input_ports,
    ...missingInputPorts,
  ];

  return { ...step, output_ports: outputPorts, input_ports: inputPorts };
}

export function syncLifecycleStepPortsForArtifactEdges(input: {
  steps: ActivityDefinition[];
  edges: ActivityTransition[];
  workflows: AgentProcedure[];
}): { steps: ActivityDefinition[]; changed: boolean } {
  const workflowByKey = new Map(input.workflows.map((workflow) => [workflow.key, workflow]));
  const stepIndexByKey = new Map(input.steps.map((step, index) => [step.key, index]));
  let changed = false;
  const steps = input.steps.map((step) => ({
    ...step,
    output_ports: step.output_ports.map(cloneOutputPort),
    input_ports: step.input_ports.map(cloneInputPort),
  }));

  for (const edge of input.edges) {
    if (edge.kind !== "artifact") continue;
    const binding = edge.artifact_bindings[0];
    if (!binding?.from_port || !binding.to_port) continue;

    const sourceIndex = stepIndexByKey.get(edge.from);
    if (sourceIndex != null) {
      const sourceStep = steps[sourceIndex];
      const hasOutputPort = sourceStep.output_ports.some((port) => port.key === binding.from_port);
      if (!hasOutputPort) {
        const workflow = workflowForStep(sourceStep, workflowByKey);
        const recommended = workflow?.contract.output_ports.find((port) => port.key === binding.from_port);
        sourceStep.output_ports.push(recommended ? cloneOutputPort(recommended) : fallbackOutputPort(binding.from_port));
        changed = true;
      }
    }

    const targetIndex = stepIndexByKey.get(edge.to);
    if (targetIndex != null) {
      const targetStep = steps[targetIndex];
      const hasInputPort = targetStep.input_ports.some((port) => port.key === binding.to_port);
      if (!hasInputPort) {
        const workflow = workflowForStep(targetStep, workflowByKey);
        const recommended = workflow?.contract.input_ports.find((port) => port.key === binding.to_port);
        targetStep.input_ports.push(recommended ? cloneInputPort(recommended) : fallbackInputPort(binding.to_port));
        changed = true;
      }
    }
  }

  return {
    steps: changed ? steps : input.steps,
    changed,
  };
}
