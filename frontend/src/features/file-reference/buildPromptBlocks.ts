import type { ReadFileResult } from "../../services/workspaceFiles";

/**
 * ACP ContentBlock 类型（仅包含 prompt 构建所需的子集）
 */
export type PromptContentBlock =
  | { type: "text"; text: string }
  | {
      type: "resource";
      resource: {
        uri: string;
        mimeType?: string;
        text: string;
      };
    }
  | {
      type: "resource_link";
      uri: string;
      name: string;
      mimeType?: string;
      size?: number;
    };

/**
 * 从纯文本 prompt + 已读取的文件列表构建 ACP ContentBlock[]。
 *
 * - text block 放在最前面（去掉 @path 引用标记）
 * - resource blocks 依次附加在后面
 * - 文件读取失败的降级为 resource_link
 */
export function buildPromptBlocks(
  text: string,
  fileResults: ReadFileResult[],
  referencedPaths: string[],
): PromptContentBlock[] {
  const blocks: PromptContentBlock[] = [];

  let cleanedText = text;
  for (const p of referencedPaths) {
    cleanedText = cleanedText.replaceAll(`@${p}`, p);
  }
  cleanedText = cleanedText.trim();

  if (cleanedText) {
    blocks.push({ type: "text", text: cleanedText });
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
