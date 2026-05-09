/**
 * Runtime Context Notice 卡片
 *
 * 展示后端实际注入给 Agent 的 runtime steering 结构，并保留 Agent 可见原文。
 */

import { useState } from "react";
import { BADGE } from "./EventCards";
import {
  parseRuntimeContextNotice,
  type CapabilityDeltaSection,
  type RuntimeContextNotice,
  type RuntimeContextNoticeSection,
  type RuntimeHookInjectionEntry,
  type RuntimeToolSchemaEntry,
  type SystemNoticeSection,
  type ToolSchemaDeltaSection,
  type ToolSchemaSection,
} from "../model/runtimeContextNotice";
import { isRecord } from "../model/platformEvent";

export interface RuntimeContextNoticeCardProps {
  data: Record<string, unknown>;
}

export function RuntimeContextNoticeCard({ data }: RuntimeContextNoticeCardProps) {
  const notice = parseRuntimeContextNotice(data);
  const [expanded, setExpanded] = useState(false);

  if (!notice) {
    return null;
  }

  const summary = summarizeNotice(notice);
  const rightHint = [
    notice.phase_node ? `阶段 ${notice.phase_node}` : null,
    notice.apply_mode ?? null,
  ].filter((item): item is string => item != null).join(" · ");

  return (
    <div className="rounded-[12px] border border-border bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setExpanded((v) => !v)}
        className="flex w-full items-center gap-2.5 px-3 py-2.5 text-left transition-colors cursor-pointer hover:bg-secondary/35"
      >
        <span className={`inline-flex shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] ${BADGE.neutral}`}>
          CTX
        </span>
        <span className="min-w-0 flex-1 truncate text-sm text-foreground/80">
          Agent 上下文已更新
        </span>
        {summary.length > 0 && (
          <span className="hidden min-w-0 flex-1 truncate text-xs text-muted-foreground/60 md:block">
            {summary.join("，")}
          </span>
        )}
        {rightHint && (
          <span className="shrink-0 text-[10px] text-muted-foreground/50">
            {rightHint}
          </span>
        )}
        <span className="shrink-0 text-[10px] text-muted-foreground/40">
          {expanded ? "▲" : "▼"}
        </span>
      </button>

      {expanded && (
        <div className="border-t border-border px-3 py-2.5 space-y-2.5">
          <div className="flex flex-wrap gap-1.5">
            <Chip label={`source: ${notice.source}`} />
            <Chip label={`delivery: ${notice.delivery_status}`} />
            <Chip label={`sections: ${notice.sections.length}`} />
          </div>
          {notice.sections.map((section, index) => (
            <NoticeSection key={`${section.kind}:${index}`} section={section} />
          ))}
          <AgentVisibleText text={notice.agent_visible_text} />
        </div>
      )}
    </div>
  );
}

function NoticeSection({ section }: { section: RuntimeContextNoticeSection }) {
  const [open, setOpen] = useState(false);
  const title = sectionTitle(section);
  const hint = sectionHint(section);

  return (
    <div className="rounded-[8px] border border-border/70 bg-secondary/20 overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-2.5 py-2 text-left hover:bg-secondary/35"
      >
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground/80">
          {title}
        </span>
        {hint && <span className="shrink-0 text-[10px] text-muted-foreground/50">{hint}</span>}
        <span className="shrink-0 text-[10px] text-muted-foreground/40">{open ? "▲" : "▼"}</span>
      </button>
      {open && (
        <div className="border-t border-border/70 px-2.5 py-2 space-y-2">
          {renderSectionBody(section)}
        </div>
      )}
    </div>
  );
}

function renderSectionBody(section: RuntimeContextNoticeSection) {
  switch (section.kind) {
    case "capability_delta":
      return <CapabilityDeltaBody section={section} />;
    case "tool_schema":
      return <ToolSchemaBody section={section} />;
    case "tool_schema_delta":
      return <ToolSchemaDeltaBody section={section} />;
    case "workflow_context":
    case "hook_injection":
      return <InjectionBody title={section.title} summary={section.summary} injections={section.injections} />;
    case "system_notice":
      return <SystemNoticeBody section={section} />;
  }
}

