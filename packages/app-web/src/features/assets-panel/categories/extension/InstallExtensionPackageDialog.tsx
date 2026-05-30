import { useEffect, useId, useState } from "react";

import { Button, CheckboxField, TextInput } from "@agentdash/ui";

import { importExtensionPackage } from "../../../../services/extensionPackage";
import { sha256OfBlob } from "../../../../utils/sha256";
import { formatBytes } from "../../../../lib/format";

const MAX_BYTES = 50 * 1024 * 1024;
const ACCEPT = ".tgz,.gz,application/gzip,application/x-gzip";

interface Props {
  projectId: string;
  open: boolean;
  onClose: () => void;
  onInstalled: (extensionKey: string) => void;
}

type Phase = "idle" | "hashing" | "installing";

export function InstallExtensionPackageDialog({
  projectId,
  open,
  onClose,
  onInstalled,
}: Props) {
  const titleId = useId();
  const fileInputId = useId();
  const keyInputId = useId();
  const nameInputId = useId();
  const [file, setFile] = useState<File | null>(null);
  const [extensionKey, setExtensionKey] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [overwrite, setOverwrite] = useState(true);
  const [phase, setPhase] = useState<Phase>("idle");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && phase === "idle") onClose();
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [open, phase, onClose]);

  if (!open) return null;

  const busy = phase !== "idle";

  function pickFile(picked: File | null) {
    setError(null);
    if (!picked) {
      setFile(null);
      return;
    }
    const lower = picked.name.toLowerCase();
    if (!(lower.endsWith(".tgz") || lower.endsWith(".tar.gz"))) {
      setError("仅支持 .tgz / .agentdash-extension.tgz 包");
      setFile(null);
      return;
    }
    if (picked.size > MAX_BYTES) {
      setError(`文件超过 ${formatBytes(MAX_BYTES)} 上限`);
      setFile(null);
      return;
    }
    setFile(picked);
  }

  async function submit() {
    if (!file) return;
    setError(null);
    try {
      setPhase("hashing");
      const digest = await sha256OfBlob(file);
      setPhase("installing");
      const result = await importExtensionPackage(projectId, file, digest, {
        extension_key: extensionKey,
        display_name: displayName,
        overwrite,
      });
      onInstalled(result.installation.extension_key);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "安装扩展包失败");
      setPhase("idle");
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
              从本地包安装
            </h4>
          </header>
          <div className="space-y-3 p-5">
            <TextInput
              id={fileInputId}
              type="file"
              accept={ACCEPT}
              disabled={busy}
              onChange={(event) => {
                const picked = event.target.files && event.target.files[0];
                pickFile(picked ?? null);
              }}
            />
            {file && (
              <div className="rounded-[8px] border border-border bg-secondary/30 px-3 py-2 text-xs text-muted-foreground">
                <p className="truncate font-mono text-foreground">{file.name}</p>
                <p className="mt-0.5">{formatBytes(file.size)}</p>
              </div>
            )}
            <TextInput
              id={keyInputId}
              value={extensionKey}
              placeholder="extension_key"
              disabled={busy}
              onChange={(event) => setExtensionKey(event.target.value)}
            />
            <TextInput
              id={nameInputId}
              value={displayName}
              placeholder="显示名称"
              disabled={busy}
              onChange={(event) => setDisplayName(event.target.value)}
            />
            <CheckboxField
              label="覆盖同名 Extension"
              checked={overwrite}
              disabled={busy}
              onChange={(event) => setOverwrite(event.target.checked)}
            />
            {phase === "hashing" && (
              <p className="text-xs text-muted-foreground">计算 digest...</p>
            )}
            {phase === "installing" && (
              <p className="text-xs text-muted-foreground">安装中...</p>
            )}
            {error && <p className="text-xs text-destructive">{error}</p>}
          </div>
          <footer className="flex items-center justify-end gap-2 border-t border-border px-5 py-4">
            <Button variant="secondary" onClick={onClose} disabled={busy}>
              取消
            </Button>
            <Button
              variant="primary"
              onClick={() => void submit()}
              disabled={!file || busy}
            >
              {phase === "hashing"
                ? "计算 digest..."
                : phase === "installing"
                  ? "安装中..."
                  : "安装"}
            </Button>
          </footer>
        </section>
      </div>
    </>
  );
}

export default InstallExtensionPackageDialog;
