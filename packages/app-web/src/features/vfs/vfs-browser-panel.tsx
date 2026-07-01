/**
 * VFS 浏览器面板 — 用于 WorkspacePanel Tab 的完整双栏布局
 *
 * 左栏：Mount 选择器 + 懒加载文件树
 * 右栏：CodeMirror 文件编辑器
 * 使用 react-resizable-panels 实现左右分栏。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { ConfirmDialog, PromptDialog } from "@agentdash/ui";
import { Group, Panel, Separator } from "react-resizable-panels";
import {
  createSurfaceFile,
  deleteSurfaceFile,
  readSurfaceFileBlob,
  readSurfaceFile,
  renameSurfaceFile,
  uploadSurfaceFileBlob,
  writeSurfaceFile,
} from "../../services/vfs";
import type { SurfaceMountEntry } from "../../services/vfs";
import type { ResolvedVfsSurface } from "../../types";
import { VfsFileTree } from "./vfs-file-tree";
import { VfsCodeEditor } from "./vfs-code-editor";
import { isVfsMountBrowsable, selectDefaultVfsMount } from "./vfs-browser-panel-policy";
import { formatBytes } from "./vfs-format";
import { VfsImageFilePreview } from "./vfs-image-file-preview";

export interface VfsBrowserPanelProps {
  surface?: ResolvedVfsSurface | null;
  initialMountId?: string;
  initialFilePath?: string;
  /** 将浏览器裁切到 mount 内的某个子目录；文件操作仍使用完整 mount-relative path。 */
  rootPath?: string;
  protectedFilePaths?: string[];
  /** 当用户切换 mount 或文件时回调，用于更新 Tab URI */
  onNavigate?: (mountId: string, filePath: string | null) => void;
  /** 可选右侧检查器，用于在不重写文件树/编辑器的前提下承载业务语义面板。 */
  renderInspector?: (context: VfsBrowserPanelInspectorContext) => ReactNode;
}

export interface VfsBrowserPanelMountOption {
  id: string;
  displayName: string;
  provider: string;
  backendOnline?: boolean | null;
  browsable: boolean;
  canWrite: boolean;
  editCapabilities: {
    create: boolean;
    delete: boolean;
    rename: boolean;
  };
}

export interface VfsBrowserPanelInspectorContext {
  surfaceRef: string;
  mount: VfsBrowserPanelMountOption | null;
  mountId: string | null;
  filePath: string | null;
  displayPath: string | null;
  rootPath: string;
  fileContent: string | null;
  fileLoading: boolean;
  readOnly: boolean;
  fileProtected: boolean;
  operationBusy: boolean;
  operationError: string | null;
  saveFile: (content: string) => Promise<void>;
  refreshTree: () => void;
}

interface SelectedBinaryFile {
  kind: "image" | "binary";
  path: string;
  objectUrl?: string;
  mimeType?: string | null;
  size?: number | null;
}

type FilePromptState =
  | { kind: "create"; value: string }
  | { kind: "rename"; value: string }
  | { kind: "upload"; value: string; file: File };

