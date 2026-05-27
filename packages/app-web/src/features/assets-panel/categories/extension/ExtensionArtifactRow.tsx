/**
 * ExtensionArtifactRow — Project 已上传的扩展归档条目。
 */

import { Button } from "@agentdash/ui";

import type { ExtensionPackageArtifactResponse } from "../../../../generated/extension-package-contracts";

interface Props {
  artifact: ExtensionPackageArtifactResponse;
  busy: boolean;
  onInstall: () => void;
  onDownload: () => void;
}

export function ExtensionArtifactRow({ artifact, busy, onInstall, onDownload }: Props) {
  const sizeLabel = formatBytes(Number(artifact.byte_size));
  return (
    <article className="flex items-center justify-between gap-3 rounded-[8px] border border-border bg-background p-4 transition-colors hover:border-primary/25">
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <h3 className="truncate text-sm font-semibold text-foreground">
            {artifact.extension_id}
          </h3>
          <span className="truncate font-mono text-[11px] text-muted-foreground">
            {artifact.package_name}@{artifact.package_version}
          </span>
        </div>
        <p
          className="mt-1 truncate font-mono text-[11px] text-muted-foreground"
          title={artifact.archive_digest}
        >
          {truncateDigest(artifact.archive_digest)} · {sizeLabel} · {formatDateTime(artifact.created_at)}
        </p>
      </div>
      <div className="flex shrink-0 items-center gap-1.5">
        <Button variant="primary" size="sm" onClick={onInstall} disabled={busy}>
          从归档安装
        </Button>
        <Button variant="secondary" size="sm" onClick={onDownload} disabled={busy}>
          下载
        </Button>
      </div>
    </article>
  );
}

function truncateDigest(digest: string): string {
  if (digest.length <= 24) return digest;
  return `${digest.slice(0, 16)}…${digest.slice(-6)}`;
}

function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  if (value < 1024) return `${value} B`;
  if (value < 1024 * 1024) return `${(value / 1024).toFixed(1)} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

function formatDateTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString();
}

export default ExtensionArtifactRow;
