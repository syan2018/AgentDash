import { useEffect, useMemo, useState } from "react";
import type { CanvasFile } from "../../types";

export interface CanvasFilesEditorSaveInput {
  entryFile: string;
  files: CanvasFile[];
}

export interface CanvasFilesEditorProps {
  value: CanvasFile[];
  entryFile: string;
  isSaving?: boolean;
  error?: string | null;
  onSave: (input: CanvasFilesEditorSaveInput) => Promise<void> | void;
  onCancel?: (input: CanvasFilesEditorSaveInput) => void;
}

interface FileValidationResult {
  generalError: string | null;
  rowErrors: string[];
}

const DEFAULT_NEW_FILE_CONTENT = [
  "export function CanvasApp() {",
  "  return <div>New Canvas File</div>;",
  "}",
  "",
].join("\n");

export function CanvasFilesEditor({
  value,
  entryFile,
  isSaving = false,
  error = null,
  onSave,
  onCancel,
}: CanvasFilesEditorProps) {
  const [draftFiles, setDraftFiles] = useState<CanvasFile[]>(value);
  const [draftEntryFile, setDraftEntryFile] = useState(entryFile);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(entryFile || value[0]?.path || null);
  const [isDirty, setIsDirty] = useState(false);

  useEffect(() => {
    setDraftFiles(value);
    setDraftEntryFile(entryFile);
    setSelectedFilePath(entryFile || value[0]?.path || null);
    setIsDirty(false);
  }, [entryFile, value]);

  useEffect(() => {
    if (selectedFilePath && draftFiles.some((file) => file.path === selectedFilePath)) {
      return;
    }
    setSelectedFilePath(draftEntryFile || draftFiles[0]?.path || null);
  }, [draftEntryFile, draftFiles, selectedFilePath]);

  const selectedFile = useMemo(() => {
    if (!selectedFilePath) {
      return null;
    }
    return draftFiles.find((file) => file.path === selectedFilePath) ?? null;
  }, [draftFiles, selectedFilePath]);

  const validation = useMemo(
    () => validateFiles(draftFiles, draftEntryFile),
    [draftEntryFile, draftFiles],
  );

  const canSave = !isSaving && isDirty && !validation.generalError && validation.rowErrors.every((item) => item.length === 0);

  const handleFilePathChange = (currentPath: string, nextPath: string) => {
    setDraftFiles((prev) =>
      prev.map((file) => (file.path === currentPath ? { ...file, path: nextPath } : file)),
    );
    if (selectedFilePath === currentPath) {
      setSelectedFilePath(nextPath);
    }
    if (draftEntryFile === currentPath) {
      setDraftEntryFile(nextPath);
    }
    setIsDirty(true);
  };

  const handleFileContentChange = (currentPath: string, nextContent: string) => {
    setDraftFiles((prev) =>
      prev.map((file) => (file.path === currentPath ? { ...file, content: nextContent } : file)),
    );
    setIsDirty(true);
  };

  const handleAddFile = () => {
    const newFile = createEmptyFile(draftFiles);
    setDraftFiles((prev) => [...prev, newFile]);
    setSelectedFilePath(newFile.path);
    if (!draftEntryFile) {
      setDraftEntryFile(newFile.path);
    }
    setIsDirty(true);
  };

  const handleRemoveFile = (targetPath: string) => {
    setDraftFiles((prev) => {
      const nextFiles = prev.filter((file) => file.path !== targetPath);
      const fallbackPath = nextFiles[0]?.path ?? "";
      if (selectedFilePath === targetPath) {
        setSelectedFilePath(fallbackPath || null);
      }
      if (draftEntryFile === targetPath) {
        setDraftEntryFile(fallbackPath);
      }
      return nextFiles;
    });
    setIsDirty(true);
  };

  const handleCancel = () => {
    const nextSelection = entryFile || value[0]?.path || null;
    setDraftFiles(value);
    setDraftEntryFile(entryFile);
    setSelectedFilePath(nextSelection);
    setIsDirty(false);
    onCancel?.({
      entryFile,
      files: value,
    });
  };

  const handleSave = async () => {
    if (!canSave) {
      return;
    }
    const normalizedFiles = draftFiles.map(normalizeCanvasFile);
    const normalizedEntryFile = normalizeCanvasPath(draftEntryFile);
    try {
      await onSave({
        entryFile: normalizedEntryFile,
        files: normalizedFiles,
      });
      setIsDirty(false);
    } catch {
      // 保存失败时保留 dirty 状态，方便用户继续修改后重试。
    }
  };

  return (
    <section className="space-y-3 rounded-[10px] border border-border bg-secondary/20 p-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">资产文件编辑</p>
          <p className="mt-1 text-xs text-muted-foreground">
            这里编辑的是 Canvas 资产源文件；保存后会刷新右侧运行时预览。
          </p>
        </div>
        <button
          type="button"
          onClick={handleAddFile}
          disabled={isSaving}
          className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
        >
          新增文件
        </button>
      </div>

      <div className="flex flex-wrap gap-2">
        {draftFiles.map((file) => {
          const isSelected = file.path === selectedFilePath;
          const isEntry = file.path === draftEntryFile;
          return (
            <button
              key={file.path}
              type="button"
              onClick={() => setSelectedFilePath(file.path)}
              className={[
                "rounded-[8px] border px-2.5 py-1 text-xs transition-colors",
                isSelected
                  ? "border-foreground bg-foreground text-background"
                  : "border-border bg-background text-muted-foreground hover:bg-secondary hover:text-foreground",
              ].join(" ")}
            >
              {file.path}
              {isEntry ? " · entry" : ""}
            </button>
          );
        })}
      </div>

      {draftFiles.length === 0 && (
        <div className="rounded-[8px] border border-dashed border-border bg-background px-3 py-3 text-xs text-muted-foreground">
          当前没有文件，点击“新增文件”开始编辑 Canvas 资产。
        </div>
      )}

      {selectedFile && (
        <div className="space-y-3 rounded-[10px] border border-border bg-background p-3">
          <div className="flex flex-wrap items-center gap-2">
            <label className="min-w-0 flex-1 space-y-1">
              <span className="text-[11px] text-muted-foreground">文件路径</span>
              <input
                value={selectedFile.path}
                onChange={(event) => handleFilePathChange(selectedFile.path, event.target.value)}
                disabled={isSaving}
                className="w-full rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-foreground outline-none transition-colors focus:border-foreground/40"
                placeholder="例如：src/main.tsx"
              />
            </label>
            <div className="flex items-end gap-2">
              <button
                type="button"
                onClick={() => {
                  setDraftEntryFile(selectedFile.path);
                  setIsDirty(true);
                }}
                disabled={isSaving}
                className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
              >
                设为入口
              </button>
              <button
                type="button"
                onClick={() => handleRemoveFile(selectedFile.path)}
                disabled={isSaving}
                className="rounded-[8px] border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs text-destructive transition-colors hover:bg-destructive/20 disabled:cursor-not-allowed disabled:opacity-50"
              >
                删除文件
              </button>
            </div>
          </div>

          <div className="rounded-[8px] border border-border bg-secondary/15 px-3 py-2 text-[11px] text-muted-foreground">
            当前入口文件：{draftEntryFile || "未设置"}
          </div>

          <label className="block space-y-1">
            <span className="text-[11px] text-muted-foreground">文件内容</span>
            <textarea
              value={selectedFile.content}
              onChange={(event) => handleFileContentChange(selectedFile.path, event.target.value)}
              disabled={isSaving}
              spellCheck={false}
              className="min-h-[320px] w-full rounded-[8px] border border-border bg-slate-950 px-3 py-2 font-mono text-[12px] leading-6 text-slate-100 outline-none transition-colors focus:border-slate-400"
            />
          </label>

          {validation.rowErrors[draftFiles.findIndex((file) => file.path === selectedFile.path)] && (
            <div className="rounded-[8px] border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs text-destructive">
              {validation.rowErrors[draftFiles.findIndex((file) => file.path === selectedFile.path)]}
            </div>
          )}
        </div>
      )}

      {(validation.generalError || error) && (
        <div className="rounded-[8px] border border-destructive/40 bg-destructive/10 px-2 py-1 text-xs text-destructive">
          {validation.generalError ?? error}
        </div>
      )}

      <div className="flex items-center justify-end gap-2 pt-1">
        <button
          type="button"
          onClick={handleCancel}
          disabled={isSaving || !isDirty}
          className="rounded-[8px] border border-border bg-background px-3 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:cursor-not-allowed disabled:opacity-50"
        >
          取消
        </button>
        <button
          type="button"
          onClick={() => void handleSave()}
          disabled={!canSave}
          className="rounded-[8px] border border-border bg-foreground px-3 py-1 text-xs text-background transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
        >
          {isSaving ? "保存中..." : "保存文件"}
        </button>
      </div>
    </section>
  );
}

