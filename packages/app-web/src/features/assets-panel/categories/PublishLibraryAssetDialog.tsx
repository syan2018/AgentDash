import { useMemo, useState } from "react";
import type { ApiHttpError } from "../../../api/client";
import { publishLibraryAsset } from "../../../services/sharedLibrary";
import type { PublishLibraryAssetKind } from "../../../types";

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
  onClose: () => void;
  onPublished: (message: string) => void;
}

export function PublishLibraryAssetDialog({
  projectId,
  assetKind,
  projectAssetId,
  defaults,
  onClose,
  onPublished,
}: PublishLibraryAssetDialogProps) {
  const [key, setKey] = useState(defaults.key);
  const [displayName, setDisplayName] = useState(defaults.display_name);
  const [description, setDescription] = useState(defaults.description ?? "");
  const [version, setVersion] = useState("1.0.0");
  const [overwrite, setOverwrite] = useState(false);
  const [isSaving, setIsSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [conflicted, setConflicted] = useState(false);

  const title = useMemo(() => publishTitle(assetKind), [assetKind]);

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
        description: description.trim() || null,
        version: trimmedVersion,
        overwrite,
      });
      onPublished(`已发布到资源市场：${asset.display_name} v${asset.version}`);
      onClose();
    } catch (err) {
      const status = (err as ApiHttpError).status;
      const message = err instanceof Error ? err.message : "发布失败";
      if (status === 409) {
        setConflicted(true);
        setError(`${message}。勾选覆盖后可发布新版本。`);
      } else {
        setError(message);
      }
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <>
      <div className="fixed inset-0 z-[92] bg-foreground/18 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[93] flex items-center justify-center p-4">
        <div className="w-full max-w-lg rounded-[12px] border border-border bg-background shadow-2xl">
          <header className="border-b border-border px-5 py-4">
            <span className="agentdash-panel-header-tag">Shared Library</span>
            <h3 className="mt-1 text-base font-semibold text-foreground">{title}</h3>
          </header>

          <div className="space-y-3 p-5">
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
              <label className="block space-y-1.5">
                <span className="agentdash-form-label">key</span>
                <input
                  value={key}
                  onChange={(e) => {
                    setKey(e.target.value);
                    setError(null);
                  }}
                  className="agentdash-form-input"
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
            {(conflicted || overwrite) && (
              <label className="flex items-center gap-2 rounded-[8px] border border-border bg-secondary/20 px-3 py-2">
                <input
                  type="checkbox"
                  checked={overwrite}
                  onChange={(e) => setOverwrite(e.target.checked)}
                />
                <span className="text-xs text-foreground">覆盖同 key 的个人市场资产</span>
              </label>
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
            <button type="button" onClick={() => void handleSubmit()} disabled={isSaving} className="agentdash-button-primary">
              {isSaving ? "发布中…" : "发布"}
            </button>
          </footer>
        </div>
      </div>
    </>
  );
}

function publishTitle(kind: PublishLibraryAssetKind): string {
  switch (kind) {
    case "project_agent":
      return "发布 Agent 模板";
    case "mcp_preset":
      return "发布 MCP Server 模板";
    case "workflow_bundle":
      return "发布 Workflow 模板";
    case "skill_asset":
      return "发布 Skill 模板";
  }
}
