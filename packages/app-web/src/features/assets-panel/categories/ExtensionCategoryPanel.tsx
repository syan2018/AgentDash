/**
 * ExtensionCategoryPanel — Assets 页 Extension 类目。
 *
 * 两段：
 * - 已安装：来自 Project extension runtime projection（按 extension_key 聚合）
 * - 归档：Project 下已上传的扩展 archive
 *
 * 写操作（上传 / 安装 / 卸载 / 下载）都收口到本面板；
 * 安装与卸载成功后强制刷 projection。
 */

import { useCallback, useEffect, useMemo, useState } from "react";

import { Button } from "@agentdash/ui";

import { useProjectStore } from "../../../stores/projectStore";
import { useProjectExtensionRuntime } from "../../extension-runtime";
import { useExtensionRuntimeStore } from "../../extension-runtime/model/extensionRuntimeStore";
import {
  downloadExtensionArtifact,
  listExtensionArtifacts,
} from "../../../services/extensionPackage";
import { uninstallExtensionInstallation } from "../../../services/extensionRuntime";
import type {
  ExtensionPackageArtifactResponse,
  ExtensionPackageInstallationResponse,
} from "../../../generated/extension-package-contracts";
import { Notice, type NoticeData } from "../_shared/Notice";
import {
  aggregateInstalledExtensions,
  type InstalledExtensionRowVM,
} from "./extension/extensionAggregation";
import { InstalledExtensionRow } from "./extension/InstalledExtensionRow";
import { ExtensionArtifactRow } from "./extension/ExtensionArtifactRow";
import { UploadExtensionDialog } from "./extension/UploadExtensionDialog";
import { InstallFromArtifactDialog } from "./extension/InstallFromArtifactDialog";
import { UninstallConfirmDialog } from "./extension/UninstallConfirmDialog";

type BusyState =
  | { kind: "upload" }
  | { kind: "install"; artifactId: string }
  | { kind: "download_installed"; installationId: string }
  | { kind: "download_artifact"; artifactId: string }
  | { kind: "uninstall"; installationId: string };

type DialogState =
  | { kind: "closed" }
  | { kind: "upload" }
  | { kind: "install"; artifact: ExtensionPackageArtifactResponse }
  | {
      kind: "uninstall";
      installationId: string;
      installationName: string;
      extensionKey: string;
    };

