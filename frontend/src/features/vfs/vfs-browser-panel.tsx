/**
 * VFS 浏览器面板 — 用于 WorkspacePanel Tab 的完整双栏布局
 *
 * 左栏：Mount 选择器 + 懒加载文件树
 * 右栏：CodeMirror 文件编辑器
 * 使用 react-resizable-panels 实现左右分栏。
 */

import { useCallback, useEffect, useMemo, useState } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import {
  readSurfaceFile,
  writeSurfaceFile,
} from "../../services/vfs";
import type { ExecutionVfs, ResolvedVfsSurface } from "../../types";
import { VfsFileTree } from "./vfs-file-tree";
import { VfsCodeEditor } from "./vfs-code-editor";

export interface VfsBrowserPanelProps {
  surface?: ResolvedVfsSurface | null;
  vfs?: ExecutionVfs | null;
  initialMountId?: string;
}

interface MountOption {
  id: string;
  displayName: string;
  provider: string;
  canWrite: boolean;
}

export function VfsBrowserPanel({
  surface,
  vfs,
  initialMountId,
}: VfsBrowserPanelProps) {
  const [selectedMountId, setSelectedMountId] = useState<string | null>(initialMountId ?? null);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(null);
  const [fileContent, setFileContent] = useState<string | null>(null);
  const [fileLoading, setFileLoading] = useState(false);

  const surfaceRef = surface?.surface_ref ?? null;

  const mounts = useMemo<MountOption[]>(() => {
    const source = surface?.mounts ?? vfs?.mounts ?? [];
    return source.map((m) => ({
      id: m.id,
      displayName: m.display_name || m.id,
      provider: m.provider,
      canWrite: m.default_write || m.capabilities.includes("write"),
    }));
  }, [surface, vfs]);

  const selectedMount = useMemo(
    () => mounts.find((m) => m.id === selectedMountId) ?? null,
    [mounts, selectedMountId],
  );

  // 默认选中第一个 mount
  useEffect(() => {
    if (selectedMountId && mounts.some((m) => m.id === selectedMountId)) return;
    const defaultId = initialMountId && mounts.some((m) => m.id === initialMountId)
      ? initialMountId
      : mounts[0]?.id ?? null;
    setSelectedMountId(defaultId);
    setSelectedFilePath(null);
    setFileContent(null);
  }, [mounts, selectedMountId, initialMountId]);

  const handleSelectFile = useCallback(
    async (path: string) => {
      if (!surfaceRef || !selectedMountId) return;
      setSelectedFilePath(path);
      setFileLoading(true);
      try {
        const result = await readSurfaceFile({
          surfaceRef,
          mountId: selectedMountId,
          path,
        });
        setFileContent(result.content);
      } catch (err) {
        setFileContent(`读取失败: ${err instanceof Error ? err.message : "未知错误"}`);
      } finally {
        setFileLoading(false);
      }
    },
    [surfaceRef, selectedMountId],
  );

  const handleSave = useCallback(
    async (content: string) => {
      if (!surfaceRef || !selectedMountId || !selectedFilePath) return;
      await writeSurfaceFile({
        surfaceRef,
        mountId: selectedMountId,
        path: selectedFilePath,
        content,
      });
      setFileContent(content);
    },
    [surfaceRef, selectedMountId, selectedFilePath],
  );

  if (mounts.length === 0) {
    return (
      <div className="flex h-full items-center justify-center px-6">
        <p className="text-center text-sm text-muted-foreground">
          当前没有可用的挂载点。
        </p>
      </div>
    );
  }

  if (!surfaceRef) {
    return (
      <div className="flex h-full items-center justify-center px-6">
        <p className="text-center text-sm text-muted-foreground">
          当前入口未附带可浏览的 resolved surface。
        </p>
      </div>
    );
  }

  return (
    <div className="flex h-full flex-col overflow-hidden">
      {/* Mount 选择器 */}
      <div className="flex shrink-0 items-center gap-2 border-b border-border px-3 py-1.5">
        <label className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground/60">
          Mount
        </label>
        <select
          value={selectedMountId ?? ""}
          onChange={(e) => {
            setSelectedMountId(e.target.value);
            setSelectedFilePath(null);
            setFileContent(null);
          }}
          className="min-w-0 flex-1 rounded-[6px] border border-border bg-background px-2 py-1 text-xs text-foreground focus:border-primary/40 focus:outline-none"
        >
          {mounts.map((m) => (
            <option key={m.id} value={m.id}>
              {m.displayName} ({m.provider})
            </option>
          ))}
        </select>
      </div>

      {/* 左右分栏 */}
      <Group orientation="horizontal" className="min-h-0 flex-1">
        {/* 左栏：文件树 */}
        <Panel defaultSize="30%" minSize="15%" maxSize="50%">
          <div className="h-full overflow-y-auto border-r border-border/50">
            {selectedMountId && (
              <VfsFileTree
                surfaceRef={surfaceRef}
                mountId={selectedMountId}
                onSelectFile={(path) => void handleSelectFile(path)}
                selectedPath={selectedFilePath}
              />
            )}
          </div>
        </Panel>

        <Separator className="group relative w-1 shrink-0 bg-border/20 transition-colors hover:bg-primary/20 active:bg-primary/40 data-[separator]:cursor-col-resize">
          <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-border/40 transition-colors group-hover:bg-primary/40" />
        </Separator>

        {/* 右栏：文件编辑器 */}
        <Panel defaultSize="70%" minSize="30%">
          {fileLoading && (
            <div className="flex h-full items-center justify-center text-xs text-muted-foreground">
              正在读取文件…
            </div>
          )}
          {!fileLoading && fileContent != null && selectedFilePath && (
            <VfsCodeEditor
              content={fileContent}
              filePath={selectedFilePath}
              readOnly={!selectedMount?.canWrite}
              onSave={(content) => void handleSave(content)}
            />
          )}
          {!fileLoading && fileContent == null && (
            <div className="flex h-full items-center justify-center px-6">
              <p className="text-center text-sm text-muted-foreground">
                在左侧文件树中选择一个文件以查看内容
              </p>
            </div>
          )}
        </Panel>
      </Group>
    </div>
  );
}
