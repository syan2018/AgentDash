/**
 * InstallFromArtifactDialog — 把已上传的 archive 安装为 Project extension installation。
 *
 * 表单字段：
 * - extension_key（可空，留空使用 manifest 默认值）
 * - display_name（可空）
 * - overwrite（默认 false）
 */

import { useEffect, useId, useState } from "react";

import { Button, CheckboxField, TextInput } from "@agentdash/ui";

import { installExtensionArtifact } from "../../../../services/extensionPackage";
import type {
  ExtensionPackageArtifactResponse,
  ExtensionPackageInstallationResponse,
} from "../../../../generated/extension-package-contracts";

interface Props {
  projectId: string;
  artifact: ExtensionPackageArtifactResponse;
  open: boolean;
  onClose: () => void;
  onInstalled: (installation: ExtensionPackageInstallationResponse) => void;
}

export function InstallFromArtifactDialog({
  projectId,
  artifact,
  open,
  onClose,
  onInstalled,
}: Props) {
  const titleId = useId();
  const keyId = useId();
  const nameId = useId();
  const [extensionKey, setExtensionKey] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [overwrite, setOverwrite] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !busy) onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, busy, onClose]);

  if (!open) return null;

  async function submit() {
    setBusy(true);
    setError(null);
    try {
      const result = await installExtensionArtifact(projectId, artifact.id, {
        extension_key: extensionKey.trim() === "" ? null : extensionKey.trim(),
        display_name: displayName.trim() === "" ? null : displayName.trim(),
        overwrite,
      });
      onInstalled(result);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "安装失败");
      setBusy(false);
    }
  }

  return (
    <>
      <div
        className="fixed inset-0 z-[90] bg-foreground/24 backdrop-blur-[2px]"
        onClick={busy ? undefined : onClose}
      />
      <div
        className="fixed inset-0 z-[91] flex items-center justify-center p-4"
        onClick={busy ? undefined : onClose}
      >
        <section
          role="dialog"
          aria-modal="true"
          aria-labelledby={titleId}
          className="w-full max-w-lg rounded-[12px] border border-border bg-background shadow-2xl"
          onClick={(event) => event.stopPropagation()}
        >
          <header className="border-b border-border px-5 py-4">
            <h4 id={titleId} className="text-base font-semibold text-foreground">
              从归档安装扩展
            </h4>
            <p className="mt-1 truncate font-mono text-[11px] text-muted-foreground">
              {artifact.extension_id} · v{artifact.package_version}
            </p>
          </header>
          <div className="space-y-3 p-5">
            <label className="block space-y-1.5" htmlFor={keyId}>
              <span className="agentdash-form-label">extension_key</span>
              <TextInput
                id={keyId}
                value={extensionKey}
                onChange={(event) => setExtensionKey(event.target.value)}
                placeholder={artifact.extension_id}
                disabled={busy}
              />
            </label>
            <label className="block space-y-1.5" htmlFor={nameId}>
              <span className="agentdash-form-label">display_name</span>
              <TextInput
                id={nameId}
                value={displayName}
                onChange={(event) => setDisplayName(event.target.value)}
                placeholder={artifact.extension_id}
                disabled={busy}
              />
            </label>
            <CheckboxField
              label="覆盖已存在的同 key 安装"
              checked={overwrite}
              disabled={busy}
              onChange={(event) => setOverwrite(event.target.checked)}
            />
            {error && <p className="text-xs text-destructive">{error}</p>}
          </div>
          <footer className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
            <Button variant="secondary" onClick={onClose} disabled={busy}>
              取消
            </Button>
            <Button variant="primary" onClick={() => void submit()} disabled={busy}>
              {busy ? "安装中…" : "安装"}
            </Button>
          </footer>
        </section>
      </div>
    </>
  );
}

export default InstallFromArtifactDialog;
