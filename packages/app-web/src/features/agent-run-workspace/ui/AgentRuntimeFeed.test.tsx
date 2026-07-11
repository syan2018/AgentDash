import { createRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { interactionResponseFromText } from "../model/interactionResponse";
import { AgentRuntimeFeed } from "./AgentRuntimeFeed";

describe("AgentRuntimeFeed", () => {
  it("renders canonical transcript roles and terminal state", () => {
    const html = renderToStaticMarkup(
      <AgentRuntimeFeed
        containerRef={createRef<HTMLDivElement>()}
        entries={[
          { id: "user-1", turn_id: "turn-1", role: "user", text: "hello", status: "completed" },
          { id: "agent-1", turn_id: "turn-1", role: "agent", text: "world", status: "lost" },
        ]}
        isLoading={false}
        onScroll={vi.fn()}
        interactionAvailability={{ status: "available" }}
        onResolveInteraction={vi.fn()}
      />,
    );
    expect(html).toContain("hello");
    expect(html).toContain("world");
    expect(html).toContain("lost");
  });

  it("offers approve and deny actions only for typed approval interactions", () => {
    const html = renderToStaticMarkup(
      <AgentRuntimeFeed
        containerRef={createRef<HTMLDivElement>()}
        entries={[{
          id: "interaction:approval-1",
          turn_id: "turn-1",
          role: "system",
          text: "Allow command?",
          status: "streaming",
          interaction: {
            interaction_id: "approval-1",
            interaction_kind: "command_approval",
            terminal: null,
          },
        }]}
        isLoading={false}
        onScroll={vi.fn()}
        interactionAvailability={{ status: "available" }}
        onResolveInteraction={vi.fn()}
      />,
    );

    expect(html).toContain("命令审批");
    expect(html).toContain("批准");
    expect(html).toContain("拒绝");
  });

  it("renders canonical interaction unavailability reason", () => {
    const html = renderToStaticMarkup(
      <AgentRuntimeFeed
        containerRef={createRef<HTMLDivElement>()}
        entries={[{
          id: "interaction:input-1",
          turn_id: "turn-1",
          role: "system",
          text: "Need input",
          status: "streaming",
          interaction: {
            interaction_id: "input-1",
            interaction_kind: "user_input_request",
            terminal: null,
          },
        }]}
        isLoading={false}
        onScroll={vi.fn()}
        interactionAvailability={{
          status: "unavailable",
          unmet: [{ kind: "pending_interaction" }],
          reason: "interaction response is unavailable",
        }}
        onResolveInteraction={vi.fn()}
      />,
    );

    expect(html).toContain("interaction response is unavailable");
    expect(html).toContain("disabled");
  });

  it("builds explicit typed input and JSON responses", () => {
    expect(interactionResponseFromText("user_input_request", " answer ")).toEqual({
      ok: true,
      response: { kind: "user_input", input: [{ kind: "text", text: "answer" }] },
    });
    expect(interactionResponseFromText("mcp_elicitation", '{"approved":true}')).toEqual({
      ok: true,
      response: { kind: "mcp_elicitation", value: { approved: true } },
    });
    expect(interactionResponseFromText("dynamic_tool_execution", "not-json")).toEqual({
      ok: false,
      error: "请输入有效 JSON。",
    });
  });
});
