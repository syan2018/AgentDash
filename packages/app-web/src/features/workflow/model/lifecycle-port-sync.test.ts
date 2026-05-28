import { describe, expect, it } from "vitest";

import {
  mergeWorkflowPortsIntoLifecycleStep,
  syncLifecycleStepPortsForArtifactEdges,
} from "./lifecycle-port-sync";
import type { ActivityDefinition, ActivityTransition, WorkflowDefinition } from "../../../types";

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
    target_kinds: ["story"],
    source: "user_authored",
    version: 1,
    contract: {
      injection: { guidance: null, context_bindings: [] },
      hook_rules: [],
      capability_config: { tool_directives: [], mount_directives: [] },
      output_ports: (ports.output ?? []).map((port) => ({
        key: port,
        description: `${port} output`,
        gate_strategy: "existence",
      })),
      input_ports: (ports.input ?? []).map((port) => ({
        key: port,
        description: `${port} input`,
        context_strategy: "full",
        standalone_fulfillment: "required",
      })),
    },
    created_at: "2026-05-06T00:00:00.000Z",
    updated_at: "2026-05-06T00:00:00.000Z",
  };
}

function step(key: string, workflow_key: string): ActivityDefinition {
  return {
    key,
    description: "",
    executor: { kind: "agent", workflow_key, session_policy: "spawn_child" },
    output_ports: [],
    input_ports: [],
    completion_policy: { kind: "executor_terminal" },
    iteration_policy: { max_attempts: 1, artifact_alias: "latest" },
    join_policy: "all",
  };
}

function artifactTransition(from: string, fromPort: string, to: string, toPort: string): ActivityTransition {
  return {
    kind: "artifact",
    from,
    to,
    condition: { kind: "always" },
    artifact_bindings: [{
      from_activity: from,
      from_port: fromPort,
      to_port: toPort,
      alias: "latest",
    }],
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
        artifactTransition("research", "research_report", "implement", "research_input"),
      ],
    });

    expect(result.changed).toBe(true);
    expect(result.steps[0].output_ports).toEqual([
      { key: "research_report", description: "research_report output", gate_strategy: "existence" },
    ]);
    expect(result.steps[1].input_ports).toEqual([
      {
        key: "research_input",
        description: "research_input input",
        context_strategy: "full",
        standalone_fulfillment: "required",
      },
    ]);
  });

  it("creates default step ports when no workflow recommendation exists", () => {
    const result = syncLifecycleStepPortsForArtifactEdges({
      steps: [step("research", ""), step("implement", "")],
      workflows: [],
      edges: [
        artifactTransition("research", "research_report", "implement", "research_input"),
      ],
    });

    expect(result.steps[0].output_ports).toEqual([
      { key: "research_report", description: "", gate_strategy: "existence" },
    ]);
    expect(result.steps[1].input_ports).toEqual([
      {
        key: "research_input",
        description: "",
        context_strategy: "full",
        standalone_fulfillment: "required",
      },
    ]);
  });

  it("merges saved workflow ports into the lifecycle step without duplicating existing ports", () => {
    const merged = mergeWorkflowPortsIntoLifecycleStep(
      {
        ...step("implement", "implement_wf"),
        input_ports: [{
          key: "research_input",
          description: "existing",
          context_strategy: "full",
          standalone_fulfillment: "required",
        }],
      },
      workflow("implement_wf", { input: ["research_input", "spec_input"], output: ["implementation_report"] }),
    );

    expect(merged.input_ports.map((port) => port.key)).toEqual(["research_input", "spec_input"]);
    expect(merged.output_ports.map((port) => port.key)).toEqual(["implementation_report"]);
  });
});
