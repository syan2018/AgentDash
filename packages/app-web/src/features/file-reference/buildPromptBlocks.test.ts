import { describe, expect, it } from "vitest";

import { buildPromptBlocks } from "./buildPromptBlocks";
import type { ReadFileResult } from "../../services/filePicker";

describe("buildPromptBlocks", () => {
  it("保留文本中的 @ 锚点并附加 resource block", () => {
    const files: ReadFileResult[] = [
      {
        relPath: "src/main.ts",
        uri: "file:///workspace/src/main.ts",
        mimeType: "text/typescript",
        content: "export const a = 1;",
        size: 19,
        error: null,
      },
    ];

    const blocks = buildPromptBlocks(
      "请在 @src/main.ts 下面插入日志",
      files,
    );

    expect(blocks).toHaveLength(2);
    expect(blocks[0]).toEqual({
      type: "text",
      text: "请在 @src/main.ts 下面插入日志",
    });
    expect(blocks[1]).toEqual({
      type: "resource",
      resource: {
        uri: "file:///workspace/src/main.ts",
        mimeType: "text/typescript",
        text: "export const a = 1;",
      },
    });
  });

  it("文件读取失败时降级为 resource_link", () => {
    const files: ReadFileResult[] = [
      {
        relPath: "src/missing.ts",
        uri: "file:///workspace/src/missing.ts",
        mimeType: "text/typescript",
        content: null,
        size: 0,
        error: "not found",
      },
    ];

    const blocks = buildPromptBlocks("查看 @src/missing.ts", files);

    expect(blocks[1]).toEqual({
      type: "resource_link",
      uri: "file:///workspace/src/missing.ts",
      name: "src/missing.ts",
      mimeType: "text/typescript",
      size: undefined,
    });
  });
});
