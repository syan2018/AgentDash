import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { AgentDashThreadItem } from "../../../../generated/backbone-protocol";
import { ReadCardBody } from "./ReadCardBody";

describe("ReadCardBody", () => {
  it("renders image content returned by fs_read", () => {
    const item: Extract<AgentDashThreadItem, { type: "fsRead" }> = {
      type: "fsRead",
      id: "read-image-1",
      path: "main://logo.png",
      offset: null,
      limit: null,
      arguments: { path: "main://logo.png" },
      status: "completed",
      contentItems: [{
        type: "inputImage",
        imageUrl: "data:image/png;base64,AAAA",
      }],
      success: true,
    };

    const html = renderToStaticMarkup(<ReadCardBody item={item} />);

    expect(html).toContain("<img");
    expect(html).toContain("data:image/png;base64,AAAA");
    expect(html).not.toContain("尚无读取内容");
  });
});
