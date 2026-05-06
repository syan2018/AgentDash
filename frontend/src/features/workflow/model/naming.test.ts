import { describe, expect, it } from "vitest";

import { buildLifecycleStepWorkflowNames, normalizeIdentifier, uniqueIdentifier } from "./naming";
import type { WorkflowDefinition } from "../../../types";

function workflow(key: string): WorkflowDefinition {
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
      output_ports: [],
      input_ports: [],
    },
    created_at: "2026-05-06T00:00:00.000Z",
    updated_at: "2026-05-06T00:00:00.000Z",
  };
}

describe("workflow naming helpers", () => {
  it("normalizes identifiers to lower snake case", () => {
    expect(normalizeIdentifier("Task Lifecycle / Review-Step", "fallback")).toBe("task_lifecycle_review_step");
    expect(normalizeIdentifier("  ", "fallback")).toBe("fallback");
  });

  it("deduplicates identifiers with numeric suffixes", () => {
    expect(uniqueIdentifier("task_lifecycle", ["task_lifecycle", "task_lifecycle_2"], "fallback")).toBe(
      "task_lifecycle_3",
    );
  });

  it("builds lifecycle step workflow key and display name from lifecycle and step", () => {
    expect(
      buildLifecycleStepWorkflowNames({
        lifecycleKey: "Task_Lifecycle",
        lifecycleDisplayName: "Task Lifecycle",
        stepKey: "Review",
        existingWorkflows: [workflow("task_lifecycle_review")],
      }),
    ).toEqual({
      key: "task_lifecycle_review_2",
      name: "Task Lifecycle / review",
    });
  });
});
