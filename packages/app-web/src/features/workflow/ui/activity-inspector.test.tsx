/**
 * ActivityInspector happy-path 渲染测试。
 *
 * 用 renderToStaticMarkup 校验三段（Identity / Executor / Ports & Policy）+
 * AgentProcedure Contract 折叠区均出现，覆盖 4 种 executor.kind 与 5 种
 * completion_policy.kind 的字段渲染入口。
 */

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ActivityCompletionPolicy, ActivityDefinition, ActivityExecutorSpec } from "../../../types";
import { createActivityProcedureDraft } from "../../../stores/workflowStore";
import { ActivityInspector } from "./activity-inspector";

function makeActivity(
  overrides: Partial<ActivityDefinition> = {},
): ActivityDefinition {
  return {
    key: "implement",
    description: "实现该需求",
    executor: {
      kind: "agent",
      procedure_key: "demo.implement",
      agent_reuse_policy: "create_activity_agent",
      runtime_session_policy: "create_new",
    },
    output_ports: [{ key: "done", description: "完成", gate_strategy: "existence" }],
    input_ports: [],
    completion_policy: { kind: "executor_terminal" },
    iteration_policy: { max_attempts: 3, artifact_alias: "per_attempt" },
    join_policy: { n_of_m: { n: 2 } },
    ...overrides,
  };
}

function renderInspector(activity: ActivityDefinition) {
  const draft = createActivityProcedureDraft("p1", "demo", activity.key, ["story"]);
  return renderToStaticMarkup(
    <ActivityInspector
      activity={activity}
      procedureDraft={draft}
      isEntry={false}
      availableProcedures={[]}
      hookPresets={[]}
      targetKinds={["story"]}
      projectId="p1"
      onActivityChange={() => undefined}
      onProcedureDraftChange={() => undefined}
      onSetExecutor={() => null}
      onSetCompletionPolicy={() => undefined}
      onSetIterationPolicy={() => undefined}
      onSetJoinPolicy={() => undefined}
      onSetEntry={() => undefined}
      onRemove={() => undefined}
      onClose={() => undefined}
    />,
  );
}

describe("ActivityInspector", () => {
  it("Agent activity 渲染 Activity/Contract tab + 三段 + iteration/join 折叠 + 主字段", () => {
    const markup = renderInspector(makeActivity());
    // 顶部 tab 切换：Agent 时两个 tab 都存在
    expect(markup).toContain(">Activity<");
    expect(markup).toContain(">Contract<");
    // Activity tab 默认渲染：三段标题 + 主字段
    expect(markup).toContain("Identity");
    expect(markup).toContain("Executor");
    expect(markup).toContain("Ports");
    // iteration / join 折叠在「高级」details 中（DOM 仍渲染但 summary 显示概要）
    expect(markup).toContain("高级（迭代 / 汇聚）");
    expect(markup).toContain("Iteration Policy");
    expect(markup).toContain("Join Policy");
    expect(markup).toContain("iter:3/per_attempt");
    expect(markup).toContain("n_of_m(2)");
    expect(markup).toContain("Procedure 来源");
    expect(markup).toContain("Agent Reuse");
    expect(markup).toContain("Runtime Session");
  });

  it("Function bash_exec executor 渲染 command/args/working_directory 字段", () => {
    const executor: ActivityExecutorSpec = {
      kind: "function",
      type: "bash_exec",
      command: "pnpm",
      args: ["test", "workflow"],
      working_directory: "/tmp",
    };
    const markup = renderInspector(
      makeActivity({
        executor,
        completion_policy: { kind: "output_ports", required_ports: ["done"] },
      }),
    );
    expect(markup).toContain("Command");
    expect(markup).toContain("Args");
    expect(markup).toContain("Working Directory");
    // Function api_request 表单字段不应出现
    expect(markup).not.toContain("URL Template");
    // AgentProcedure Contract 段（Agent only）也不应出现
    expect(markup).not.toContain("资产标准接口");
  });

  it("Function api_request executor 渲染 method/url_template/body_template 字段", () => {
    const executor: ActivityExecutorSpec = {
      kind: "function",
      type: "api_request",
      method: "POST",
      url_template: "https://x.example.com/y",
      body_template: { foo: "bar" },
    };
    const markup = renderInspector(
      makeActivity({
        executor,
        completion_policy: { kind: "executor_terminal" },
      }),
    );
    expect(markup).toContain("Method");
    expect(markup).toContain("URL Template");
    expect(markup).toContain("Body Template");
  });

  it("Human approval executor 渲染 title/form_schema_key 与 human_decision policy", () => {
    const executor: ActivityExecutorSpec = {
      kind: "human",
      type: "approval",
      form_schema_key: "approve",
      title: "等待审批",
    };
    const policy: ActivityCompletionPolicy = { kind: "human_decision", decision_port: "decision" };
    const markup = renderInspector(
      makeActivity({ executor, completion_policy: policy }),
    );
    expect(markup).toContain("Form Schema Key");
    expect(markup).toContain("Decision Port");
  });

  it("hook_gate completion_policy 渲染 hook_key 输入", () => {
    const markup = renderInspector(
      makeActivity({ completion_policy: { kind: "hook_gate", hook_key: "my_hook" } }),
    );
    expect(markup).toContain("Hook Key");
    expect(markup).toContain('value="my_hook"');
  });
});
