/**
 * VFS 浏览器面板 — 用于 WorkspacePanel Tab 的完整双栏布局
 *
 * 左栏：Mount 选择器 + 懒加载文件树
 * 右栏：CodeMirror 文件编辑器
 * 使用 react-resizable-panels 实现左右分栏。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Group, Panel, Separator } from "react-resizable-panels";
import {
  createSurfaceFile,
  deleteSurfaceFile,
  readSurfaceFile,
  renameSurfaceFile,
  writeSurfaceFile,
} from "../../services/vfs";
import type { ExecutionVfs, ResolvedVfsSurface } from "../../types";
import { VfsFileTree } from "./vfs-file-tree";
import { VfsCodeEditor } from "./vfs-code-editor";

export interface VfsBrowserPanelProps {
  surface?: ResolvedVfsSurface | null;
  vfs?: ExecutionVfs | null;
  initialMountId?: string;
  initialFilePath?: string;
  /** 将浏览器裁切到 mount 内的某个子目录；文件操作仍使用完整 mount-relative path。 */
  rootPath?: string;
  protectedFilePaths?: string[];
  /** 当用户切换 mount 或文件时回调，用于更新 Tab URI */
  onNavigate?: (mountId: string, filePath: string | null) => void;
}

interface MountOption {
  id: string;
  displayName: string;
  provider: string;
  canWrite: boolean;
  editCapabilities: {
    create: boolean;
    delete: boolean;
    rename: boolean;
  };
}

