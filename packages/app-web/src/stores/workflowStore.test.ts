import { afterEach, describe, expect, it } from "vitest";

import {
  createEmptyLifecycleDraft,
  ensurePolicyForExecutor,
  transitionId,
  useWorkflowStore,
} from "./workflowStore";
import type { ActivityCompletionPolicy } from "../types";

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
      workflow_key: "my_wf_start",
      session_policy: "spawn_child",
    });
    expect(editor.workflowDraftsByActivityKey["start"]).toBeDefined();
    expect(editor.workflowDraftsByActivityKey["start"].key).toBe("my_wf_start");
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
    expect(editor.workflowDraftsByActivityKey[newKey!]?.key).toBe(`lc1_${newKey}`);
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

describe("selection 模型", () => {
  afterEach(() => {
    useWorkflowStore.getState().closeLifecycleEditor();
  });

  it("selectLifecycleActivity 同步 selection 与派生 selectedActivityKey", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc", initial_activity_key: "start" });
    store.selectLifecycleActivity(null);
    let editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.selection).toBeNull();
    expect(editor.selectedActivityKey).toBeNull();

    store.selectLifecycleActivity("start");
    editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.selection).toEqual({ kind: "activity", activityKey: "start" });
    expect(editor.selectedActivityKey).toBe("start");
  });

  it("selectLifecycleTransition 切换 selection 并清空 selectedActivityKey 派生", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc", initial_activity_key: "start" });
    const second = store.addLifecycleEditorActivity()!;
    store.updateLifecycleEditorDraft({
      transitions: [
        { kind: "flow", from: "start", to: second, condition: { kind: "always" }, artifact_bindings: [] },
      ],
    });
    const id = transitionId(useWorkflowStore.getState().lifecycleEditor.draft!.transitions[0], 0);
    store.selectLifecycleTransition(id);
    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.selection).toEqual({ kind: "transition", transitionId: id });
    expect(editor.selectedActivityKey).toBeNull();
  });

  it("removeLifecycleEditorActivity 删除当前选中节点时落到首个剩余 activity", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc", initial_activity_key: "start" });
    const second = store.addLifecycleEditorActivity()!;
    store.selectLifecycleActivity(second);
    store.removeLifecycleEditorActivity(second);
    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.selectedActivityKey).toBe("start");
    expect(editor.selection).toEqual({ kind: "activity", activityKey: "start" });
  });

  it("删除 activity 同时清空对该 transition 的选中", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc", initial_activity_key: "start" });
    const second = store.addLifecycleEditorActivity()!;
    store.updateLifecycleEditorDraft({
      transitions: [
        { kind: "flow", from: "start", to: second, condition: { kind: "always" }, artifact_bindings: [] },
      ],
    });
    const id = transitionId(useWorkflowStore.getState().lifecycleEditor.draft!.transitions[0], 0);
    store.selectLifecycleTransition(id);
    store.removeLifecycleEditorActivity(second);
    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.selection).toBeNull();
  });
});

describe("ensurePolicyForExecutor 联动矩阵", () => {
  const policy = (kind: ActivityCompletionPolicy["kind"]): ActivityCompletionPolicy => {
    switch (kind) {
      case "output_ports":
        return { kind: "output_ports", required_ports: ["done"] };
      case "human_decision":
        return { kind: "human_decision", decision_port: "decision" };
      case "hook_gate":
        return { kind: "hook_gate", hook_key: "h" };
      case "executor_terminal":
        return { kind: "executor_terminal" };
      case "open_ended":
        return { kind: "open_ended" };
    }
  };

  it("agent → human 强制 human_decision", () => {
    const r = ensurePolicyForExecutor(policy("executor_terminal"), "human");
    expect(r.reset).toBe(true);
    expect(r.policy.kind).toBe("human_decision");
  });

  it("human → agent 落 executor_terminal", () => {
    const r = ensurePolicyForExecutor(policy("human_decision"), "agent");
    expect(r.reset).toBe(true);
    expect(r.policy.kind).toBe("executor_terminal");
  });

  it("agent 兼容 hook_gate 不重置", () => {
    const r = ensurePolicyForExecutor(policy("hook_gate"), "agent");
    expect(r.reset).toBe(false);
    expect(r.policy.kind).toBe("hook_gate");
  });

  it("function 不允许 hook_gate，重置到 executor_terminal", () => {
    const r = ensurePolicyForExecutor(policy("hook_gate"), "function");
    expect(r.reset).toBe(true);
    expect(r.policy.kind).toBe("executor_terminal");
  });

  it("function 不允许 open_ended，重置到 executor_terminal", () => {
    const r = ensurePolicyForExecutor(policy("open_ended"), "function");
    expect(r.reset).toBe(true);
    expect(r.policy.kind).toBe("executor_terminal");
  });

  it("function 兼容 output_ports 不重置", () => {
    const r = ensurePolicyForExecutor(policy("output_ports"), "function");
    expect(r.reset).toBe(false);
    expect(r.policy.kind).toBe("output_ports");
  });

  it("agent 不允许 human_decision，重置到 executor_terminal", () => {
    const r = ensurePolicyForExecutor(policy("human_decision"), "agent");
    expect(r.reset).toBe(true);
    expect(r.policy.kind).toBe("executor_terminal");
  });
});

