import { useEffect, useMemo, useState } from "react";
import type { ApiHttpError } from "../../../api/client";
import { fetchLibraryAssets, publishLibraryAsset } from "../../../services/sharedLibrary";
import type {
  LibraryAssetDto,
  LibraryAssetType,
  PublishLibraryAssetKind,
} from "../../../types";
import { suggestNextVersion } from "./version";

export interface PublishLibraryAssetDefaults {
  key: string;
  display_name: string;
  description?: string | null;
}

export interface PublishLibraryAssetDialogProps {
  projectId: string;
  assetKind: PublishLibraryAssetKind;
  projectAssetId: string;
  defaults: PublishLibraryAssetDefaults;
  /** 当前用户 id；用于探测同 owner 的 user_authored 资产，未登录时传 null */
  currentUserId: string | null;
  onClose: () => void;
  onPublished: (message: string) => void;
}

type DialogMode =
  | { kind: "loading" }
  | { kind: "create" }
  | { kind: "update"; existing: LibraryAssetDto };

export function PublishLibraryAssetDialog({
  projectId,
  assetKind,
  projectAssetId,
  defaults,
  currentUserId,
  onClose,
  onPublished,
}: PublishLibraryAssetDialogProps) {
  const [mode, setMode] = useState<DialogMode>({ kind: "loading" });
  const [key, setKey] = useState(defaults.key);
  const [displayName, setDisplayName] = useState(defaults.display_name);
  const [description, setDescription] = useState(defaults.description ?? "");
  const [version, setVersion] = useState("1.0.0");
  const [overwrite, setOverwrite] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const title = useMemo(() => publishTitle(assetKind, mode.kind === "update"), [assetKind, mode]);

  // 冲突探测：打开时按 (asset_type, owner_id) 拉一次同 owner user_authored 资产，
  // 找到同 key 项 → 切 update 形态。失败时静默 fallback create，由后端 409 兜底。
  useEffect(() => {
    let cancelled = false;
    const probe = async () => {
      try {
        const list = await fetchLibraryAssets({
          asset_type: kindToAssetType(assetKind),
          owner_id: currentUserId ?? undefined,
          include_deprecated: true,
        });
        if (cancelled) return;
        const existing = list.find(
          (a) =>
            a.key === defaults.key &&
            a.source === "user_authored" &&
            (currentUserId ? a.owner_id === currentUserId : true),
        );
        if (existing) {
          setMode({ kind: "update", existing });
          setKey(existing.key);
          setDisplayName(existing.display_name);
          setDescription(existing.description ?? "");
          setVersion(suggestNextVersion(existing.version));
          setOverwrite(true);
        } else {
          setMode({ kind: "create" });
        }
      } catch (err) {
        // probe 失败不阻断：保持 create 形态，让用户继续走，后端 409 会兜底
        console.warn("[PublishLibraryAssetDialog] conflict probe failed", err);
        if (!cancelled) setMode({ kind: "create" });
      }
    };
    void probe();
    return () => {
      cancelled = true;
    };
  }, [assetKind, currentUserId, defaults.key]);

  const handleSubmit = async () => {
    const trimmedKey = key.trim();
    const trimmedDisplayName = displayName.trim();
    const trimmedVersion = version.trim();
    if (!trimmedKey || !trimmedDisplayName || !trimmedVersion) {
      setError("key、显示名称和版本不能为空");
      return;
    }
    setIsSaving(true);
    setError(null);
    try {
      const asset = await publishLibraryAsset(projectId, {
        asset_kind: assetKind,
        project_asset_id: projectAssetId,
        scope: "user",
        key: trimmedKey,
        display_name: trimmedDisplayName,
        description: description.trim() || undefined,
        version: trimmedVersion,
        overwrite,
      });
      onPublished(`已发布到资源市场：${asset.display_name} v${asset.version}`);
      onClose();
    } catch (err) {
      const status = (err as ApiHttpError).status;
      const message = err instanceof Error ? err.message : "发布失败";
      if (status === 409) {
        setMode((prev) => prev.kind === "update" ? prev : { kind: "create" });
        setOverwrite(true);
        setError(`${message}。已切换为覆盖模式，再次点击发布即可。`);
      } else {
        setError(message);
      }
    } finally {
      setIsSaving(false);
    }
  };

  const isUpdate = mode.kind === "update";

  return (
    <>
      <div className="fixed inset-0 z-[92] bg-foreground/18 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[93] flex items-center justify-center p-4">
        <div className="w-full max-w-lg rounded-[12px] border border-border bg-background shadow-2xl">
          <header className="border-b border-border px-5 py-4">
            <span className="agentdash-panel-header-tag">Shared Library</span>
            <h3 className="mt-1 text-base font-semibold text-foreground">{title}</h3>
            {isUpdate && (
              <p className="mt-1 text-xs text-muted-foreground">
                已存在 v{mode.existing.version}，发布将覆盖现有版本并通知所有安装方源已更新。
              </p>
            )}
          </header>

          <div className="space-y-3 p-5">
            {mode.kind === "loading" ? (
              <p className="rounded-[8px] border border-dashed border-border bg-secondary/20 px-3 py-3 text-xs text-muted-foreground">
                正在检查是否已存在同 key 的发布…
              </p>
            ) : (
              <>
                <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
                  <label className="block space-y-1.5">
                    <span className="agentdash-form-label">key</span>
                    <input
                      value={key}
                      onChange={(e) => {
                        setKey(e.target.value);
                        setError(null);
                      }}
                      disabled={isUpdate}
                      className="agentdash-form-input"
                      title={isUpdate ? "更新发布时 key 锁定，避免误改 identity" : undefined}
                    />
                  </label>
                  <label className="block space-y-1.5">
                    <span className="agentdash-form-label">version</span>
                    <input
                      value={version}
                      onChange={(e) => {
                        setVersion(e.target.value);
                        setError(null);
                      }}
                      className="agentdash-form-input"
                    />
                  </label>
                </div>
                <label className="block space-y-1.5">
                  <span className="agentdash-form-label">显示名称</span>
                  <input
                    value={displayName}
                    onChange={(e) => {
                      setDisplayName(e.target.value);
                      setError(null);
                    }}
                    className="agentdash-form-input"
                  />
                </label>
                <label className="block space-y-1.5">
                  <span className="agentdash-form-label">描述</span>
                  <textarea
                    value={description}
                    onChange={(e) => setDescription(e.target.value)}
                    rows={3}
                    className="agentdash-form-textarea"
                  />
                </label>
                {isUpdate && (
                  <label className="flex items-center gap-2 rounded-[8px] border border-border bg-secondary/20 px-3 py-2">
                    <input
                      type="checkbox"
                      checked={overwrite}
                      onChange={(e) => setOverwrite(e.target.checked)}
                    />
                    <span className="text-xs text-foreground">覆盖现有 v{mode.existing.version}</span>
                  </label>
                )}
              </>
            )}
            {error && (
              <p className="rounded-[8px] border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
                {error}
              </p>
            )}
          </div>

          <footer className="flex justify-end gap-2 border-t border-border px-5 py-4">
            <button type="button" onClick={onClose} disabled={isSaving} className="agentdash-button-secondary">
              取消
            </button>
            <button
              type="button"
              onClick={() => void handleSubmit()}
              disabled={isSaving || mode.kind === "loading"}
              className="agentdash-button-primary"
            >
              {isSaving ? "发布中…" : isUpdate ? "更新发布" : "发布"}
            </button>
          </footer>
        </div>
      </div>
    </>
  );
}

function publishTitle(kind: PublishLibraryAssetKind, isUpdate: boolean): string {
  const prefix = isUpdate ? "更新发布" : "发布";
  switch (kind) {
    case "project_agent":
      return `${prefix} Agent 模板`;
    case "mcp_preset":
      return `${prefix} MCP Server 模板`;
    case "workflow_bundle":
      return `${prefix} Workflow 模板`;
    case "skill_asset":
      return `${prefix} Skill 模板`;
    case "vfs_mount":
      return `${prefix} VFS Mount 模板`;
    case "extension_installation":
      return `${prefix} Extension 模板`;
  }
}

function kindToAssetType(kind: PublishLibraryAssetKind): LibraryAssetType {
  switch (kind) {
    case "project_agent":
      return "agent_template";
    case "mcp_preset":
      return "mcp_server_template";
    case "workflow_bundle":
      return "workflow_template";
    case "skill_asset":
      return "skill_template";
    case "vfs_mount":
      return "vfs_mount_template";
    case "extension_installation":
      return "extension_template";
  }
}
