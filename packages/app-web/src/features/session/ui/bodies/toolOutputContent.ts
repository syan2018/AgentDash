/**
 * 工具出参 normalize 层
 *
 * 将 DynamicToolCallOutputContentItem[] 和 MCP result.content
 * 统一转为 UI 内部 ToolOutputBlock[]，供 ToolOutputContentViewer 渲染。
 */

import type {
  DynamicToolCallOutputContentItem,
  JsonValue,
} from "../../../../generated/backbone-protocol";

// ─── view model ────────────────────────────────────────────

export type ToolOutputBlock =
  | { kind: "text"; text: string }
  | { kind: "image"; imageUrl: string; label?: string }
  | { kind: "resource"; uri: string; label?: string; text?: string }
  | { kind: "json"; value: unknown; label?: string };

// ─── dynamic (native / PI_AGENT) ──────────────────────────

export function normalizeDynamicOutput(
  items: DynamicToolCallOutputContentItem[] | null | undefined,
): ToolOutputBlock[] {
  if (!items || items.length === 0) return [];
  const blocks: ToolOutputBlock[] = [];
  for (const item of items) {
    switch (item.type) {
      case "inputText":
        blocks.push({ kind: "text", text: item.text });
        break;
      case "inputImage":
        blocks.push({ kind: "image", imageUrl: item.imageUrl });
        break;
      default:
        blocks.push({ kind: "json", value: item });
    }
  }
  return mergeAdjacentText(blocks);
}

// ─── MCP ───────────────────────────────────────────────────

export function normalizeMcpOutput(
  content: JsonValue[] | null | undefined,
): ToolOutputBlock[] {
  if (!content || !Array.isArray(content) || content.length === 0) return [];
  const blocks: ToolOutputBlock[] = [];
  for (const item of content) {
    if (!item || typeof item !== "object" || Array.isArray(item)) {
      blocks.push({ kind: "json", value: item });
      continue;
    }
    const obj = item as Record<string, unknown>;
    const type = obj["type"];
    switch (type) {
      case "text":
        if (typeof obj["text"] === "string") {
          blocks.push({ kind: "text", text: obj["text"] });
        } else {
          blocks.push({ kind: "json", value: item });
        }
        break;
      case "image": {
        const data = obj["data"];
        const mimeType = typeof obj["mimeType"] === "string" ? obj["mimeType"] : "image/png";
        if (typeof data === "string") {
          blocks.push({
            kind: "image",
            imageUrl: `data:${mimeType};base64,${data}`,
          });
        } else {
          blocks.push({ kind: "json", value: item });
        }
        break;
      }
      case "resource": {
        const res = obj["resource"];
        if (res && typeof res === "object" && !Array.isArray(res)) {
          const r = res as Record<string, unknown>;
          blocks.push({
            kind: "resource",
            uri: typeof r["uri"] === "string" ? r["uri"] : "",
            label: typeof r["name"] === "string" ? r["name"] : undefined,
            text: typeof r["text"] === "string" ? r["text"] : undefined,
          });
        } else {
          blocks.push({ kind: "json", value: item });
        }
        break;
      }
      case "resource_link":
        blocks.push({
          kind: "resource",
          uri: typeof obj["uri"] === "string" ? obj["uri"] : "",
          label: typeof obj["name"] === "string" ? obj["name"] : undefined,
        });
        break;
      default:
        blocks.push({ kind: "json", value: item });
    }
  }
  return mergeAdjacentText(blocks);
}

// ─── helpers ───────────────────────────────────────────────

function mergeAdjacentText(blocks: ToolOutputBlock[]): ToolOutputBlock[] {
  if (blocks.length <= 1) return blocks;
  const merged: ToolOutputBlock[] = [];
  for (const b of blocks) {
    const prev = merged[merged.length - 1];
    if (b.kind === "text" && prev?.kind === "text") {
      merged[merged.length - 1] = { kind: "text", text: prev.text + "\n" + b.text };
    } else {
      merged.push(b);
    }
  }
  return merged;
}
