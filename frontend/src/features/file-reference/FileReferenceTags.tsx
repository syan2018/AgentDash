import type { FileReference } from "./useFileReference";
import {
  FILE_PILL_BADGE_CLASS,
  FILE_PILL_CLASS,
  FILE_PILL_LABEL_CLASS,
  FILE_PILL_REMOVE_CLASS,
  getDisplayFileName,
  getFileKindLabel,
} from "./fileReferenceUi";

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
    <div className="flex flex-wrap gap-2 px-3 pt-2 pb-1">
      {references.map((ref) => {
        const fileName = getDisplayFileName(ref.relPath);

        return (
          <span
            key={ref.relPath}
            className={`${FILE_PILL_CLASS} group`}
            title={`${ref.relPath} (${formatSize(ref.size)})`}
          >
            <span className={FILE_PILL_BADGE_CLASS}>{getFileKindLabel(ref.relPath)}</span>
            <span className={FILE_PILL_LABEL_CLASS}>
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
              className={`${FILE_PILL_REMOVE_CLASS} ml-0.5 focus:outline-none focus:ring-1 focus:ring-ring`}
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
