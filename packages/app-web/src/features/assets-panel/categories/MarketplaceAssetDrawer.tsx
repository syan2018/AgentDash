/**
 * MarketplaceAssetDrawer — 资源市场卡片详情抽屉。
 *
 * - 右侧滑入，不遮挡 grid 的滚动状态
 * - 按 asset_type 分发到 4 个 type-specific body；解析失败 fallback 到 raw JSON
 * - 抽屉内 footer 也能直接安装/更新（与卡片同 helper）
 * - update_available 走 ConfirmOverwriteDialog，首装直连
 */

import { useEffect, useMemo } from "react";
import type { LibraryAssetDto, LibraryAssetType, SharedLibrarySourceStatus } from "../../../types";

const ASSET_TYPE_LABELS: Record<LibraryAssetType, string> = {
  agent_template: "Agent Template",
  mcp_server_template: "MCP Server",
  workflow_template: "Workflow",
  skill_template: "Skill",
  filespace_template: "Filespace",
  extension_template: "Extension",
};

export interface InstallTarget {
  asset_kind: string;
  project_asset_key: string;
  installed_version: string;
  current_source_version: string | null;
  item_status: SharedLibrarySourceStatus;
}

export interface InstallSummary {
  status: SharedLibrarySourceStatus;
  installations: InstallTarget[];
}

export interface MarketplaceAssetDrawerProps {
  asset: LibraryAssetDto | null;
  installSummary?: InstallSummary;
  busy: boolean;
  onClose: () => void;
  onInstall: (overwrite: boolean) => void;
}

