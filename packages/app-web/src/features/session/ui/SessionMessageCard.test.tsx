import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { SessionMessageCard } from "./SessionMessageCard";

describe("SessionMessageCard thinking", () => {
  it("renders empty streaming thinking as a compact non-empty row", () => {
    const html = renderToStaticMarkup(
      <SessionMessageCard type="thinking" content="" isStreaming />,
    );

    expect(html).toContain("THINK");
    expect(html).toContain("正在思考");
    expect(html).not.toContain("<pre");
    expect(html).toContain("rounded-[6px]");
  });

  it("renders streaming thinking content with expandable body", () => {
    const html = renderToStaticMarkup(
      <SessionMessageCard
        type="thinking"
        content="分析中"
        isStreaming
        defaultCollapsed={false}
      />,
    );

    expect(html).toContain("THINK");
    expect(html).toContain("正在思考");
    expect(html).toContain("分析中");
    expect(html).toContain("border-l border-border/40");
  });

  it("renders historical thinking as thinking history", () => {
    const html = renderToStaticMarkup(
      <SessionMessageCard
        type="thinking"
        content="已经想完"
        defaultCollapsed={false}
      />,
    );

    expect(html).toContain("THINK");
    expect(html).toContain("思考");
    expect(html).toContain("已经想完");
    expect(html).not.toContain("正在思考");
  });
});
