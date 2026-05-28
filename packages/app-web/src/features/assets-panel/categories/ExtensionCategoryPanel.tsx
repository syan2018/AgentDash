import { useCallback, useEffect, useMemo, useState } from "react";

import {
  AssetCard,
  Badge,
  Button,
  CardMenu,
  CreateButton,
  DangerConfirmDialog,
  DetailMenu,
  DetailPanel,
  DetailSection,
  DismissibleNotice,
  type DismissibleNoticeData,
  EmptyState,
  InspectorRow,
  OriginBadge as UiOriginBadge,
} from "@agentdash/ui";
import type { DetailMenuItem } from "@agentdash/ui";

import { buildAssetMenuItems, type BuildAssetMenuOptions } from "../_shared/assetMenu";

import { asRecord } from "../../../api/mappers";
import type {
  ProjectExtensionManagementItemResponse,
  ProjectExtensionPackageArtifactRefResponse,
} from "../../../generated/extension-management-contracts";
import { downloadExtensionArtifact } from "../../../services/extensionPackage";
import { fetchProjectExtensions } from "../../../services/extensionManagement";
import { uninstallExtensionInstallation } from "../../../services/extensionRuntime";
import { useCurrentUserStore } from "../../../stores/currentUserStore";
import { useProjectStore } from "../../../stores/projectStore";
import type { LibraryAssetDto } from "../../../types";
import { PublishedBadge } from "../_shared/PublishedBadge";
import { SelectProjectEmpty } from "../_shared/SelectProjectEmpty";
import { resolveOriginBadge } from "../_shared/origin-badge-tone";
import { useLibraryPublishedAssets } from "../_shared/useLibraryPublishedAssets";
import { PublishLibraryAssetDialog } from "../publish/PublishLibraryAssetDialog";
import { InstallExtensionPackageDialog } from "./extension/InstallExtensionPackageDialog";

type BusyState =
  | { kind: "refresh" }
  | { kind: "download"; installationId: string }
  | { kind: "uninstall"; installationId: string };
type GlobalBusyKind = "refresh";

type DialogState =
  | { kind: "closed" }
  | { kind: "import" }
  | { kind: "uninstall"; extension: ProjectExtensionManagementItemResponse };

export function ExtensionCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const currentUserId = useCurrentUserStore((s) => s.currentUser?.user_id ?? null);
  const [extensions, setExtensions] = useState<ProjectExtensionManagementItemResponse[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<DismissibleNoticeData | null>(null);
  const [busy, setBusy] = useState<BusyState | null>(null);
  const [dialog, setDialog] = useState<DialogState>({ kind: "closed" });
  const [detailInstallationId, setDetailInstallationId] = useState<string | null>(null);
  const [publishTarget, setPublishTarget] =
    useState<ProjectExtensionManagementItemResponse | null>(null);
  const { publishedByKey, reloadPublished } = useLibraryPublishedAssets("extension_template");

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
      setDetailInstallationId(null);
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

  const statsText = useMemo(() => extensionStatsText(extensions), [extensions]);
  const selectedExtension = useMemo(
    () =>
      detailInstallationId
        ? extensions.find((extension) => extension.installation_id === detailInstallationId) ??
          null
        : null,
    [detailInstallationId, extensions],
  );
  const selectedPublished = selectedExtension
    ? publishedByKey.get(selectedExtension.extension_key) ?? null
    : null;

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
      if (detailInstallationId === extension.installation_id) setDetailInstallationId(null);
      setDialog({ kind: "closed" });
      await refresh(currentProjectId);
    } catch (err) {
      showError(err instanceof Error ? err.message : "卸载 Extension 失败");
    } finally {
      setBusy(null);
    }
  }, [
    currentProjectId,
    detailInstallationId,
    dialog,
    refresh,
    showError,
    showSuccess,
  ]);

  if (!currentProjectId) {
    return <SelectProjectEmpty assetLabel="Extension 资产" />;
  }

  const actionHandlers: ExtensionActionHandlers = {
    onOpenDetail: (extension) => setDetailInstallationId(extension.installation_id),
    onPublish: (extension) => setPublishTarget(extension),
    onDownload: (extension) => void handleDownload(extension),
    onUninstall: (extension) => setDialog({ kind: "uninstall", extension }),
  };

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div className="space-y-1">
          <h2 className="text-base font-semibold tracking-tight text-foreground">
            Extension 资产
          </h2>
          <p className="text-xs text-muted-foreground">{statsText}</p>
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
          <CreateButton
            entity="本地包"
            onClick={() => setDialog({ kind: "import" })}
          />
        </div>
      </header>

      <DismissibleNotice notice={notice} onDismiss={clearNotice} />

      {error && (
        <div className="rounded-[8px] border border-destructive/30 bg-destructive/5 p-4 text-sm text-destructive">
          {error}
        </div>
      )}

      {isLoading && sortedExtensions.length === 0 ? (
        <EmptyState className="px-6 py-10">正在加载 Extension 资产…</EmptyState>
      ) : (
        <ExtensionGrid
          extensions={sortedExtensions}
          publishedByKey={publishedByKey}
          busy={busy}
          actions={actionHandlers}
        />
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

      <ExtensionDetailPanel
        extension={selectedExtension}
        published={selectedPublished}
        busy={busy}
        actions={actionHandlers}
        onClose={() => setDetailInstallationId(null)}
      />

      <DangerConfirmDialog
        open={dialog.kind === "uninstall"}
        title="卸载 Extension"
        description={
          dialog.kind === "uninstall"
            ? `确定要卸载 ${dialog.extension.display_name} 吗？此操作不可撤销。`
            : ""
        }
        confirmLabel={busy?.kind === "uninstall" ? "卸载中…" : "卸载"}
        isConfirming={busy?.kind === "uninstall"}
        onClose={() => setDialog({ kind: "closed" })}
        onConfirm={() => void handleUninstallConfirm()}
      />

      {publishTarget && (
        <PublishLibraryAssetDialog
          projectId={currentProjectId}
          assetKind="extension_installation"
          projectAssetId={publishTarget.installation_id}
          defaults={{
            key: publishTarget.extension_key,
            display_name: publishTarget.display_name,
            description: publishTarget.extension_id,
          }}
          currentUserId={currentUserId}
          onClose={() => setPublishTarget(null)}
          onPublished={(message) => {
            showSuccess(message);
            void refresh(currentProjectId);
            reloadPublished();
          }}
        />
      )}
    </div>
  );
}

