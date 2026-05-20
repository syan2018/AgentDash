import { afterEach, describe, expect, it } from "vitest";

import { createEmptyLifecycleDraft, useWorkflowStore } from "./workflowStore";

describe("workflow lifecycle draft defaults", () => {
  it("creates the initial lifecycle node as an AgentNode", () => {
    const draft = createEmptyLifecycleDraft("project-1", { initial_step_key: "start" });

    expect(draft.entry_activity_key).toBe("start");
    expect(draft.activities).toHaveLength(1);
    expect(draft.activities[0].executor.kind).toBe("agent");
  });
});

describe("unified lifecycle editor", () => {
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
    expect(editor.draft?.activities).toHaveLength(1);
    expect(editor.draft?.activities[0].key).toBe("start");
    // 自动派生 workflow_key
    expect(editor.draft?.activities[0].executor).toEqual({
      kind: "agent",
      workflow_key: "my_wf.start",
      session_policy: "spawn_child",
    });
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
    expect(editor.draft?.activities).toHaveLength(2);
    expect(editor.selectedStepKey).toBe(newKey);
    expect(editor.workflowDraftsByStepKey[newKey!]?.key).toBe(`lc1.${newKey}`);
  });

  it("removeLifecycleEditorStep 同步清掉 workflow draft 和相关 edges", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc2", initial_step_key: "start" });
    const second = store.addLifecycleEditorStep()!;
    store.updateLifecycleEditorDraft({
      transitions: [
        { kind: "flow", from: "start", to: second, condition: { kind: "always" }, artifact_bindings: [] },
      ],
    });
    store.removeLifecycleEditorStep(second);

    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.draft?.activities).toHaveLength(1);
    expect(editor.draft?.transitions).toHaveLength(0);
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
      transitions: [{ kind: "flow", from: "start", to: second, condition: { kind: "always" }, artifact_bindings: [] }],
    });
    store.updateLifecycleEditorStep("start", { key: "kickoff" });

    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.draft?.entry_activity_key).toBe("kickoff");
    expect(editor.draft?.transitions[0].from).toBe("kickoff");
    expect(editor.workflowDraftsByStepKey.kickoff).toBeDefined();
    expect(editor.workflowDraftsByStepKey.start).toBeUndefined();
  });
});
