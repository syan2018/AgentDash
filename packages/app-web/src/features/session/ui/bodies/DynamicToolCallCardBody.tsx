/**
 * DynamicToolCall body — 按 tool 名分流到专用 renderer，未知 tool 走 GenericJsonBody。
 *
 * read → ReadCardBody（行号 + 折叠预览）
 * edit / str_replace_editor / write → DiffCardBody（unified diff 渲染）
 * applypatch → DiffCardBody（直接解析 patch 字段）
 * 其他 → GenericJsonBody（入参/出参双分区 JSON 树）
 */

import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { GenericJsonBody } from "./GenericJsonBody";
import { ReadCardBody } from "./ReadCardBody";
import { DiffCardBodyAuto } from "./DiffCardBody";
import { TaskToolCardBody } from "./TaskToolCardBody";

type DynamicItem = Extract<ThreadItem, { type: "dynamicToolCall" }>;

export function DynamicToolCallCardBody({ item }: { item: DynamicItem }) {
  const tool = item.tool.toLowerCase();
  const args = item.arguments as Record<string, unknown> | null;

  if (tool === "read") {
    return <ReadCardBody item={item} />;
  }

  if (tool === "task_read" || tool === "task_write") {
    return <TaskToolCardBody item={item} />;
  }

  if (tool === "edit" || tool === "str_replace_editor") {
    return (
      <DiffCardBodyAuto
        oldText={readStr(args, "old_string")}
        newText={readStr(args, "new_string")}
      />
    );
  }

  if (tool === "write") {
    return (
      <DiffCardBodyAuto
        oldText=""
        newText={readStr(args, "content") ?? readStr(args, "new_string") ?? ""}
      />
    );
  }

  if (tool === "applypatch") {
    return <DiffCardBodyAuto diff={readStr(args, "patch") ?? ""} />;
  }

  return (
    <GenericJsonBody
      arguments={item.arguments}
      contentItems={item.contentItems}
    />
  );
}

function readStr(
  args: Record<string, unknown> | null | undefined,
  key: string,
): string {
  if (!args) return "";
  const v = args[key];
  return typeof v === "string" ? v : "";
}