describe("setActivityExecutor", () => {
  afterEach(() => {
    useWorkflowStore.getState().closeLifecycleEditor();
  });

  it("切到 human 同时强制 completion_policy 为 human_decision 并返回 reset=true", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc", initial_activity_key: "start" });
    const result = store.setActivityExecutor("start", {
      kind: "human",
      type: "approval",
      form_schema_key: "approve_form",
      title: null,
    });
    expect(result?.reset).toBe(true);
    expect(result?.previous.kind).toBe("executor_terminal");
    const activity = useWorkflowStore.getState().lifecycleEditor.draft!.activities[0];
    expect(activity.executor.kind).toBe("human");
    expect(activity.completion_policy.kind).toBe("human_decision");
  });

  it("切到兼容 executor 不重置 completion_policy", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc", initial_activity_key: "start" });
    store.setActivityCompletionPolicy("start", { kind: "output_ports", required_ports: ["done"] });
    const result = store.setActivityExecutor("start", {
      kind: "function",
      type: "bash_exec",
      command: "echo",
      args: ["hi"],
      working_directory: null,
    });
    expect(result?.reset).toBe(false);
    const activity = useWorkflowStore.getState().lifecycleEditor.draft!.activities[0];
    expect(activity.completion_policy.kind).toBe("output_ports");
  });
});

describe("transition 编辑 actions", () => {
  afterEach(() => {
    useWorkflowStore.getState().closeLifecycleEditor();
  });

  function setupTwoActivitiesWithTransition() {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("project-1", { key: "lc", initial_activity_key: "start" });
    const second = store.addLifecycleEditorActivity()!;
    store.updateLifecycleEditorDraft({
      transitions: [
        {
          kind: "artifact",
          from: "start",
          to: second,
          condition: { kind: "always" },
          artifact_bindings: [
            { from_activity: "start", from_port: "out", to_port: "in", alias: "latest" },
          ],
        },
      ],
    });
    return {
      store,
      second,
      id: transitionId(useWorkflowStore.getState().lifecycleEditor.draft!.transitions[0], 0),
    };
  }

  it("setTransitionKind artifact → flow 清空 bindings", () => {
    const { store, id } = setupTwoActivitiesWithTransition();
    store.setTransitionKind(id, "flow");
    const t = useWorkflowStore.getState().lifecycleEditor.draft!.transitions[0];
    expect(t.kind).toBe("flow");
    expect(t.artifact_bindings).toHaveLength(0);
  });

  it("addArtifactBinding / updateArtifactBinding / removeArtifactBinding 增删改", () => {
    const { store, id } = setupTwoActivitiesWithTransition();
    store.addArtifactBinding(id, {
      from_activity: "start",
      from_port: "out2",
      to_port: "in2",
      alias: "per_attempt",
    });
    let t = useWorkflowStore.getState().lifecycleEditor.draft!.transitions[0];
    expect(t.artifact_bindings).toHaveLength(2);

    store.updateArtifactBinding(id, 1, { alias: "latest_and_history" });
    t = useWorkflowStore.getState().lifecycleEditor.draft!.transitions[0];
    expect(t.artifact_bindings[1].alias).toBe("latest_and_history");

    store.removeArtifactBinding(id, 0);
    t = useWorkflowStore.getState().lifecycleEditor.draft!.transitions[0];
    expect(t.artifact_bindings).toHaveLength(1);
    expect(t.artifact_bindings[0].from_port).toBe("out2");
  });

  it("updateLifecycleEditorTransition 改 max_traversals + condition", () => {
    const { store, id } = setupTwoActivitiesWithTransition();
    store.updateLifecycleEditorTransition(id, {
      max_traversals: 3,
      condition: {
        kind: "human_decision_equals",
        activity: "start",
        decision_port: "decision",
        value: "approved",
      },
    });
    const t = useWorkflowStore.getState().lifecycleEditor.draft!.transitions[0];
    expect(t.max_traversals).toBe(3);
    expect(t.condition.kind).toBe("human_decision_equals");
  });
});
