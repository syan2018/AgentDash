/**
 * TransitionInspector happy-path 渲染测试。
 *
 * 覆盖：flow vs artifact 两种 kind、ConditionEditor 4 种 kind 的字段渲染、
 * artifact 模式下 ArtifactBindingsEditor 的列表渲染。
 */

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { ActivityDefinition, ActivityTransition } from "../../../types";
import { TransitionInspector } from "./transition-inspector";

function activity(key: string, overrides: Partial<ActivityDefinition> = {}): ActivityDefinition {
  return {
    key,
    description: "",
    executor: { kind: "agent", workflow_key: `demo.${key}`, session_policy: "spawn_child" },
    output_ports: [],
    input_ports: [],
    completion_policy: { kind: "executor_terminal" },
    iteration_policy: { max_attempts: 1, artifact_alias: "latest" },
    join_policy: "all",
    ...overrides,
  };
}

const activities: ActivityDefinition[] = [
  activity("plan", {
    output_ports: [{ key: "spec", description: "", gate_strategy: "existence" }],
  }),
  activity("approve", {
    output_ports: [{ key: "decision", description: "", gate_strategy: "existence" }],
  }),
  activity("implement", {
    input_ports: [{
      key: "spec",
      description: "",
      context_strategy: "full",
      standalone_fulfillment: "required",
    }],
  }),
];

function render(transition: ActivityTransition) {
  return renderToStaticMarkup(
    <TransitionInspector
      transition={transition}
      activities={activities}
      onClose={() => undefined}
      onSetKind={() => undefined}
      onConditionChange={() => undefined}
      onMaxTraversalsChange={() => undefined}
      onAddBinding={() => undefined}
      onUpdateBinding={() => undefined}
      onRemoveBinding={() => undefined}
    />,
  );
}

describe("TransitionInspector", () => {
  it("flow + always 渲染 condition select 与 max_traversals", () => {
    const markup = render({
      kind: "flow",
      from: "plan",
      to: "implement",
      condition: { kind: "always" },
      artifact_bindings: [],
    });
    expect(markup).toContain("plan → implement");
    expect(markup).toContain("Kind");
    expect(markup).toContain("Condition");
    expect(markup).toContain("Max Traversals");
    expect(markup).not.toContain("Artifact Bindings");
  });

  it("flow + human_decision_equals 渲染 decision_port + value select", () => {
    const markup = render({
      kind: "flow",
      from: "approve",
      to: "implement",
      condition: {
        kind: "human_decision_equals",
        activity: "approve",
        decision_port: "decision",
        value: "approved",
      },
      artifact_bindings: [],
    });
    expect(markup).toContain("Human Decision Equals");
    expect(markup).toContain("Decision Port");
    expect(markup).toContain("approved");
    expect(markup).toContain("rejected");
  });

  it("flow + agent_signal_equals 渲染 signal_key + value 输入", () => {
    const markup = render({
      kind: "flow",
      from: "plan",
      to: "implement",
      condition: {
        kind: "agent_signal_equals",
        activity: "plan",
        signal_key: "status",
        value: "completed",
      },
      artifact_bindings: [],
    });
    expect(markup).toContain("Agent Signal Equals");
    expect(markup).toContain("Signal Key");
  });

  it("artifact + artifact_field_equals 渲染 bindings 列表 + path/value", () => {
    const markup = render({
      kind: "artifact",
      from: "plan",
      to: "implement",
      condition: {
        kind: "artifact_field_equals",
        activity: "plan",
        port: "spec",
        path: "$.status",
        value: "ok",
      },
      max_traversals: 3,
      artifact_bindings: [
        { from_activity: "plan", from_port: "spec", to_port: "spec", alias: "latest" },
      ],
    });
    expect(markup).toContain("Artifact Field Equals");
    expect(markup).toContain("JSON Path");
    expect(markup).toContain("Artifact Bindings (1)");
    expect(markup).toContain("From Activity");
    expect(markup).toContain("Alias");
  });

  it("artifact 模式空 bindings 渲染空状态提示", () => {
    const markup = render({
      kind: "artifact",
      from: "plan",
      to: "implement",
      condition: { kind: "always" },
      artifact_bindings: [],
    });
    expect(markup).toContain("Artifact Bindings (0)");
    expect(markup).toContain("artifact transition 至少需要一条");
  });
});
