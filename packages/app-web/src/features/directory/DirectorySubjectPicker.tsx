import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import type { DirectoryUser } from "../../types";
import { UserAvatar } from "../../components/ui/user-avatar";
import {
  fetchDirectoryGroupTree,
  fetchDirectoryGroups,
  fetchDirectoryUsers,
} from "../../services/directory";
import type { DirectoryTreeNode } from "../../services/directory";
import { resolveGroupLabel, resolveUserLabel } from "./directorySubjectUtils";
import type { DirectoryGroupSummary } from "./directorySubjectUtils";

/* ------------------------------------------------------------------ */
/*  Shared types                                                       */
/* ------------------------------------------------------------------ */

export type DirectorySubjectMode = "user" | "group";

export interface DirectoryResponseStatus {
  source?: string;
  is_projection_only: boolean;
}

export interface SelectedSubject {
  type: "user" | "group";
  id: string;
}

/* ------------------------------------------------------------------ */
/*  Constants                                                          */
/* ------------------------------------------------------------------ */

const SEARCH_LIMIT = 20;
const TREE_LIMIT = 30;
const SEARCH_DEBOUNCE_MS = 300;

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function statusFrom(r: { source?: string; is_projection_only: boolean }): DirectoryResponseStatus {
  return { source: r.source, is_projection_only: r.is_projection_only };
}
function treeNodeToGroup(n: DirectoryTreeNode): DirectoryGroupSummary {
  return { group_id: n.group_id, display_name: n.display_name, path: n.path, provider: n.provider, source: n.source };
}

function flattenTree(nodes: DirectoryTreeNode[]): DirectoryGroupSummary[] {
  const out: DirectoryGroupSummary[] = [];
  for (const n of nodes) {
    out.push(treeNodeToGroup(n));
    if (n.children?.length) out.push(...flattenTree(n.children));
  }
  return out;
}

/* ------------------------------------------------------------------ */
/*  Row components                                                     */
/* ------------------------------------------------------------------ */

function UserRow({
  user,
  checked,
  disabled,
  disabledReason,
  onToggle,
}: {
  user: DirectoryUser;
  checked: boolean;
  disabled: boolean;
  disabledReason?: string;
  onToggle: () => void;
}) {
  const name = resolveUserLabel(user);
  const email = user.email?.trim();
  const subject = user.subject?.trim();

  return (
    <label
      className={`group/row flex w-full cursor-pointer items-center gap-3 rounded-[8px] px-3 py-2 transition-colors ${
        disabled
          ? "cursor-not-allowed opacity-55"
          : checked
            ? "bg-primary/8"
            : "hover:bg-secondary/50"
      }`}
    >
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={onToggle}
        className="h-3.5 w-3.5 shrink-0 accent-primary"
      />
      <UserAvatar avatarUrl={user.avatar_url} fallback={name} sizeClassName="h-8 w-8" />
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-2">
          <span className="truncate text-sm font-medium text-foreground">{name}</span>
          {disabledReason && (
            <span className="shrink-0 rounded-[6px] border border-border bg-secondary/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {disabledReason}
            </span>
          )}
        </span>
        <span className="mt-0.5 flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
          {subject && <span className="font-mono">{subject}</span>}
          {email && <span>{email}</span>}
          {user.source && (
            <span className="rounded-[4px] bg-muted/60 px-1 py-0.5 text-[10px]">{user.source}</span>
          )}
        </span>
      </span>
    </label>
  );
}
function GroupRow({
  group,
  checked,
  disabled,
  disabledReason,
  onToggle,
}: {
  group: DirectoryGroupSummary;
  checked: boolean;
  disabled: boolean;
  disabledReason?: string;
  onToggle: () => void;
}) {
  const name = resolveGroupLabel(group);

  return (
    <label
      className={`group/row flex w-full cursor-pointer items-center gap-3 rounded-[8px] px-3 py-2 transition-colors ${
        disabled
          ? "cursor-not-allowed opacity-55"
          : checked
            ? "bg-primary/8"
            : "hover:bg-secondary/50"
      }`}
    >
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={onToggle}
        className="h-3.5 w-3.5 shrink-0 accent-primary"
      />
      <span className="min-w-0 flex-1">
        <span className="flex items-center gap-2">
          <span className="truncate text-sm font-medium text-foreground">{name}</span>
          {disabledReason && (
            <span className="shrink-0 rounded-[6px] border border-border bg-secondary/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
              {disabledReason}
            </span>
          )}
        </span>
        {group.path && (
          <span className="mt-0.5 block truncate text-xs text-muted-foreground">{group.path}</span>
        )}
      </span>
      {group.source && (
        <span className="shrink-0 rounded-[4px] bg-muted/60 px-1 py-0.5 text-[10px] text-muted-foreground">
          {group.source}
        </span>
      )}
    </label>
  );
}

