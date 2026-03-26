import { useCallback, useEffect, useState } from "react";
import {
  type BrowseDirectoryEntry,
  browseDirectory,
} from "../../services/browseDirectory";

interface DirectoryBrowserDialogProps {
  open: boolean;
  backendId: string;
  initialPath?: string;
  onSelect: (path: string) => void;
  onClose: () => void;
}

interface BreadcrumbSegment {
  label: string;
  path: string | null;
}

function normalizeWindowsPath(path: string): string {
  if (!path) return "";
  if (path.startsWith("\\\\?\\UNC\\")) {
    return `\\\\${path.slice("\\\\?\\UNC\\".length)}`;
  }
  if (path.startsWith("\\\\?\\")) {
    return path.slice("\\\\?\\".length);
  }
  return path;
}

function parseBreadcrumbs(currentPath: string): BreadcrumbSegment[] {
  const safePath = normalizeWindowsPath(currentPath);
  if (!safePath) {
    return [{ label: "此电脑", path: null }];
  }

  const segments: BreadcrumbSegment[] = [{ label: "此电脑", path: null }];
  const normalized = safePath.replace(/\\/g, "/");
  const parts = normalized.split("/").filter(Boolean);

  let accumulated = "";
  for (const part of parts) {
    accumulated = accumulated ? `${accumulated}/${part}` : part;
    const fullPath = accumulated.includes(":") && !accumulated.endsWith("/")
      ? `${accumulated}/`
      : accumulated;
    segments.push({ label: part, path: normalizeWindowsPath(fullPath) });
  }
  return segments;
}

