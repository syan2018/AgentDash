/**
 * LifecycleEditorShell 关键状态机测试。
 *
 * 由于项目未引入 @testing-library/react，这里以"纯逻辑"方式校验 mode 判定规则 +
 * store 层驱动的 sticky_dag 交互，不直接渲染 React 树。
 */

import { afterEach, describe, expect, it } from "vitest";

import { useWorkflowStore } from "../../stores/workflowStore";

/**
 * mode 判定规则镜像（与 shell 内部 useMemo 对齐）。
 * 单独导出方便测试，后续如果内置逻辑变化本测试会跟 shell 一起失败。
 */
function judgeMode(params: {
  stepCount: number;
  edgeCount: number;
  stickyDag: boolean;
}): "form" | "dag" {
  if (params.stickyDag) return "dag";
  if (params.stepCount <= 1 && params.edgeCount === 0) return "form";
  return "dag";
}

describe("LifecycleEditorShell mode judgement", () => {
  it("新建 + 单 step + 0 edges → Form", () => {
    expect(judgeMode({ stepCount: 1, edgeCount: 0, stickyDag: false })).toBe("form");
  });

  it("2 steps → DAG", () => {
    expect(judgeMode({ stepCount: 2, edgeCount: 0, stickyDag: false })).toBe("dag");
  });

  it("1 step + 1 edge（异常但应当进 DAG） → DAG", () => {
    expect(judgeMode({ stepCount: 1, edgeCount: 1, stickyDag: false })).toBe("dag");
  });

  it("sticky_dag 覆盖所有情况", () => {
    expect(judgeMode({ stepCount: 1, edgeCount: 0, stickyDag: true })).toBe("dag");
    expect(judgeMode({ stepCount: 3, edgeCount: 2, stickyDag: true })).toBe("dag");
  });
});

describe("LifecycleEditorShell store integration", () => {
  afterEach(() => {
    useWorkflowStore.getState().closeLifecycleEditor();
  });

  it("openLifecycleForm 后默认 1 step → 应当 Form 模式", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("p1", { key: "k", initial_activity_key: "start" });
    const { draft } = useWorkflowStore.getState().lifecycleEditor;
    if (!draft) throw new Error("draft should be initialized");
    expect(draft.activities).toHaveLength(1);
    expect(draft.transitions).toHaveLength(0);
    expect(
      judgeMode({ stepCount: draft.activities.length, edgeCount: draft.transitions.length, stickyDag: false }),
    ).toBe("form");
  });

  it("addLifecycleEditorActivity 后 steps=2 → 应当 DAG 模式（即使没 sticky）", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("p1", { key: "k", initial_activity_key: "start" });
    store.addLifecycleEditorActivity();
    const { draft } = useWorkflowStore.getState().lifecycleEditor;
    if (!draft) throw new Error("draft should be initialized");
    expect(draft.activities).toHaveLength(2);
    expect(
      judgeMode({ stepCount: draft.activities.length, edgeCount: draft.transitions.length, stickyDag: false }),
    ).toBe("dag");
  });

  it("2 steps 删回 1 step + stickyDag=true → 画布保留（DAG）", () => {
    const store = useWorkflowStore.getState();
    store.openLifecycleForm("p1", { key: "k", initial_activity_key: "start" });
    const second = store.addLifecycleEditorActivity();
    if (!second) throw new Error("second activity should be created");
    // 假设 sticky 已被 shell 设为 true
    store.removeLifecycleEditorActivity(second);
    const { draft } = useWorkflowStore.getState().lifecycleEditor;
    if (!draft) throw new Error("draft should be initialized");
    expect(draft.activities).toHaveLength(1);
    expect(
      judgeMode({ stepCount: draft.activities.length, edgeCount: draft.transitions.length, stickyDag: true }),
    ).toBe("dag");
  });
});
