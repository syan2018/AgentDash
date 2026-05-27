import { useCallback, useEffect, useMemo, useState } from "react";

import { Badge, Button, ConfirmDialog } from "@agentdash/ui";

import type { ProjectExtensionManagementItemResponse } from "../../../generated/extension-management-contracts";
import { downloadExtensionArtifact } from "../../../services/extensionPackage";
import { fetchProjectExtensions } from "../../../services/extensionManagement";
import { uninstallExtensionInstallation } from "../../../services/extensionRuntime";
import { useProjectStore } from "../../../stores/projectStore";
import { Notice, type NoticeData } from "../_shared/Notice";
import { InstallExtensionPackageDialog } from "./extension/InstallExtensionPackageDialog";

type BusyState =
  | { kind: "import" }
  | { kind: "refresh" }
  | { kind: "download"; installationId: string }
  | { kind: "uninstall"; installationId: string };
type GlobalBusyKind = "import" | "refresh";

type DialogState =
  | { kind: "closed" }
  | { kind: "import" }
  | { kind: "uninstall"; extension: ProjectExtensionManagementItemResponse };

export function ExtensionCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const [extensions, setExtensions] = useState<ProjectExtensionManagementItemResponse[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<NoticeData | null>(null);
  const [busy, setBusy] = useState<BusyState | null>(null);
  const [dialog, setDialog] = useState<DialogState>({ kind: "closed" });

  const showSuccess = useCallback(
    (message: string) => setNotice({ tone: "success", message }),
    [],
  );
  const showError = useCallback(
    (message: string) => setNotice({ tone: "danger", message }),
    [],
  );
  const clearNotice = useCallback(() => setNotice(null), []);

  const refresh = useCallback(
    async (projectId: string, busyKind: GlobalBusyKind | null = null) => {
      if (busyKind) setBusy({ kind: busyKind });
      setIsLoading(true);
      setError(null);
      try {
        const result = await fetchProjectExtensions(projectId);
        setExtensions(result.extensions);
      } catch (err) {
        const message = err instanceof Error ? err.message : "加载 Extension 失败";
        setError(message);
        showError(message);
      } finally {
        setIsLoading(false);
        if (busyKind) setBusy(null);
      }
    },
    [showError],
  );

  useEffect(() => {
    if (!currentProjectId) {
      setExtensions([]);
      setError(null);
      return;
    }
    void refresh(currentProjectId);
  }, [currentProjectId, refresh]);

  const sortedExtensions = useMemo(
    () =>
      [...extensions].sort((a, b) =>
        a.display_name.localeCompare(b.display_name, "zh-Hans"),
      ),
    [extensions],
  );

  const handleDownload = useCallback(
    async (extension: ProjectExtensionManagementItemResponse) => {
      if (!currentProjectId || !extension.package_artifact) return;
      setBusy({ kind: "download", installationId: extension.installation_id });
      try {
        const { blob, filename } = await downloadExtensionArtifact(
          currentProjectId,
          extension.package_artifact.artifact_id,
        );
        const fallback = `${extension.extension_id}-${extension.package_artifact.package_version}.agentdash-extension.tgz`;
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = filename || fallback;
        document.body.appendChild(anchor);
        anchor.click();
        document.body.removeChild(anchor);
        URL.revokeObjectURL(url);
      } catch (err) {
        showError(err instanceof Error ? err.message : "下载 Extension 包失败");
      } finally {
        setBusy(null);
      }
    },
    [currentProjectId, showError],
  );

  const handleUninstallConfirm = useCallback(async () => {
    if (!currentProjectId || dialog.kind !== "uninstall") return;
    const extension = dialog.extension;
    setBusy({ kind: "uninstall", installationId: extension.installation_id });
    try {
      await uninstallExtensionInstallation(currentProjectId, extension.installation_id);
      showSuccess(`已卸载 ${extension.extension_key}`);
      setDialog({ kind: "closed" });
      await refresh(currentProjectId);
    } catch (err) {
      showError(err instanceof Error ? err.message : "卸载 Extension 失败");
    } finally {
      setBusy(null);
    }
  }, [currentProjectId, dialog, refresh, showError, showSuccess]);

  if (!currentProjectId) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        请选择项目
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="text-[11px] uppercase text-muted-foreground">Project Extensions</p>
          <h2 className="text-lg font-semibold text-foreground">Extension</h2>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => void refresh(currentProjectId, "refresh")}
            disabled={isLoading || busy?.kind === "refresh"}
          >
            刷新
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={() => setDialog({ kind: "import" })}
            disabled={busy?.kind === "import"}
          >
            从本地包安装
          </Button>
        </div>
      </header>

      <Notice notice={notice} onDismiss={clearNotice} />

      {error && (
        <div className="rounded-[8px] border border-destructive/30 bg-destructive/5 p-4 text-sm text-destructive">
          {error}
        </div>
      )}

      {isLoading && sortedExtensions.length === 0 ? (
        <div className="rounded-[8px] border border-border bg-background p-6 text-sm text-muted-foreground">
          正在加载 Extension...
        </div>
      ) : sortedExtensions.length === 0 ? (
        <div className="rounded-[8px] border border-dashed border-border bg-secondary/20 p-6 text-center text-sm text-muted-foreground">
          当前项目还未安装 Extension
        </div>
      ) : (
        <div className="grid gap-3">
          {sortedExtensions.map((extension) => (
            <ExtensionAssetCard
              key={extension.installation_id}
              extension={extension}
              busy={isBusyForExtension(busy, extension.installation_id)}
              onDownload={
                extension.package_artifact ? () => void handleDownload(extension) : null
              }
              onUninstall={() => setDialog({ kind: "uninstall", extension })}
            />
          ))}
        </div>
      )}

      {dialog.kind === "import" && (
        <InstallExtensionPackageDialog
          projectId={currentProjectId}
          open={true}
          onClose={() => setDialog({ kind: "closed" })}
          onInstalled={(extensionKey) => {
            showSuccess(`已安装 ${extensionKey}`);
            void refresh(currentProjectId);
          }}
        />
      )}

      <ConfirmDialog
        open={dialog.kind === "uninstall"}
        title="卸载 Extension"
        description={
          dialog.kind === "uninstall"
            ? `确认卸载 ${dialog.extension.display_name}？`
            : ""
        }
        confirmLabel="卸载"
        tone="danger"
        isConfirming={busy?.kind === "uninstall"}
        onClose={() => setDialog({ kind: "closed" })}
        onConfirm={() => void handleUninstallConfirm()}
      />
    </div>
  );
}

