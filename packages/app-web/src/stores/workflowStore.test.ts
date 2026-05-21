import { afterEach, describe, expect, it } from "vitest";

import { createEmptyLifecycleDraft, useWorkflowStore } from "./workflowStore";

describe("workflow lifecycle draft defaults", () => {
  it("creates the initial lifecycle node as an AgentNode", () => {
    const draft = createEmptyLifecycleDraft("project-1", { initial_activity_key: "start" });

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
      initial_activity_key: "start",
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
    expect(editor.workflowDraftsByActivityKey["start"]).toBeDefined();
    expect(editor.workflowDraftsByActivityKey["start"].key).toBe("my_wf.start");
    expect(editor.selectedActivityKey).toBe("start");
  });

  it("addLifecycleEditorActivity 添加新 step 自动派生 workflow_key 并选中", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc1", initial_activity_key: "start" });
    const newKey = store.addLifecycleEditorActivity();

    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(newKey).toBeTruthy();
    expect(editor.draft?.activities).toHaveLength(2);
    expect(editor.selectedActivityKey).toBe(newKey);
    expect(editor.workflowDraftsByActivityKey[newKey!]?.key).toBe(`lc1.${newKey}`);
  });

  it("removeLifecycleEditorActivity 同步清掉 workflow draft 和相关 edges", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc2", initial_activity_key: "start" });
    const second = store.addLifecycleEditorActivity()!;
    store.updateLifecycleEditorDraft({
      transitions: [
        { kind: "flow", from: "start", to: second, condition: { kind: "always" }, artifact_bindings: [] },
      ],
    });
    store.removeLifecycleEditorActivity(second);

    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.draft?.activities).toHaveLength(1);
    expect(editor.draft?.transitions).toHaveLength(0);
    expect(editor.workflowDraftsByActivityKey[second]).toBeUndefined();
  });

  it("updateLifecycleEditorDraft target_kinds 同步到所有 step workflow drafts", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc3", initial_activity_key: "start" });
    store.addLifecycleEditorActivity();
    store.updateLifecycleEditorDraft({ target_kinds: ["project"] });

    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.draft?.target_kinds).toEqual(["project"]);
    for (const draft of Object.values(editor.workflowDraftsByActivityKey)) {
      expect(draft.target_kinds).toEqual(["project"]);
    }
  });

  it("updateLifecycleEditorActivity rename 时同步 edges + entry + selectedActivityKey + drafts 索引", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc4", initial_activity_key: "start" });
    const second = store.addLifecycleEditorActivity()!;
    store.updateLifecycleEditorDraft({
      transitions: [{ kind: "flow", from: "start", to: second, condition: { kind: "always" }, artifact_bindings: [] }],
    });
    store.updateLifecycleEditorActivity("start", { key: "kickoff" });

    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.draft?.entry_activity_key).toBe("kickoff");
    expect(editor.draft?.transitions[0].from).toBe("kickoff");
    expect(editor.workflowDraftsByActivityKey.kickoff).toBeDefined();
    expect(editor.workflowDraftsByActivityKey.start).toBeUndefined();
  });
});
