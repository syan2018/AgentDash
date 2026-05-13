import type { ContextSourceKind } from "../../types";

const SOURCE_KIND_META: Record<ContextSourceKind, { label: string; icon: string; color: string }> = {
  file: { label: "文件", icon: "📄", color: "text-blue-600" },
  manual_text: { label: "文本", icon: "📝", color: "text-emerald-600" },
  project_snapshot: { label: "快照", icon: "📸", color: "text-violet-600" },
  http_fetch: { label: "HTTP", icon: "🌐", color: "text-orange-600" },
  mcp_resource: { label: "MCP", icon: "🔌", color: "text-pink-600" },
  entity_ref: { label: "实体", icon: "🔗", color: "text-cyan-600" },
};

export function sourceKindMeta(kind: ContextSourceKind) {
  return SOURCE_KIND_META[kind] ?? { label: kind, icon: "📎", color: "text-muted-foreground" };
}