export function MarketplaceAssetDrawer({
  asset,
  installSummary,
  busy,
  onClose,
  onInstall,
}: MarketplaceAssetDrawerProps) {
  // ESC 关闭
  useEffect(() => {
    if (!asset) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [asset, onClose]);

  if (!asset) return null;

  const status = installSummary?.status;
  const isInstalled = status === "up_to_date";
  const hasUpdate = status === "update_available";
  const sourceMissing = status === "source_missing";

  return (
    <>
      <div className="fixed inset-0 z-[80] bg-foreground/18 backdrop-blur-[2px]" onClick={onClose} />
      <aside
        className="fixed right-0 top-0 z-[81] flex h-full w-[480px] max-w-full flex-col border-l border-border bg-background shadow-2xl"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <header className="flex items-start justify-between gap-3 border-b border-border px-5 py-4">
          <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-1.5">
              <span className="rounded-[6px] border border-border bg-secondary/70 px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
                {ASSET_TYPE_LABELS[asset.asset_type]}
              </span>
              <span className="rounded-[6px] border border-border bg-background px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
                {sourceLabel(asset.source)}
              </span>
              <InstallStatusChip summary={installSummary} />
              {asset.deprecated && (
                <span className="rounded-[6px] border border-warning/30 bg-warning/10 px-1.5 py-0.5 text-[10px] font-medium text-warning">
                  已废弃
                </span>
              )}
            </div>
            <h3 className="mt-1.5 truncate text-base font-semibold text-foreground">
              {asset.display_name}
            </h3>
            <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
              {asset.key} · v{asset.version}
            </p>
          </div>
          <button
            type="button"
            onClick={onClose}
            aria-label="关闭"
            className="shrink-0 rounded-[6px] border border-border px-2 py-1 text-xs text-muted-foreground hover:bg-secondary"
          >
            ×
          </button>
        </header>

        {/* Body */}
        <div className="flex-1 space-y-4 overflow-y-auto px-5 py-4">
          {asset.description && (
            <section>
              <p className="text-xs leading-6 text-foreground/85">{asset.description}</p>
            </section>
          )}

          {installSummary && installSummary.installations.length > 0 && (
            <InstallationsBlock summary={installSummary} />
          )}

          <TypeSpecificBody asset={asset} />
        </div>

        {/* Footer */}
        <footer className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
          <button type="button" onClick={onClose} className="agentdash-button-secondary" disabled={busy}>
            关闭
          </button>
          <button
            type="button"
            onClick={() => onInstall(hasUpdate)}
            disabled={busy || asset.deprecated || isInstalled || sourceMissing}
            className="agentdash-button-primary"
            title={
              sourceMissing
                ? "市场来源已废弃或不可用"
                : isInstalled
                  ? "项目已是最新版本"
                  : undefined
            }
          >
            {busy
              ? "处理中…"
              : asset.deprecated
                ? "已废弃"
                : sourceMissing
                  ? "来源缺失"
                  : isInstalled
                    ? "已是最新"
                    : hasUpdate
                      ? `更新到 v${asset.version}`
                      : "安装到项目"}
          </button>
        </footer>
      </aside>
    </>
  );
}

/* ─── Install status chip ─── */

export function InstallStatusChip({ summary }: { summary?: InstallSummary }) {
  if (!summary) return null;
  const cls =
    summary.status === "source_missing"
      ? "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300"
      : summary.status === "update_available"
        ? "border-sky-500/30 bg-sky-500/10 text-sky-700 dark:text-sky-300"
        : "border-emerald-500/30 bg-emerald-500/10 text-emerald-700 dark:text-emerald-300";
  const label =
    summary.status === "source_missing"
      ? "来源缺失"
      : summary.status === "update_available"
        ? "有新版"
        : "已安装";
  const tooltip = summary.installations
    .map((i) => `${i.asset_kind} · ${i.project_asset_key} (v${i.installed_version})`)
    .join("\n");
  return (
    <span
      className={`rounded-[6px] border px-1.5 py-0.5 text-[10px] font-medium ${cls}`}
      title={tooltip || undefined}
    >
      {label}
    </span>
  );
}

function InstallationsBlock({ summary }: { summary: InstallSummary }) {
  return (
    <section className="rounded-[8px] border border-border bg-secondary/20 p-3">
      <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">
        已安装位置 ({summary.installations.length})
      </p>
      <ul className="mt-2 space-y-1">
        {summary.installations.map((i) => (
          <li
            key={`${i.asset_kind}:${i.project_asset_key}`}
            className="flex items-center justify-between gap-2 text-xs"
          >
            <span className="truncate font-mono text-[11px] text-foreground/85">
              {i.asset_kind} · {i.project_asset_key}
            </span>
            <span className="shrink-0 text-[11px] text-muted-foreground">
              v{i.installed_version}
              {i.current_source_version && i.current_source_version !== i.installed_version
                ? ` → v${i.current_source_version}`
                : ""}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

/* ─── Type-specific bodies ─── */

function TypeSpecificBody({ asset }: { asset: LibraryAssetDto }) {
  switch (asset.asset_type) {
    case "skill_template":
      return <SkillTemplateBody payload={asset.payload} />;
    case "filespace_template":
      return <FilespaceTemplateBody payload={asset.payload} />;
    case "workflow_template":
      return <WorkflowTemplateBody payload={asset.payload} />;
    case "mcp_server_template":
      return <McpServerTemplateBody payload={asset.payload} />;
    case "agent_template":
      return <AgentTemplateBody payload={asset.payload} />;
    case "extension_template":
      return <ExtensionTemplateBody payload={asset.payload} />;
    default:
      return <RawPayloadFallback payload={asset.payload} />;
  }
}

function FilespaceTemplateBody({ payload }: { payload: unknown }) {
  const parsed = useMemo(() => parseFilespacePayload(payload), [payload]);
  if (!parsed) return <RawPayloadFallback payload={payload} />;
  return (
    <section className="space-y-3">
      <SectionLabel>Filespace 模板</SectionLabel>
      <div className="rounded-[8px] border border-border bg-secondary/20 p-3">
        <p className="text-xs text-muted-foreground">文件数量</p>
        <p className="mt-1 text-sm font-medium text-foreground">{parsed.files.length}</p>
      </div>
      <ul className="space-y-1">
        {parsed.files.slice(0, 12).map((file) => (
          <li key={file.path} className="truncate text-xs text-muted-foreground">
            {file.content_kind} · {file.path}
          </li>
        ))}
      </ul>
    </section>
  );
}

function parseFilespacePayload(payload: unknown): null | { files: Array<{ path: string; content_kind: string }> } {
  if (!isObject(payload) || !Array.isArray(payload.files)) return null;
  return {
    files: payload.files
      .filter(isObject)
      .map((file) => ({
        path: asString(file.path) ?? "",
        content_kind: asString(file.content_kind) ?? "text",
      }))
      .filter((file) => file.path.length > 0),
  };
}

function isObject(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

function asString(v: unknown): string | null {
  return typeof v === "string" ? v : null;
}

/* Skill */

function SkillTemplateBody({ payload }: { payload: unknown }) {
  const parsed = useMemo(() => parseSkillPayload(payload), [payload]);
  if (!parsed) return <RawPayloadFallback payload={payload} />;
  const skillMd = parsed.files.find((f) => f.kind === "primary" || /SKILL\.md$/i.test(f.path));
  return (
    <section className="space-y-3">
      <SectionLabel>Skill 模板</SectionLabel>
      <div className="flex flex-wrap gap-1.5">
        <MetaChip>{parsed.files.length} 个文件</MetaChip>
        {parsed.disable_model_invocation && (
          <MetaChip tone="amber">disable-model-invocation</MetaChip>
        )}
      </div>
      {skillMd && (
        <details className="rounded-[8px] border border-border bg-secondary/15 p-3">
          <summary className="cursor-pointer text-xs font-medium text-foreground">
            {skillMd.path}
          </summary>
          <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap font-mono text-[11px] leading-5 text-muted-foreground">
            {truncate(skillMd.content, 1200)}
          </pre>
        </details>
      )}
      <div className="rounded-[8px] border border-border">
        <div className="border-b border-border bg-secondary/20 px-3 py-1.5">
          <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">文件列表</p>
        </div>
        <ul className="divide-y divide-border">
          {parsed.files.map((f) => (
            <li key={f.path} className="flex items-center justify-between px-3 py-2">
              <span className="truncate font-mono text-[11px] text-foreground/85">{f.path}</span>
              <span className="shrink-0 text-[10px] text-muted-foreground">
                {f.kind} · {formatBytes(f.content.length)}
              </span>
            </li>
          ))}
        </ul>
      </div>
    </section>
  );
}

interface SkillFileLite {
  path: string;
  content: string;
  kind: string;
}

function parseSkillPayload(raw: unknown): { files: SkillFileLite[]; disable_model_invocation: boolean } | null {
  if (!isObject(raw)) return null;
  const files = raw.files;
  if (!Array.isArray(files)) return null;
  const parsedFiles: SkillFileLite[] = [];
  for (const item of files) {
    if (!isObject(item)) continue;
    const path = asString(item.path);
    const content = asString(item.content);
    const kind = asString(item.kind) ?? "extra";
    if (!path || content === null) continue;
    parsedFiles.push({ path, content, kind });
  }
  return {
    files: parsedFiles,
    disable_model_invocation: Boolean(raw.disable_model_invocation),
  };
}

/* Workflow */

function WorkflowTemplateBody({ payload }: { payload: unknown }) {
  const parsed = useMemo(() => parseWorkflowPayload(payload), [payload]);
  if (!parsed) return <RawPayloadFallback payload={payload} />;
  const visibleActivities = parsed.activities.slice(0, 8);
  const remaining = parsed.activities.length - visibleActivities.length;
  return (
    <section className="space-y-3">
      <SectionLabel>Workflow 模板</SectionLabel>
      <div className="flex flex-wrap gap-1.5">
        <MetaChip>{parsed.activities.length} activity</MetaChip>
        <MetaChip>{parsed.transitions.length} transition</MetaChip>
        {parsed.workflowCount > 0 && <MetaChip>{parsed.workflowCount} sub-workflow</MetaChip>}
        {parsed.bindingKinds.length > 0 && (
          <MetaChip>target: {parsed.bindingKinds.join(", ")}</MetaChip>
        )}
      </div>
      {parsed.lifecycleName && (
        <p className="text-xs text-foreground/85">
          <span className="text-muted-foreground">Lifecycle：</span>
          <span className="font-medium">{parsed.lifecycleName}</span>
          {parsed.entryActivityKey && (
            <span className="ml-1 font-mono text-[11px] text-muted-foreground">
              ({parsed.entryActivityKey})
            </span>
          )}
        </p>
      )}
      {visibleActivities.length > 0 && (
        <div className="rounded-[8px] border border-border">
          <div className="border-b border-border bg-secondary/20 px-3 py-1.5">
            <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">Activities</p>
          </div>
          <ul className="divide-y divide-border">
            {visibleActivities.map((s) => (
              <li key={s.key} className="flex items-center justify-between gap-2 px-3 py-2">
                <span className="truncate text-xs text-foreground/85">{s.name || s.key}</span>
                <span className="shrink-0 font-mono text-[10px] text-muted-foreground">
                  {s.key}
                </span>
              </li>
            ))}
          </ul>
          {remaining > 0 && (
            <p className="px-3 py-1.5 text-[11px] text-muted-foreground">
              还有 {remaining} 个 Activity 未显示…
            </p>
          )}
        </div>
      )}
    </section>
  );
}

interface WorkflowParsed {
  activities: Array<{ key: string; name: string }>;
  transitions: unknown[];
  workflowCount: number;
  bindingKinds: string[];
  lifecycleName: string | null;
  entryActivityKey: string | null;
}

function parseWorkflowPayload(raw: unknown): WorkflowParsed | null {
  if (!isObject(raw)) return null;
  const template = raw.template;
  if (!isObject(template)) return null;
  const lifecycle = isObject(template.lifecycle) ? template.lifecycle : null;
  if (!lifecycle) return null;
  const activitiesRaw = Array.isArray(lifecycle.activities) ? lifecycle.activities : [];
  const activities: Array<{ key: string; name: string }> = [];
  for (const s of activitiesRaw) {
    if (!isObject(s)) continue;
    const key = asString(s.key);
    if (!key) continue;
    activities.push({ key, name: asString(s.name) ?? asString(s.description) ?? key });
  }
  const transitions = Array.isArray(lifecycle.transitions) ? lifecycle.transitions : [];
  const workflows = Array.isArray(template.workflows) ? template.workflows : [];
  const bindingKinds = Array.isArray(template.binding_kinds)
    ? template.binding_kinds.filter((v): v is string => typeof v === "string")
    : [];
  return {
    activities,
    transitions,
    workflowCount: workflows.length,
    bindingKinds,
    lifecycleName: asString(lifecycle.name),
    entryActivityKey: asString(lifecycle.entry_activity_key),
  };
}

/* MCP Server */

function McpServerTemplateBody({ payload }: { payload: unknown }) {
  const parsed = useMemo(() => parseMcpPayload(payload), [payload]);
  if (!parsed) return <RawPayloadFallback payload={payload} />;
  return (
    <section className="space-y-3">
      <SectionLabel>MCP Server 模板</SectionLabel>
      <div className="flex flex-wrap gap-1.5">
        <MetaChip>transport: {parsed.transportType}</MetaChip>
        {parsed.routePolicy && <MetaChip>route: {parsed.routePolicy}</MetaChip>}
      </div>
      {parsed.transportSummary && (
        <pre className="overflow-auto rounded-[8px] border border-border bg-secondary/20 px-3 py-2 font-mono text-[11px] leading-5 text-foreground/85">
          {parsed.transportSummary}
        </pre>
      )}
      {parsed.capabilities.length > 0 && (
        <div>
          <p className="mb-1.5 text-[11px] uppercase tracking-[0.12em] text-muted-foreground">
            capabilities
          </p>
          <div className="flex flex-wrap gap-1">
            {parsed.capabilities.map((c) => (
              <span
                key={c}
                className="rounded-[8px] border border-border bg-background px-2 py-0.5 font-mono text-[10.5px] text-foreground/80"
              >
                {c}
              </span>
            ))}
          </div>
        </div>
      )}
    </section>
  );
}

interface McpParsed {
  transportType: string;
  routePolicy: string | null;
  transportSummary: string | null;
  capabilities: string[];
}

function parseMcpPayload(raw: unknown): McpParsed | null {
  if (!isObject(raw)) return null;
  const transport = isObject(raw.transport) ? raw.transport : null;
  if (!transport) return null;
  const transportType = asString(transport.type) ?? "unknown";
  const routePolicy = asString(raw.route_policy);
  const capsRaw = Array.isArray(raw.capabilities) ? raw.capabilities : [];
  const capabilities = capsRaw.filter((v): v is string => typeof v === "string");

  let summary: string | null = null;
  if (transportType === "http" || transportType === "sse") {
    const url = asString(transport.url);
    if (url) summary = `${transportType.toUpperCase()} ${url}`;
  } else if (transportType === "stdio") {
    const command = asString(transport.command);
    const argsRaw = Array.isArray(transport.args) ? transport.args : [];
    const args = argsRaw.filter((v): v is string => typeof v === "string").join(" ");
    if (command) summary = `${command}${args ? ` ${args}` : ""}`;
  }

  return { transportType, routePolicy, transportSummary: summary, capabilities };
}

/* Agent */

function AgentTemplateBody({ payload }: { payload: unknown }) {
  const parsed = useMemo(() => parseAgentPayload(payload), [payload]);
  if (!parsed) return <RawPayloadFallback payload={payload} />;
  return (
    <section className="space-y-3">
      <SectionLabel>Agent 模板</SectionLabel>
      <div className="flex flex-wrap gap-1.5">
        {parsed.modelId && <MetaChip>model: {parsed.modelId}</MetaChip>}
        {parsed.executor && <MetaChip>executor: {parsed.executor}</MetaChip>}
        {parsed.thinkingLevel && <MetaChip>thinking: {parsed.thinkingLevel}</MetaChip>}
        <MetaChip>{parsed.mcpSlotCount} MCP slot</MetaChip>
        <MetaChip>{parsed.capabilityCount} capability</MetaChip>
      </div>
      {parsed.systemPrompt && (
        <details className="rounded-[8px] border border-border bg-secondary/15 p-3">
          <summary className="cursor-pointer text-xs font-medium text-foreground">
            System prompt 摘要
          </summary>
          <pre className="mt-2 max-h-64 overflow-auto whitespace-pre-wrap font-mono text-[11px] leading-5 text-muted-foreground">
            {truncate(parsed.systemPrompt, 800)}
          </pre>
        </details>
      )}
    </section>
  );
}

interface AgentParsed {
  modelId: string | null;
  executor: string | null;
  thinkingLevel: string | null;
  systemPrompt: string | null;
  mcpSlotCount: number;
  capabilityCount: number;
}

function parseAgentPayload(raw: unknown): AgentParsed | null {
  if (!isObject(raw)) return null;
  const config = isObject(raw.config) ? raw.config : null;
  if (!config) return null;
  const mcpSlots = Array.isArray(config.mcp_slots) ? config.mcp_slots : [];
  const caps = Array.isArray(config.capability_directives) ? config.capability_directives : [];
  return {
    modelId: asString(config.model_id),
    executor: asString(config.executor),
    thinkingLevel: asString(config.thinking_level),
    systemPrompt: asString(config.system_prompt),
    mcpSlotCount: mcpSlots.length,
    capabilityCount: caps.length,
  };
}

/* Extension */

function ExtensionTemplateBody({ payload }: { payload: unknown }) {
  const parsed = useMemo(() => parseExtensionPayload(payload), [payload]);
  if (!parsed) return <RawPayloadFallback payload={payload} />;
  return (
    <section className="space-y-3">
      <SectionLabel>Extension 模板</SectionLabel>
      <div className="flex flex-wrap gap-1.5">
        <MetaChip>{parsed.commands.length} command</MetaChip>
        <MetaChip>{parsed.flags.length} flag</MetaChip>
        <MetaChip>{parsed.renderers.length} renderer</MetaChip>
      </div>
      <p className="font-mono text-[11px] text-muted-foreground">
        {parsed.extensionId} · manifest v{parsed.manifestVersion}
      </p>
      {parsed.commands.length > 0 && (
        <CompactList
          title="commands"
          items={parsed.commands.map((command) => ({
            key: command.name,
            meta: command.handlerKind,
            description: command.description,
          }))}
        />
      )}
      {parsed.flags.length > 0 && (
        <CompactList
          title="flags"
          items={parsed.flags.map((flag) => ({
            key: flag.name,
            meta: `${flag.type} = ${flag.defaultValue}`,
            description: flag.description,
          }))}
        />
      )}
      {parsed.renderers.length > 0 && (
        <CompactList
          title="renderers"
          items={parsed.renderers.map((renderer) => ({
            key: renderer.customType,
            meta: renderer.kind,
            description: null,
          }))}
        />
      )}
    </section>
  );
}

interface ExtensionParsed {
  manifestVersion: string;
  extensionId: string;
  commands: Array<{ name: string; description: string; handlerKind: string }>;
  flags: Array<{ name: string; type: string; defaultValue: string; description: string }>;
  renderers: Array<{ customType: string; kind: string }>;
}

function parseExtensionPayload(raw: unknown): ExtensionParsed | null {
  if (!isObject(raw)) return null;
  const manifestVersion = asString(raw.manifest_version);
  const extensionId = asString(raw.extension_id);
  if (!manifestVersion || !extensionId) return null;
  const commandsRaw = Array.isArray(raw.commands) ? raw.commands : [];
  const commands = commandsRaw.flatMap((item) => {
    if (!isObject(item)) return [];
    const name = asString(item.name);
    if (!name) return [];
    const handler = isObject(item.handler) ? item.handler : null;
    return [{
      name,
      description: asString(item.description) ?? "",
      handlerKind: handler ? asString(handler.kind) ?? "unknown" : "unknown",
    }];
  });
  const flagsRaw = Array.isArray(raw.flags) ? raw.flags : [];
  const flags = flagsRaw.flatMap((item) => {
    if (!isObject(item)) return [];
    const name = asString(item.name);
    if (!name) return [];
    return [{
      name,
      type: asString(item.type) ?? "unknown",
      defaultValue: stringifyLite(item.default),
      description: asString(item.description) ?? "",
    }];
  });
  const renderersRaw = Array.isArray(raw.message_renderers) ? raw.message_renderers : [];
  const renderers = renderersRaw.flatMap((item) => {
    if (!isObject(item)) return [];
    const customType = asString(item.custom_type);
    const renderer = isObject(item.renderer) ? item.renderer : null;
    if (!customType) return [];
    return [{ customType, kind: renderer ? asString(renderer.kind) ?? "unknown" : "unknown" }];
  });
  return { manifestVersion, extensionId, commands, flags, renderers };
}

function CompactList({
  title,
  items,
}: {
  title: string;
  items: Array<{ key: string; meta: string; description: string | null }>;
}) {
  return (
    <div className="rounded-[8px] border border-border">
      <div className="border-b border-border bg-secondary/20 px-3 py-1.5">
        <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">{title}</p>
      </div>
      <ul className="divide-y divide-border">
        {items.map((item) => (
          <li key={item.key} className="px-3 py-2">
            <div className="flex items-center justify-between gap-2">
              <span className="truncate font-mono text-[11px] text-foreground/85">{item.key}</span>
              <span className="shrink-0 text-[10px] text-muted-foreground">{item.meta}</span>
            </div>
            {item.description && (
              <p className="mt-1 line-clamp-2 text-[11px] text-muted-foreground">
                {item.description}
              </p>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

function stringifyLite(value: unknown): string {
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "boolean" || typeof value === "number") return String(value);
  if (value == null) return "null";
  return "json";
}

function sourceLabel(source: LibraryAssetDto["source"]): string {
  switch (source) {
    case "plugin_embedded":
      return "Plugin";
    case "user_authored":
      return "User";
    case "remote_imported":
      return "Remote";
    case "builtin":
      return "Builtin";
  }
}

/* Fallback */

function RawPayloadFallback({ payload }: { payload: unknown }) {
  const json = useMemo(() => {
    try {
      return JSON.stringify(payload, null, 2);
    } catch {
      return String(payload);
    }
  }, [payload]);
  return (
    <section>
      <SectionLabel>原始 payload</SectionLabel>
      <p className="mt-2 mb-2 text-xs text-muted-foreground">
        无法解析为已知 schema，展示原始 JSON。
      </p>
      <pre className="max-h-96 overflow-auto rounded-[8px] border border-border bg-secondary/20 px-3 py-2 font-mono text-[11px] leading-5 text-muted-foreground">
        {json}
      </pre>
    </section>
  );
}

/* ─── Atoms ─── */

function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <p className="text-[11px] font-semibold uppercase tracking-[0.12em] text-muted-foreground">
      {children}
    </p>
  );
}

function MetaChip({
  children,
  tone = "neutral",
}: {
  children: React.ReactNode;
  tone?: "neutral" | "amber";
}) {
  const cls =
    tone === "amber"
      ? "border-amber-500/30 bg-amber-500/10 text-amber-700 dark:text-amber-300"
      : "border-border bg-secondary/40 text-muted-foreground";
  return (
    <span className={`rounded-[6px] border px-1.5 py-0.5 text-[11px] ${cls}`}>{children}</span>
  );
}

function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  return `${text.slice(0, max)}\n…（已截断 ${text.length - max} 字符）`;
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

/* ─── ConfirmOverwriteDialog ─── */

export interface ConfirmOverwriteDialogProps {
  asset: LibraryAssetDto;
  installedVersion: string;
  busy: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}

export function ConfirmOverwriteDialog({
  asset,
  installedVersion,
  busy,
  onCancel,
  onConfirm,
}: ConfirmOverwriteDialogProps) {
  return (
    <div
      className="fixed inset-0 z-[92] flex items-center justify-center bg-black/40"
      onClick={onCancel}
    >
      <div
        className="w-[420px] rounded-[12px] border border-border bg-background p-5 shadow-xl"
        onClick={(e) => e.stopPropagation()}
      >
        <h3 className="text-sm font-semibold text-foreground">确认覆盖更新</h3>
        <p className="mt-2 text-xs leading-5 text-muted-foreground">
          将更新「<span className="font-medium text-foreground">{asset.display_name}</span>」
          v{installedVersion} → v{asset.version}。
          <span className="mt-1 block">本地若有未同步修改将被覆盖，且不可撤销。</span>
        </p>
        <div className="mt-4 flex justify-end gap-2">
          <button
            type="button"
            onClick={onCancel}
            disabled={busy}
            className="agentdash-button-secondary"
          >
            取消
          </button>
          <button
            type="button"
            onClick={onConfirm}
            disabled={busy}
            className="rounded-[8px] border border-destructive/30 bg-destructive px-3 py-1.5 text-xs text-destructive-foreground transition-colors hover:opacity-90 disabled:opacity-50"
          >
            {busy ? "更新中…" : "覆盖更新"}
          </button>
        </div>
      </div>
    </div>
  );
}
