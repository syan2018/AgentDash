/**
 * InstalledExtensionRow — 已安装扩展条目。
 *
 * 行内信息：display_name + extension_key + source badge + version + 能力计数
 * 可展开后显示：permissions / workspace tabs / runtime actions / bundle digest
 *
 * onDownload 为 null 时不渲染下载按钮（marketplace 来源、无 package_artifact 时）。
 */

import { useState } from "react";

import { Button } from "@agentdash/ui";

import type { InstalledExtensionRowVM, InstalledExtensionSource } from "./extensionAggregation";

interface Props {
  row: InstalledExtensionRowVM;
  busy: boolean;
  onDownload: (() => void) | null;
  onUninstall: () => void;
}

const SOURCE_LABEL: Record<InstalledExtensionSource, string> = {
  marketplace: "Marketplace",
  local_archive: "本地归档",
  marketplace_with_archive: "Marketplace（含归档）",
  unknown: "未知",
};

const SOURCE_CLASSNAME: Record<InstalledExtensionSource, string> = {
  marketplace: "border-secondary/60 bg-secondary/40 text-foreground/80",
  local_archive: "border-primary/30 bg-primary/8 text-primary",
  marketplace_with_archive: "border-primary/20 bg-primary/5 text-foreground/80",
  unknown: "border-border bg-secondary/30 text-muted-foreground",
};

export function InstalledExtensionRow({ row, busy, onDownload, onUninstall }: Props) {
  const [expanded, setExpanded] = useState(false);
  const { installation, source, version, permissions, workspaceTabs, runtimeActions, commands, flags, messageRenderers, bundle } = row;

  const counts: string[] = [];
  if (workspaceTabs.length) counts.push(`${workspaceTabs.length} tabs`);
  if (runtimeActions.length) counts.push(`${runtimeActions.length} actions`);
  if (commands.length) counts.push(`${commands.length} commands`);
  if (flags.length) counts.push(`${flags.length} flags`);
  if (messageRenderers.length) counts.push(`${messageRenderers.length} renderers`);
  if (permissions.length) counts.push(`${permissions.length} permissions`);

  return (
    <article className="rounded-[8px] border border-border bg-background transition-colors hover:border-primary/25">
      <div className="flex items-start justify-between gap-3 p-4">
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="flex min-w-0 flex-1 items-start gap-2 text-left"
          aria-expanded={expanded}
        >
          <Chevron expanded={expanded} />
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-2">
              <h3 className="truncate text-sm font-semibold text-foreground">
                {installation.display_name}
              </h3>
              <span className="truncate font-mono text-[11px] text-muted-foreground">
                {installation.extension_key}
              </span>
              <span
                className={`rounded-[6px] border px-1.5 py-0.5 text-[10px] font-medium ${SOURCE_CLASSNAME[source]}`}
              >
                {SOURCE_LABEL[source]}
              </span>
            </div>
            <p className="mt-0.5 truncate font-mono text-[11px] text-muted-foreground">
              {installation.extension_id}
              {version ? ` · v${version}` : ""}
            </p>
            {counts.length > 0 && (
              <p className="mt-1 text-[11px] text-muted-foreground">{counts.join(" · ")}</p>
            )}
          </div>
        </button>
        <div className="flex shrink-0 items-center gap-1.5">
          {onDownload && (
            <Button variant="secondary" size="sm" onClick={onDownload} disabled={busy}>
              下载归档
            </Button>
          )}
          <Button variant="danger" size="sm" onClick={onUninstall} disabled={busy}>
            卸载
          </Button>
        </div>
      </div>
      {expanded && (
        <div className="space-y-3 border-t border-border/60 bg-secondary/20 px-4 py-3 text-xs">
          {permissions.length > 0 && (
            <DetailGroup label="Permissions">
              <ul className="space-y-1">
                {permissions.map((permission, index) => (
                  <li key={index} className="font-mono text-foreground/80">
                    {describePermission(permission)}
                  </li>
                ))}
              </ul>
            </DetailGroup>
          )}
          {workspaceTabs.length > 0 && (
            <DetailGroup label="Workspace Tabs">
              <ul className="space-y-1">
                {workspaceTabs.map((tab) => (
                  <li key={tab.type_id} className="font-mono text-foreground/80">
                    {tab.type_id} · {tab.renderer.kind}
                  </li>
                ))}
              </ul>
            </DetailGroup>
          )}
          {runtimeActions.length > 0 && (
            <DetailGroup label="Runtime Actions">
              <ul className="space-y-1">
                {runtimeActions.map((action) => (
                  <li key={action.action_key} className="font-mono text-foreground/80">
                    {action.action_key} · {action.kind}
                  </li>
                ))}
              </ul>
            </DetailGroup>
          )}
          {commands.length > 0 && (
            <DetailGroup label="Commands">
              <ul className="space-y-1">
                {commands.map((command) => (
                  <li key={command.name} className="font-mono text-foreground/80">
                    {command.name}
                  </li>
                ))}
              </ul>
            </DetailGroup>
          )}
          {bundle && (
            <DetailGroup label="Bundle">
              <p className="font-mono text-foreground/80">
                {bundle.entry} · {truncateDigest(bundle.digest)}
              </p>
            </DetailGroup>
          )}
        </div>
      )}
    </article>
  );
}

function DetailGroup({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <p className="mb-1 text-[10px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
        {label}
      </p>
      {children}
    </div>
  );
}

function describePermission(permission: InstalledExtensionRowVM["permissions"][number]): string {
  switch (permission.kind) {
    case "local_profile":
      return `local_profile · ${permission.access}`;
    case "workspace":
      return `workspace · ${permission.access}`;
    case "runtime_action":
      return `runtime_action · ${permission.action_key}`;
    default:
      return JSON.stringify(permission);
  }
}

function truncateDigest(digest: string): string {
  if (digest.length <= 20) return digest;
  return `${digest.slice(0, 14)}…${digest.slice(-6)}`;
}

function Chevron({ expanded }: { expanded: boolean }) {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      className={`mt-1 shrink-0 text-muted-foreground transition-transform ${expanded ? "rotate-90" : ""}`}
      aria-hidden="true"
    >
      <polyline points="9 18 15 12 9 6" />
    </svg>
  );
}

export default InstalledExtensionRow;