function ExtensionAssetCard({
  extension,
  busy,
  onDownload,
  onUninstall,
}: {
  extension: ProjectExtensionManagementItemResponse;
  busy: boolean;
  onDownload: (() => void) | null;
  onUninstall: () => void;
}) {
  const counts = capabilityCounts(extension);
  const manifestJson = JSON.stringify(extension.manifest, null, 2) ?? "";
  return (
    <article className="rounded-[8px] border border-border bg-background transition-colors hover:border-primary/25">
      <div className="flex flex-wrap items-start justify-between gap-3 p-4">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="truncate text-sm font-semibold text-foreground">
              {extension.display_name}
            </h3>
            <span className="truncate font-mono text-[11px] text-muted-foreground">
              {extension.extension_key}
            </span>
            <Badge variant={sourceBadgeVariant(extension)}>{sourceLabel(extension)}</Badge>
            <Badge variant={packageBadgeVariant(extension)}>
              {packageModeLabel(extension.package_mode)}
            </Badge>
            {extension.source_status && (
              <Badge variant={sourceStatusVariant(extension.source_status)}>
                {sourceStatusLabel(extension.source_status)}
              </Badge>
            )}
          </div>
          <p className="mt-1 truncate font-mono text-[11px] text-muted-foreground">
            {extension.extension_id}
            {extension.package_artifact
              ? ` · ${extension.package_artifact.package_name}@${extension.package_artifact.package_version}`
              : ""}
          </p>
          {counts.length > 0 && (
            <p className="mt-2 text-[11px] text-muted-foreground">{counts.join(" · ")}</p>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-2">
          {onDownload && (
            <Button variant="secondary" size="sm" onClick={onDownload} disabled={busy}>
              下载包
            </Button>
          )}
          <Button variant="danger" size="sm" onClick={onUninstall} disabled={busy}>
            卸载
          </Button>
        </div>
      </div>
      <details className="border-t border-border/60 bg-secondary/20 px-4 py-3 text-xs">
        <summary className="cursor-pointer text-muted-foreground">详情</summary>
        <div className="mt-3 grid gap-3 md:grid-cols-2">
          <Detail label="Source">
            {extension.installed_source
              ? `${extension.installed_source.source_ref} · ${extension.installed_source.source_version}`
              : "local package"}
          </Detail>
          <Detail label="Package">
            {extension.package_artifact
              ? `${extension.package_artifact.archive_digest}`
              : packageModeLabel(extension.package_mode)}
          </Detail>
          <div className="md:col-span-2">
            <p className="mb-1 text-[10px] font-semibold uppercase text-muted-foreground">
              Manifest
            </p>
            <pre className="max-h-56 overflow-auto rounded-[8px] border border-border bg-background p-3 font-mono text-[11px] text-foreground/80">
              {manifestJson}
            </pre>
          </div>
        </div>
      </details>
    </article>
  );
}

function Detail({ label, children }: { label: string; children: string }) {
  return (
    <div>
      <p className="mb-1 text-[10px] font-semibold uppercase text-muted-foreground">
        {label}
      </p>
      <p className="break-all font-mono text-[11px] text-foreground/80">{children}</p>
    </div>
  );
}

function isBusyForExtension(busy: BusyState | null, installationId: string): boolean {
  return (
    (busy?.kind === "download" && busy.installationId === installationId) ||
    (busy?.kind === "uninstall" && busy.installationId === installationId)
  );
}

function capabilityCounts(extension: ProjectExtensionManagementItemResponse): string[] {
  const summary = extension.capability_summary;
  const counts: string[] = [];
  if (summary.workspace_tabs > 0) counts.push(`${summary.workspace_tabs} tabs`);
  if (summary.runtime_actions > 0) counts.push(`${summary.runtime_actions} actions`);
  if (summary.protocol_channels > 0) counts.push(`${summary.protocol_channels} channels`);
  if (summary.commands > 0) counts.push(`${summary.commands} commands`);
  if (summary.flags > 0) counts.push(`${summary.flags} flags`);
  if (summary.message_renderers > 0) counts.push(`${summary.message_renderers} renderers`);
  if (summary.permissions > 0) counts.push(`${summary.permissions} permissions`);
  if (summary.bundles > 0) counts.push(`${summary.bundles} bundles`);
  return counts;
}

function sourceLabel(extension: ProjectExtensionManagementItemResponse): string {
  return extension.installed_source ? "Marketplace" : "Local package";
}

function sourceBadgeVariant(
  extension: ProjectExtensionManagementItemResponse,
): "accent" | "info" {
  return extension.installed_source ? "accent" : "info";
}

function packageModeLabel(mode: ProjectExtensionManagementItemResponse["package_mode"]): string {
  switch (mode) {
    case "packaged":
      return "Packaged";
    case "declaration_only":
      return "Declaration";
    case "invalid_missing_artifact":
      return "Missing package";
  }
}

function packageBadgeVariant(
  extension: ProjectExtensionManagementItemResponse,
): "success" | "neutral" | "danger" {
  switch (extension.package_mode) {
    case "packaged":
      return "success";
    case "declaration_only":
      return "neutral";
    case "invalid_missing_artifact":
      return "danger";
  }
}

function sourceStatusLabel(status: string): string {
  switch (status) {
    case "up_to_date":
      return "Up to date";
    case "update_available":
      return "Update available";
    case "source_missing":
      return "Source missing";
    default:
      return status;
  }
}

function sourceStatusVariant(status: string): "success" | "warning" | "danger" | "neutral" {
  switch (status) {
    case "up_to_date":
      return "success";
    case "update_available":
      return "warning";
    case "source_missing":
      return "danger";
    default:
      return "neutral";
  }
}

export default ExtensionCategoryPanel;
