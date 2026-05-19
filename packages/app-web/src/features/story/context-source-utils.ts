import type { ContextSourceKind } from "../../types";

const SOURCE_KIND_META: Record<ContextSourceKind, { label: string; icon: string; color: string }> = {
  file: { label: "文件", icon: "FILE", color: "text-info" },
  manual_text: { label: "文本", icon: "TEXT", color: "text-success" },
  project_snapshot: { label: "快照", icon: "SNAP", color: "text-primary" },
  http_fetch: { label: "HTTP", icon: "HTTP", color: "text-warning" },
  mcp_resource: { label: "MCP", icon: "MCP", color: "text-accent-foreground" },
  entity_ref: { label: "实体", icon: "REF", color: "text-info" },
};

export function sourceKindMeta(kind: ContextSourceKind) {
  return SOURCE_KIND_META[kind] ?? { label: kind, icon: "CTX", color: "text-muted-foreground" };
}