export function ExtensionCategoryPanel() {
  const currentProjectId = useProjectStore((s) => s.currentProjectId);
  const runtime = useProjectExtensionRuntime(currentProjectId);
  const fetchProject = useExtensionRuntimeStore((s) => s.fetchProject);

  const [artifacts, setArtifacts] = useState<ExtensionPackageArtifactResponse[]>([]);
  const [artifactsLoading, setArtifactsLoading] = useState(false);
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

  const refreshArtifacts = useCallback(
    async (projectId: string) => {
      setArtifactsLoading(true);
      try {
        const list = await listExtensionArtifacts(projectId);
        setArtifacts(list);
      } catch (err) {
        showError(err instanceof Error ? err.message : "加载归档列表失败");
      } finally {
        setArtifactsLoading(false);
      }
    },
    [showError],
  );

  useEffect(() => {
    if (!currentProjectId) {
      setArtifacts([]);
      return;
    }
    void refreshArtifacts(currentProjectId);
  }, [currentProjectId, refreshArtifacts]);

  const installedRows = useMemo(
    () => aggregateInstalledExtensions(runtime.projection),
    [runtime.projection],
  );

  const handleUploaded = useCallback(
    (artifact: ExtensionPackageArtifactResponse) => {
      if (!currentProjectId) return;
      showSuccess(`已上传 ${artifact.extension_id}`);
      void refreshArtifacts(currentProjectId);
      setDialog({ kind: "install", artifact });
    },
    [currentProjectId, refreshArtifacts, showSuccess],
  );

  const handleInstalled = useCallback(
    (installation: ExtensionPackageInstallationResponse) => {
      if (!currentProjectId) return;
      showSuccess(`已安装 ${installation.extension_key}`);
      void fetchProject(currentProjectId);
    },
    [currentProjectId, fetchProject, showSuccess],
  );

  const triggerDownload = useCallback(
    async (
      projectId: string,
      artifactId: string,
      fallbackFilename: string,
      busyKey: BusyState,
    ) => {
      setBusy(busyKey);
      try {
        const { blob, filename } = await downloadExtensionArtifact(projectId, artifactId);
        const url = URL.createObjectURL(blob);
        const anchor = document.createElement("a");
        anchor.href = url;
        anchor.download = filename || fallbackFilename;
        document.body.appendChild(anchor);
        anchor.click();
        document.body.removeChild(anchor);
        URL.revokeObjectURL(url);
      } catch (err) {
        showError(err instanceof Error ? err.message : "下载归档失败");
      } finally {
        setBusy(null);
      }
    },
    [showError],
  );

  const handleDownloadInstalled = useCallback(
    (row: InstalledExtensionRowVM) => {
      if (!currentProjectId) return;
      const ref = row.installation.package_artifact;
      if (!ref) return;
      const fallback = `${row.installation.extension_id}-${ref.package_version}.agentdash-extension.tgz`;
      void triggerDownload(currentProjectId, ref.artifact_id, fallback, {
        kind: "download_installed",
        installationId: row.installation.installation_id,
      });
    },
    [currentProjectId, triggerDownload],
  );

  const handleDownloadArtifact = useCallback(
    (artifact: ExtensionPackageArtifactResponse) => {
      if (!currentProjectId) return;
      const fallback = `${artifact.extension_id}-${artifact.package_version}.agentdash-extension.tgz`;
      void triggerDownload(currentProjectId, artifact.id, fallback, {
        kind: "download_artifact",
        artifactId: artifact.id,
      });
    },
    [currentProjectId, triggerDownload],
  );

  const handleUninstallConfirm = useCallback(async () => {
    if (!currentProjectId || dialog.kind !== "uninstall") return;
    const { installationId, extensionKey } = dialog;
    setBusy({ kind: "uninstall", installationId });
    try {
      await uninstallExtensionInstallation(currentProjectId, installationId);
      showSuccess(`已卸载 ${extensionKey}`);
      setDialog({ kind: "closed" });
      await fetchProject(currentProjectId);
    } catch (err) {
      showError(err instanceof Error ? err.message : "卸载失败");
    } finally {
      setBusy(null);
    }
  }, [currentProjectId, dialog, fetchProject, showError, showSuccess]);

  if (!currentProjectId) {
    return (
      <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
        请选择项目
      </div>
    );
  }

  const uploadBusy = busy?.kind === "upload";
  const showInstalledLoading = runtime.status === "loading";
  const showInstalledError = runtime.status === "error";

  return (
    <div className="flex h-full flex-col gap-4 p-6">
      <header className="flex flex-wrap items-center justify-between gap-3">
        <div>
          <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
            Project Extensions
          </p>
          <h2 className="text-lg font-semibold text-foreground">Extension</h2>
        </div>
        <div className="flex items-center gap-2">
          <Button
            variant="secondary"
            size="sm"
            onClick={() => {
              if (!currentProjectId) return;
              void fetchProject(currentProjectId);
              void refreshArtifacts(currentProjectId);
            }}
            disabled={artifactsLoading || runtime.status === "loading"}
          >
            刷新
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={() => setDialog({ kind: "upload" })}
            disabled={uploadBusy}
          >
            上传归档
          </Button>
        </div>
      </header>

      <Notice notice={notice} onDismiss={clearNotice} />

      <section className="flex flex-col gap-2">
        <h3 className="text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          已安装 ({installedRows.length})
        </h3>
        {showInstalledLoading ? (
          <div className="rounded-[8px] border border-border bg-background p-6 text-sm text-muted-foreground">
            正在加载已安装扩展…
          </div>
        ) : showInstalledError ? (
          <div className="rounded-[8px] border border-destructive/30 bg-destructive/5 p-6 text-sm text-destructive">
            {runtime.error ?? "加载已安装扩展失败"}
          </div>
        ) : installedRows.length === 0 ? (
          <div className="rounded-[8px] border border-dashed border-border bg-secondary/20 p-6 text-center text-sm text-muted-foreground">
            当前项目还未安装扩展
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            {installedRows.map((row) => {
              const installationId = row.installation.installation_id;
              const downloadBusy =
                busy?.kind === "download_installed" && busy.installationId === installationId;
              const uninstallBusy =
                busy?.kind === "uninstall" && busy.installationId === installationId;
              return (
                <InstalledExtensionRow
                  key={installationId}
                  row={row}
                  busy={downloadBusy || uninstallBusy}
                  onDownload={
                    row.installation.package_artifact
                      ? () => handleDownloadInstalled(row)
                      : null
                  }
                  onUninstall={() =>
                    setDialog({
                      kind: "uninstall",
                      installationId,
                      installationName: row.installation.display_name,
                      extensionKey: row.installation.extension_key,
                    })
                  }
                />
              );
            })}
          </div>
        )}
      </section>

      <section className="flex flex-col gap-2">
        <h3 className="text-[11px] font-semibold uppercase tracking-[0.14em] text-muted-foreground">
          归档库 ({artifacts.length})
        </h3>
        {artifactsLoading ? (
          <div className="rounded-[8px] border border-border bg-background p-6 text-sm text-muted-foreground">
            正在加载归档…
          </div>
        ) : artifacts.length === 0 ? (
          <div className="rounded-[8px] border border-dashed border-border bg-secondary/20 p-6 text-center text-sm text-muted-foreground">
            还没有上传过归档
          </div>
        ) : (
          <div className="flex flex-col gap-2">
            {artifacts.map((artifact) => {
              const installBusy = busy?.kind === "install" && busy.artifactId === artifact.id;
              const downloadBusy =
                busy?.kind === "download_artifact" && busy.artifactId === artifact.id;
              return (
                <ExtensionArtifactRow
                  key={artifact.id}
                  artifact={artifact}
                  busy={installBusy || downloadBusy}
                  onInstall={() => setDialog({ kind: "install", artifact })}
                  onDownload={() => handleDownloadArtifact(artifact)}
                />
              );
            })}
          </div>
        )}
      </section>

      {dialog.kind === "upload" && (
        <UploadExtensionDialog
          projectId={currentProjectId}
          open={true}
          onClose={() => setDialog({ kind: "closed" })}
          onUploaded={handleUploaded}
        />
      )}
      {dialog.kind === "install" && (
        <InstallFromArtifactDialog
          projectId={currentProjectId}
          artifact={dialog.artifact}
          open={true}
          onClose={() => setDialog({ kind: "closed" })}
          onInstalled={handleInstalled}
        />
      )}
      {dialog.kind === "uninstall" && (
        <UninstallConfirmDialog
          open={true}
          installationName={dialog.installationName}
          extensionKey={dialog.extensionKey}
          onClose={() => setDialog({ kind: "closed" })}
          onConfirm={handleUninstallConfirm}
        />
      )}
    </div>
  );
}

export default ExtensionCategoryPanel;
