/**
 * ContextFrame Section 渲染器集合
 *
 * 每个 section.kind 对应一组渲染函数，按 PRD「各 section body 的渲染规则」章节
 * 输出单列长页正文。严格遵循 badge-only 原则：
 *
 * - diff 只使用 `+ / − / ↻` 纯符号标记，不额外染色
 * - section header 的徽标由 `sectionKindToToken` 决定颜色变体
 * - 超长内容使用 `max-h-*` + 内部滚动，不过滤、不截断
 */

import { useState } from "react";
import { BADGE } from "../EventCards";
import type {
  AutoResumeSection,
  CapabilityDeltaSection,
  CompactionSummarySection,
  ContinuationContextSection,
  ContextFrameSection,
  ContextTokenInfo,
  IdentitySection,
  AssignmentContextSection,
  PendingActionSection,
  RuntimeContextFragmentEntry,
  RuntimeHookInjectionEntry,
  RuntimeSkillEntry,
  SkillDeltaSection,
  RuntimeToolSchemaEntry,
  SystemNoticeSection,
  ToolSchemaDeltaSection,
  ToolSchemaSection,
} from "../../model/contextFrame";
import { sectionKindToToken } from "../../model/contextFrame";
import { isRecord } from "../../model/platformEvent";

// ─── section header + body 组合 ──────────────────────────────────────────────

/** 渲染单个 section：顶部一行 token badge + 标题 + 计数；下方直出 body，不再独立折叠 */
export function SectionBlock({ section }: { section: ContextFrameSection }) {
  const token = sectionKindToToken(section.kind);
  const title = sectionTitle(section);
  const hint = sectionHint(section);

  return (
    <section className="space-y-2 rounded-[8px] border border-border/70 bg-secondary/15 px-3 py-2.5">
      <header className="flex items-center gap-2">
        <TokenBadge token={token} />
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground/85">
          {title}
        </span>
        {hint && (
          <span className="shrink-0 text-[10px] text-muted-foreground/60">{hint}</span>
        )}
      </header>
      <div className="space-y-2">{renderSectionBody(section)}</div>
    </section>
  );
}

function sectionTitle(section: ContextFrameSection): string {
  switch (section.kind) {
    case "identity":
      return section.title || "Identity";
    case "assignment_context":
      return section.title || "Assignment Context";
    case "continuation_context":
      return section.title || "Session Continuation";
    case "capability_delta":
      return "能力与工具变化";
    case "tool_schema":
      return "初始工具 Schema";
    case "tool_schema_delta":
      return "工具 Schema 变化";
    case "skill_delta":
      return "Skill 变化";
    case "hook_injection":
      return section.title || "Hook Injection";
    case "system_notice":
      return section.title || "系统通知";
    case "pending_action":
      return section.title || "Pending Action";
    case "auto_resume":
      return section.title || "Auto Resume";
    case "compaction_summary":
      return section.title || "Compaction Summary";
  }
}

function sectionHint(section: ContextFrameSection): string | null {
  switch (section.kind) {
    case "identity":
      return section.mode || "append";
    case "assignment_context":
      return `${section.fragments.length} 个片段`;
    case "continuation_context":
      return section.summary || null;
    case "capability_delta": {
      const added =
        section.added_capabilities.length +
        section.unblocked_tool_paths.length +
        section.whitelisted_tool_paths.length +
        section.added_mcp_servers.length +
        section.vfs_mounts_added.length;
      const removed =
        section.removed_capabilities.length +
        section.blocked_tool_paths.length +
        section.removed_whitelist_paths.length +
        section.removed_mcp_servers.length +
        section.vfs_mounts_removed.length;
      const changed = section.changed_mcp_servers.length;
      const total = added + removed + changed;
      if (total === 0) return "无变化";
      return `+${added} −${removed}${changed > 0 ? ` ↻${changed}` : ""}`;
    }
    case "tool_schema":
      return `${section.tools.length} 个工具`;
    case "tool_schema_delta": {
      // 路径级变化归 CAP；TOOL 只统计真正新增给 Agent 的工具 schema 数。
      const count = section.added_tools.length;
      return count > 0 ? `${count} 项变化` : "无变化";
    }
    case "skill_delta": {
      const added = section.added_skills.length;
      const removed = section.removed_skills.length;
      const changed = section.changed_skills.length;
      if (added + removed + changed === 0) return "无变化";
      return `+${added} −${removed}${changed > 0 ? ` ↻${changed}` : ""}`;
    }
    case "hook_injection":
      return `${section.injections.length} 项注入`;
    case "system_notice":
      return null;
    case "pending_action":
      return section.status || "pending";
    case "auto_resume":
      return section.reason || "系统续跑";
    case "compaction_summary":
      return `${section.messages_compacted} 条消息`;
  }
}

