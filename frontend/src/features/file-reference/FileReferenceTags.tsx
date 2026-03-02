import type { FileReference } from "./useFileReference";

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function getFileIcon(relPath: string): string {
  const ext = relPath.split(".").pop()?.toLowerCase();

  switch (ext) {
    case "ts":
    case "tsx":
    case "js":
    case "jsx":
      return "📜";
    case "json":
    case "jsonc":
      return "📋";
    case "md":
    case "mdx":
      return "📝";
    case "css":
    case "scss":
    case "less":
      return "🎨";
    case "html":
    case "htm":
      return "🌐";
    case "yml":
    case "yaml":
    case "toml":
      return "⚙️";
    case "rs":
      return "🦀";
    case "py":
      return "🐍";
    case "go":
      return "🐹";
    case "java":
      return "☕";
    case "sql":
      return "🗄️";
    case "sh":
    case "bash":
    case "zsh":
      return "🐚";
    case "dockerfile":
      return "🐳";
    default:
      return "📄";
  }
}

interface FileReferenceTagsProps {
  references: FileReference[];
  onRemove: (relPath: string) => void;
}

export function FileReferenceTags({ references, onRemove }: FileReferenceTagsProps) {
  if (references.length === 0) return null;

  return (
    <div className="flex flex-wrap gap-2 px-3 pt-2 pb-1">
      {references.map((ref) => {
        const fileName = ref.relPath.split("/").pop() ?? ref.relPath;
        const icon = getFileIcon(ref.relPath);

        return (
          <span
            key={ref.relPath}
            className="group inline-flex items-center gap-1.5 rounded-[6px] border border-[#5865F2]/20 bg-[#5865F2]/10 px-2 py-0.5 text-xs font-medium text-[#5865F2] shadow-sm transition-colors hover:bg-[#5865F2]/15 dark:border-[#5865F2]/30 dark:bg-[#5865F2]/20 dark:text-[#00A8FC]"
            title={`${ref.relPath} (${formatSize(ref.size)})`}
          >
            <span className="text-sm">{icon}</span>
            <span className="max-w-[150px] truncate underline decoration-[#5865F2]/40 underline-offset-2 group-hover:decoration-[#5865F2]/70 dark:decoration-[#00A8FC]/40 group-hover:dark:decoration-[#00A8FC]/70">
              {fileName}
            </span>

            <button
              type="button"
              aria-label={`移除引用 ${ref.relPath}`}
              title="移除"
              onClick={(e) => {
                e.preventDefault();
                e.stopPropagation();
                onRemove(ref.relPath);
              }}
              className="ml-0.5 inline-flex h-5 w-5 items-center justify-center rounded-[4px] text-xs text-[#5865F2]/70 transition-colors hover:bg-[#5865F2]/15 hover:text-[#5865F2] focus:outline-none focus:ring-1 focus:ring-ring dark:text-[#00A8FC]/70 dark:hover:bg-[#5865F2]/25 dark:hover:text-[#00A8FC]"
            >
              ×
            </button>
          </span>
        );
      })}
    </div>
  );
}

export default FileReferenceTags;