export function VfsBrowserPanel({
  surface,
  vfs,
  initialMountId,
  initialFilePath,
  rootPath,
  protectedFilePaths = [],
  onNavigate,
}: VfsBrowserPanelProps) {
  const [selectedMountId, setSelectedMountId] = useState<string | null>(initialMountId ?? null);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(initialFilePath ?? null);
  const [fileContent, setFileContent] = useState<string | null>(null);
  const [fileLoading, setFileLoading] = useState(false);
  const [treeRefreshKey, setTreeRefreshKey] = useState(0);
  const [operationBusy, setOperationBusy] = useState(false);
  const [operationError, setOperationError] = useState<string | null>(null);

  const surfaceRef = surface?.surface_ref ?? null;
  const scopedRootPath = useMemo(() => normalizeScopedRootPath(rootPath), [rootPath]);
  const protectedPathSet = useMemo(
    () => new Set(protectedFilePaths.map((path) => normalizeScopedRootPath(path))),
    [protectedFilePaths],
  );
  const selectedFileProtected = selectedFilePath ? protectedPathSet.has(selectedFilePath) : false;

  const mounts = useMemo<MountOption[]>(() => {
    const source = surface?.mounts ?? vfs?.mounts ?? [];
    return source.map((m) => ({
      id: m.id,
      displayName: m.display_name || m.id,
      provider: m.provider,
      canWrite: m.default_write || m.capabilities.includes("write"),
      editCapabilities: "edit_capabilities" in m
        ? m.edit_capabilities
        : {
            create: m.default_write || m.capabilities.includes("write"),
            delete: false,
            rename: false,
          },
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
    if (!initialFilePath) {
      setSelectedFilePath(null);
      setFileContent(null);
    }
    setOperationError(null);
  }, [mounts, selectedMountId, initialMountId, initialFilePath]);

  // 有 initialFilePath 时自动加载文件内容
  const initialLoadDone = useRef(false);
  useEffect(() => {
    if (initialLoadDone.current || !initialFilePath || !surfaceRef || !selectedMountId) return;
    initialLoadDone.current = true;
    setFileLoading(true);
    readSurfaceFile({ surfaceRef, mountId: selectedMountId, path: initialFilePath })
      .then((result) => setFileContent(result.content))
      .catch((err) => setFileContent(`读取失败: ${err instanceof Error ? err.message : "未知错误"}`))
      .finally(() => setFileLoading(false));
  }, [initialFilePath, surfaceRef, selectedMountId]);

  const handleSelectFile = useCallback(
    async (path: string) => {
      if (!surfaceRef || !selectedMountId) return;
      setSelectedFilePath(path);
      setOperationError(null);
      onNavigate?.(selectedMountId, path);
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
    [surfaceRef, selectedMountId, onNavigate],
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

  const refreshTree = useCallback(() => {
    setTreeRefreshKey((current) => current + 1);
  }, []);

  const handleCreateFile = useCallback(async () => {
    if (!surfaceRef || !selectedMountId || !selectedMount?.editCapabilities.create) return;
    const suggestedPath = selectedFilePath
      ? `${parentPath(toScopedDisplayPath(selectedFilePath, scopedRootPath))}new-file.txt`
      : "new-file.txt";
    const path = window.prompt("新建文件路径", suggestedPath);
    const normalizedPath = resolveScopedPath(scopedRootPath, path);
    if (!normalizedPath) return;

    setOperationBusy(true);
    setOperationError(null);
    try {
      await createSurfaceFile({
        surfaceRef,
        mountId: selectedMountId,
        path: normalizedPath,
        content: "",
      });
      refreshTree();
      setSelectedFilePath(normalizedPath);
      setFileContent("");
      onNavigate?.(selectedMountId, normalizedPath);
    } catch (err) {
      setOperationError(err instanceof Error ? err.message : "新建文件失败");
    } finally {
      setOperationBusy(false);
    }
  }, [surfaceRef, selectedMountId, selectedMount, selectedFilePath, scopedRootPath, refreshTree, onNavigate]);

  const handleDeleteFile = useCallback(async () => {
    if (!surfaceRef || !selectedMountId || !selectedFilePath || selectedFileProtected || !selectedMount?.editCapabilities.delete) return;
    if (!window.confirm(`删除文件「${toScopedDisplayPath(selectedFilePath, scopedRootPath)}」？`)) return;

    setOperationBusy(true);
    setOperationError(null);
    try {
      await deleteSurfaceFile({
        surfaceRef,
        mountId: selectedMountId,
        path: selectedFilePath,
      });
      refreshTree();
      setSelectedFilePath(null);
      setFileContent(null);
      onNavigate?.(selectedMountId, null);
    } catch (err) {
      setOperationError(err instanceof Error ? err.message : "删除文件失败");
    } finally {
      setOperationBusy(false);
    }
  }, [surfaceRef, selectedMountId, selectedFilePath, selectedFileProtected, selectedMount, scopedRootPath, refreshTree, onNavigate]);

  const handleRenameFile = useCallback(async () => {
    if (!surfaceRef || !selectedMountId || !selectedFilePath || selectedFileProtected || !selectedMount?.editCapabilities.rename) return;
    const path = window.prompt("重命名为", toScopedDisplayPath(selectedFilePath, scopedRootPath));
    const normalizedPath = resolveScopedPath(scopedRootPath, path);
    if (!normalizedPath || normalizedPath === selectedFilePath) return;

    setOperationBusy(true);
    setOperationError(null);
    try {
      await renameSurfaceFile({
        surfaceRef,
        mountId: selectedMountId,
        fromPath: selectedFilePath,
        toPath: normalizedPath,
      });
      refreshTree();
      setSelectedFilePath(normalizedPath);
      onNavigate?.(selectedMountId, normalizedPath);
    } catch (err) {
      setOperationError(err instanceof Error ? err.message : "重命名文件失败");
    } finally {
      setOperationBusy(false);
    }
  }, [surfaceRef, selectedMountId, selectedFilePath, selectedFileProtected, selectedMount, scopedRootPath, refreshTree, onNavigate]);

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
            const newMountId = e.target.value;
            setSelectedMountId(newMountId);
            setSelectedFilePath(null);
            setFileContent(null);
            setOperationError(null);
            onNavigate?.(newMountId, null);
          }}
          className="min-w-0 flex-1 rounded-[6px] border border-border bg-background px-2 py-1 text-xs text-foreground focus:border-primary/40 focus:outline-none"
        >
          {mounts.map((m) => (
            <option key={m.id} value={m.id}>
              {m.displayName} ({m.provider})
            </option>
          ))}
        </select>
        <div className="flex shrink-0 items-center gap-1">
          <FileActionButton
            title="新建文件"
            disabled={operationBusy || !selectedMount?.editCapabilities.create}
            onClick={() => void handleCreateFile()}
          >
            <PlusIcon />
          </FileActionButton>
          <FileActionButton
            title="重命名当前文件"
            disabled={operationBusy || selectedFileProtected || !selectedFilePath || !selectedMount?.editCapabilities.rename}
            onClick={() => void handleRenameFile()}
          >
            <RenameIcon />
          </FileActionButton>
          <FileActionButton
            title="删除当前文件"
            disabled={operationBusy || selectedFileProtected || !selectedFilePath || !selectedMount?.editCapabilities.delete}
            onClick={() => void handleDeleteFile()}
            danger
          >
            <TrashIcon />
          </FileActionButton>
        </div>
      </div>
      {operationError && (
        <div className="shrink-0 border-b border-destructive/20 bg-destructive/5 px-3 py-1 text-xs text-destructive">
          {operationError}
        </div>
      )}

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
                rootPath={scopedRootPath}
                refreshKey={treeRefreshKey}
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

function normalizeScopedRootPath(path?: string): string {
  return path?.trim().replace(/\\/g, "/").replace(/^\/+|\/+$/g, "") ?? "";
}

function resolveScopedPath(rootPath: string, input: string | null | undefined): string | null {
  const value = input?.trim().replace(/\\/g, "/").replace(/^\/+|\/+$/g, "") ?? "";
  if (!value) return null;
  if (!rootPath) return value;
  if (value === rootPath || value.startsWith(`${rootPath}/`)) return value;
  return `${rootPath}/${value}`;
}

function toScopedDisplayPath(path: string, rootPath: string): string {
  if (!rootPath) return path;
  if (path === rootPath) return "";
  return path.startsWith(`${rootPath}/`) ? path.slice(rootPath.length + 1) : path;
}

function parentPath(path: string): string {
  const index = path.lastIndexOf("/");
  if (index < 0) return "";
  return `${path.slice(0, index)}/`;
}

function FileActionButton({
  children,
  title,
  disabled,
  danger = false,
  onClick,
}: {
  children: ReactNode;
  title: string;
  disabled?: boolean;
  danger?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      title={title}
      aria-label={title}
      onClick={onClick}
      disabled={disabled}
      className={`inline-flex h-7 w-7 items-center justify-center rounded-[4px] border transition-colors disabled:cursor-not-allowed disabled:opacity-40 ${
        danger
          ? "border-destructive/25 text-destructive hover:bg-destructive/10"
          : "border-border text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

function PlusIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 5v14" />
      <path d="M5 12h14" />
    </svg>
  );
}

function RenameIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.12 2.12 0 0 1 3 3L7 19l-4 1 1-4Z" />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M3 6h18" />
      <path d="M8 6V4h8v2" />
      <path d="M19 6l-1 14H6L5 6" />
      <path d="M10 11v6" />
      <path d="M14 11v6" />
    </svg>
  );
}