function CapabilityDeltaBody({ section }: { section: CapabilityDeltaSection }) {
  const rows: Array<[string, string[]]> = [
    ["新增能力", section.added_capabilities],
    ["移除能力", section.removed_capabilities],
    ["当前能力", section.effective_capabilities],
    ["屏蔽工具", section.blocked_tool_paths],
    ["恢复工具", section.unblocked_tool_paths],
    ["白名单工具", section.whitelisted_tool_paths],
    ["移出白名单", section.removed_whitelist_paths],
    ["新增 MCP", section.added_mcp_servers],
    ["移除 MCP", section.removed_mcp_servers],
    ["变更 MCP", section.changed_mcp_servers],
    ["新增挂载", section.vfs_mounts_added],
    ["移除挂载", section.vfs_mounts_removed],
  ];

  return (
    <div className="space-y-1.5">
      {rows.map(([label, values]) =>
        values.length > 0 ? <ListLine key={label} label={label} values={values} /> : null
      )}
      {section.default_mount_before !== section.default_mount_after && (
        <p className="text-xs leading-5 text-muted-foreground">
          默认挂载：{section.default_mount_before ?? "none"} → {section.default_mount_after ?? "none"}
        </p>
      )}
    </div>
  );
}

function ToolSchemaBody({ section }: { section: ToolSchemaSection }) {
  if (section.tools.length === 0) {
    return <p className="text-xs text-muted-foreground">暂无工具 schema</p>;
  }
  return (
    <div className="space-y-2">
      {section.tools.map((tool) => (
        <ToolSchemaItem key={tool.name} tool={tool} />
      ))}
    </div>
  );
}

function ToolSchemaDeltaBody({ section }: { section: ToolSchemaDeltaSection }) {
  const rows: Array<[string, string[]]> = [
    ["恢复工具", section.restored_tool_paths],
    ["屏蔽工具", section.blocked_tool_paths],
    ["移除工具", section.removed_tool_paths],
  ];
  return (
    <div className="space-y-2">
      <div className="space-y-1.5">
        {rows.map(([label, values]) =>
          values.length > 0 ? <ListLine key={label} label={label} values={values} /> : null
        )}
      </div>
      {section.added_tools.length > 0 && (
        <div className="space-y-2">
          {section.added_tools.map((tool) => (
            <ToolSchemaItem key={tool.name} tool={tool} />
          ))}
        </div>
      )}
    </div>
  );
}

function ToolSchemaItem({ tool }: { tool: RuntimeToolSchemaEntry }) {
  const [open, setOpen] = useState(false);
  const fieldNames = schemaFieldNames(tool.parameters_schema);
  return (
    <div className="rounded-[6px] border border-border/70 bg-background px-2.5 py-2">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-start gap-2 text-left"
      >
        <span className="min-w-0 flex-1">
          <span className="block truncate font-mono text-[11px] text-foreground/80">{tool.name}</span>
          {tool.description && (
            <span className="block truncate text-[11px] text-muted-foreground">{tool.description}</span>
          )}
          {(tool.capability_key || tool.source) && (
            <span className="mt-1 flex flex-wrap gap-1">
              {tool.capability_key && <Chip label={tool.capability_key} />}
              {tool.source && <Chip label={tool.source} />}
              {tool.tool_path && <Chip label={tool.tool_path} />}
            </span>
          )}
        </span>
        {fieldNames.length > 0 && (
          <span className="shrink-0 text-[10px] text-muted-foreground/50">
            {fieldNames.slice(0, 3).join("，")}{fieldNames.length > 3 ? ` 等 ${fieldNames.length} 项` : ""}
          </span>
        )}
        <span className="shrink-0 text-[10px] text-muted-foreground/40">{open ? "▲" : "▼"}</span>
      </button>
      {open && (
        <pre className="mt-2 max-h-64 overflow-auto rounded-[6px] border border-border/70 bg-secondary/20 p-2 text-[11px] leading-relaxed text-muted-foreground">
          {formatJson(tool.parameters_schema)}
        </pre>
      )}
    </div>
  );
}

