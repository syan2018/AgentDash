/**
 * VFS 懒加载文件树
 *
 * 展开目录节点时才调用 listSurfaceMountEntries 加载子条目。
 * 文件节点点击时触发 onSelectFile 回调。
 */

import { useCallback, useEffect, useState } from "react";
import { listSurfaceMountEntries, type SurfaceMountEntry } from "../../services/vfs";

// ─── Types ──────────────────────────────────────────────

interface TreeNode {
  path: string;
  name: string;
  isDir: boolean;
  size?: number | null;
  contentKind?: string | null;
  mimeType?: string | null;
  children?: TreeNode[];
  isLoaded: boolean;
  isLoading: boolean;
  isExpanded: boolean;
}

export interface VfsFileTreeProps {
  surfaceRef: string;
  mountId: string;
  onSelectFile: (entry: SurfaceMountEntry) => void;
  selectedPath: string | null;
  rootPath?: string;
  refreshKey?: number;
}

// ─── Component ──────────────────────────────────────────

export function VfsFileTree({
  surfaceRef,
  mountId,
  onSelectFile,
  selectedPath,
  rootPath,
  refreshKey = 0,
}: VfsFileTreeProps) {
  const [rootNodes, setRootNodes] = useState<TreeNode[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // 加载根目录
  useEffect(() => {
    setRootNodes([]);
    setError(null);
    setLoading(true);
    void (async () => {
      try {
        const result = await listSurfaceMountEntries({
          surfaceRef,
          mountId,
          path: normalizeTreeRootPath(rootPath),
          recursive: false,
        });
        setRootNodes(entriesToNodes(result.entries));
      } catch (err) {
        setError(err instanceof Error ? err.message : "加载文件树失败");
      } finally {
        setLoading(false);
      }
    })();
  }, [surfaceRef, mountId, rootPath, refreshKey]);

  const toggleDir = useCallback(
    async (path: string) => {
      setRootNodes((prev) =>
        updateNodeInTree(prev, path, (node) => {
          if (!node.isDir) return node;
          if (node.isExpanded) return { ...node, isExpanded: false };
          if (node.isLoaded) return { ...node, isExpanded: true };
          return { ...node, isExpanded: true, isLoading: true };
        }),
      );

      const needsLoad = !findNode(rootNodes, path)?.isLoaded;
      if (!needsLoad) return;

      try {
        const result = await listSurfaceMountEntries({
          surfaceRef,
          mountId,
          path,
          recursive: false,
        });
        setRootNodes((prev) =>
          updateNodeInTree(prev, path, (node) => ({
            ...node,
            children: entriesToNodes(result.entries),
            isLoaded: true,
            isLoading: false,
          })),
        );
      } catch {
        setRootNodes((prev) =>
          updateNodeInTree(prev, path, (node) => ({
            ...node,
            isLoading: false,
          })),
        );
      }
    },
    [surfaceRef, mountId, rootNodes],
  );

  if (loading) {
    return (
      <div className="flex items-center justify-center py-6 text-xs text-muted-foreground">
        加载中…
      </div>
    );
  }

  if (error) {
    return (
      <div className="px-2 py-3 text-xs text-destructive">
        {error}
      </div>
    );
  }

  if (rootNodes.length === 0) {
    return (
      <div className="px-2 py-3 text-center text-xs text-muted-foreground">
        空目录
      </div>
    );
  }

  return (
    <div className="overflow-y-auto text-xs">
      {rootNodes.map((node) => (
        <TreeNodeItem
          key={node.path}
          node={node}
          depth={0}
          selectedPath={selectedPath}
          onToggleDir={toggleDir}
          onSelectFile={onSelectFile}
        />
      ))}
    </div>
  );
}

// ─── TreeNodeItem ───────────────────────────────────────

function TreeNodeItem({
  node,
  depth,
  selectedPath,
  onToggleDir,
  onSelectFile,
}: {
  node: TreeNode;
  depth: number;
  selectedPath: string | null;
  onToggleDir: (path: string) => void;
  onSelectFile: (entry: SurfaceMountEntry) => void;
}) {
  const isSelected = node.path === selectedPath;
  const paddingLeft = 8 + depth * 16;

  return (
    <>
      <button
        type="button"
        onClick={() => {
          if (node.isDir) {
            onToggleDir(node.path);
          } else {
            onSelectFile(nodeToEntry(node));
          }
        }}
        className={`flex w-full items-center gap-1.5 py-1 pr-2 text-left transition-colors hover:bg-secondary/40 ${
          isSelected ? "bg-primary/8 text-foreground" : "text-foreground/80"
        }`}
        style={{ paddingLeft }}
      >
        {node.isDir ? (
          <span className="shrink-0 text-muted-foreground/60">
            {node.isLoading ? (
              <LoadingSpinner />
            ) : node.isExpanded ? (
              <ChevronDown />
            ) : (
              <ChevronRight />
            )}
          </span>
        ) : (
          <span className="w-3 shrink-0" />
        )}
        <span className="shrink-0 text-muted-foreground/60">
          {node.isDir ? "📁" : "📄"}
        </span>
        <span className="min-w-0 flex-1 truncate font-mono">
          {node.name}
        </span>
        {!node.isDir && node.size != null && (
          <span className="shrink-0 text-[10px] text-muted-foreground/50">
            {formatSize(node.size)}
          </span>
        )}
      </button>
      {node.isDir && node.isExpanded && node.children && (
        <>
          {node.children.map((child) => (
            <TreeNodeItem
              key={child.path}
              node={child}
              depth={depth + 1}
              selectedPath={selectedPath}
              onToggleDir={onToggleDir}
              onSelectFile={onSelectFile}
            />
          ))}
          {node.children.length === 0 && node.isLoaded && (
            <div
              className="py-1 text-[10px] text-muted-foreground/50"
              style={{ paddingLeft: paddingLeft + 24 }}
            >
              空目录
            </div>
          )}
        </>
      )}
    </>
  );
}

// ─── Helpers ────────────────────────────────────────────

function normalizeTreeRootPath(path?: string): string {
  const normalized = path?.trim().replace(/\\/g, "/").replace(/^\/+|\/+$/g, "") ?? "";
  return normalized || ".";
}

function entriesToNodes(entries: SurfaceMountEntry[]): TreeNode[] {
  const dirs = entries.filter((e) => e.is_dir).sort((a, b) => a.path.localeCompare(b.path));
  const files = entries.filter((e) => !e.is_dir).sort((a, b) => a.path.localeCompare(b.path));
  return [...dirs, ...files].map((e) => ({
    path: e.path,
    name: extractFileName(e.path),
    isDir: e.is_dir,
    size: e.size,
    contentKind: e.content_kind,
    mimeType: e.mime_type,
    children: undefined,
    isLoaded: false,
    isLoading: false,
    isExpanded: false,
  }));
}

function nodeToEntry(node: TreeNode): SurfaceMountEntry {
  return {
    path: node.path,
    entry_type: node.isDir ? "directory" : "file",
    size: node.size ?? undefined,
    content_kind: node.contentKind ?? undefined,
    mime_type: node.mimeType ?? undefined,
    is_dir: node.isDir,
  };
}

function extractFileName(path: string): string {
  const parts = path.split("/");
  return parts[parts.length - 1] || path;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function findNode(nodes: TreeNode[], path: string): TreeNode | null {
  for (const node of nodes) {
    if (node.path === path) return node;
    if (node.children) {
      const found = findNode(node.children, path);
      if (found) return found;
    }
  }
  return null;
}

function updateNodeInTree(
  nodes: TreeNode[],
  path: string,
  updater: (node: TreeNode) => TreeNode,
): TreeNode[] {
  return nodes.map((node) => {
    if (node.path === path) return updater(node);
    if (node.children) {
      return { ...node, children: updateNodeInTree(node.children, path, updater) };
    }
    return node;
  });
}

// ─── Tiny Icons ─────────────────────────────────────────

function ChevronRight() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="m9 18 6-6-6-6" />
    </svg>
  );
}

function ChevronDown() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="m6 9 6 6 6-6" />
    </svg>
  );
}

function LoadingSpinner() {
  return (
    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" className="animate-spin">
      <path d="M12 2v4M12 18v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M2 12h4M18 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83" />
    </svg>
  );
}
