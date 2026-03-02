import { describe, expect, it } from "vitest";
import type { ContentBlock } from "@agentclientprotocol/sdk";

import { extractTextFromContentBlock } from "./types";

describe("extractTextFromContentBlock", () => {
  it("返回 text block 原文", () => {
    const block: ContentBlock = {
      type: "text",
      text: "hello",
    };

    expect(extractTextFromContentBlock(block)).toBe("hello");
  });

  it("可以渲染 resource_link", () => {
    const block: ContentBlock = {
      type: "resource_link",
      name: "src/main.ts",
      uri: "file:///workspace/src/main.ts",
    };

    const rendered = extractTextFromContentBlock(block);
    expect(rendered).toContain("引用文件");
    expect(rendered).toContain("src/main.ts");
  });

  it("可以渲染 resource(text)", () => {
    const block: ContentBlock = {
      type: "resource",
      resource: {
        uri: "file:///workspace/src/main.ts",
        text: "console.log(1);",
        mimeType: "text/typescript",
      },
    };

    const rendered = extractTextFromContentBlock(block);
    expect(rendered).toContain("引用文件内容");
    expect(rendered).toContain("src/main.ts");
    expect(rendered).toContain("字符");
  });
});
