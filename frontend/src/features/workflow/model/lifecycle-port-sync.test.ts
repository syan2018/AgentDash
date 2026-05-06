import { describe, expect, it } from "vitest";

import {
  mergeWorkflowPortsIntoLifecycleStep,
  syncLifecycleStepPortsForArtifactEdges,
} from "./lifecycle-port-sync";
import type { LifecycleStepDefinition, WorkflowDefinition } from "../../../types";

function workflow(key: string, ports: {
  output?: string[];
  input?: string[];
} = {}): WorkflowDefinition {
  return {
    id: key,
    project_id: "project-1",
    key,
    name: key,
    description: "",
    target_kind: "story",
    recommended_roles: ["story"],
    source: "user_authored",
    version: 1,
    contract: {
      injection: { goal: null, instructions: [], context_bindings: [] },
      hook_rules: [],
      capability_directives: [],
      output_ports: (ports.output ?? []).map((port) => ({
        key: port,
        description: `${port} output`,
        gate_strategy: "existence",
      })),
      input_ports: (ports.input ?? []).map((port) => ({
        key: port,
        description: `${port} input`,
        context_strategy: "full",
      })),
    },
    created_at: "2026-05-06T00:00:00.000Z",
    updated_at: "2026-05-06T00:00:00.000Z",
  };
}

function step(key: string, workflow_key: string): LifecycleStepDefinition {
  return {
    key,
    description: "",
    workflow_key,
    node_type: "agent_node",
    output_ports: [],
    input_ports: [],
  };
}

describe("lifecycle port sync", () => {
  it("copies workflow ports referenced by artifact edges into step-level ports", () => {
    const result = syncLifecycleStepPortsForArtifactEdges({
      steps: [step("research", "research_wf"), step("implement", "implement_wf")],
      workflows: [
        workflow("research_wf", { output: ["research_report"] }),
        workflow("implement_wf", { input: ["research_input"] }),
      ],
      edges: [
        {
          kind: "artifact",
          from_node: "research",
          from_port: "research_report",
          to_node: "implement",
          to_port: "research_input",
        },
      ],
    });

    expect(result.changed).toBe(true);
    expect(result.steps[0].output_ports).toEqual([
      { key: "research_report", description: "research_report output", gate_strategy: "existence" },
    ]);
    expect(result.steps[1].input_ports).toEqual([
      { key: "research_input", description: "research_input input", context_strategy: "full" },
    ]);
  });

  it("creates default step ports when no workflow recommendation exists", () => {
    const result = syncLifecycleStepPortsForArtifactEdges({
      steps: [step("research", ""), step("implement", "")],
      workflows: [],
      edges: [
        {
          kind: "artifact",
          from_node: "research",
          from_port: "research_report",
          to_node: "implement",
          to_port: "research_input",
        },
      ],
    });

    expect(result.steps[0].output_ports).toEqual([
      { key: "research_report", description: "", gate_strategy: "existence" },
    ]);
    expect(result.steps[1].input_ports).toEqual([
      { key: "research_input", description: "", context_strategy: "full" },
    ]);
  });

  it("merges saved workflow ports into the lifecycle step without duplicating existing ports", () => {
    const merged = mergeWorkflowPortsIntoLifecycleStep(
      {
        ...step("implement", "implement_wf"),
        input_ports: [{ key: "research_input", description: "existing", context_strategy: "full" }],
      },
      workflow("implement_wf", { input: ["research_input", "spec_input"], output: ["implementation_report"] }),
    );

    expect(merged.input_ports.map((port) => port.key)).toEqual(["research_input", "spec_input"]);
    expect(merged.output_ports.map((port) => port.key)).toEqual(["implementation_report"]);
  });
});
