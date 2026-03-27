/**
 * 统一 Address Space 浏览器
 *
 * 在 Project / Story / Session 三个层级展示 mount 列表，
 * 支持按目录浏览文件树和预览文件内容。
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ExecutionAddressSpace } from "../../types";
import {
  listMountEntries,
  previewAddressSpace,
  readMountFile,
  writeMountFile,
  type MountEntry,
  type PreviewAddressSpaceResponse,
} from "../../services/addressSpaces";

// ─── Props ─────────────────────────────────────────────────

export interface AddressSpaceBrowserProps {
  /** 直接传入已有的 address space（会话页场景） */
  addressSpace?: ExecutionAddressSpace | null;
  /** 预览模式参数（项目/Story 设置页场景） */
  preview?: {
    projectId: string;
    storyId?: string;
    ownerType?: string;
    ownerId?: string;
    target?: "project" | "story" | "task";
  };
  /** 初始选中的 mount id */
  initialMountId?: string;
}

// ─── 内部类型 ──────────────────────────────────────────────

interface MountInfo {
  id: string;
  provider: string;
  backend_id: string;
  root_ref: string;
  capabilities: string[];
  default_write: boolean;
  display_name: string;
  backend_online?: boolean | null;
  file_count?: number | null;
}

const PROVIDER_LABELS: Record<string, string> = {
  relay_fs: "工作区文件",
  inline_fs: "内联文件",
  lifecycle_vfs: "Lifecycle 记录",
  external_service: "外部服务",
};

const CAPABILITY_ICONS: Record<string, string> = {
  read: "读",
  write: "写",
  list: "列",
  search: "搜",
  exec: "执行",
};

// ─── 组件 ──────────────────────────────────────────────────

