import { useCallback, useEffect, useState } from "react";
import { fetchCanvas, fetchCanvasRuntimeSnapshot } from "../../services/canvas";
import type { Canvas, CanvasRuntimeSnapshot } from "../../types";

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

  const loadCanvasData = useCallback(async () => {
    if (!canvasId) {
      setCanvas(null);
      setSnapshot(null);
      setError(null);
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
    } finally {
      setIsLoading(false);
    }
  }, [canvasId, sessionId]);

  useEffect(() => {
    void loadCanvasData();
  }, [loadCanvasData]);

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
              <p className="text-xs text-foreground/85">
                会话: {snapshot.session_id || sessionId || "未指定"}
              </p>
            </section>

            <section className="space-y-2 rounded-[10px] border border-border bg-secondary/20 p-3">
              <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">数据绑定</p>
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

            <section className="space-y-2 rounded-[10px] border border-border bg-secondary/20 p-3">
              <p className="text-[11px] uppercase tracking-[0.12em] text-muted-foreground">文件摘要</p>
              {snapshot.files.length === 0 && (
                <p className="text-xs text-muted-foreground">当前没有可运行文件。</p>
              )}
              {snapshot.files.map((file) => (
                <div key={file.path} className="rounded-[8px] border border-border bg-background px-2 py-2 text-xs">
                  <p className="break-all font-medium text-foreground">{file.path}</p>
                  <p className="text-muted-foreground">type: {file.file_type}</p>
                  <p className="text-muted-foreground">size: {file.content.length} chars</p>
                </div>
              ))}
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
