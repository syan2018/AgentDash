import { useCallback, useEffect, useMemo, useState } from "react";
import {
  fetchCanvas,
  fetchCanvasRuntimeSnapshot,
  updateCanvas,
} from "../../services/canvas";
import type { Canvas, CanvasDataBinding, CanvasFile, CanvasRuntimeSnapshot } from "../../types";
import { CanvasBindingsEditor } from "./CanvasBindingsEditor";
import { CanvasFilesEditor } from "./CanvasFilesEditor";
import { CanvasRuntimePreview } from "./CanvasRuntimePreview";

export interface CanvasSessionPanelProps {
  canvasId: string | null;
  sessionId: string | null;
  onClose: () => void;
}

export function CanvasSessionPanel({ canvasId, sessionId, onClose }: CanvasSessionPanelProps) {
  const [canvas, setCanvas] = useState<Canvas | null>(null);
  const [snapshot, setSnapshot] = useState<CanvasRuntimeSnapshot | null>(null);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [isSavingBindings, setIsSavingBindings] = useState(false);
  const [bindingsError, setBindingsError] = useState<string | null>(null);
  const [isSavingFiles, setIsSavingFiles] = useState(false);
  const [filesError, setFilesError] = useState<string | null>(null);

  const loadCanvasData = useCallback(async () => {
    if (!canvasId) {
      setCanvas(null);
      setSnapshot(null);
      setError(null);
      setBindingsError(null);
      setSelectedFilePath(null);
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const [nextCanvas, nextSnapshot] = await Promise.all([
        fetchCanvas(canvasId),
        fetchCanvasRuntimeSnapshot(canvasId, sessionId),
      ]);
      setCanvas(nextCanvas);
      setSnapshot(nextSnapshot);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Canvas 加载失败");
      setCanvas(null);
      setSnapshot(null);
      setBindingsError(null);
      setFilesError(null);
    } finally {
      setIsLoading(false);
    }
  }, [canvasId, sessionId]);

  useEffect(() => {
    void loadCanvasData();
  }, [loadCanvasData]);

  useEffect(() => {
    const nextFilePath = snapshot?.entry ?? snapshot?.files[0]?.path ?? null;
    setSelectedFilePath((current) => {
      if (!snapshot) {
        return null;
      }
      if (current && snapshot.files.some((file) => file.path === current)) {
        return current;
      }
      return nextFilePath;
    });
  }, [snapshot]);

  const selectedFile = useMemo(() => {
    if (!snapshot || !selectedFilePath) {
      return null;
    }
    return snapshot.files.find((file) => file.path === selectedFilePath) ?? null;
  }, [selectedFilePath, snapshot]);

  const handleBindingsSave = useCallback(async (bindings: CanvasDataBinding[]) => {
    if (!canvasId) {
      return;
    }

    setIsSavingBindings(true);
    setBindingsError(null);
    try {
      const nextCanvas = await updateCanvas(canvasId, { bindings });
      setCanvas(nextCanvas);
      const nextSnapshot = await fetchCanvasRuntimeSnapshot(canvasId, sessionId);
      setSnapshot(nextSnapshot);
      setFilesError(null);
    } catch (err) {
      setBindingsError(err instanceof Error ? err.message : "保存绑定失败");
      throw err;
    } finally {
      setIsSavingBindings(false);
    }
  }, [canvasId, sessionId]);

  const handleFilesSave = useCallback(async (input: {
    entryFile: string;
    files: CanvasFile[];
  }) => {
    if (!canvasId) {
      return;
    }

    setIsSavingFiles(true);
    setFilesError(null);
    try {
      const nextCanvas = await updateCanvas(canvasId, {
        entry_file: input.entryFile,
        files: input.files,
      });
      setCanvas(nextCanvas);
      const nextSnapshot = await fetchCanvasRuntimeSnapshot(canvasId, sessionId);
      setSnapshot(nextSnapshot);
    } catch (err) {
      setFilesError(err instanceof Error ? err.message : "保存文件失败");
      throw err;
    } finally {
      setIsSavingFiles(false);
    }
  }, [canvasId, sessionId]);

  if (!canvasId) {
    return null;
  }

  return (
    <aside className="flex h-full w-full flex-col overflow-hidden bg-background">
      <header className="flex items-center justify-between border-b border-border px-4 py-3">
        <div className="min-w-0">
          <p className="text-[11px] uppercase tracking-[0.14em] text-muted-foreground">Canvas</p>
          <h3 className="truncate text-sm font-semibold text-foreground">
            {canvas?.title || canvasId}
          </h3>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => void loadCanvasData()}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            刷新
          </button>
          <button
            type="button"
            onClick={onClose}
            className="rounded-[8px] border border-border bg-background px-2 py-1 text-xs text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          >
            关闭
          </button>
        </div>
      </header>

      <div className="flex-1 space-y-4 overflow-y-auto px-4 py-4">
        {isLoading && <p className="text-sm text-muted-foreground">正在加载 Canvas 运行时快照...</p>}
        {error && (
          <div className="rounded-[10px] border border-destructive/30 bg-destructive/10 px-3 py-2 text-xs text-destructive">
            {error}
          </div>
        )}

        {!isLoading && !error && snapshot && (
          <>
            <section className="space-y-1 rounded-[10px] border border-border bg-secondary/20 p-3">
              <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">基本信息</p>
              <p className="text-xs text-foreground/85">入口文件: {snapshot.entry || "-"}</p>
              <p className="text-xs text-foreground/85">文件数: {snapshot.files.length}</p>
              <p className="text-xs text-foreground/85">绑定数: {snapshot.bindings.length}</p>
              <p className="text-xs text-foreground/85">库依赖: {snapshot.libraries.join(", ") || "-"}</p>
              <p className="text-xs text-foreground/85">
                会话: {snapshot.session_id || sessionId || "未指定"}
              </p>
            </section>

            <CanvasRuntimePreview snapshot={snapshot} />

            <section className="space-y-2 rounded-[10px] border border-border bg-secondary/20 p-3">
              <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">绑定状态</p>
              {snapshot.bindings.length === 0 && (
                <p className="text-xs text-muted-foreground">当前没有数据绑定。</p>
              )}
              {snapshot.bindings.map((binding) => (
                <div key={`${binding.alias}:${binding.source_uri}`} className="rounded-[8px] border border-border bg-background px-2 py-2 text-xs">
                  <p className="font-medium text-foreground">{binding.alias}</p>
                  <p className="break-all text-muted-foreground">source: {binding.source_uri}</p>
                  <p className="text-muted-foreground">path: {binding.data_path}</p>
                  <p className="text-muted-foreground">resolved: {binding.resolved ? "yes" : "no"}</p>
                </div>
              ))}
            </section>

            <CanvasBindingsEditor
              value={canvas?.bindings ?? []}
              isSaving={isSavingBindings}
              error={bindingsError}
              onSave={handleBindingsSave}
            />

            <CanvasFilesEditor
              value={canvas?.files ?? []}
              entryFile={canvas?.entry_file ?? ""}
              isSaving={isSavingFiles}
              error={filesError}
              onSave={handleFilesSave}
            />

            <section className="space-y-3 rounded-[10px] border border-border bg-secondary/20 p-3">
              <div>
                <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">运行时文件快照</p>
                <h4 className="mt-1 text-sm font-semibold text-foreground">当前快照展开结果</h4>
              </div>

              {snapshot.files.length === 0 && (
                <p className="text-xs text-muted-foreground">当前没有可运行文件。</p>
              )}

              {snapshot.files.length > 0 && (
                <div className="space-y-3">
                  <div className="flex flex-wrap gap-2">
                    {snapshot.files.map((file) => {
                      const isSelected = file.path === selectedFilePath;
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
                        </button>
                      );
                    })}
                  </div>

                  {selectedFile && (
                    <div className="space-y-2 rounded-[10px] border border-border bg-background p-3">
                      <div className="flex items-center justify-between gap-3">
                        <p className="min-w-0 truncate text-xs font-medium text-foreground">
                          {selectedFile.path}
                        </p>
                        <span className="shrink-0 text-[11px] text-muted-foreground">
                          {selectedFile.file_type}
                        </span>
                      </div>
                      <pre className="max-h-[260px] overflow-auto rounded-[8px] border border-border bg-slate-950 px-3 py-2 text-[11px] leading-5 text-slate-100">
                        <code>{selectedFile.content}</code>
                      </pre>
                    </div>
                  )}
                </div>
              )}
            </section>
          </>
        )}

        {!isLoading && !error && !snapshot && (
          <div className="rounded-[10px] border border-border bg-secondary/20 px-3 py-3 text-xs text-muted-foreground">
            当前还没有可展示的 Canvas 运行时快照。
          </div>
        )}
      </div>
    </aside>
  );
}

export default CanvasSessionPanel;
