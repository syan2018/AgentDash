import type { FileReference } from "./useFileReference";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

interface FileReferenceTagsProps {
  references: FileReference[];
  onRemove: (relPath: string) => void;
}

export function FileReferenceTags({ references, onRemove }: FileReferenceTagsProps) {
  if (references.length === 0) return null;

  return (
    <div className="flex flex-wrap gap-1.5 px-1">
      {references.map((ref) => {
        const fileName = ref.relPath.split("/").pop() ?? ref.relPath;
        return (
          <span
            key={ref.relPath}
            className="group inline-flex items-center gap-1 rounded-md border border-border bg-muted/50 px-2 py-0.5 text-xs"
            title={`${ref.relPath} (${formatSize(ref.size)})`}
          >
            <span className="font-mono text-foreground">{fileName}</span>
            <button
              type="button"
              onClick={() => onRemove(ref.relPath)}
              className="ml-0.5 inline-flex h-3.5 w-3.5 items-center justify-center rounded-sm text-muted-foreground opacity-60 transition-opacity hover:bg-destructive/20 hover:text-destructive hover:opacity-100"
              title="移除引用"
            >
              ×
            </button>
          </span>
        );
      })}
    </div>
  );
}
