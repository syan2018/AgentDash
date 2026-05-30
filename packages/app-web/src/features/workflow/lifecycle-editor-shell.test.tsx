/**
 * LifecycleEditorShell store-level integration 测试。
 *
 * shell 已收敛为 DAG 单一布局；不再有 Form/DAG mode 判定。本测试覆盖 selection
 * 模型驱动的 sidebar 路由：activity / transition / null 三态对应不同 inspector。
 *
 * 项目未引入 @testing-library/react；通过 store action + 派生 selection 字段
 * 验证状态机正确，UI 渲染层由 activity-inspector / transition-inspector 自身
 * 测试覆盖。
 */

import { afterEach, describe, expect, it } from "vitest";

import { transitionId, useWorkflowStore } from "../../stores/workflowStore";

describe("LifecycleEditorShell selection 路由", () => {
  afterEach(() => {
    useWorkflowStore.getState().closeLifecycleEditor();
  });

  it("openLifecycleForm 后默认选中入口 activity → sidebar 渲染 ActivityInspector", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("p1", { key: "k", initial_activity_key: "start" });
    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.selection).toEqual({ kind: "activity", activityKey: "start" });
  });

  it("selectLifecycleActivity(null) → selection 清空，sidebar 显示 LifecycleHeader", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("p1", { key: "k", initial_activity_key: "start" });
    store.selectLifecycleActivity(null);
    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.selection).toBeNull();
  });

  it("addLifecycleEditorActivity → 选中切到新增 activity", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("p1", { key: "k", initial_activity_key: "start" });
    const newKey = store.addLifecycleEditorActivity()!;
    const editor = useWorkflowStore.getState().lifecycleEditor;
    expect(editor.selection).toEqual({ kind: "activity", activityKey: newKey });
  });

  it("selectLifecycleTransition → sidebar 渲染 TransitionInspector（selection.kind === 'transition'）", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("p1", { key: "k", initial_activity_key: "start" });
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
  });

  it("setActivityExecutor 切到 human → completion_policy 自动联动 + reset 返回值供 toast 使用", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("p1", { key: "k", initial_activity_key: "start" });
    const result = store.setActivityExecutor("start", {
      kind: "human",
      type: "approval",
      form_schema_key: "approve",
      title: undefined,
    });
    expect(result?.reset).toBe(true);
    expect(result?.previous.kind).toBe("executor_terminal");
    const activity = useWorkflowStore.getState().lifecycleEditor.draft!.activities[0];
    expect(activity.completion_policy.kind).toBe("human_decision");
  });
});
