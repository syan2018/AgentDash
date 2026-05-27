/**
 * UploadExtensionDialog — 选择本地 .agentdash-extension.tgz / .tgz，
 * 计算 sha256，再以 multipart 上传到当前 Project 的 extension-artifacts。
 *
 * 流程：
 * 1. 文件选择 + 客户端校验（扩展名、字节数）
 * 2. 计算 digest（loading）
 * 3. 上传（loading）
 * 4. 成功后 onUploaded 把 artifact 交给父组件
 */

import { useEffect, useId, useState } from "react";

import { Button, TextInput } from "@agentdash/ui";

import { sha256OfBlob } from "../../../../utils/sha256";
import { uploadExtensionArtifact } from "../../../../services/extensionPackage";
import type { ExtensionPackageArtifactResponse } from "../../../../generated/extension-package-contracts";

const MAX_BYTES = 50 * 1024 * 1024;
const ACCEPT = ".tgz,.gz,application/gzip,application/x-gzip";

interface Props {
  projectId: string;
  open: boolean;
  onClose: () => void;
  onUploaded: (artifact: ExtensionPackageArtifactResponse) => void;
}

type Phase = "idle" | "hashing" | "uploading";

export function UploadExtensionDialog({ projectId, open, onClose, onUploaded }: Props) {
  const titleId = useId();
  const inputId = useId();
  const [file, setFile] = useState<File | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [phase, setPhase] = useState<Phase>("idle");

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
      setError("仅支持 .tgz / .agentdash-extension.tgz 归档");
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
      setPhase("uploading");
      const artifact = await uploadExtensionArtifact(projectId, file, digest);
      onUploaded(artifact);
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : "上传归档失败");
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
              上传扩展归档
            </h4>
          </header>
          <div className="space-y-3 p-5">
            <TextInput
              id={inputId}
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
            {phase === "hashing" && (
              <p className="text-xs text-muted-foreground">计算 digest…</p>
            )}
            {phase === "uploading" && (
              <p className="text-xs text-muted-foreground">上传中…</p>
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
                ? "计算 digest…"
                : phase === "uploading"
                  ? "上传中…"
                  : "上传"}
            </Button>
          </footer>
        </section>
      </div>
    </>
  );
}

function formatBytes(value: number): string {
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

export default UploadExtensionDialog;
