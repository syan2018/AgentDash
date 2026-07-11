import { createRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

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
      />,
    );
    expect(html).toContain("hello");
    expect(html).toContain("world");
    expect(html).toContain("lost");
  });
});