export function VfsBrowserPanel({
  surface,
  initialMountId,
  initialFilePath,
  rootPath,
  protectedFilePaths = [],
  onNavigate,
  renderInspector,
}: VfsBrowserPanelProps) {
  const [selectedMountId, setSelectedMountId] = useState<string | null>(initialMountId ?? null);
  const [selectedFilePath, setSelectedFilePath] = useState<string | null>(initialFilePath ?? null);
  const [fileContent, setFileContent] = useState<string | null>(null);
  const [selectedBinaryFile, setSelectedBinaryFile] = useState<SelectedBinaryFile | null>(null);
  const [fileLoading, setFileLoading] = useState(false);
  const [treeRefreshKey, setTreeRefreshKey] = useState(0);
  const [operationBusy, setOperationBusy] = useState(false);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [filePrompt, setFilePrompt] = useState<FilePromptState | null>(null);
  const [deleteConfirmOpen, setDeleteConfirmOpen] = useState(false);
  const uploadInputRef = useRef<HTMLInputElement | null>(null);

  const surfaceRef = surface?.surface_ref ?? null;
  const scopedRootPath = useMemo(() => normalizeScopedRootPath(rootPath), [rootPath]);
  const protectedPathSet = useMemo(
    () => new Set(protectedFilePaths.map((path) => normalizeScopedRootPath(path))),
    [protectedFilePaths],
  );
  const selectedFileProtected = selectedFilePath ? protectedPathSet.has(selectedFilePath) : false;

  const mounts = useMemo<VfsBrowserPanelMountOption[]>(() => {
    return (surface?.mounts ?? []).map((m) => ({
      id: m.id,
      displayName: m.display_name || m.id,
      provider: m.provider,
      backendOnline: "backend_online" in m ? m.backend_online : null,
      browsable: isVfsMountBrowsable(m),
      canWrite: m.default_write || m.capabilities.includes("write"),
      editCapabilities: m.edit_capabilities,
    }));
  }, [surface]);

  const selectedMount = useMemo(
    () => mounts.find((m) => m.id === selectedMountId) ?? null,
    [mounts, selectedMountId],
  );

  const selectedMountBrowsable = selectedMount?.browsable ?? false;
  const canUploadImage = Boolean(
    selectedMount
      && selectedMount.provider === "inline_fs"
      && selectedMountBrowsable
      && selectedMount.editCapabilities.create,
  );

  const replaceBinaryFile = useCallback((next: SelectedBinaryFile | null) => {
    setSelectedBinaryFile((current) => {
      if (current?.objectUrl) URL.revokeObjectURL(current.objectUrl);
      return next;
    });
  }, []);

  useEffect(() => () => {
    if (selectedBinaryFile?.objectUrl) URL.revokeObjectURL(selectedBinaryFile.objectUrl);
  }, [selectedBinaryFile?.objectUrl]);

  // 默认选中第一个可浏览 mount，避免离线 relay_fs 在预览页自动触发 503。
  useEffect(() => {
    if (selectedMountId && mounts.some((m) => m.id === selectedMountId)) return;
    const defaultId = selectDefaultVfsMount(mounts, {
      initialMountId,
      defaultMountId: surface?.default_mount_id,
    })?.id ?? null;
    setSelectedMountId(defaultId);
    if (!initialFilePath) {
      setSelectedFilePath(null);
      setFileContent(null);
      replaceBinaryFile(null);
    }
    setOperationError(null);
  }, [mounts, selectedMountId, initialMountId, surface?.default_mount_id, initialFilePath, replaceBinaryFile]);

  // 有 initialFilePath 时自动加载文件内容
  const initialLoadDone = useRef(false);
  useEffect(() => {
    if (
      initialLoadDone.current
      || !initialFilePath
      || !surfaceRef
      || !selectedMountId
      || !selectedMountBrowsable
    ) return;
    initialLoadDone.current = true;
    setFileLoading(true);
    readSurfaceFile({ surfaceRef, mountId: selectedMountId, path: initialFilePath })
      .then((result) => setFileContent(result.content))
      .catch((err) => setFileContent(`读取失败: ${err instanceof Error ? err.message : "未知错误"}`))
      .finally(() => setFileLoading(false));
  }, [initialFilePath, surfaceRef, selectedMountId, selectedMountBrowsable]);

  const handleSelectFile = useCallback(
    async (entry: SurfaceMountEntry) => {
      if (!surfaceRef || !selectedMountId || !selectedMountBrowsable) return;
      const path = entry.path;
      setSelectedFilePath(path);
      setOperationError(null);
      onNavigate?.(selectedMountId, path);
      setFileLoading(true);
      setFileContent(null);
      replaceBinaryFile(null);
      try {
        if (isImageEntry(entry)) {
          const blob = await readSurfaceFileBlob({
            surfaceRef,
            mountId: selectedMountId,
            path,
          });
          const objectUrl = URL.createObjectURL(blob);
          replaceBinaryFile({
            kind: "image",
            path,
            objectUrl,
            mimeType: entry.mime_type ?? blob.type,
            size: entry.size,
          });
          return;
        }
        if (entry.content_kind === "binary") {
          replaceBinaryFile({
            kind: "binary",
            path,
            mimeType: entry.mime_type,
            size: entry.size,
          });
          return;
        }
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
    [surfaceRef, selectedMountId, selectedMountBrowsable, onNavigate, replaceBinaryFile],
  );

  const handleSave = useCallback(
    async (content: string) => {
      if (!surfaceRef || !selectedMountId || !selectedFilePath || !selectedMountBrowsable) return;
      await writeSurfaceFile({
        surfaceRef,
        mountId: selectedMountId,
        path: selectedFilePath,
        content,
      });
      setFileContent(content);
      replaceBinaryFile(null);
    },
    [surfaceRef, selectedMountId, selectedFilePath, selectedMountBrowsable, replaceBinaryFile],
  );

  const refreshTree = useCallback(() => {
    setTreeRefreshKey((current) => current + 1);
  }, []);

  const handleCreateFile = useCallback(async () => {
    if (!surfaceRef || !selectedMountId || !selectedMountBrowsable || !selectedMount?.editCapabilities.create) return;
    const suggestedPath = selectedFilePath
      ? `${parentPath(toScopedDisplayPath(selectedFilePath, scopedRootPath))}new-file.txt`
      : "new-file.txt";
    setFilePrompt({ kind: "create", value: suggestedPath });
  }, [surfaceRef, selectedMountId, selectedMountBrowsable, selectedMount, selectedFilePath, scopedRootPath]);

  const handleConfirmCreateFile = useCallback(async (path: string) => {
    if (!surfaceRef || !selectedMountId || !selectedMountBrowsable || !selectedMount?.editCapabilities.create) return;
    const normalizedPath = resolveScopedPath(scopedRootPath, path);
    if (!normalizedPath) return;
    setOperationBusy(true);
    setOperationError(null);
    setFilePrompt(null);
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
      replaceBinaryFile(null);
      onNavigate?.(selectedMountId, normalizedPath);
    } catch (err) {
      setOperationError(err instanceof Error ? err.message : "新建文件失败");
    } finally {
      setOperationBusy(false);
    }
  }, [surfaceRef, selectedMountId, selectedMountBrowsable, selectedMount, scopedRootPath, refreshTree, onNavigate, replaceBinaryFile]);

  const handleDeleteFile = useCallback(async () => {
    if (!surfaceRef || !selectedMountId || !selectedMountBrowsable || !selectedFilePath || selectedFileProtected || !selectedMount?.editCapabilities.delete) return;
    setDeleteConfirmOpen(true);
  }, [surfaceRef, selectedMountId, selectedMountBrowsable, selectedFilePath, selectedFileProtected, selectedMount]);

  const handleConfirmDeleteFile = useCallback(async () => {
    if (!surfaceRef || !selectedMountId || !selectedMountBrowsable || !selectedFilePath || selectedFileProtected || !selectedMount?.editCapabilities.delete) return;
    setOperationBusy(true);
    setOperationError(null);
    setDeleteConfirmOpen(false);
    try {
      await deleteSurfaceFile({
        surfaceRef,
        mountId: selectedMountId,
        path: selectedFilePath,
      });
      refreshTree();
      setSelectedFilePath(null);
      setFileContent(null);
      replaceBinaryFile(null);
      onNavigate?.(selectedMountId, null);
    } catch (err) {
      setOperationError(err instanceof Error ? err.message : "删除文件失败");
    } finally {
      setOperationBusy(false);
    }
  }, [surfaceRef, selectedMountId, selectedMountBrowsable, selectedFilePath, selectedFileProtected, selectedMount, refreshTree, onNavigate, replaceBinaryFile]);

  const handleRenameFile = useCallback(async () => {
    if (!surfaceRef || !selectedMountId || !selectedMountBrowsable || !selectedFilePath || selectedFileProtected || !selectedMount?.editCapabilities.rename) return;
    setFilePrompt({ kind: "rename", value: toScopedDisplayPath(selectedFilePath, scopedRootPath) });
  }, [surfaceRef, selectedMountId, selectedMountBrowsable, selectedFilePath, selectedFileProtected, selectedMount, scopedRootPath]);

  const handleConfirmRenameFile = useCallback(async (path: string) => {
    if (!surfaceRef || !selectedMountId || !selectedMountBrowsable || !selectedFilePath || selectedFileProtected || !selectedMount?.editCapabilities.rename) return;
    const normalizedPath = resolveScopedPath(scopedRootPath, path);
    if (!normalizedPath || normalizedPath === selectedFilePath) return;
    setOperationBusy(true);
    setOperationError(null);
    setFilePrompt(null);
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
  }, [surfaceRef, selectedMountId, selectedMountBrowsable, selectedFilePath, selectedFileProtected, selectedMount, scopedRootPath, refreshTree, onNavigate]);

  const handleUploadImage = useCallback(async (files: FileList | null) => {
    if (!surfaceRef || !selectedMountId || !canUploadImage || !files?.length) return;
    const file = files[0];
    const suggestedPath = `assets/${file.name}`;
    setFilePrompt({ kind: "upload", value: suggestedPath, file });
  }, [surfaceRef, selectedMountId, canUploadImage]);

  const handleConfirmUploadImage = useCallback(async (path: string, file: File) => {
    if (!surfaceRef || !selectedMountId || !canUploadImage) return;
    const normalizedPath = resolveScopedPath(scopedRootPath, path);
    if (!normalizedPath) return;
    setOperationBusy(true);
    setOperationError(null);
    setFilePrompt(null);
    try {
      const result = await uploadSurfaceFileBlob({
        surfaceRef,
        mountId: selectedMountId,
        path: normalizedPath,
        file,
      });
      refreshTree();
      await handleSelectFile({
        path: result.path,
        entry_type: "file",
        size: result.size,
        content_kind: result.content_kind,
        mime_type: result.mime_type,
        is_dir: false,
      });
    } catch (err) {
      setOperationError(err instanceof Error ? err.message : "上传图片失败");
    } finally {
      setOperationBusy(false);
      if (uploadInputRef.current) uploadInputRef.current.value = "";
    }
  }, [surfaceRef, selectedMountId, canUploadImage, scopedRootPath, refreshTree, handleSelectFile]);

  const handleConfirmFilePrompt = useCallback(() => {
    if (!filePrompt) return;
    if (filePrompt.kind === "create") {
      void handleConfirmCreateFile(filePrompt.value);
      return;
    }
    if (filePrompt.kind === "rename") {
      void handleConfirmRenameFile(filePrompt.value);
      return;
    }
    void handleConfirmUploadImage(filePrompt.value, filePrompt.file);
  }, [filePrompt, handleConfirmCreateFile, handleConfirmRenameFile, handleConfirmUploadImage]);

  const handleCloseFilePrompt = useCallback(() => {
    setFilePrompt(null);
    if (uploadInputRef.current) uploadInputRef.current.value = "";
  }, []);

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

  const inspectorContext: VfsBrowserPanelInspectorContext = {
    surfaceRef,
    mount: selectedMount,
    mountId: selectedMountId,
    filePath: selectedFilePath,
    displayPath: selectedFilePath ? toScopedDisplayPath(selectedFilePath, scopedRootPath) : null,
    rootPath: scopedRootPath,
    fileContent,
    fileLoading,
    readOnly: !selectedMount?.canWrite,
    fileProtected: selectedFileProtected,
    operationBusy,
    operationError,
    saveFile: handleSave,
    refreshTree,
  };
  const inspector = renderInspector?.(inspectorContext) ?? null;
  const promptNormalizedPath = filePrompt ? resolveScopedPath(scopedRootPath, filePrompt.value) : null;
  const promptDisabled = operationBusy
    || !promptNormalizedPath
    || (filePrompt?.kind === "rename" && promptNormalizedPath === selectedFilePath);
  const promptTitle = filePrompt?.kind === "create"
    ? "新建文件"
    : filePrompt?.kind === "rename"
      ? "重命名文件"
      : "上传图片";
  const promptLabel = filePrompt?.kind === "upload" ? "图片保存路径" : "文件路径";
  const promptConfirmLabel = filePrompt?.kind === "rename" ? "重命名" : filePrompt?.kind === "upload" ? "上传" : "新建";

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
            replaceBinaryFile(null);
            setOperationError(null);
            onNavigate?.(newMountId, null);
          }}
          className="min-w-0 flex-1 rounded-[6px] border border-border bg-background px-2 py-1 text-xs text-foreground focus:border-primary/40 focus:outline-none"
        >
          {mounts.map((m) => (
            <option key={m.id} value={m.id}>
              {formatMountOptionLabel(m)}
            </option>
          ))}
        </select>
        <div className="flex shrink-0 items-center gap-1">
          <FileActionButton
            title="新建文件"
            disabled={operationBusy || !selectedMountBrowsable || !selectedMount?.editCapabilities.create}
            onClick={() => void handleCreateFile()}
          >
            <PlusIcon />
          </FileActionButton>
          <FileActionButton
            title="上传图片"
            disabled={operationBusy || !canUploadImage}
            onClick={() => uploadInputRef.current?.click()}
          >
            <UploadIcon />
          </FileActionButton>
          <input
            ref={uploadInputRef}
            type="file"
            accept="image/*"
            className="hidden"
            onChange={(event) => void handleUploadImage(event.currentTarget.files)}
          />
          <FileActionButton
            title="重命名当前文件"
            disabled={operationBusy || !selectedMountBrowsable || selectedFileProtected || !selectedFilePath || !selectedMount?.editCapabilities.rename}
            onClick={() => void handleRenameFile()}
          >
            <RenameIcon />
          </FileActionButton>
          <FileActionButton
            title="删除当前文件"
            disabled={operationBusy || !selectedMountBrowsable || selectedFileProtected || !selectedFilePath || !selectedMount?.editCapabilities.delete}
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
            {selectedMountId && selectedMountBrowsable && (
              <VfsFileTree
                surfaceRef={surfaceRef}
                mountId={selectedMountId}
                onSelectFile={(entry) => void handleSelectFile(entry)}
                selectedPath={selectedFilePath}
                rootPath={scopedRootPath}
                refreshKey={treeRefreshKey}
              />
            )}
            {selectedMountId && !selectedMountBrowsable && (
              <OfflineMountNotice mount={selectedMount} />
            )}
          </div>
        </Panel>

        <Separator className="group relative w-1 shrink-0 bg-border/20 transition-colors hover:bg-primary/20 active:bg-primary/40 data-[separator]:cursor-col-resize">
          <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-border/40 transition-colors group-hover:bg-primary/40" />
        </Separator>

        {/* 中栏：文件编辑器 */}
        <Panel defaultSize={inspector ? "50%" : "70%"} minSize="30%">
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
          {!fileLoading && selectedBinaryFile?.kind === "image" && selectedBinaryFile.objectUrl && (
            <VfsImageFilePreview
              path={selectedBinaryFile.path}
              src={selectedBinaryFile.objectUrl}
              mimeType={selectedBinaryFile.mimeType}
              size={selectedBinaryFile.size}
            />
          )}
          {!fileLoading && selectedBinaryFile?.kind === "binary" && (
            <BinaryFileNotice file={selectedBinaryFile} />
          )}
          {!fileLoading && fileContent == null && selectedBinaryFile == null && (
            <div className="flex h-full items-center justify-center px-6">
              <p className="text-center text-sm text-muted-foreground">
                {selectedMountBrowsable
                  ? "在左侧文件树中选择一个文件以查看内容"
                  : "当前 Mount 的 Backend 离线，连接后即可浏览文件。"}
              </p>
            </div>
          )}
        </Panel>

        {inspector && (
          <>
            <Separator className="group relative w-1 shrink-0 bg-border/20 transition-colors hover:bg-primary/20 active:bg-primary/40 data-[separator]:cursor-col-resize">
              <div className="absolute inset-y-0 left-1/2 w-px -translate-x-1/2 bg-border/40 transition-colors group-hover:bg-primary/40" />
            </Separator>

            <Panel defaultSize="25%" minSize="20%" maxSize="40%">
              <div className="h-full overflow-y-auto border-l border-border/50 bg-secondary/10">
                {inspector}
              </div>
            </Panel>
          </>
        )}
      </Group>
      <PromptDialog
        open={filePrompt !== null}
        title={promptTitle}
        label={promptLabel}
        value={filePrompt?.value ?? ""}
        confirmLabel={promptConfirmLabel}
        disabled={promptDisabled}
        isConfirming={operationBusy}
        onValueChange={(value) => setFilePrompt((current) => current ? { ...current, value } : current)}
        onClose={handleCloseFilePrompt}
        onConfirm={handleConfirmFilePrompt}
      />
      <ConfirmDialog
        open={deleteConfirmOpen}
        title="删除文件"
        description={`确定删除文件「${selectedFilePath ? toScopedDisplayPath(selectedFilePath, scopedRootPath) : ""}」？`}
        confirmLabel="删除"
        tone="danger"
        disabled={operationBusy}
        isConfirming={operationBusy}
        onClose={() => setDeleteConfirmOpen(false)}
        onConfirm={handleConfirmDeleteFile}
      />
    </div>
  );
}

