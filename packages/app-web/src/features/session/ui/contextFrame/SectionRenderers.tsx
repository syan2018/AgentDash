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
import { CB } from "../bodies/cardBodyTokens";
import { JsonTree } from "../bodies/JsonTree";
import type {
  AutoResumeSection,
  CapabilityKeyDeltaSection,
  CompactionSummarySection,
  CompanionAgentRosterDeltaSection,
  ContextFrameSection,
  ContextTokenInfo,
  IdentitySection,
  McpServerDeltaSection,
  MemoryInventorySection,
  AssignmentContextSection,
  PendingActionSection,
  ProjectGuidelinesSection,
  RuntimeContextFragmentEntry,
  RuntimeCompanionAgentEntry,
  RuntimeHookInjectionEntry,
  RuntimeMemorySourceEntry,
  RuntimeSkillEntry,
  SkillDeltaSection,
  SystemNoticeSection,
  ToolPathDeltaSection,
  ToolSchemaDeltaSection,
  UnknownSection,
  UserPreferencesSection,
  UserContextSection,
  VfsDeltaSection,
} from "../../model/contextFrame";
import { sectionKindToToken } from "../../model/contextFrame";
import { isRecord } from "../../model/platformEvent";
import { skillDisplayLabel, skillIdentityKey } from "../../../../types/context";

// ─── section header + body 组合 ──────────────────────────────────────────────

/** 渲染单个 section：标题行 + body 平铺，不加外框 */
export function SectionBlock({ section }: { section: ContextFrameSection }) {
  if (
    section.kind === "capability_key_delta" &&
    section.added_capabilities.length + section.removed_capabilities.length === 0
  ) {
    return null;
  }

  const token = sectionKindToToken(section.kind);
  const title = sectionTitle(section);
  const hint = sectionHint(section);

  return (
    <section className={CB.sectionGap}>
      <header className="flex items-center gap-2">
        <TokenBadge token={token} />
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground/70">
          {title}
        </span>
        {hint && (
          <span className={CB.meta}>{hint}</span>
        )}
      </header>
      <div className={CB.sectionGap}>{renderSectionBody(section)}</div>
    </section>
  );
}

function sectionTitle(section: ContextFrameSection): string {
  switch (section.kind) {
    case "identity":
      return section.title || "Identity";
    case "assignment_context":
      return section.title || "Assignment Context";
    case "capability_key_delta":
      return "Capability Keys";
    case "tool_path_delta":
      return "Tool Paths";
    case "mcp_server_delta":
      return "MCP Servers";
    case "vfs_delta":
      return "VFS Mounts";
    case "tool_schema_delta":
      return "Tool Schema";
    case "skill_delta":
      return "Skills";
    case "memory_inventory":
      return section.title || "Memory Inventory";
    case "companion_agent_roster_delta":
      return "Companion Agents";
    case "system_notice":
      return section.title || "System Notice";
    case "pending_action":
      return section.title || "Pending Action";
    case "auto_resume":
      return section.title || "Auto Resume";
    case "compaction_summary":
      return section.title || "Compaction Summary";
    case "user_preferences":
      return section.title || "User Preferences";
    case "project_guidelines":
      return section.title || "Project Guidelines";
    case "user_context":
      return section.title || "User Context";
    case "unknown_section":
      return `Unknown Section: ${section.original_kind}`;
  }
}