function InjectionBody({
  title,
  summary,
  injections,
}: {
  title: string;
  summary: string;
  injections: RuntimeHookInjectionEntry[];
}) {
  return (
    <div className="space-y-2">
      <p className="text-xs leading-5 text-muted-foreground">{title}：{summary}</p>
      {injections.map((injection, index) => (
        <div key={`${injection.slot}:${injection.source}:${index}`} className="space-y-1 rounded-[6px] border border-border/70 bg-background px-2.5 py-2">
          <div className="flex flex-wrap gap-1.5">
            <Chip label={injection.slot || "slot"} />
            <Chip label={injection.source || "unknown"} />
          </div>
          {injection.content && (
            <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-xs leading-relaxed text-foreground/75">
              {injection.content}
            </pre>
          )}
        </div>
      ))}
    </div>
  );
}

function SystemNoticeBody({ section }: { section: SystemNoticeSection }) {
  return (
    <div className="space-y-1.5">
      <p className="text-xs leading-5 text-muted-foreground">{section.title}：{section.summary}</p>
      {section.body && (
        <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-xs leading-relaxed text-foreground/75">
          {section.body}
        </pre>
      )}
    </div>
  );
}

function AgentVisibleText({ text }: { text: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="rounded-[8px] border border-border/70 bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-2.5 py-2 text-left hover:bg-secondary/35"
      >
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground/80">
          Agent 实际收到的文本
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground/40">{open ? "▲" : "▼"}</span>
      </button>
      {open && (
        <pre className="max-h-96 overflow-auto whitespace-pre-wrap border-t border-border/70 bg-secondary/20 p-2.5 text-xs leading-relaxed text-foreground/75">
          {text}
        </pre>
      )}
    </div>
  );
}

function Chip({ label }: { label: string }) {
  return (
    <span className="rounded-[6px] border border-border bg-secondary/35 px-1.5 py-0.5 text-[10px] text-muted-foreground/60">
      {label}
    </span>
  );
}

function ListLine({ label, values }: { label: string; values: string[] }) {
  return (
    <p className="text-xs leading-5 text-muted-foreground">
      {label}：{values.join("，")}
    </p>
  );
}

function sectionTitle(section: RuntimeContextNoticeSection): string {
  switch (section.kind) {
    case "capability_delta": return "能力与工具变化";
    case "tool_schema": return "初始工具 Schema";
    case "tool_schema_delta": return "工具 Schema 变化";
    case "workflow_context": return section.title || "Workflow Context";
    case "hook_injection": return section.title || "Hook Injection";
    case "system_notice": return section.title || "系统通知";
  }
}

function sectionHint(section: RuntimeContextNoticeSection): string | null {
  switch (section.kind) {
    case "capability_delta": {
      const count =
        section.added_capabilities.length +
        section.removed_capabilities.length +
        section.blocked_tool_paths.length +
        section.unblocked_tool_paths.length +
        section.whitelisted_tool_paths.length +
        section.removed_whitelist_paths.length;
      return count > 0 ? `${count} 项变化` : "无变化";
    }
    case "tool_schema": return `${section.tools.length} 个工具`;
    case "tool_schema_delta": {
      const count = toolSchemaDeltaAffectedCount(section);
      return count > 0 ? `${count} 项变化` : "无变化";
    }
    case "workflow_context":
    case "hook_injection": return `${section.injections.length} 项注入`;
    case "system_notice": return null;
  }
}

function toolSchemaDeltaAffectedCount(section: ToolSchemaDeltaSection): number {
  const affected = new Set<string>();
  for (const path of section.removed_tool_paths) affected.add(path);
  for (const path of section.restored_tool_paths) affected.add(path);
  for (const path of section.blocked_tool_paths) affected.add(path);
  for (const tool of section.added_tools) {
    affected.add(tool.tool_path ?? tool.name);
  }
  return affected.size;
}

function summarizeNotice(notice: RuntimeContextNotice): string[] {
  const parts: string[] = [];
  for (const section of notice.sections) {
    const hint = sectionHint(section);
    if (hint) parts.push(`${sectionTitle(section)} ${hint}`);
  }
  return parts.slice(0, 3);
}

function schemaFieldNames(schema: unknown): string[] {
  if (!isRecord(schema)) return [];
  const properties = schema.properties;
  if (!isRecord(properties)) return [];
  return Object.keys(properties);
}

function formatJson(value: unknown): string {
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return String(value);
  }
}

export default RuntimeContextNoticeCard;
