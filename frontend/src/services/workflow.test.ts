import { describe, expect, it } from "vitest";

import { mapWorkflowDefinition } from "./workflow";

describe("workflow service mappers", () => {
  it("preserves backend capability_config tool directives", () => {
    const definition = mapWorkflowDefinition({
      id: "wf-1",
      project_id: "project-1",
      key: "builtin_workflow_admin_apply",
      name: "Workflow Admin / Apply",
      description: "",
      binding_kinds: ["project"],
      source: "builtin_seed",
      version: 1,
      contract: {
        injection: {
          guidance: "进入 Apply 阶段",
          context_bindings: [],
        },
        hook_rules: [],
        capability_config: {
          tool_directives: [{ add: "workflow_management" }],
          mount_directives: [{ op: "remove_mount", mount_id: "old" }],
        },
        output_ports: [],
        input_ports: [],
      },
      created_at: "2026-05-07T00:00:00.000Z",
      updated_at: "2026-05-07T00:00:00.000Z",
    });

    expect(definition.contract.injection.guidance).toBe("进入 Apply 阶段");
    expect(definition.contract.capability_config.tool_directives).toEqual([
      { add: "workflow_management" },
    ]);
    expect(definition.contract.capability_config.mount_directives).toEqual([
      { op: "remove_mount", mount_id: "old" },
    ]);
  });
});