function formatMountOptionLabel(mount: VfsBrowserPanelMountOption): string {
  return mount.displayName && mount.displayName !== mount.id
    ? `${mount.id} · ${mount.displayName}`
    : mount.id;
}

function OfflineMountNotice({ mount }: { mount: VfsBrowserPanelMountOption | null }) {
  return (
    <div className="px-3 py-4 text-xs leading-5 text-muted-foreground">
      <p className="font-medium text-foreground">Backend 离线</p>
      <p className="mt-1">
        {mount?.displayName ?? "当前 Mount"} 暂时不可浏览。预览页不会主动请求离线 backend 的文件列表。
      </p>
    </div>
  );
}

function BinaryFileNotice({ file }: { file: SelectedBinaryFile }) {
  return (
    <div className="flex h-full items-center justify-center px-6">
      <div className="max-w-md rounded-[8px] border border-border bg-secondary/20 px-4 py-3 text-sm">
        <div className="font-medium text-foreground">二进制文件</div>
        <div className="mt-1 break-all font-mono text-xs text-muted-foreground">{file.path}</div>
        <div className="mt-2 text-xs text-muted-foreground">
          {file.mimeType ?? "unknown"} · {file.size != null ? formatBytes(file.size) : "未知大小"}
        </div>
      </div>
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

function isImageEntry(entry: SurfaceMountEntry): boolean {
  return entry.content_kind === "binary" && isImageMime(entry.mime_type);
}

function isImageMime(mimeType?: string | null): boolean {
  return Boolean(mimeType?.startsWith("image/"));
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

function UploadIcon() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
      <path d="M12 3v12" />
      <path d="m7 8 5-5 5 5" />
      <path d="M5 21h14" />
    </svg>
  );
}