function validateFiles(files: CanvasFile[], entryFile: string): FileValidationResult {
  if (files.length === 0) {
    return {
      generalError: "至少需要保留一个文件",
      rowErrors: [],
    };
  }

  const seen = new Set<string>();
  const rowErrors = files.map((file) => {
    const path = normalizeCanvasPath(file.path);
    if (!path) {
      return "文件路径不能为空";
    }
    if (seen.has(path)) {
      return "文件路径不能重复";
    }
    seen.add(path);
    return "";
  });

  const normalizedEntryFile = normalizeCanvasPath(entryFile);
  if (!normalizedEntryFile) {
    return {
      generalError: "入口文件不能为空",
      rowErrors,
    };
  }

  if (!files.some((file) => normalizeCanvasPath(file.path) === normalizedEntryFile)) {
    return {
      generalError: "入口文件必须指向现有文件",
      rowErrors,
    };
  }

  return {
    generalError: null,
    rowErrors,
  };
}

function normalizeCanvasFile(file: CanvasFile): CanvasFile {
  return {
    path: normalizeCanvasPath(file.path),
    content: file.content,
  };
}

function normalizeCanvasPath(path: string): string {
  return path.trim().replace(/\\/g, "/");
}

function createEmptyFile(existingFiles: CanvasFile[]): CanvasFile {
  const existingPaths = new Set(existingFiles.map((file) => normalizeCanvasPath(file.path)));
  let index = 1;
  while (true) {
    const path = index === 1 ? "src/new-file.tsx" : `src/new-file-${index}.tsx`;
    if (!existingPaths.has(path)) {
      return {
        path,
        content: DEFAULT_NEW_FILE_CONTENT,
      };
    }
    index += 1;
  }
}

export default CanvasFilesEditor;
