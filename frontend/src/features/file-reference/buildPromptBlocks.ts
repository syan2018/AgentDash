import type { ReadFileResult } from "../../services/workspaceFiles";
import type { ContentBlock } from "@agentclientprotocol/sdk";

/**
 * 从纯文本 prompt + 已读取的文件列表构建 ACP ContentBlock[]。
 * 当前文件引用场景会生成 text/resource/resource_link 三类 block。
 */
export function buildPromptBlocks(
  text: string,
  fileResults: ReadFileResult[],
): ContentBlock[] {
  const blocks: ContentBlock[] = [];

  const promptText = text.trim();

  if (promptText) {
    blocks.push({ type: "text", text: promptText });
  }

  for (const file of fileResults) {
    if (file.content != null && !file.error) {
      blocks.push({
        type: "resource",
        resource: {
          uri: file.uri,
          mimeType: file.mimeType || undefined,
          text: file.content,
        },
      });
    } else {
      blocks.push({
        type: "resource_link",
        uri: file.uri || `file:///${file.relPath}`,
        name: file.relPath,
        mimeType: file.mimeType || undefined,
        size: file.size || undefined,
      });
    }
  }

  return blocks;
}