function renderSectionBody(section: ContextFrameSection) {
  switch (section.kind) {
    case "identity":
      return <IdentityBody section={section} />;
    case "assignment_context":
      return <AssignmentContextBody section={section} />;
    case "continuation_context":
      return <ContinuationContextBody section={section} />;
    case "capability_delta":
      return <CapabilityDeltaBody section={section} />;
    case "tool_schema":
      return <ToolSchemaBody section={section} />;
    case "tool_schema_delta":
      return <ToolSchemaDeltaBody section={section} />;
    case "skill_delta":
      return <SkillDeltaBody section={section} />;
    case "hook_injection":
      return <InjectionBody injections={section.injections} />;
    case "system_notice":
      return <SystemNoticeBody section={section} />;
    case "pending_action":
      return <PendingActionBody section={section} />;
    case "auto_resume":
      return <AutoResumeBody section={section} />;
    case "compaction_summary":
      return <CompactionSummaryBody section={section} />;
  }
}

// ─── 各 section body ─────────────────────────────────────────────────────────

function IdentityBody({ section }: { section: IdentitySection }) {
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap gap-1.5">
        <Chip label={`mode: ${section.mode || "append"}`} />
      </div>
      {section.summary && (
        <p className="text-xs leading-relaxed text-foreground/75">{section.summary}</p>
      )}
      <pre className="max-h-64 overflow-auto whitespace-pre-wrap rounded-[6px] border border-border/70 bg-background p-2 text-xs leading-relaxed text-foreground/75">
        {section.effective_prompt || section.base_prompt}
      </pre>
    </div>
  );
}

function AssignmentContextBody({ section }: { section: AssignmentContextSection }) {
  if (section.fragments.length === 0) {
    return <p className="text-xs text-muted-foreground/60">暂无片段</p>;
  }
  return (
    <div className="space-y-2">
      {section.fragments.map((fragment, index) => (
        <FragmentItem
          key={`${fragment.slot}:${fragment.source}:${index}`}
          fragment={fragment}
        />
      ))}
    </div>
  );
}

function ContinuationContextBody({ section }: { section: ContinuationContextSection }) {
  return (
    <div className="space-y-2">
      {section.summary && (
        <p className="text-xs leading-relaxed text-foreground/75">{section.summary}</p>
      )}
      {section.owner_context && (
        <pre className="max-h-40 overflow-auto whitespace-pre-wrap rounded-[6px] border border-border/70 bg-background p-2 text-xs leading-relaxed text-foreground/75">
          {section.owner_context}
        </pre>
      )}
      {section.transcript_markdown ? (
        <pre className="max-h-72 overflow-auto whitespace-pre-wrap rounded-[6px] border border-border/70 bg-background p-2 text-xs leading-relaxed text-foreground/75">
          {section.transcript_markdown}
        </pre>
      ) : (
        <p className="text-xs text-muted-foreground/60">暂无可恢复 transcript</p>
      )}
    </div>
  );
}

