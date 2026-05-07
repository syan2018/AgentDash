import { describe, expect, it } from "vitest";

import {
  buildLifecycleStepWorkflowNames,
  formatDisplaySegment,
  normalizeIdentifier,
  uniqueIdentifier,
} from "./naming";
import type { WorkflowDefinition } from "../../../types";

function workflow(key: string): WorkflowDefinition {
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

  it("formats display segments from identifier-shaped values", () => {
    expect(formatDisplaySegment("review", "Step")).toBe("Review");
    expect(formatDisplaySegment("code_review", "Step")).toBe("Code Review");
    expect(formatDisplaySegment("MissionCreate", "Step")).toBe("MissionCreate");
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
      name: "Task Lifecycle / Review",
    });
  });
});