interface ExtensionActionHandlers {
  onOpenDetail: (extension: ProjectExtensionManagementItemResponse) => void;
  onPublish: (extension: ProjectExtensionManagementItemResponse) => void;
  onDownload: (extension: ProjectExtensionManagementItemResponse) => void;
  onUninstall: (extension: ProjectExtensionManagementItemResponse) => void;
}

function ExtensionGrid({
  extensions,
  publishedByKey,
  busy,
  actions,
}: {
  extensions: ProjectExtensionManagementItemResponse[];
  publishedByKey: Map<string, LibraryAssetDto>;
  busy: BusyState | null;
  actions: ExtensionActionHandlers;
}) {
  if (extensions.length === 0) {
    return (
      <EmptyState className="px-6 py-14">
        <p className="text-sm text-foreground">暂无 Extension 资产</p>
        <p className="mt-1.5 text-xs text-muted-foreground">
          点击右上角"+ 本地包"导入扩展包，或从资源市场安装 Extension 模板
        </p>
      </EmptyState>
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
      {extensions.map((extension) => {
        const published = publishedByKey.get(extension.extension_key) ?? null;
        const isBusy = isBusyForExtension(busy, extension.installation_id);
        const menuItems = buildAssetMenuItems(
          extensionMenuOptions(extension, published, isBusy, actions),
        );
        return (
          <AssetCard
            key={extension.installation_id}
            onOpen={() => actions.onOpenDetail(extension)}
            openTitle="查看详情"
            title={extension.display_name}
            subtitle={
              <span className="font-mono text-[11px]">
                extensions/{extension.extension_key}
              </span>
            }
            headerRight={
              <>
                {published && <PublishedBadge version={published.version} />}
                <ExtensionOriginBadge extension={extension} />
                <CardMenu items={menuItems} />
              </>
            }
          >
            <p className="mt-1.5 truncate font-mono text-[11px] text-muted-foreground">
              {extension.extension_id}
            </p>
            <div className="mt-3 flex flex-wrap items-center gap-1.5">
              <Badge variant={packageBadgeVariant(extension)}>
                {packageModeLabel(extension.package_mode)}
              </Badge>
              {extension.source_status && (
                <Badge variant={sourceStatusVariant(extension.source_status)}>
                  {sourceStatusLabel(extension.source_status)}
                </Badge>
              )}
              {capabilityLabels(extension)
                .slice(0, 4)
                .map((label) => (
                  <span
                    key={label}
                    className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 text-[11px] text-muted-foreground"
                  >
                    {label}
                  </span>
                ))}
            </div>
          </AssetCard>
        );
      })}
    </div>
  );
}

function ExtensionDetailPanel({
  extension,
  published,
  busy,
  actions,
  onClose,
}: {
  extension: ProjectExtensionManagementItemResponse | null;
  published: LibraryAssetDto | null;
  busy: BusyState | null;
  actions: ExtensionActionHandlers;
  onClose: () => void;
}) {
  const manifestJson = extension ? JSON.stringify(extension.manifest, null, 2) : "";
  const menuItems = extension
    ? extensionDetailMenuItems(
        extension,
        published,
        isBusyForExtension(busy, extension.installation_id),
        actions,
      )
    : [];

  return (
    <DetailPanel
      open={Boolean(extension)}
      title={extension?.display_name ?? "Extension"}
      subtitle={extension ? `extensions/${extension.extension_key}` : undefined}
      onClose={onClose}
      headerExtra={extension ? <DetailMenu items={menuItems} /> : null}
      widthClassName="max-w-3xl"
    >
      {extension && (
        <div className="space-y-4 p-5">
          <DetailSection
            title="来源"
            extra={
              <div className="flex items-center gap-1">
                {published && <PublishedBadge version={published.version} />}
                <ExtensionOriginBadge extension={extension} />
              </div>
            }
          >
            <div className="grid gap-3 sm:grid-cols-2">
              <InspectorRow label="extension_key" value={extension.extension_key} mono />
              <InspectorRow label="extension_id" value={extension.extension_id} mono />
              {extension.installed_source ? (
                <>
                  <InspectorRow
                    label="source_ref"
                    value={extension.installed_source.source_ref}
                    mono
                  />
                  <InspectorRow
                    label="source_version"
                    value={extension.installed_source.source_version}
                    mono
                  />
                  <InspectorRow
                    label="current_source_version"
                    value={extension.current_source_version ?? "未找到"}
                    mono
                  />
                  <InspectorRow
                    label="source_status"
                    value={
                      extension.source_status
                        ? sourceStatusLabel(extension.source_status)
                        : "未安装来源"
                    }
                  />
                </>
              ) : (
                <InspectorRow label="source" value="本地包导入" />
              )}
            </div>
          </DetailSection>

          <DetailSection
            title="Package"
            extra={
              extension.package_artifact ? (
                <Button
                  variant="secondary"
                  size="sm"
                  onClick={() => actions.onDownload(extension)}
                  disabled={isBusyForExtension(busy, extension.installation_id)}
                >
                  下载包
                </Button>
              ) : null
            }
          >
            <PackageInspector artifact={extension.package_artifact} extension={extension} />
          </DetailSection>

          <DetailSection title="能力">
            <CapabilitySummary extension={extension} />
          </DetailSection>

          <DetailSection title="Manifest">
            <pre className="max-h-80 overflow-auto rounded-[8px] border border-border bg-background p-3 font-mono text-[11px] leading-5 text-foreground/80">
              {manifestJson}
            </pre>
          </DetailSection>
        </div>
      )}
    </DetailPanel>
  );
}

function PackageInspector({
  artifact,
  extension,
}: {
  artifact: ProjectExtensionPackageArtifactRefResponse | null;
  extension: ProjectExtensionManagementItemResponse;
}) {
  if (!artifact) {
    return (
      <div className="grid gap-3 sm:grid-cols-2">
        <InspectorRow label="mode" value={packageModeLabel(extension.package_mode)} />
        <InspectorRow label="manifest package" value={manifestPackageLabel(extension)} mono />
      </div>
    );
  }

  return (
    <div className="grid gap-3 sm:grid-cols-2">
      <InspectorRow label="package" value={`${artifact.package_name}@${artifact.package_version}`} mono />
      <InspectorRow label="asset_version" value={artifact.asset_version} mono />
      <InspectorRow label="source_version" value={artifact.source_version} mono />
      <InspectorRow label="artifact_id" value={artifact.artifact_id} mono />
      <InspectorRow label="archive_digest" value={artifact.archive_digest} mono className="sm:col-span-2" />
      <InspectorRow label="manifest_digest" value={artifact.manifest_digest} mono className="sm:col-span-2" />
    </div>
  );
}

function CapabilitySummary({
  extension,
}: {
  extension: ProjectExtensionManagementItemResponse;
}) {
  const labels = capabilityLabels(extension);
  if (labels.length === 0) {
    return <p className="text-sm text-muted-foreground">无声明能力</p>;
  }
  return (
    <div className="flex flex-wrap gap-1.5">
      {labels.map((label) => (
        <span
          key={label}
          className="rounded-[6px] border border-border bg-secondary/40 px-1.5 py-0.5 text-[11px] text-muted-foreground"
        >
          {label}
        </span>
      ))}
    </div>
  );
}

function ExtensionOriginBadge({
  extension,
}: {
  extension: ProjectExtensionManagementItemResponse;
}) {
  const origin = resolveOriginBadge(
    extension.installed_source ? "marketplace" : "local_package",
    Boolean(extension.installed_source),
  );
  return (
    <UiOriginBadge
      label={origin.label}
      tone={origin.tone}
      url={extension.installed_source?.source_ref ?? null}
    />
  );
}

function extensionMenuOptions(
  extension: ProjectExtensionManagementItemResponse,
  published: LibraryAssetDto | null,
  busy: boolean,
  actions: ExtensionActionHandlers,
): BuildAssetMenuOptions {
  const extras: BuildAssetMenuOptions["extras"] = [];
  if (extension.package_artifact) {
    extras.push({
      key: "download",
      label: busy ? "处理中…" : "下载包",
      onSelect: () => {
        if (!busy) actions.onDownload(extension);
      },
    });
  }
  return {
    primary: { label: "详情", onSelect: () => actions.onOpenDetail(extension) },
    publish: canPublishExtension(extension)
      ? {
          published: Boolean(published),
          onSelect: () => {
            if (!busy) actions.onPublish(extension);
          },
        }
      : null,
    extras,
    danger: {
      label: "卸载",
      busy,
      onSelect: () => {
        if (!busy) actions.onUninstall(extension);
      },
    },
  };
}

function extensionDetailMenuItems(
  extension: ProjectExtensionManagementItemResponse,
  published: LibraryAssetDto | null,
  busy: boolean,
  actions: ExtensionActionHandlers,
): DetailMenuItem[] {
  const items: DetailMenuItem[] = [];
  if (canPublishExtension(extension)) {
    items.push({
      key: "publish",
      label: published ? "更新发布" : "发布到资源市场",
      disabled: busy,
      onSelect: () => actions.onPublish(extension),
    });
  }
  if (extension.package_artifact) {
    items.push({
      key: "download",
      label: "下载包",
      disabled: busy,
      onSelect: () => actions.onDownload(extension),
    });
  }
  items.push({
    key: "uninstall",
    label: "卸载",
    danger: true,
    disabled: busy,
    onSelect: () => actions.onUninstall(extension),
  });
  return items;
}

function isBusyForExtension(busy: BusyState | null, installationId: string): boolean {
  return (
    (busy?.kind === "download" && busy.installationId === installationId) ||
    (busy?.kind === "uninstall" && busy.installationId === installationId)
  );
}

function canPublishExtension(extension: ProjectExtensionManagementItemResponse): boolean {
  return extension.package_mode !== "invalid_missing_artifact";
}

function capabilityLabels(extension: ProjectExtensionManagementItemResponse): string[] {
  const summary = extension.capability_summary;
  const labels: string[] = [];
  if (summary.workspace_tabs > 0) labels.push(`${summary.workspace_tabs} tabs`);
  if (summary.runtime_actions > 0) labels.push(`${summary.runtime_actions} actions`);
  if (summary.protocol_channels > 0) labels.push(`${summary.protocol_channels} channels`);
  if (summary.commands > 0) labels.push(`${summary.commands} commands`);
  if (summary.flags > 0) labels.push(`${summary.flags} flags`);
  if (summary.message_renderers > 0) labels.push(`${summary.message_renderers} renderers`);
  if (summary.permissions > 0) labels.push(`${summary.permissions} permissions`);
  if (summary.bundles > 0) labels.push(`${summary.bundles} bundles`);
  return labels;
}

function extensionStatsText(
  extensions: ProjectExtensionManagementItemResponse[],
): string {
  if (extensions.length === 0) {
    return "0 个 Extension";
  }
  const marketplace = extensions.filter((extension) => extension.installed_source).length;
  const local = extensions.length - marketplace;
  const updateAvailable = extensions.filter(
    (extension) => extension.source_status === "update_available",
  ).length;
  const invalid = extensions.filter(
    (extension) => extension.package_mode === "invalid_missing_artifact",
  ).length;
  const parts = [
    `${extensions.length} 个 Extension`,
    `${marketplace} 个来自市场`,
    `${local} 个本地导入`,
  ];
  if (updateAvailable > 0) parts.push(`${updateAvailable} 个可更新`);
  if (invalid > 0) parts.push(`${invalid} 个缺少包`);
  return parts.join(" · ");
}

function packageModeLabel(
  mode: ProjectExtensionManagementItemResponse["package_mode"],
): string {
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

function sourceStatusVariant(
  status: string,
): "success" | "warning" | "danger" | "neutral" {
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

function manifestPackageLabel(
  extension: ProjectExtensionManagementItemResponse,
): string {
  const manifest = asRecord(extension.manifest);
  const packageValue = manifest ? asRecord(manifest.package) : null;
  const name = typeof packageValue?.name === "string" ? packageValue.name : null;
  const version = typeof packageValue?.version === "string" ? packageValue.version : null;
  if (name && version) return `${name}@${version}`;
  if (name) return name;
  return "未声明";
}

export default ExtensionCategoryPanel;