function sectionHint(section: ContextFrameSection): string | null {
  switch (section.kind) {
    case "identity":
      return `${section.fragments.length} fragments`;
    case "assignment_context":
      return `${section.fragments.length} fragments`;
    case "capability_key_delta": {
      const added = section.added_capabilities.length;
      const removed = section.removed_capabilities.length;
      if (added + removed === 0) return "no change";
      return `+${added} −${removed}`;
    }
    case "tool_path_delta": {
      const added = section.unblocked_tool_paths.length + section.whitelisted_tool_paths.length;
      const removed = section.blocked_tool_paths.length + section.removed_whitelist_paths.length;
      if (added + removed === 0) return "no change";
      return `+${added} −${removed}`;
    }
    case "mcp_server_delta": {
      const added = section.added_mcp_servers.length;
      const removed = section.removed_mcp_servers.length;
      const changed = section.changed_mcp_servers.length;
      if (added + removed + changed === 0) return "no change";
      return `+${added} −${removed}${changed > 0 ? ` ↻${changed}` : ""}`;
    }
    case "vfs_delta": {
      const added = section.vfs_mounts_added.length;
      const removed = section.vfs_mounts_removed.length;
      const mountChanged = (section.default_mount_before ?? null) !== (section.default_mount_after ?? null);
      if (added + removed === 0 && !mountChanged) return "no change";
      return `+${added} −${removed}${mountChanged ? " ↻default" : ""}`;
    }
    case "tool_schema_delta": {
      const added = section.added_tools.length;
      const removed = section.removed_tools.length;
      const changed = section.changed_tools.length;
      if (added + removed + changed === 0) return "no change";
      return `+${added} −${removed}${changed > 0 ? ` ↻${changed}` : ""}`;
    }
    case "skill_delta": {
      const added = section.added_skills.length;
      const removed = section.removed_skills.length;
      const changed = section.changed_skills.length;
      if (added + removed + changed === 0) return "no change";
      return `+${added} −${removed}${changed > 0 ? ` ↻${changed}` : ""}`;
    }
    case "memory_inventory": {
      if (section.mode === "snapshot") {
        return `${section.sources.length} sources`;
      }
      const added = section.added_sources.length;
      const removed = section.removed_sources.length;
      const changed = section.changed_sources.length;
      if (added + removed + changed === 0) return "no change";
      return `+${added} −${removed}${changed > 0 ? ` ↻${changed}` : ""}`;
    }
    case "companion_agent_roster_delta": {
      const added = section.added_agents.length;
      const removed = section.removed_agent_keys.length;
      const changed = section.changed_agents.length;
      if (added + removed + changed === 0) {
        return `${section.effective_agents.length} available`;
      }
      return `+${added} −${removed}${changed > 0 ? ` ↻${changed}` : ""}`;
    }
    case "system_notice":
      return null;
    case "pending_action":
      return section.status || "pending";
    case "auto_resume":
      return section.reason || "auto";
    case "compaction_summary":
      return `${section.messages_compacted} messages`;
    case "user_preferences":
      return `${section.items.length} items`;
    case "project_guidelines":
      return `${section.entries.length} files`;
    case "user_context":
      return section.provider || `${section.groups.length} groups`;
    case "unknown_section":
      return section.original_kind;
  }
}

function renderSectionBody(section: ContextFrameSection) {
  switch (section.kind) {
    case "identity":
      return <IdentityBody section={section} />;
    case "assignment_context":
      return <AssignmentContextBody section={section} />;
    case "capability_key_delta":
      return <CapabilityKeyDeltaBody section={section} />;
    case "tool_path_delta":
      return <ToolPathDeltaBody section={section} />;
    case "mcp_server_delta":
      return <McpServerDeltaBody section={section} />;
    case "vfs_delta":
      return <VfsDeltaBody section={section} />;
    case "tool_schema_delta":
      return <ToolSchemaDeltaBody section={section} />;
    case "skill_delta":
      return <SkillDeltaBody section={section} />;
    case "memory_inventory":
      return <MemoryInventoryBody section={section} />;
    case "companion_agent_roster_delta":
      return <CompanionAgentRosterDeltaBody section={section} />;
    case "system_notice":
      return <SystemNoticeBody section={section} />;
    case "pending_action":
      return <PendingActionBody section={section} />;
    case "auto_resume":
      return <AutoResumeBody section={section} />;
    case "compaction_summary":
      return <CompactionSummaryBody section={section} />;
    case "user_preferences":
      return <UserPreferencesBody section={section} />;
    case "project_guidelines":
      return <ProjectGuidelinesBody section={section} />;
    case "user_context":
      return <UserContextBody section={section} />;
    case "unknown_section":
      return <UnknownSectionBody section={section} />;
  }
}

// ─── 各 section body ─────────────────────────────────────────────────────────