export function AddressSpaceBrowser({
  addressSpace,
  preview,
  initialMountId,
}: AddressSpaceBrowserProps) {
  const previewProjectId = preview?.projectId;
  const previewStoryId = preview?.storyId;
  const previewTarget = preview?.target;
  const [mounts, setMounts] = useState<MountInfo[]>([]);
  const [defaultMountId, setDefaultMountId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [selectedMountId, setSelectedMountId] = useState<string | null>(initialMountId ?? null);

  const previewOwnerType = preview?.ownerType;
  const previewOwnerId = preview?.ownerId;

  // 加载 mounts：优先用 preview API（可获取最新 lifecycle mount），否则用 addressSpace 快照
  useEffect(() => {
    if (previewProjectId) {
      let cancelled = false;
      setLoading(true);
      setError(null);

      void (async () => {
        try {
          const result: PreviewAddressSpaceResponse = await previewAddressSpace({
            projectId: previewProjectId,
            storyId: previewStoryId,
            ownerType: previewOwnerType,
            ownerId: previewOwnerId,
            target: previewTarget,
          });
          if (cancelled) return;
          setMounts(result.mounts);
          setDefaultMountId(result.default_mount_id ?? null);
          setSelectedMountId((current) => current ?? result.default_mount_id ?? result.mounts[0]?.id ?? null);
        } catch (err) {
          if (cancelled) return;
          setError(err instanceof Error ? err.message : String(err));
        } finally {
          if (!cancelled) setLoading(false);
        }
      })();

      return () => { cancelled = true; };
    }

    if (addressSpace) {
      setMounts(
        addressSpace.mounts.map((m) => ({
          id: m.id,
          provider: m.provider,
          backend_id: m.backend_id,
          root_ref: m.root_ref,
          capabilities: m.capabilities,
          default_write: m.default_write,
          display_name: m.display_name,
          backend_online: null,
          file_count: null,
        })),
      );
      setDefaultMountId(addressSpace.default_mount_id ?? null);
      setSelectedMountId((current) => current ?? addressSpace.default_mount_id ?? addressSpace.mounts[0]?.id ?? null);
      return;
    }
  }, [addressSpace, previewProjectId, previewStoryId, previewOwnerType, previewOwnerId, previewTarget]);

  const selectedMount = useMemo(
    () => mounts.find((m) => m.id === selectedMountId) ?? null,
    [mounts, selectedMountId],
  );

  if (loading) {
    return (
      <div className="flex items-center justify-center py-8 text-xs text-muted-foreground">
        正在加载 Address Space…
      </div>
    );
  }

  if (error) {
    return (
      <div className="rounded-[10px] border border-destructive/20 bg-destructive/5 px-3 py-2 text-xs text-destructive">
        {error}
      </div>
    );
  }

  if (mounts.length === 0) {
    return (
      <div className="rounded-[10px] border border-dashed border-border px-3 py-4 text-center text-xs text-muted-foreground">
        当前配置下没有可用的 Mount。请先配置工作空间或上下文容器。
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {/* Mount 选择器 */}
      <MountSelector
        mounts={mounts}
        selectedId={selectedMountId}
        defaultMountId={defaultMountId}
        onSelect={setSelectedMountId}
      />

      {/* 文件浏览区 */}
      {selectedMount && preview && (
        <MountFileBrowser
          mount={selectedMount}
          projectId={preview.projectId}
          storyId={preview.storyId}
          ownerType={preview.ownerType}
          ownerId={preview.ownerId}
        />
      )}
    </div>
  );
}

// ─── MountSelector ─────────────────────────────────────────

function MountSelector({
  mounts,
  selectedId,
  defaultMountId,
  onSelect,
}: {
  mounts: MountInfo[];
  selectedId: string | null;
  defaultMountId: string | null;
  onSelect: (id: string) => void;
}) {
  return (
    <div className="space-y-1.5">
      <p className="text-[11px] font-medium uppercase tracking-[0.14em] text-muted-foreground/70">
        挂载点 · {mounts.length}
      </p>
      <div className="flex flex-wrap gap-1.5">
        {mounts.map((mount) => {
          const isSelected = mount.id === selectedId;
          const isDefault = mount.id === defaultMountId;
          return (
            <button
              key={mount.id}
              type="button"
              onClick={() => onSelect(mount.id)}
              className={`group flex items-center gap-1.5 rounded-[8px] border px-2.5 py-1.5 text-left text-xs transition-all ${
                isSelected
                  ? "border-primary/30 bg-primary/8 text-foreground shadow-sm"
                  : "border-border bg-background/60 text-muted-foreground hover:border-border hover:bg-secondary/50 hover:text-foreground"
              }`}
            >
              <MountStatusDot mount={mount} />
              <span className="font-medium">{mount.display_name}</span>
              <span className="rounded-[4px] bg-muted/60 px-1 py-0.5 font-mono text-[10px] text-muted-foreground">
                {PROVIDER_LABELS[mount.provider] ?? mount.provider}
              </span>
              {isDefault && (
                <span className="rounded-[4px] bg-primary/12 px-1 py-0.5 text-[10px] text-primary">
                  默认
                </span>
              )}
              {mount.default_write && (
                <span className="rounded-[4px] bg-amber-500/12 px-1 py-0.5 text-[10px] text-amber-600">
                  可写
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* 选中 mount 的详细信息 */}
      {mounts.find((m) => m.id === selectedId) && (
        <MountDetailBar mount={mounts.find((m) => m.id === selectedId)!} />
      )}
    </div>
  );
}

function MountStatusDot({ mount }: { mount: MountInfo }) {
  if (mount.provider === "relay_fs") {
    const online = mount.backend_online;
    if (online === true) {
      return <span className="inline-block h-1.5 w-1.5 rounded-full bg-emerald-500" title="Backend 在线" />;
    }
    if (online === false) {
      return <span className="inline-block h-1.5 w-1.5 rounded-full bg-red-400" title="Backend 离线" />;
    }
    return <span className="inline-block h-1.5 w-1.5 rounded-full bg-muted-foreground/30" title="状态未知" />;
  }
  if (mount.provider === "inline_fs") {
    return <span className="inline-block h-1.5 w-1.5 rounded-full bg-blue-400" title="内联文件" />;
  }
  return <span className="inline-block h-1.5 w-1.5 rounded-full bg-muted-foreground/30" />;
}

function MountDetailBar({ mount }: { mount: MountInfo }) {
  return (
    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 rounded-[8px] border border-border bg-secondary/20 px-2.5 py-1.5 text-[10px] text-muted-foreground">
      <span>
        路径: <span className="font-mono text-foreground/70">{mount.root_ref}</span>
      </span>
      {mount.file_count != null && (
        <span>{mount.file_count} 个文件</span>
      )}
      <span className="flex gap-1">
        {mount.capabilities.map((cap) => (
          <span
            key={cap}
            className="rounded-full border border-border bg-background px-1 py-0.5"
          >
            {CAPABILITY_ICONS[cap] ?? cap}
          </span>
        ))}
      </span>
    </div>
  );
}

// ─── MountFileBrowser ──────────────────────────────────────

function MountFileBrowser({
  mount,
  projectId,
  storyId,
  ownerType,
  ownerId,
}: {
  mount: MountInfo;
  projectId: string;
  storyId?: string;
  ownerType?: string;
  ownerId?: string;
}) {
  const [currentPath, setCurrentPath] = useState(".");
  const [entries, setEntries] = useState<MountEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [searchPattern, setSearchPattern] = useState("");

  const [previewFile, setPreviewFile] = useState<{ path: string; content: string; size: number } | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [editing, setEditing] = useState(false);
  const [editContent, setEditContent] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);

  const canWrite = mount.capabilities.includes("write");

  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadEntries = useCallback(
    async (path: string, pattern?: string) => {
      setLoading(true);
      setError(null);
      try {
        const result = await listMountEntries({
          projectId,
          storyId,
          ownerType,
          ownerId,
          mountId: mount.id,
          path,
          pattern: pattern || undefined,
          recursive: false,
        });
        setEntries(result.entries);
      } catch (err) {
        setError(err instanceof Error ? err.message : "加载文件列表失败");
        setEntries([]);
      } finally {
        setLoading(false);
      }
    },
    [projectId, storyId, ownerType, ownerId, mount.id],
  );

  useEffect(() => {
    setCurrentPath(".");
    setSearchPattern("");
    setPreviewFile(null);
    void loadEntries(".");
  }, [mount.id, loadEntries]);

  const handleNavigate = useCallback(
    (path: string) => {
      setCurrentPath(path);
      setSearchPattern("");
      setPreviewFile(null);
      void loadEntries(path);
    },
    [loadEntries],
  );

  const handleSearch = useCallback(
    (pattern: string) => {
      setSearchPattern(pattern);
      if (debounceRef.current) clearTimeout(debounceRef.current);
      debounceRef.current = setTimeout(() => {
        void loadEntries(currentPath, pattern);
      }, 250);
    },
    [currentPath, loadEntries],
  );

  const handleFileClick = useCallback(
    async (entry: MountEntry) => {
      if (entry.is_dir) {
        handleNavigate(entry.path);
        return;
      }
      setPreviewLoading(true);
      try {
        const result = await readMountFile({
          projectId,
          storyId,
          ownerType,
          ownerId,
          mountId: mount.id,
          path: entry.path,
        });
        setPreviewFile({
          path: result.path,
          content: result.content,
          size: result.size,
        });
      } catch (err) {
        setPreviewFile({
          path: entry.path,
          content: `读取失败: ${err instanceof Error ? err.message : "未知错误"}`,
          size: 0,
        });
      } finally {
        setPreviewLoading(false);
      }
    },
    [projectId, storyId, ownerType, ownerId, mount.id, handleNavigate],
  );

  const handleSave = useCallback(async () => {
    if (!previewFile) return;
    setSaving(true);
    setSaveError(null);
    try {
      await writeMountFile({
        projectId,
        storyId,
        mountId: mount.id,
        path: previewFile.path,
        content: editContent,
      });
      setPreviewFile({ ...previewFile, content: editContent, size: new Blob([editContent]).size });
      setEditing(false);
    } catch (err) {
      setSaveError(err instanceof Error ? err.message : "保存失败");
    } finally {
      setSaving(false);
    }
  }, [previewFile, editContent, projectId, storyId, mount.id]);

  const handleStartEdit = useCallback(() => {
    if (!previewFile) return;
    setEditContent(previewFile.content);
    setEditing(true);
    setSaveError(null);
  }, [previewFile]);

  const handleCancelEdit = useCallback(() => {
    setEditing(false);
    setSaveError(null);
  }, []);

  const pathSegments = useMemo(() => {
    if (currentPath === ".") return [{ label: mount.display_name, path: "." }];
    const parts = currentPath.split("/");
    const segments = [{ label: mount.display_name, path: "." }];
    for (let i = 0; i < parts.length; i++) {
      segments.push({
        label: parts[i],
        path: parts.slice(0, i + 1).join("/"),
      });
    }
    return segments;
  }, [currentPath, mount.display_name]);

  const sortedEntries = useMemo(() => {
    const dirs = entries.filter((e) => e.is_dir).sort((a, b) => a.path.localeCompare(b.path));
    const files = entries.filter((e) => !e.is_dir).sort((a, b) => a.path.localeCompare(b.path));
    return [...dirs, ...files];
  }, [entries]);

  return (
    <div className="rounded-[10px] border border-border bg-background/60">
      {/* 面包屑 + 搜索 */}
      <div className="flex items-center gap-2 border-b border-border px-3 py-2">
        <nav className="flex min-w-0 flex-1 items-center gap-0.5 overflow-x-auto text-xs">
          {pathSegments.map((seg, i) => (
            <span key={seg.path} className="flex shrink-0 items-center">
              {i > 0 && <span className="mx-1 text-muted-foreground/40">/</span>}
              <button
                type="button"
                onClick={() => handleNavigate(seg.path)}
                className={`rounded-[4px] px-1 py-0.5 transition-colors hover:bg-secondary ${
                  i === pathSegments.length - 1
                    ? "font-medium text-foreground"
                    : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {seg.label}
              </button>
            </span>
          ))}
        </nav>
        <input
          type="text"
          value={searchPattern}
          onChange={(e) => handleSearch(e.target.value)}
          placeholder="搜索…"
          className="w-32 shrink-0 rounded-[6px] border border-border bg-background px-2 py-1 text-xs text-foreground placeholder:text-muted-foreground/50 focus:border-primary/40 focus:outline-none"
        />
      </div>

      {/* 文件列表 */}
      <div className="max-h-[320px] overflow-y-auto">
        {loading && (
          <div className="flex items-center justify-center py-6 text-xs text-muted-foreground">
            加载中…
          </div>
        )}
        {error && (
          <div className="flex flex-col items-center gap-1.5 px-4 py-5 text-center">
            <span className="text-lg">
              {error.includes("不在线") ? "🔌" : "⚠️"}
            </span>
            <p className="text-xs text-muted-foreground">{error}</p>
            {error.includes("不在线") && (
              <p className="text-[11px] text-muted-foreground/60">
                Backend 连接后即可浏览文件
              </p>
            )}
            {!error.includes("不在线") && (
              <button
                type="button"
                onClick={() => void loadEntries(currentPath, searchPattern || undefined)}
                className="mt-1 rounded-[6px] border border-border px-2.5 py-1 text-[11px] text-muted-foreground transition-colors hover:text-foreground"
              >
                重试
              </button>
            )}
          </div>
        )}
        {!loading && !error && sortedEntries.length === 0 && (
          <div className="px-3 py-4 text-center text-xs text-muted-foreground">
            {searchPattern ? "没有匹配的文件" : "空目录"}
          </div>
        )}
        {!loading &&
          !error &&
          sortedEntries.map((entry) => (
            <button
              key={entry.path}
              type="button"
              onClick={() => void handleFileClick(entry)}
              className={`flex w-full items-center gap-2 border-b border-border/50 px-3 py-1.5 text-left text-xs transition-colors hover:bg-secondary/30 last:border-0 ${
                previewFile?.path === entry.path ? "bg-primary/5" : ""
              }`}
            >
              <span className="shrink-0 text-muted-foreground/60">
                {entry.is_dir ? "📁" : "📄"}
              </span>
              <span className="min-w-0 flex-1 truncate font-mono text-foreground/85">
                {extractFileName(entry.path)}
              </span>
              {!entry.is_dir && entry.size != null && (
                <span className="shrink-0 text-[10px] text-muted-foreground">
                  {formatFileSize(entry.size)}
                </span>
              )}
              {entry.is_dir && (
                <span className="shrink-0 text-[10px] text-muted-foreground">→</span>
              )}
            </button>
          ))}
      </div>

      {/* 文件预览 / 编辑 */}
      {previewLoading && (
        <div className="border-t border-border px-3 py-3 text-xs text-muted-foreground">
          正在读取文件…
        </div>
      )}
      {previewFile && !previewLoading && (
        <div className="border-t border-border">
          <div className="flex items-center justify-between px-3 py-1.5">
            <span className="truncate font-mono text-[11px] text-foreground/70">
              {previewFile.path}
            </span>
            <div className="flex shrink-0 items-center gap-2">
              <span className="text-[10px] text-muted-foreground">
                {formatFileSize(previewFile.size)}
              </span>
              {canWrite && !editing && (
                <button
                  type="button"
                  onClick={handleStartEdit}
                  className="rounded-[4px] border border-primary/30 bg-primary/8 px-1.5 py-0.5 text-[10px] text-primary transition-colors hover:bg-primary/15"
                >
                  编辑
                </button>
              )}
              {editing && (
                <>
                  <button
                    type="button"
                    onClick={() => void handleSave()}
                    disabled={saving}
                    className="rounded-[4px] border border-emerald-500/30 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] text-emerald-600 transition-colors hover:bg-emerald-500/20 disabled:opacity-50"
                  >
                    {saving ? "保存中…" : "保存"}
                  </button>
                  <button
                    type="button"
                    onClick={handleCancelEdit}
                    disabled={saving}
                    className="rounded-[4px] border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground disabled:opacity-50"
                  >
                    取消
                  </button>
                </>
              )}
              {!editing && (
                <button
                  type="button"
                  onClick={() => { setPreviewFile(null); setEditing(false); }}
                  className="rounded-[4px] border border-border px-1.5 py-0.5 text-[10px] text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
                >
                  关闭
                </button>
              )}
            </div>
          </div>
          {saveError && (
            <div className="mx-3 mb-1 rounded-[4px] border border-destructive/20 bg-destructive/5 px-2 py-1 text-[10px] text-destructive">
              {saveError}
            </div>
          )}
          {editing ? (
            <textarea
              value={editContent}
              onChange={(e) => setEditContent(e.target.value)}
              disabled={saving}
              className="block max-h-[300px] min-h-[200px] w-full resize-y bg-secondary/20 px-3 py-2 font-mono text-[11px] leading-5 text-foreground/85 focus:outline-none disabled:opacity-50"
            />
          ) : (
            <pre className="max-h-[300px] overflow-auto bg-secondary/20 px-3 py-2 font-mono text-[11px] leading-5 text-foreground/85">
              {previewFile.content}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

// ─── 工具函数 ──────────────────────────────────────────────

function extractFileName(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1] || path;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}