function FragmentItem({ fragment }: { fragment: RuntimeContextFragmentEntry }) {
  return (
    <article className="space-y-1 rounded-[6px] border border-border/70 bg-background px-2.5 py-2">
      <div className="flex flex-wrap gap-1.5">
        <Chip label={fragment.slot || "slot"} />
        <Chip label={fragment.label || "context"} />
        <Chip label={fragment.source || "unknown"} />
      </div>
      {fragment.content && (
        <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-xs leading-relaxed text-foreground/75">
          {fragment.content}
        </pre>
      )}
    </article>
  );
}

function CapabilityDeltaBody({ section }: { section: CapabilityDeltaSection }) {
  // 顺序：增 → 减 → 变更
  const added: Array<{ label: string; value: string }> = [
    ...section.added_capabilities.map((value) => ({ label: "能力", value })),
    ...section.unblocked_tool_paths.map((value) => ({ label: "工具解除屏蔽", value })),
    ...section.whitelisted_tool_paths.map((value) => ({ label: "工具加入白名单", value })),
    ...section.added_mcp_servers.map((value) => ({ label: "MCP", value })),
    ...section.vfs_mounts_added.map((value) => ({ label: "挂载", value })),
  ];
  const removed: Array<{ label: string; value: string }> = [
    ...section.removed_capabilities.map((value) => ({ label: "能力", value })),
    ...section.blocked_tool_paths.map((value) => ({ label: "工具屏蔽", value })),
    ...section.removed_whitelist_paths.map((value) => ({ label: "工具移出白名单", value })),
    ...section.removed_mcp_servers.map((value) => ({ label: "MCP", value })),
    ...section.vfs_mounts_removed.map((value) => ({ label: "挂载", value })),
  ];
  const changed: Array<{ label: string; value: string }> = [
    ...section.changed_mcp_servers.map((value) => ({ label: "MCP", value })),
  ];

  const defaultMountChanged =
    (section.default_mount_before ?? null) !== (section.default_mount_after ?? null);

  const hasDiff = added.length + removed.length + changed.length > 0 || defaultMountChanged;

  return (
    <div className="space-y-2">
      {hasDiff ? (
        <div className="space-y-1 rounded-[6px] border border-border/70 bg-background px-2.5 py-2">
          {added.map((row, index) => (
            <DiffLine key={`add-${index}`} symbol="+" label={row.label} value={row.value} />
          ))}
          {removed.map((row, index) => (
            <DiffLine key={`rm-${index}`} symbol="−" label={row.label} value={row.value} />
          ))}
          {changed.map((row, index) => (
            <DiffLine key={`ch-${index}`} symbol="↻" label={row.label} value={row.value} />
          ))}
          {defaultMountChanged && (
            <DiffLine
              symbol="↻"
              label="默认挂载"
              value={`${section.default_mount_before ?? "none"} → ${section.default_mount_after ?? "none"}`}
            />
          )}
        </div>
      ) : (
        <p className="text-xs text-muted-foreground/60">本次无能力/工具变更</p>
      )}
      {section.effective_capabilities.length > 0 && (
        <EffectiveCapabilitiesBlock capabilities={section.effective_capabilities} />
      )}
    </div>
  );
}