export function DirectoryBrowserDialog({
  open,
  backendId,
  initialPath,
  onSelect,
  onClose,
}: DirectoryBrowserDialogProps) {
  const [currentPath, setCurrentPath] = useState<string>("");
  const [entries, setEntries] = useState<BrowseDirectoryEntry[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);

  const loadDirectory = useCallback(
    async (path?: string) => {
      if (!backendId) return;
      setIsLoading(true);
      setError(null);
      setSelectedPath(null);
      try {
        const result = await browseDirectory(backendId, path);
        setCurrentPath(normalizeWindowsPath(result.current_path));
        setEntries(result.entries.map((entry) => ({
          ...entry,
          path: normalizeWindowsPath(entry.path),
        })));
      } catch (loadError) {
        setError((loadError as Error).message);
        setEntries([]);
      } finally {
        setIsLoading(false);
      }
    },
    [backendId],
  );

  useEffect(() => {
    if (!open) return;
    void loadDirectory(initialPath || undefined);
  }, [open, initialPath, loadDirectory]);

  const handleNavigate = useCallback(
    (path: string | null) => {
      setSelectedPath(null);
      void loadDirectory(path ?? undefined);
    },
    [loadDirectory],
  );

  const handleEntryClick = useCallback(
    (entry: BrowseDirectoryEntry) => {
      setSelectedPath(entry.path);
    },
    [],
  );

  const handleEntryDoubleClick = useCallback(
    (entry: BrowseDirectoryEntry) => {
      if (entry.is_dir) {
        setSelectedPath(null);
        void loadDirectory(entry.path);
      }
    },
    [loadDirectory],
  );

  const handleConfirm = useCallback(() => {
    const target = normalizeWindowsPath(selectedPath ?? currentPath);
    if (target) {
      onSelect(target);
      onClose();
    }
  }, [selectedPath, currentPath, onSelect, onClose]);

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (event.key === "Escape") {
        onClose();
      }
    },
    [onClose],
  );

  if (!open) return null;

  const breadcrumbs = parseBreadcrumbs(currentPath);
  const displayPath = normalizeWindowsPath(selectedPath ?? currentPath);

  return (
    <>
      <div
        className="fixed inset-0 z-[90] bg-foreground/24 backdrop-blur-[2px]"
        onClick={onClose}
      />
      <div
        className="fixed inset-0 z-[91] flex items-center justify-center p-4"
        onKeyDown={handleKeyDown}
      >
        <div className="flex w-full max-w-2xl flex-col rounded-[16px] border border-border bg-background shadow-2xl">
          {/* 标题栏 */}
          <div className="flex items-center justify-between border-b border-border px-5 py-4">
            <div>
              <span className="mb-1 block text-[10px] uppercase tracking-[0.16em] text-muted-foreground">
                Directory Browser
              </span>
              <h4 className="text-base font-semibold text-foreground">
                选择目录
              </h4>
            </div>
            <button
              type="button"
              onClick={onClose}
              className="inline-flex h-8 w-8 items-center justify-center rounded-[10px] border border-border bg-background text-base leading-none text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
              aria-label="关闭"
            >
              ×
            </button>
          </div>

          {/* 面包屑导航 */}
          <div className="flex flex-wrap items-center gap-1 border-b border-border/60 bg-secondary/20 px-5 py-2.5">
            {breadcrumbs.map((segment, index) => (
              <span key={segment.path ?? "__root__"} className="flex items-center gap-1">
                {index > 0 && (
                  <span className="text-xs text-muted-foreground/50">/</span>
                )}
                <button
                  type="button"
                  onClick={() => handleNavigate(segment.path)}
                  className={`rounded-md px-1.5 py-0.5 text-xs transition-colors ${
                    index === breadcrumbs.length - 1
                      ? "font-medium text-foreground"
                      : "text-muted-foreground hover:bg-secondary hover:text-foreground"
                  }`}
                >
                  {segment.label}
                </button>
              </span>
            ))}
          </div>

          {/* 目录列表 */}
          <div className="h-[400px] overflow-y-auto p-2">
            {isLoading && (
              <div className="flex h-full items-center justify-center">
                <p className="text-sm text-muted-foreground">正在加载...</p>
              </div>
            )}

            {error && (
              <div className="flex h-full flex-col items-center justify-center gap-3">
                <p className="text-sm text-destructive">{error}</p>
                <button
                  type="button"
                  onClick={() => handleNavigate(null)}
                  className="agentdash-button-secondary text-xs"
                >
                  返回根目录
                </button>
              </div>
            )}

            {!isLoading && !error && entries.length === 0 && (
              <div className="flex h-full items-center justify-center">
                <p className="text-sm text-muted-foreground">
                  {currentPath ? "此目录下没有子目录" : "未发现可用盘符"}
                </p>
              </div>
            )}

            {!isLoading && !error && entries.length > 0 && (
              <div className="grid grid-cols-1 gap-0.5">
                {entries.map((entry) => (
                  <button
                    key={entry.path}
                    type="button"
                    onClick={() => handleEntryClick(entry)}
                    onDoubleClick={() => handleEntryDoubleClick(entry)}
                    className={`flex items-center gap-3 rounded-[10px] px-3 py-2.5 text-left transition-colors ${
                      selectedPath === entry.path
                        ? "border border-primary/20 bg-primary/8 text-foreground"
                        : "border border-transparent text-foreground hover:bg-secondary/60"
                    }`}
                  >
                    <span className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[8px] bg-secondary/60 text-sm">
                      {entry.is_dir
                        ? currentPath === ""
                          ? "💿"
                          : "📁"
                        : "📄"}
                    </span>
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-sm font-medium">
                        {entry.name}
                      </p>
                      <p className="truncate text-[11px] text-muted-foreground">
                        {entry.path}
                      </p>
                    </div>
                    {entry.is_dir && (
                      <span className="shrink-0 text-xs text-muted-foreground/60">
                        ›
                      </span>
                    )}
                  </button>
                ))}
              </div>
            )}
          </div>

          {/* 底部操作栏 */}
          <div className="flex items-center justify-between gap-3 border-t border-border px-5 py-4">
            <div className="min-w-0 flex-1">
              <p className="truncate text-xs text-muted-foreground">
                {displayPath ? (
                  <>
                    已选择：
                    <span className="font-mono text-foreground">
                      {displayPath}
                    </span>
                  </>
                ) : (
                  "请选择一个目录"
                )}
              </p>
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={onClose}
                className="agentdash-button-secondary"
              >
                取消
              </button>
              <button
                type="button"
                onClick={handleConfirm}
                disabled={!displayPath}
                className="agentdash-button-primary disabled:cursor-not-allowed disabled:opacity-50"
              >
                确认选择
              </button>
            </div>
          </div>
        </div>
      </div>
    </>
  );
}
