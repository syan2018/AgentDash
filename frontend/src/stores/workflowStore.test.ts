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

describe("unified lifecycle editor (PR2)", () => {
  afterEach(() => {
    useWorkflowStore.getState().closeLifecycleEditor();
  });

  it("openLifecycleForm 初始化单 step + 对应 workflow draft", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", {
      key: "my_wf",
      name: "My Workflow",
      initial_step_key: "start",
    });

    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.draft).not.toBeNull();
    expect(editor.draft?.key).toBe("my_wf");
    expect(editor.draft?.steps).toHaveLength(1);
    expect(editor.draft?.steps[0].key).toBe("start");
    // 自动派生 workflow_key
    expect(editor.draft?.steps[0].workflow_key).toBe("my_wf.start");
    expect(editor.workflowDraftsByStepKey["start"]).toBeDefined();
    expect(editor.workflowDraftsByStepKey["start"].key).toBe("my_wf.start");
    expect(editor.selectedStepKey).toBe("start");
  });

  it("addLifecycleEditorStep 添加新 step 自动派生 workflow_key 并选中", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc1", initial_step_key: "start" });
    const newKey = store.addLifecycleEditorStep();

    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(newKey).toBeTruthy();
    expect(editor.draft?.steps).toHaveLength(2);
    expect(editor.selectedStepKey).toBe(newKey);
    expect(editor.workflowDraftsByStepKey[newKey!]?.key).toBe(`lc1.${newKey}`);
  });

  it("removeLifecycleEditorStep 同步清掉 workflow draft 和相关 edges", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc2", initial_step_key: "start" });
    const second = store.addLifecycleEditorStep()!;
    store.updateLifecycleEditorDraft({
      edges: [
        { kind: "flow", from_node: "start", to_node: second },
      ],
    });
    store.removeLifecycleEditorStep(second);

    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.draft?.steps).toHaveLength(1);
    expect(editor.draft?.edges).toHaveLength(0);
    expect(editor.workflowDraftsByStepKey[second]).toBeUndefined();
  });

  it("updateLifecycleEditorDraft target_kinds 同步到所有 step workflow drafts", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc3", initial_step_key: "start" });
    store.addLifecycleEditorStep();
    store.updateLifecycleEditorDraft({ target_kinds: ["project"] });

    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.draft?.target_kinds).toEqual(["project"]);
    for (const draft of Object.values(editor.workflowDraftsByStepKey)) {
      expect(draft.target_kinds).toEqual(["project"]);
    }
  });

  it("updateLifecycleEditorStep rename 时同步 edges + entry + selectedStepKey + drafts 索引", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc4", initial_step_key: "start" });
    const second = store.addLifecycleEditorStep()!;
    store.updateLifecycleEditorDraft({
      edges: [{ kind: "flow", from_node: "start", to_node: second }],
    });
    store.updateLifecycleEditorStep("start", { key: "kickoff" });

    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.draft?.entry_step_key).toBe("kickoff");
    expect(editor.draft?.edges[0].from_node).toBe("kickoff");
    expect(editor.workflowDraftsByStepKey.kickoff).toBeDefined();
    expect(editor.workflowDraftsByStepKey.start).toBeUndefined();
  });
});