function EffectiveCapabilitiesBlock({ capabilities }: { capabilities: string[] }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="rounded-[6px] border border-border/70 bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left hover:bg-secondary/35"
      >
        <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          当前生效能力 ({capabilities.length} 项)
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground/40">{open ? "▲" : "▼"}</span>
      </button>
      {open && (
        <div className="max-h-48 overflow-auto border-t border-border/70 px-2.5 py-2">
          <ul className="space-y-0.5">
            {capabilities.map((capability) => (
              <li
                key={capability}
                className="font-mono text-[11px] leading-5 text-muted-foreground"
              >
                {capability}
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

function DiffLine({
  symbol,
  label,
  value,
}: {
  symbol: string;
  label: string;
  value: string;
}) {
  return (
    <p className="flex items-start gap-2 text-xs leading-5">
      <span className="shrink-0 w-4 select-none text-muted-foreground/70">{symbol}</span>
      <span className="shrink-0 text-muted-foreground/80">{label}</span>
      <span className="min-w-0 break-all font-mono text-foreground/80">{value}</span>
    </p>
  );
}

function ToolSchemaBody({ section }: { section: ToolSchemaSection }) {
  if (section.tools.length === 0) {
    return <p className="text-xs text-muted-foreground/60">暂无工具 schema</p>;
  }
  return (
    <div className="max-h-96 overflow-auto space-y-1.5">
      {section.tools.map((tool) => (
        <ToolSchemaItem key={tool.name} tool={tool} />
      ))}
    </div>
  );
}

function ToolSchemaDeltaBody({ section }: { section: ToolSchemaDeltaSection }) {
  // 瘦身后 TOOL 只渲染 `added_tools`；工具路径级的屏蔽 / 恢复 / 移除由 CAP 承载。
  if (section.added_tools.length === 0) {
    return <p className="text-xs text-muted-foreground/60">无新增工具 schema</p>;
  }
  return (
    <div className="max-h-96 overflow-auto space-y-1.5">
      {section.added_tools.map((tool) => (
        <ToolSchemaItem key={tool.name} tool={tool} />
      ))}
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
          <span className="block truncate font-mono text-[11px] text-foreground/85">
            {tool.name}
          </span>
          {tool.description && (
            <span className="block truncate text-[11px] text-muted-foreground/80">
              {tool.description}
            </span>
          )}
          {(tool.capability_key || tool.source || tool.tool_path) && (
            <span className="mt-1 flex flex-wrap gap-1">
              {tool.capability_key && <Chip label={tool.capability_key} />}
              {tool.source && <Chip label={tool.source} />}
              {tool.tool_path && <Chip label={tool.tool_path} />}
            </span>
          )}
        </span>
        {fieldNames.length > 0 && (
          <span className="shrink-0 text-[10px] text-muted-foreground/50">
            {fieldNames.slice(0, 3).join("，")}
            {fieldNames.length > 3 ? ` 等 ${fieldNames.length} 项` : ""}
          </span>
        )}
        <span className="shrink-0 text-[10px] text-muted-foreground/40">
          {open ? "▲" : "▼"}
        </span>
      </button>
      {open && (
        <pre className="mt-2 max-h-64 overflow-auto rounded-[6px] border border-border/70 bg-secondary/20 p-2 text-[11px] leading-relaxed text-muted-foreground">
          {formatJson(tool.parameters_schema)}
        </pre>
      )}
    </div>
  );
}

function SkillDeltaBody({ section }: { section: SkillDeltaSection }) {
  const render = (items: RuntimeSkillEntry[], symbol: string, label: string) =>
    items.map((skill, index) => (
      <DiffLine
        key={`${label}-${skill.name}-${index}`}
        symbol={symbol}
        label={label}
        value={skill.name}
      />
    ));

  const hasDelta =
    section.added_skills.length +
      section.removed_skills.length +
      section.changed_skills.length >
    0;

  if (!hasDelta) {
    return <p className="text-xs text-muted-foreground/60">本次无 skill 变化</p>;
  }

  return (
    <div className="space-y-1 rounded-[6px] border border-border/70 bg-background px-2.5 py-2">
      {render(section.added_skills, "+", "skill")}
      {render(section.removed_skills, "−", "skill")}
      {render(section.changed_skills, "↻", "skill")}
    </div>
  );
}

function InjectionBody({ injections }: { injections: RuntimeHookInjectionEntry[] }) {
  if (injections.length === 0) {
    return <p className="text-xs text-muted-foreground/60">无注入内容</p>;
  }
  return (
    <div className="space-y-2">
      {injections.map((injection, index) => (
        <InjectionItem
          key={`${injection.slot}:${injection.source}:${index}`}
          injection={injection}
        />
      ))}
    </div>
  );
}

function PendingActionBody({ section }: { section: PendingActionSection }) {
  return (
    <div className="space-y-2">
      <div className="flex flex-wrap gap-1.5">
        <Chip label={`id: ${section.action_id}`} />
        <Chip label={`type: ${section.action_type}`} />
        <Chip label={`status: ${section.status}`} />
        <Chip label={`rev: ${section.revision}`} />
        {section.turn_id && <Chip label={`turn: ${section.turn_id}`} />}
      </div>
      {section.summary && (
        <p className="text-xs leading-relaxed text-foreground/75">{section.summary}</p>
      )}
      {section.instructions.length > 0 && (
        <div className="space-y-1 rounded-[6px] border border-border/70 bg-background px-2.5 py-2">
          {section.instructions.map((line, index) => (
            <pre
              key={`${section.action_id}-inst-${index}`}
              className="whitespace-pre-wrap text-xs leading-relaxed text-foreground/75"
            >
              {line}
            </pre>
          ))}
        </div>
      )}
      {section.injections.length > 0 && <InjectionBody injections={section.injections} />}
    </div>
  );
}

function InjectionItem({ injection }: { injection: RuntimeHookInjectionEntry }) {
  return (
    <article className="space-y-1 rounded-[6px] border border-border/70 bg-background px-2.5 py-2">
      <div className="flex flex-wrap gap-1.5">
        <Chip label={injection.slot || "slot"} />
        <Chip label={injection.source || "unknown"} />
      </div>
      {injection.content && (
        <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-xs leading-relaxed text-foreground/75">
          {injection.content}
        </pre>
      )}
    </article>
  );
}

function SystemNoticeBody({ section }: { section: SystemNoticeSection }) {
  if (!section.body) {
    return <p className="text-xs text-muted-foreground/60">{section.summary || "无补充内容"}</p>;
  }
  return (
    <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-xs leading-relaxed text-foreground/75">
      {section.body}
    </pre>
  );
}

function AutoResumeBody({ section }: { section: AutoResumeSection }) {
  return (
    <div className="space-y-1.5">
      {section.reason && <Chip label={`reason: ${section.reason}`} />}
      {section.prompt && (
        <pre className="max-h-96 overflow-auto whitespace-pre-wrap rounded-[6px] border border-border/70 bg-background p-2 text-xs leading-relaxed text-foreground/75">
          {section.prompt}
        </pre>
      )}
    </div>
  );
}

function CompactionSummaryBody({ section }: { section: CompactionSummarySection }) {
  return (
    <div className="space-y-1.5">
      <div className="flex flex-wrap gap-1.5">
        <Chip label={`messages: ${section.messages_compacted}`} />
        <Chip label={`tokens: ${section.tokens_before}`} />
        {section.timestamp_ms != null && <Chip label={`time: ${section.timestamp_ms}`} />}
      </div>
      {section.compacted_until_ref != null && (
        <CompactedUntilRefBlock value={section.compacted_until_ref} />
      )}
    </div>
  );
}

function CompactedUntilRefBlock({ value }: { value: unknown }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="rounded-[6px] border border-border/70 bg-background overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 px-2.5 py-1.5 text-left hover:bg-secondary/35"
      >
        <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          compacted_until_ref
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground/40">{open ? "▲" : "▼"}</span>
      </button>
      {open && (
        <pre className="max-h-48 overflow-auto whitespace-pre-wrap border-t border-border/70 p-2 text-[11px] leading-relaxed text-muted-foreground">
          {formatJson(value)}
        </pre>
      )}
    </div>
  );
}

// ─── 辅助通用组件 ────────────────────────────────────────────────────────────

export function TokenBadge({ token }: { token: ContextTokenInfo }) {
  return (
    <span
      className={`inline-flex shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] ${BADGE[token.variant]}`}
    >
      {token.token}
    </span>
  );
}

export function Chip({ label }: { label: string }) {
  return (
    <span className="rounded-[6px] border border-border bg-secondary/35 px-1.5 py-0.5 text-[10px] text-muted-foreground/80">
      {label}
    </span>
  );
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