function TreeRow({
  node,
  checked,
  disabled,
  disabledReason,
  expanded,
  level,
  onToggle,
  onToggleExpand,
}: {
  node: DirectoryTreeNode;
  checked: boolean;
  disabled: boolean;
  disabledReason?: string;
  expanded: boolean;
  level: number;
  onToggle: () => void;
  onToggleExpand: () => void;
}) {
  const name = node.display_name?.trim() || node.path?.trim() || node.group_id;

  return (
    <div
      className={`flex items-center gap-1 rounded-[8px] py-1 pr-2 transition-colors ${
        disabled
          ? "opacity-55"
          : checked
            ? "bg-primary/8"
            : "hover:bg-secondary/50"
      }`}
      style={{ paddingLeft: `${8 + level * 20}px` }}
    >
      {/* Expand toggle — always occupies space for alignment */}
      <button
        type="button"
        onClick={(e) => { e.preventDefault(); onToggleExpand(); }}
        disabled={!node.has_children}
        className="flex h-5 w-5 shrink-0 items-center justify-center rounded-[4px] text-[11px] text-muted-foreground transition-colors hover:bg-secondary disabled:invisible"
      >
        {expanded ? "▾" : "▸"}
      </button>

      <label className={`flex min-w-0 flex-1 cursor-pointer items-center gap-2 ${disabled ? "cursor-not-allowed" : ""}`}>
        <input
          type="checkbox"
          checked={checked}
          disabled={disabled}
          onChange={onToggle}
          className="h-3.5 w-3.5 shrink-0 accent-primary"
        />
        <span className="min-w-0 flex-1">
          <span className="flex items-center gap-2">
            <span className="truncate text-sm text-foreground">{name}</span>
            {disabledReason && (
              <span className="shrink-0 rounded-[6px] border border-border bg-secondary/50 px-1.5 py-0.5 text-[10px] text-muted-foreground">
                {disabledReason}
              </span>
            )}
          </span>
          {node.path && (
            <span className="mt-0.5 block truncate text-xs text-muted-foreground">{node.path}</span>
          )}
        </span>
      </label>
    </div>
  );
}

function SelectedTag({
  label,
  type,
  onRemove,
}: {
  label: string;
  type: "user" | "group";
  onRemove: () => void;
}) {
  return (
    <span className="inline-flex max-w-52 items-center gap-1 rounded-[6px] border border-border bg-secondary/30 px-2 py-0.5 text-xs text-foreground">
      <span className="shrink-0 text-[10px] text-muted-foreground">{type === "user" ? "U" : "G"}</span>
      <span className="truncate">{label}</span>
      <button
        type="button"
        onClick={(e) => { e.stopPropagation(); onRemove(); }}
        className="ml-0.5 shrink-0 text-muted-foreground transition-colors hover:text-foreground"
      >
        ×
      </button>
    </span>
  );
}

