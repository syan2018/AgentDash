import type {
  InputPortDefinition,
  LifecycleEdge,
  LifecycleStepDefinition,
  OutputPortDefinition,
  WorkflowDefinition,
} from "../../../types";

function cloneOutputPort(port: OutputPortDefinition): OutputPortDefinition {
  return {
    ...port,
    gate_params: port.gate_params ? { ...port.gate_params } : port.gate_params,
  };
}

function cloneInputPort(port: InputPortDefinition): InputPortDefinition {
  return { ...port };
}

function fallbackOutputPort(key: string): OutputPortDefinition {
  return { key, description: "", gate_strategy: "existence" };
}

function fallbackInputPort(key: string): InputPortDefinition {
  return { key, description: "", context_strategy: "full" };
}

function workflowForStep(
  step: LifecycleStepDefinition,
  workflowByKey: Map<string, WorkflowDefinition>,
): WorkflowDefinition | null {
  const workflowKey = step.workflow_key?.trim();
  return workflowKey ? workflowByKey.get(workflowKey) ?? null : null;
}

export function mergeWorkflowPortsIntoLifecycleStep(
  step: LifecycleStepDefinition,
  workflow: WorkflowDefinition,
): LifecycleStepDefinition {
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
  steps: LifecycleStepDefinition[];
  edges: LifecycleEdge[];
  workflows: WorkflowDefinition[];
}): { steps: LifecycleStepDefinition[]; changed: boolean } {
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
    if (!edge.from_port || !edge.to_port) continue;

    const sourceIndex = stepIndexByKey.get(edge.from_node);
    if (sourceIndex != null) {
      const sourceStep = steps[sourceIndex];
      const hasOutputPort = sourceStep.output_ports.some((port) => port.key === edge.from_port);
      if (!hasOutputPort) {
        const workflow = workflowForStep(sourceStep, workflowByKey);
        const recommended = workflow?.contract.output_ports.find((port) => port.key === edge.from_port);
        sourceStep.output_ports.push(recommended ? cloneOutputPort(recommended) : fallbackOutputPort(edge.from_port));
        changed = true;
      }
    }

    const targetIndex = stepIndexByKey.get(edge.to_node);
    if (targetIndex != null) {
      const targetStep = steps[targetIndex];
      const hasInputPort = targetStep.input_ports.some((port) => port.key === edge.to_port);
      if (!hasInputPort) {
        const workflow = workflowForStep(targetStep, workflowByKey);
        const recommended = workflow?.contract.input_ports.find((port) => port.key === edge.to_port);
        targetStep.input_ports.push(recommended ? cloneInputPort(recommended) : fallbackInputPort(edge.to_port));
        changed = true;
      }
    }
  }

  return {
    steps: changed ? steps : input.steps,
    changed,
  };
}
