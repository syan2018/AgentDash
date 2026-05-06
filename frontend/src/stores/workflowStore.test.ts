import { afterEach, describe, expect, it } from "vitest";

import { createEmptyLifecycleDraft, useWorkflowStore } from "./workflowStore";

describe("workflow lifecycle draft defaults", () => {
  afterEach(() => {
    useWorkflowStore.getState().closeLifecycleDraft();
  });

  it("creates the initial lifecycle node as an AgentNode", () => {
    const draft = createEmptyLifecycleDraft("project-1", { initial_step_key: "start" });

    expect(draft.entry_step_key).toBe("start");
    expect(draft.steps).toHaveLength(1);
    expect(draft.steps[0].node_type).toBe("agent_node");
  });

  it("adds editor draft nodes as AgentNode by default", () => {
    const store = useWorkflowStore.getState();
    store.openNewLifecycleDraft("project-1", { initial_step_key: "start" });
    store.addLifecycleStep();

    const draft = useWorkflowStore.getState().lcEditor.draft;
    expect(draft?.steps[1]?.node_type).toBe("agent_node");
  });
});