function IdentityBody({ section }: { section: IdentitySection }) {
  if (section.fragments.length === 0) {
    return <p className="text-xs text-muted-foreground/60">暂无片段</p>;
  }

  return (
    <div className="space-y-2">
      {section.summary && (
        <p className="text-xs leading-relaxed text-foreground/75">{section.summary}</p>
      )}
      <div className="space-y-2">
        {section.fragments.map((fragment, index) => (
          <FragmentItem
            key={`${fragment.slot}:${fragment.source}:${index}`}
            fragment={fragment}
          />
        ))}
      </div>
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

function FragmentItem({ fragment }: { fragment: RuntimeContextFragmentEntry }) {
  return (
    <article className="space-y-1">
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

function CapabilityKeyDeltaBody({ section }: { section: CapabilityKeyDeltaSection }) {
  const hasDiff = section.added_capabilities.length + section.removed_capabilities.length > 0;

  return (
    <div className="space-y-2">
      {hasDiff ? (
        <div className="space-y-0.5">
          {section.added_capabilities.map((v, i) => (
            <DeltaListItem key={`add-${i}`} symbol="+" label="能力" name={v} />
          ))}
          {section.removed_capabilities.map((v, i) => (
            <DeltaListItem key={`rm-${i}`} symbol="−" label="能力" name={v} />
          ))}
        </div>
      ) : (
        <p className={CB.meta}>本次无能力 key 变更</p>
      )}
      {section.effective_capabilities.length > 0 && (
        <EffectiveCapabilitiesBlock capabilities={section.effective_capabilities} />
      )}
    </div>
  );
}

function ToolPathDeltaBody({ section }: { section: ToolPathDeltaSection }) {
  const items: Array<{ symbol: string; label: string; name: string }> = [
    ...section.unblocked_tool_paths.map((v) => ({ symbol: "+", label: "解除屏蔽", name: v })),
    ...section.whitelisted_tool_paths.map((v) => ({ symbol: "+", label: "加入白名单", name: v })),
    ...section.blocked_tool_paths.map((v) => ({ symbol: "−", label: "屏蔽", name: v })),
    ...section.removed_whitelist_paths.map((v) => ({ symbol: "−", label: "移出白名单", name: v })),
  ];

  if (items.length === 0) {
    return <p className={CB.meta}>本次无工具路径变更</p>;
  }
  return (
    <div className="space-y-0.5">
      {items.map((row, i) => (
        <DeltaListItem key={`${row.symbol}-${i}`} symbol={row.symbol} label={row.label} name={row.name} />
      ))}
    </div>
  );
}

function McpServerDeltaBody({ section }: { section: McpServerDeltaSection }) {
  const hasDiff =
    section.added_mcp_servers.length + section.removed_mcp_servers.length + section.changed_mcp_servers.length > 0;

  if (!hasDiff) {
    return <p className={CB.meta}>本次无 MCP 变更</p>;
  }
  return (
    <div className="space-y-0.5">
      {section.added_mcp_servers.map((v, i) => (
        <DeltaListItem key={`add-${i}`} symbol="+" label="MCP" name={v} />
      ))}
      {section.removed_mcp_servers.map((v, i) => (
        <DeltaListItem key={`rm-${i}`} symbol="−" label="MCP" name={v} />
      ))}
      {section.changed_mcp_servers.map((v, i) => (
        <DeltaListItem key={`ch-${i}`} symbol="↻" label="MCP" name={v} />
      ))}
    </div>
  );
}

function VfsDeltaBody({ section }: { section: VfsDeltaSection }) {
  const defaultMountChanged =
    (section.default_mount_before ?? null) !== (section.default_mount_after ?? null);
  const hasDiff =
    section.vfs_mounts_added.length + section.vfs_mounts_removed.length > 0 || defaultMountChanged;

  if (!hasDiff) {
    return <p className={CB.meta}>本次无 VFS 变更</p>;
  }
  return (
    <div className="space-y-0.5">
      {section.vfs_mounts_added.map((v, i) => (
        <DeltaListItem key={`add-${i}`} symbol="+" label="挂载" name={v} />
      ))}
      {section.vfs_mounts_removed.map((v, i) => (
        <DeltaListItem key={`rm-${i}`} symbol="−" label="挂载" name={v} />
      ))}
      {defaultMountChanged && (
        <DeltaListItem
          symbol="↻"
          label="默认挂载"
          name={`${section.default_mount_before ?? "none"} → ${section.default_mount_after ?? "none"}`}
        />
      )}
    </div>
  );
}

function EffectiveCapabilitiesBlock({ capabilities }: { capabilities: string[] }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 rounded-[6px] px-2 py-1 text-left transition-colors hover:bg-secondary/40"
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


// ── 滚动列表容器 ──
const SCROLL_LIST = "max-h-96 space-y-0.5 overflow-auto scrollbar-thin";

/**
 * 通用 delta 行 — 所有 ContextFrame delta section 共用
 *
 * 三种模式：
 * - 单行：symbol + label? + name（CAP / VFS / MCP / ToolPath）
 * - 双行：同上 + hover 时显示 description 第二行（Skills）
 * - 可折叠：同上 + chevron，点击展开 body 内容（ToolSchema）
 */
function DeltaListItem({
  symbol,
  name,
  label,
  chips,
  meta,
  hoverDesc,
  expandContent,
}: {
  symbol: string;
  name: string;
  label?: string;
  chips?: string[];
  meta?: string;
  hoverDesc?: string;
  expandContent?: React.ReactNode;
}) {
  const clickable = expandContent != null;
  const [open, setOpen] = useState(false);

  return (
    <div className={`rounded-[6px] transition-colors hover:bg-secondary/40 ${open ? "bg-secondary/30" : ""}`}>
      <button
        type="button"
        onClick={clickable ? () => setOpen((v) => !v) : undefined}
        className="flex w-full items-center gap-2 px-2 py-1 text-left"
      >
        <span className="shrink-0 w-3 select-none text-[10px] text-muted-foreground/70">{symbol}</span>
        {label && <span className="shrink-0 text-xs text-muted-foreground/80">{label}</span>}
        <span className="min-w-0 flex-1 truncate font-mono text-xs text-foreground/70">
          {name}
        </span>
        {chips && chips.length > 0 && (
          <span className="flex shrink-0 gap-1">
            {chips.map((c) => (
              <Chip key={c} label={c} />
            ))}
          </span>
        )}
        {meta && <span className={CB.meta}>{meta}</span>}
        {clickable && <span className={CB.expandToggle}>{open ? "▲" : "▼"}</span>}
      </button>
      {hoverDesc && (
        <p
          className="line-clamp-1 px-2 pb-0.5 pl-7 text-[10px] text-muted-foreground/40"
          title={hoverDesc}
        >
          {hoverDesc}
        </p>
      )}
      {open && expandContent && (
        <div className="px-2 py-1.5">
          {expandContent}
        </div>
      )}
    </div>
  );
}

function ToolSchemaDeltaBody({ section }: { section: ToolSchemaDeltaSection }) {
  if (
    section.added_tools.length
      + section.removed_tools.length
      + section.changed_tools.length
    === 0
  ) {
    return <p className={CB.meta}>本次无工具变化</p>;
  }
  const renderTools = (
    tools: ToolSchemaDeltaSection["added_tools"],
    symbol: string,
  ) => tools.map((tool) => {
    const fieldNames = schemaFieldNames(tool.parameters_schema);
    const chips = [tool.capability_key, tool.source].filter(Boolean) as string[];
    return (
      <DeltaListItem
        key={`${symbol}-${tool.name}`}
        symbol={symbol}
        name={tool.name}
        chips={chips}
        meta={fieldNames.length > 0 ? `${fieldNames.length} params` : undefined}
        hoverDesc={tool.description || undefined}
      />
    );
  });
  return (
    <div className={SCROLL_LIST}>
      {renderTools(section.added_tools, "+")}
      {section.removed_tools.map((name) => (
        <DeltaListItem key={`removed-${name}`} symbol="−" name={name} />
      ))}
      {renderTools(section.changed_tools, "↻")}
    </div>
  );
}

function SkillDeltaBody({ section }: { section: SkillDeltaSection }) {
  const hasDelta =
    section.added_skills.length +
      section.removed_skills.length +
      section.changed_skills.length >
    0;

  if (!hasDelta) {
    return <p className={CB.meta}>本次无 skill 变化</p>;
  }

  const renderSkills = (items: RuntimeSkillEntry[], symbol: string) =>
    items.map((skill, index) => {
      const displayName = skillDisplayLabel(skill);
      const identity = skillIdentityKey(skill);
      const chips: string[] = [];
      if (skill.provider_key) chips.push(skill.provider_key);
      if (identity !== displayName) chips.push(identity);
      if (skill.exposure === "explicit_only") chips.push("explicit only");
      return (
        <DeltaListItem
          key={`${symbol}-${identity}-${index}`}
          symbol={symbol}
          name={displayName}
          chips={chips}
          hoverDesc={skill.description || undefined}
        />
      );
    });

  return (
    <div className={SCROLL_LIST}>
      {renderSkills(section.added_skills, "+")}
      {renderSkills(section.removed_skills, "−")}
      {renderSkills(section.changed_skills, "↻")}
    </div>
  );
}

function MemoryInventoryBody({ section }: { section: MemoryInventorySection }) {
  const renderSources = (items: RuntimeMemorySourceEntry[], symbol: string) =>
    items.map((source, index) => {
      const chips = [
        source.provider_key,
        source.scope,
        source.index_status,
      ].filter(Boolean);
      return (
        <DeltaListItem
          key={`${symbol}-${source.provider_key}-${source.source_key}-${index}`}
          symbol={symbol}
          name={source.display_name || source.source_key || source.source_uri}
          chips={chips}
          meta={source.revision ? `rev ${source.revision.slice(0, 8)}` : undefined}
          hoverDesc={source.summary || source.index_uri || undefined}
          expandContent={
            <div className="space-y-1 font-mono text-[11px] leading-5 text-muted-foreground">
              <div>source: {source.source_uri || "unknown"}</div>
              <div>index: {source.index_uri || "unknown"}</div>
              {source.mount_id && <div>mount: {source.mount_id}</div>}
            </div>
          }
        />
      );
    });

  const body =
    section.mode === "snapshot" ? (
      section.sources.length > 0 ? (
        <div className={SCROLL_LIST}>{renderSources(section.sources, "*")}</div>
      ) : (
        <p className={CB.meta}>当前没有 memory source</p>
      )
    ) : (
      <div className={SCROLL_LIST}>
        {renderSources(section.added_sources, "+")}
        {renderSources(section.removed_sources, "−")}
        {renderSources(section.changed_sources, "↻")}
        {section.added_sources.length +
          section.removed_sources.length +
          section.changed_sources.length ===
          0 && <p className={CB.meta}>本次无 memory source 变化</p>}
      </div>
    );

  return (
    <div className="space-y-2">
      {body}
      {section.diagnostics.length > 0 && (
        <div className="space-y-0.5">
          {section.diagnostics.map((diagnostic, index) => (
            <DeltaListItem
              key={`${diagnostic.provider_key}-${diagnostic.code}-${index}`}
              symbol="!"
              label="diagnostic"
              name={diagnostic.code}
              chips={[diagnostic.provider_key, diagnostic.source_key].filter(Boolean) as string[]}
              hoverDesc={diagnostic.message || diagnostic.uri || undefined}
            />
          ))}
        </div>
      )}
    </div>
  );
}

function CompanionAgentRosterDeltaBody({
  section,
}: {
  section: CompanionAgentRosterDeltaSection;
}) {
  const hasDelta =
    section.added_agents.length +
      section.removed_agent_keys.length +
      section.changed_agents.length >
    0;

  const agentItem = (agent: RuntimeCompanionAgentEntry, symbol: string, i: number) => {
    const display = agent.display_name || agent.agent_key;
    const chips = [`agent: ${agent.agent_key}`];
    if (agent.executor) chips.push(`executor: ${agent.executor}`);
    return (
      <DeltaListItem
        key={`${symbol}-${agent.agent_key}-${i}`}
        symbol={symbol}
        label="companion"
        name={display}
        chips={chips}
      />
    );
  };

  return (
    <div className="space-y-2">
      {hasDelta ? (
        <div className="space-y-0.5">
          {section.added_agents.map((a, i) => agentItem(a, "+", i))}
          {section.removed_agent_keys.map((key, i) => (
            <DeltaListItem key={`rm-${key}-${i}`} symbol="−" label="companion" name={key} />
          ))}
          {section.changed_agents.map((a, i) => agentItem(a, "↻", i))}
        </div>
      ) : (
        <p className={CB.meta}>
          {section.effective_agents.length === 0
            ? "当前没有可用 companion agent"
            : "本次无 companion roster 变更"}
        </p>
      )}
      {section.effective_agents.length > 0 && (
        <EffectiveCompanionAgentsBlock agents={section.effective_agents} />
      )}
    </div>
  );
}

function EffectiveCompanionAgentsBlock({
  agents,
}: {
  agents: RuntimeCompanionAgentEntry[];
}) {
  const [open, setOpen] = useState(false);
  return (
    <div className="overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 rounded-[6px] px-2 py-1 text-left transition-colors hover:bg-secondary/40"
      >
        <span className="min-w-0 flex-1 truncate text-xs text-muted-foreground">
          当前可用 companion ({agents.length} 项)
        </span>
        <span className="shrink-0 text-[10px] text-muted-foreground/40">{open ? "▲" : "▼"}</span>
      </button>
      {open && (
        <div className="max-h-48 overflow-auto border-t border-border/70 px-2.5 py-2">
          <ul className="space-y-1">
            {agents.map((agent) => (
              <li
                key={agent.agent_key}
                className="space-y-0.5 rounded-[4px] bg-secondary/20 px-2 py-1"
              >
                <div className="font-mono text-[11px] leading-5 text-foreground/80">
                  {agent.display_name || agent.agent_key}
                </div>
                <div className="flex flex-wrap gap-1.5">
                  <Chip label={`agent: ${agent.agent_key}`} />
                  {agent.executor && <Chip label={`executor: ${agent.executor}`} />}
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}
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
        <div className="space-y-1">
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
    <article className="space-y-1">
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
        <pre className={`max-h-96 overflow-auto whitespace-pre-wrap ${CB.codeBlock}`}>
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
        {section.projection_version != null && <Chip label={`projection: v${section.projection_version}`} />}
        {section.strategy && <Chip label={`strategy: ${section.strategy}`} />}
        {section.trigger && <Chip label={`trigger: ${section.trigger}`} />}
        {section.phase && <Chip label={`phase: ${section.phase}`} />}
        {section.source_start_event_seq != null && section.source_end_event_seq != null && (
          <Chip label={`source: ${section.source_start_event_seq}-${section.source_end_event_seq}`} />
        )}
        {section.first_kept_event_seq != null && <Chip label={`first kept: ${section.first_kept_event_seq}`} />}
        {section.timestamp_ms != null && <Chip label={`time: ${section.timestamp_ms}`} />}
      </div>
      {section.compaction_id && (
        <div className="truncate text-[11px] text-muted-foreground/70">
          checkpoint {section.compaction_id}
        </div>
      )}
      {section.compacted_until_ref != null && (
        <CompactedUntilRefBlock value={section.compacted_until_ref} />
      )}
    </div>
  );
}

function UserPreferencesBody({ section }: { section: UserPreferencesSection }) {
  if (section.items.length === 0) {
    return <p className="text-xs text-muted-foreground/60">暂无用户偏好</p>;
  }
  return (
    <div className="space-y-1">
      {section.items.map((item, index) => (
        <p key={`${item}-${index}`} className="text-xs leading-5 text-foreground/75">
          {item}
        </p>
      ))}
    </div>
  );
}

function ProjectGuidelinesBody({ section }: { section: ProjectGuidelinesSection }) {
  if (section.entries.length === 0) {
    return <p className="text-xs text-muted-foreground/60">暂无项目指引</p>;
  }
  return (
    <div className="space-y-2">
      {section.entries.map((entry, index) => (
        <article
          key={`${entry.path}-${index}`}
          className="space-y-1"
        >
          <div className="flex flex-wrap gap-1.5">
            <Chip label={entry.path} />
          </div>
          {entry.content && (
            <pre className="max-h-64 overflow-auto whitespace-pre-wrap text-xs leading-relaxed text-foreground/75">
              {entry.content}
            </pre>
          )}
        </article>
      ))}
    </div>
  );
}

function UserContextBody({ section }: { section: UserContextSection }) {
  const chips = [
    section.user_id ? `user: ${section.user_id}` : null,
    section.display_name ? `name: ${section.display_name}` : null,
    section.email ? `email: ${section.email}` : null,
    section.provider ? `provider: ${section.provider}` : null,
  ].filter((item): item is string => item != null);

  return (
    <div className="space-y-2">
      {section.summary && (
        <p className="text-xs leading-relaxed text-foreground/75">{section.summary}</p>
      )}
      {chips.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {chips.map((label) => (
            <Chip key={label} label={label} />
          ))}
        </div>
      )}
      {section.groups.length > 0 && (
        <div className="flex flex-wrap gap-1.5">
          {section.groups.map((group) => (
            <Chip key={group} label={`group: ${group}`} />
          ))}
        </div>
      )}
      {section.extra != null && (
        <div className={CB.codeBlock}>
          <JsonTree data={section.extra} defaultDepth={2} />
        </div>
      )}
    </div>
  );
}

function UnknownSectionBody({ section }: { section: UnknownSection }) {
  return (
    <pre className={`max-h-64 overflow-auto whitespace-pre-wrap ${CB.codeBlock}`}>
      {formatJson(section.raw)}
    </pre>
  );
}

function CompactedUntilRefBlock({ value }: { value: unknown }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="overflow-hidden">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center gap-2 rounded-[6px] px-2 py-1 text-left transition-colors hover:bg-secondary/40"
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
    <span className="shrink-0 text-[10px] font-semibold uppercase tracking-[0.08em] text-muted-foreground/60">
      {token.token}
    </span>
  );
}

export function Chip({ label }: { label: string }) {
  return (
    <span className="shrink-0 rounded-[4px] bg-secondary/40 px-1 py-px text-[9px] font-semibold text-muted-foreground/60">
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
