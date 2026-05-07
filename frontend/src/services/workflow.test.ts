import { describe, expect, it } from "vitest";

import { mapLifecycleDefinition, mapWorkflowDefinition } from "./workflow";

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

  it("preserves lifecycle step capability_config during mapping", () => {
    const definition = mapLifecycleDefinition({
      id: "lc-1",
      project_id: "project-1",
      key: "builtin_workflow_admin",
      name: "Workflow Admin",
      description: "",
      binding_kinds: ["project"],
      source: "builtin_seed",
      version: 3,
      entry_step_key: "plan",
      steps: [
        {
          key: "plan",
          description: "Plan",
          workflow_key: "builtin_workflow_admin_plan",
          node_type: "agent_node",
          output_ports: [],
          input_ports: [
            {
              key: "design",
              description: "设计输入",
              context_strategy: "full",
              standalone_fulfillment: {
                optional: { default_value: "复用当前方案" },
              },
            },
          ],
          capability_config: {
            tool_directives: [
              { add: "workflow_management" },
              { remove: "workflow_management::upsert_workflow_tool" },
            ],
            mount_directives: [{ op: "remove_mount", mount_id: "scratch" }],
          },
        },
      ],
      edges: [],
      created_at: "2026-05-07T00:00:00.000Z",
      updated_at: "2026-05-07T00:00:00.000Z",
    });

    expect(definition.steps[0].capability_config).toEqual({
      tool_directives: [
        { add: "workflow_management" },
        { remove: "workflow_management::upsert_workflow_tool" },
      ],
      mount_directives: [{ op: "remove_mount", mount_id: "scratch" }],
    });
    expect(definition.steps[0].input_ports[0].standalone_fulfillment).toEqual({
      optional: { default_value: "复用当前方案" },
    });
  });
});