function ProjectionNotice({ status }: { status: DirectoryResponseStatus | null }) {
  if (!status?.is_projection_only) return null;
  return (
    <div className="mx-3 mt-2 rounded-[8px] border border-warning/25 bg-warning/10 px-3 py-2 text-xs text-warning">
      目录 provider 暂不可用，仅显示已投影快照
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  Props                                                              */
/* ------------------------------------------------------------------ */

export interface DirectorySubjectPickerProps {
  grantedUserIds: Set<string>;
  grantedGroupIds: Set<string>;
  currentUserId?: string;
  selections: SelectedSubject[];
  onSelectionsChange: (next: SelectedSubject[]) => void;
  knownUsers: DirectoryUser[];
  knownGroups: DirectoryGroupSummary[];
  userDirectoryStatus: DirectoryResponseStatus | null;
  groupDirectoryStatus: DirectoryResponseStatus | null;
  onUsersObserved: (items: DirectoryUser[]) => void;
  onGroupsObserved: (items: DirectoryGroupSummary[]) => void;
}

/* ------------------------------------------------------------------ */
/*  Main component                                                     */
/* ------------------------------------------------------------------ */

export function DirectorySubjectPicker({
  grantedUserIds,
  grantedGroupIds,
  currentUserId,
  selections,
  onSelectionsChange,
  knownUsers,
  knownGroups,
  userDirectoryStatus,
  groupDirectoryStatus,
  onUsersObserved,
  onGroupsObserved,
}: DirectorySubjectPickerProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  const [open, setOpen] = useState(false);
  const [mode, setMode] = useState<DirectorySubjectMode>("user");
  const [groupSubTab, setGroupSubTab] = useState<"search" | "tree">("search");
  const [query, setQuery] = useState("");

  const [userResults, setUserResults] = useState<DirectoryUser[]>([]);
  const [groupResults, setGroupResults] = useState<DirectoryGroupSummary[]>([]);
  const [userSearchStatus, setUserSearchStatus] = useState<DirectoryResponseStatus | null>(null);
  const [groupSearchStatus, setGroupSearchStatus] = useState<DirectoryResponseStatus | null>(null);
  const [searchLoading, setSearchLoading] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);

  const [treeChildren, setTreeChildren] = useState<Record<string, DirectoryTreeNode[]>>({});
  const [treeCursors, setTreeCursors] = useState<Record<string, string | null>>({});
  const [treeLoading, setTreeLoading] = useState<Record<string, boolean>>({});
  const [treeError, setTreeError] = useState<Record<string, string | null>>({});
  const [treeExpanded, setTreeExpanded] = useState<Record<string, boolean>>({});
  const [treeStatus, setTreeStatus] = useState<DirectoryResponseStatus | null>(null);

  const selectedUserIds = useMemo(
    () => new Set(selections.filter((s) => s.type === "user").map((s) => s.id)),
    [selections],
  );
  const selectedGroupIds = useMemo(
    () => new Set(selections.filter((s) => s.type === "group").map((s) => s.id)),
    [selections],
  );

  const hasQuery = query.trim().length > 0;

  /* ---- Click outside & Escape ---- */
  useEffect(() => {
    if (!open) return;
    const down = (e: MouseEvent) => {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) setOpen(false);
    };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") setOpen(false); };
    document.addEventListener("mousedown", down);
    document.addEventListener("keydown", key);
    return () => { document.removeEventListener("mousedown", down); document.removeEventListener("keydown", key); };
  }, [open]);

  /* ---- User search ---- */
  useEffect(() => {
    if (!open || mode !== "user") return;
    const q = query.trim();
    if (!q) { setUserResults([]); setSearchLoading(false); setSearchError(null); setUserSearchStatus(null); return; }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      setSearchLoading(true); setSearchError(null);
      void (async () => {
        try {
          const res = await fetchDirectoryUsers({ query: q, limit: SEARCH_LIMIT });
          if (cancelled) return;
          setUserResults(res.items); setUserSearchStatus(statusFrom(res)); onUsersObserved(res.items);
        } catch (err) { if (!cancelled) setSearchError((err as Error).message); }
        finally { if (!cancelled) setSearchLoading(false); }
      })();
    }, SEARCH_DEBOUNCE_MS);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [mode, onUsersObserved, open, query]);

  /* ---- Group search ---- */
  useEffect(() => {
    if (!open || mode !== "group" || groupSubTab !== "search") return;
    const q = query.trim();
    if (!q) { setGroupResults([]); setSearchLoading(false); setSearchError(null); setGroupSearchStatus(null); return; }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      setSearchLoading(true); setSearchError(null);
      void (async () => {
        try {
          const res = await fetchDirectoryGroups({ query: q, limit: SEARCH_LIMIT });
          if (cancelled) return;
          setGroupResults(res.items); setGroupSearchStatus(statusFrom(res)); onGroupsObserved(res.items);
        } catch (err) { if (!cancelled) setSearchError((err as Error).message); }
        finally { if (!cancelled) setSearchLoading(false); }
      })();
    }, SEARCH_DEBOUNCE_MS);
    return () => { cancelled = true; window.clearTimeout(timer); };
  }, [groupSubTab, mode, onGroupsObserved, open, query]);

  /* ---- Tree ---- */
  const loadTreeChildren = useCallback(
    async (parentId: string | null, cursor?: string) => {
      const key = parentId ?? "";
      setTreeLoading((c) => ({ ...c, [key]: true }));
      setTreeError((c) => ({ ...c, [key]: null }));
      try {
        const res = await fetchDirectoryGroupTree({ parent_id: key, cursor, limit: TREE_LIMIT });
        setTreeStatus(statusFrom(res));
        onGroupsObserved(flattenTree(res.items));
        setTreeChildren((c) => {
          const existing = cursor ? (c[key] ?? []) : [];
          const next: Record<string, DirectoryTreeNode[]> = { ...c, [key]: [...existing, ...res.items] };
          for (const item of res.items) { if (item.children?.length) next[item.group_id] = item.children; }
          return next;
        });
        setTreeCursors((c) => ({ ...c, [key]: res.next_cursor ?? null }));
      } catch (err) {
        setTreeError((c) => ({ ...c, [key]: (err as Error).message }));
        setTreeChildren((c) => ({ ...c, [key]: c[key] ?? [] }));
      } finally {
        setTreeLoading((c) => ({ ...c, [key]: false }));
      }
    },
    [onGroupsObserved],
  );

  useEffect(() => {
    if (!open || mode !== "group" || groupSubTab !== "tree") return;
    if (treeChildren[""] || treeLoading[""]) return;
    void loadTreeChildren(null);
  }, [groupSubTab, loadTreeChildren, mode, open, treeChildren, treeLoading]);

  /* ---- Selection ---- */
  const toggleSubject = useCallback(
    (type: "user" | "group", id: string) => {
      const exists = selections.some((s) => s.type === type && s.id === id);
      onSelectionsChange(
        exists ? selections.filter((s) => !(s.type === type && s.id === id)) : [...selections, { type, id }],
      );
    },
    [onSelectionsChange, selections],
  );

  const removeSubject = useCallback(
    (type: "user" | "group", id: string) => {
      onSelectionsChange(selections.filter((s) => !(s.type === type && s.id === id)));
    },
    [onSelectionsChange, selections],
  );

  const toggleTreeNode = useCallback(
    (node: DirectoryTreeNode) => {
      const wasExpanded = treeExpanded[node.group_id] === true;
      setTreeExpanded((c) => ({ ...c, [node.group_id]: !wasExpanded }));
      if (!wasExpanded && node.has_children && !treeChildren[node.group_id]) {
        void loadTreeChildren(node.group_id);
      }
    },
    [loadTreeChildren, treeChildren, treeExpanded],
  );

  /* ---- Tree render ---- */
  const renderTree = (parentId: string | null, level: number): ReactNode => {
    const key = parentId ?? "";
    const nodes = treeChildren[key] ?? [];
    const loading = treeLoading[key] === true;
    const error = treeError[key];
    const cursor = treeCursors[key];

    return (
      <>
        {nodes.map((node) => (
          <div key={node.group_id}>
            <TreeRow
              node={node}
              checked={selectedGroupIds.has(node.group_id)}
              disabled={grantedGroupIds.has(node.group_id)}
              disabledReason={grantedGroupIds.has(node.group_id) ? "已授权" : undefined}
              expanded={treeExpanded[node.group_id] === true}
              level={level}
              onToggle={() => toggleSubject("group", node.group_id)}
              onToggleExpand={() => toggleTreeNode(node)}
            />
            {treeExpanded[node.group_id] && renderTree(node.group_id, level + 1)}
          </div>
        ))}
        {loading && <p className="py-2 text-center text-xs text-muted-foreground">加载中...</p>}
        {error && <p className="px-3 py-1.5 text-xs text-destructive">{error}</p>}
        {cursor && !loading && (
          <button
            type="button"
            onClick={() => void loadTreeChildren(parentId, cursor)}
            className="w-full py-1.5 text-center text-xs font-medium text-primary hover:text-primary/80"
          >
            加载更多
          </button>
        )}
      </>
    );
  };

  /* ---- Tag label resolve ---- */
  const resolveTagLabel = (s: SelectedSubject): string => {
    if (s.type === "user") {
      const u = knownUsers.find((u) => u.user_id === s.id) ?? userResults.find((u) => u.user_id === s.id);
      return u ? resolveUserLabel(u) : s.id;
    }
    const g = knownGroups.find((g) => g.group_id === s.id) ?? groupResults.find((g) => g.group_id === s.id);
    return g ? resolveGroupLabel(g) : s.id;
  };

  const activeStatus = mode === "user"
    ? (hasQuery ? userSearchStatus : userDirectoryStatus)
    : (hasQuery ? groupSearchStatus : groupDirectoryStatus);

  /* ---- Render ---- */
  return (
    <div ref={containerRef} className="relative" data-directory-picker>
      {/* Trigger */}
      <div
        className="flex min-h-[2.5rem] flex-wrap items-center gap-1.5 rounded-[8px] border border-border bg-background px-2.5 py-1.5 transition-colors focus-within:border-primary/40 focus-within:ring-1 focus-within:ring-primary/20"
        onClick={() => { inputRef.current?.focus(); setOpen(true); }}
      >
        {selections.map((s) => (
          <SelectedTag key={`${s.type}:${s.id}`} label={resolveTagLabel(s)} type={s.type} onRemove={() => removeSubject(s.type, s.id)} />
        ))}
        <input
          ref={inputRef}
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onFocus={() => setOpen(true)}
          placeholder={selections.length === 0 ? "搜索用户名称、邮箱或用户组…" : "继续添加…"}
          className="min-w-[10rem] flex-1 bg-transparent text-sm text-foreground outline-none placeholder:text-muted-foreground"
        />
      </div>

      {/* Popover */}
      {open && (
        <div className="absolute left-0 right-0 top-full z-50 mt-1 overflow-hidden rounded-[12px] border border-border bg-background shadow-lg">
          {/* Tabs */}
          <div className="flex border-b border-border bg-muted/15">
            {(["user", "group"] as const).map((m) => (
              <button
                key={m}
                type="button"
                onClick={() => { setMode(m); setQuery(""); setSearchError(null); }}
                className={`flex-1 px-4 py-2 text-sm transition-colors ${
                  mode === m ? "border-b-2 border-primary font-medium text-foreground" : "text-muted-foreground hover:text-foreground"
                }`}
              >
                {m === "user" ? "用户" : "用户组"}
                {((m === "user" && selectedUserIds.size > 0) || (m === "group" && selectedGroupIds.size > 0)) && (
                  <span className="ml-1.5 inline-flex h-[18px] min-w-[18px] items-center justify-center rounded-[8px] bg-primary/15 px-1 text-[10px] font-semibold text-primary">
                    {m === "user" ? selectedUserIds.size : selectedGroupIds.size}
                  </span>
                )}
              </button>
            ))}
          </div>

          {/* Group sub-tabs */}
          {mode === "group" && (
            <div className="flex gap-1 border-b border-border/50 px-3 py-1.5">
              {(["search", "tree"] as const).map((tab) => (
                <button
                  key={tab}
                  type="button"
                  onClick={() => setGroupSubTab(tab)}
                  className={`rounded-[6px] px-2.5 py-1 text-xs transition-colors ${
                    groupSubTab === tab ? "bg-secondary font-medium text-foreground" : "text-muted-foreground hover:text-foreground"
                  }`}
                >
                  {tab === "search" ? "搜索" : "组织树"}
                </button>
              ))}
            </div>
          )}

          <ProjectionNotice status={activeStatus} />

          {searchError && (
            <div className="mx-3 mt-2 rounded-[8px] border border-destructive/25 bg-destructive/5 px-3 py-2 text-xs text-destructive">
              {searchError}
            </div>
          )}

          {searchLoading && <p className="px-4 py-3 text-xs text-muted-foreground">搜索中...</p>}

          {/* User results */}
          {mode === "user" && !searchLoading && (
            <div className="max-h-80 overflow-y-auto p-1.5">
              {!hasQuery ? (
                <p className="px-3 py-5 text-center text-sm text-muted-foreground">
                  输入关键词搜索用户
                </p>
              ) : userResults.length === 0 ? (
                <p className="px-3 py-5 text-center text-sm text-muted-foreground">
                  没有匹配的用户
                </p>
              ) : (
                userResults.map((user) => (
                  <UserRow
                    key={user.user_id}
                    user={user}
                    checked={selectedUserIds.has(user.user_id)}
                    disabled={grantedUserIds.has(user.user_id) || user.user_id === currentUserId}
                    disabledReason={
                      grantedUserIds.has(user.user_id) ? "已授权" : user.user_id === currentUserId ? "当前用户" : undefined
                    }
                    onToggle={() => toggleSubject("user", user.user_id)}
                  />
                ))
              )}
            </div>
          )}

          {/* Group search results */}
          {mode === "group" && groupSubTab === "search" && !searchLoading && (
            <div className="max-h-80 overflow-y-auto p-1.5">
              {!hasQuery ? (
                <p className="px-3 py-5 text-center text-sm text-muted-foreground">
                  输入关键词搜索用户组，或切换到「组织树」浏览
                </p>
              ) : groupResults.length === 0 ? (
                <p className="px-3 py-5 text-center text-sm text-muted-foreground">
                  没有匹配的用户组
                </p>
              ) : (
                groupResults.map((group) => (
                  <GroupRow
                    key={group.group_id}
                    group={group}
                    checked={selectedGroupIds.has(group.group_id)}
                    disabled={grantedGroupIds.has(group.group_id)}
                    disabledReason={grantedGroupIds.has(group.group_id) ? "已授权" : undefined}
                    onToggle={() => toggleSubject("group", group.group_id)}
                  />
                ))
              )}
            </div>
          )}

          {/* Group tree */}
          {mode === "group" && groupSubTab === "tree" && (
            <div className="max-h-80 overflow-y-auto p-1.5">
              <ProjectionNotice status={treeStatus} />
              {(treeChildren[""]?.length ?? 0) === 0 && !treeLoading[""] && !treeError[""] && (
                <p className="px-3 py-5 text-center text-sm text-muted-foreground">暂无根组织</p>
              )}
              {renderTree(null, 0)}
            </div>
          )}

          {/* Footer */}
          {selections.length > 0 && (
            <div className="border-t border-border px-4 py-2 text-xs text-muted-foreground">
              已选 {selections.length} 项
              {selectedUserIds.size > 0 && ` · ${selectedUserIds.size} 用户`}
              {selectedGroupIds.size > 0 && ` · ${selectedGroupIds.size} 用户组`}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
