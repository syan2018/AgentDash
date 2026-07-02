import React, { useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { NavLink, type NavLinkRenderProps } from "react-router-dom";
import { useCurrentUserStore } from "../../stores/currentUserStore";
import { useAuthStore } from "../../stores/authStore";
import { useTheme } from "../../hooks/use-theme";
import { UserAvatar } from "../ui/user-avatar";
import type { BackendConfig } from "../../types";
import type { SidebarBackendGroups } from "./sidebarBackendVisibility";

// 底栏共享 popover：事件流移除（无人关注），仅保留后端 + 主题
export type FooterPanelKey = "backend" | "theme";

// ─── 底栏：UserCard 常驻 + IconBar + Portal overlay popup ───

interface SidebarFooterProps {
  activePanel: FooterPanelKey | null;
  onTogglePanel: (key: FooterPanelKey) => void;
  onClosePanel: () => void;
  backendGroups: SidebarBackendGroups;
  connectionState: string;
  currentUser: ReturnType<typeof useCurrentUserStore.getState>["currentUser"];
  rememberedPath: string;
}

export function SidebarFooter({
  activePanel,
  onTogglePanel,
  onClosePanel,
  backendGroups,
  connectionState,
  currentUser,
  rememberedPath,
}: SidebarFooterProps) {
  const footerRef = useRef<HTMLDivElement>(null);
  const overlayRef = useRef<HTMLDivElement>(null);
  const { theme } = useTheme();

  const [anchor, setAnchor] = useState<{ top: number; left: number; right: number } | null>(null);

  useEffect(() => {
    if (!activePanel) return;
    const update = () => {
      if (!footerRef.current) return;
      const rect = footerRef.current.getBoundingClientRect();
      setAnchor({ top: Math.round(rect.top), left: Math.round(rect.left), right: Math.round(rect.right) });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [activePanel]);

  useEffect(() => {
    if (!activePanel) return;
    const handler = (event: MouseEvent) => {
      const target = event.target as Node;
      if (overlayRef.current?.contains(target)) return;
      if (footerRef.current?.contains(target)) return;
      onClosePanel();
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [activePanel, onClosePanel]);

  const visibleBackends = [...backendGroups.projectBackends, ...backendGroups.personalBackends];
  const backendOnline = visibleBackends.filter((b) => b.online).length;
  const backendDotClass = backendOnline > 0 ? "bg-emerald-500" : "bg-muted-foreground/30";

  const panelTitle =
    activePanel === "backend" ? "后端连接" : activePanel === "theme" ? "主题" : "";

  return (
    <>
      <div ref={footerRef} className="border-t border-border">
        {currentUser && <UserCard />}
        <div className="flex items-center gap-0.5 border-t border-border/60 px-2 py-1.5">
          <FooterIconButton
            label="后端连接"
            active={activePanel === "backend"}
            onClick={() => onTogglePanel("backend")}
          >
            <span className="relative">
              <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
                <rect x="2" y="3" width="20" height="8" rx="2" />
                <rect x="2" y="13" width="20" height="8" rx="2" />
                <path d="M6 7h.01" />
                <path d="M6 17h.01" />
              </svg>
              <span className={`absolute -right-0.5 -top-0.5 h-1.5 w-1.5 rounded-full ring-2 ring-background ${backendDotClass}`} />
            </span>
          </FooterIconButton>

          <FooterIconButton
            label="主题"
            active={activePanel === "theme"}
            onClick={() => onTogglePanel("theme")}
          >
            <ThemeIcon theme={theme} />
          </FooterIconButton>

          <div className="flex-1" />

          <NavLink
            to="/settings"
            state={{ return_to: rememberedPath }}
            title="设置"
            aria-label="设置"
            className={({ isActive }: NavLinkRenderProps) =>
              `flex h-8 w-8 items-center justify-center rounded-[8px] transition-colors ${
                isActive
                  ? "bg-secondary text-foreground"
                  : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
              }`
            }
          >
            <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" />
              <circle cx="12" cy="12" r="3" />
            </svg>
          </NavLink>
        </div>
      </div>

      {/* Portal overlay：从 footer 上方向上浮出，平面化内部内容（无嵌套 card） */}
      {activePanel &&
        anchor &&
        createPortal(
          <div
            ref={overlayRef}
            style={{
              position: "fixed",
              left: anchor.left + 8,
              width: anchor.right - anchor.left - 16,
              bottom: window.innerHeight - anchor.top + 6,
              maxHeight: `calc(${anchor.top}px - 16px)`,
            }}
            className="z-40 flex flex-col overflow-hidden rounded-[12px] border border-border bg-background shadow-2xl"
          >
            <div className="flex items-center justify-between px-4 pb-2 pt-3">
              <span className="text-[10px] font-medium uppercase tracking-[0.14em] text-muted-foreground">
                {panelTitle}
              </span>
              <button
                type="button"
                onClick={onClosePanel}
                className="inline-flex h-5 w-5 items-center justify-center rounded text-muted-foreground hover:bg-secondary hover:text-foreground"
                aria-label="关闭"
              >
                <svg xmlns="http://www.w3.org/2000/svg" width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M18 6 6 18" />
                  <path d="m6 6 12 12" />
                </svg>
              </button>
            </div>
            <div className="flex-1 overflow-y-auto px-2 pb-3">
              {activePanel === "backend" && (
                <BackendPanel backendGroups={backendGroups} connectionState={connectionState} />
              )}
              {activePanel === "theme" && <ThemePanel />}
            </div>
          </div>,
          document.body,
        )}
    </>
  );
}

function FooterIconButton({
  label,
  active,
  onClick,
  children,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      aria-pressed={active}
      className={`flex h-8 w-8 items-center justify-center rounded-[8px] transition-colors ${
        active
          ? "bg-secondary text-foreground"
          : "text-muted-foreground hover:bg-secondary/60 hover:text-foreground"
      }`}
    >
      {children}
    </button>
  );
}

function ThemeIcon({ theme }: { theme: "light" | "dark" | "system" }) {
  if (theme === "light") {
    return (
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2v2" />
        <path d="M12 20v2" />
        <path d="m4.93 4.93 1.41 1.41" />
        <path d="m17.66 17.66 1.41 1.41" />
        <path d="M2 12h2" />
        <path d="M20 12h2" />
        <path d="m4.93 19.07 1.41-1.41" />
        <path d="m17.66 6.34 1.41-1.41" />
      </svg>
    );
  }
  if (theme === "dark") {
    return (
      <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
        <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
      </svg>
    );
  }
  return (
    <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
      <rect x="2" y="3" width="20" height="14" rx="2" />
      <path d="M8 21h8" />
      <path d="M12 17v4" />
    </svg>
  );
}

// ─── 常驻 UserCard（点击展开身份 popover，含退出登录） ──────────

function UserCard() {
  const { currentUser } = useCurrentUserStore();
  const logout = useAuthStore((state) => state.logout);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [anchor, setAnchor] = useState<{ top: number; left: number; right: number } | null>(null);

  useEffect(() => {
    if (!open) return;
    const update = () => {
      if (!triggerRef.current) return;
      const rect = triggerRef.current.getBoundingClientRect();
      setAnchor({ top: Math.round(rect.top), left: Math.round(rect.left), right: Math.round(rect.right) });
    };
    update();
    window.addEventListener("resize", update);
    window.addEventListener("scroll", update, true);
    return () => {
      window.removeEventListener("resize", update);
      window.removeEventListener("scroll", update, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onMouseDown = (event: MouseEvent) => {
      const target = event.target as Node;
      if (popoverRef.current?.contains(target)) return;
      if (triggerRef.current?.contains(target)) return;
      setOpen(false);
    };
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onMouseDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onMouseDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  if (!currentUser) return null;

  const title = currentUser.display_name?.trim() || currentUser.email?.trim() || currentUser.user_id;
  const subtitle = currentUser.email?.trim() || currentUser.user_id;
  const modeLabel = currentUser.auth_mode === "enterprise" ? "企业" : "个人";
  const avatarUrl = currentUser.avatar_url?.trim();
  const providerLabel = currentUser.provider?.trim();

  const handleLogout = () => {
    setOpen(false);
    logout();
  };

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => setOpen((v) => !v)}
        aria-haspopup="menu"
        aria-expanded={open}
        className={`flex w-full items-center gap-2 px-3 py-2 text-left transition-colors ${
          open ? "bg-secondary/70" : "hover:bg-secondary/60"
        }`}
      >
        <UserAvatar avatarUrl={avatarUrl} fallback={title} />
        <div className="min-w-0 flex-1">
          <p className="truncate text-xs font-medium text-foreground">{title}</p>
          {subtitle !== title && (
            <p className="truncate text-[10px] text-muted-foreground">{subtitle}</p>
          )}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          {currentUser.is_admin && (
            <span className="rounded-[4px] border border-warning/30 bg-warning/10 px-1 py-0.5 text-[9px] text-warning">
              Admin
            </span>
          )}
          <span className="rounded-[4px] border border-border bg-secondary px-1 py-0.5 text-[9px] text-muted-foreground">
            {modeLabel}
          </span>
        </div>
      </button>

      {open &&
        anchor &&
        createPortal(
          <div
            ref={popoverRef}
            role="menu"
            style={{
              position: "fixed",
              left: anchor.left + 8,
              width: anchor.right - anchor.left - 16,
              bottom: window.innerHeight - anchor.top + 6,
              maxHeight: `calc(${anchor.top}px - 16px)`,
            }}
            className="z-40 flex flex-col overflow-hidden rounded-[12px] border border-border bg-background shadow-2xl"
          >
            <div className="flex items-start gap-3 px-4 pb-3 pt-3">
              <UserAvatar avatarUrl={avatarUrl} fallback={title} sizeClassName="h-10 w-10" />
              <div className="min-w-0 flex-1">
                <p className="truncate text-sm font-medium text-foreground">{title}</p>
                {currentUser.email && (
                  <p className="truncate text-[11px] text-muted-foreground">{currentUser.email}</p>
                )}
                <div className="mt-1.5 flex flex-wrap items-center gap-1">
                  <span className="rounded-[4px] border border-border bg-secondary px-1.5 py-0.5 text-[9px] text-muted-foreground">
                    {modeLabel}
                  </span>
                  {currentUser.is_admin && (
                    <span className="rounded-[4px] border border-warning/30 bg-warning/10 px-1.5 py-0.5 text-[9px] text-warning">
                      Admin
                    </span>
                  )}
                  {providerLabel && (
                    <span className="rounded-[4px] border border-border bg-secondary px-1.5 py-0.5 text-[9px] text-muted-foreground">
                      {providerLabel}
                    </span>
                  )}
                </div>
              </div>
            </div>
            <div className="border-t border-border" />
            <button
              type="button"
              role="menuitem"
              onClick={handleLogout}
              className="flex items-center gap-2 px-4 py-2.5 text-left text-xs text-destructive transition-colors hover:bg-destructive/10"
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                width="14"
                height="14"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                strokeWidth="1.8"
                strokeLinecap="round"
                strokeLinejoin="round"
              >
                <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
                <polyline points="16 17 21 12 16 7" />
                <line x1="21" y1="12" x2="9" y2="12" />
              </svg>
              退出登录
            </button>
          </div>,
          document.body,
        )}
    </>
  );
}

// ─── BackendPanel：平面化（行 + 分割线，无嵌套 card） ────────

function BackendPanel({
  backendGroups,
  connectionState,
}: {
  backendGroups: SidebarBackendGroups;
  connectionState: string;
}) {
  const backends = [...backendGroups.projectBackends, ...backendGroups.personalBackends];
  const [expandedKey, setExpandedKey] = useState<string | null>(
    backends.length === 1
      ? backendGroups.projectBackends[0]
        ? `project:${backendGroups.projectBackends[0].id}`
        : backendGroups.personalBackends[0]
          ? `personal:${backendGroups.personalBackends[0].id}`
          : null
      : null,
  );

  const streamLabel =
    connectionState === "connected"
      ? "已连接"
      : connectionState === "reconnecting"
        ? "重连中…"
        : connectionState === "connecting"
          ? "连接中…"
          : "未连接";
  const streamDotClass =
    connectionState === "connected"
      ? "bg-emerald-500"
      : connectionState === "reconnecting" || connectionState === "connecting"
        ? "bg-amber-400 animate-pulse"
        : "bg-muted-foreground/30";

  return (
    <div>
      {backends.length === 0 ? (
        <p className="px-2 py-2 text-xs text-muted-foreground">暂无后端</p>
      ) : (
        <div className="space-y-2">
          <BackendGroup
            groupKey="project"
            title="当前项目可用"
            emptyText="当前项目暂无可用后端"
            backends={backendGroups.projectBackends}
            expandedKey={expandedKey}
            onToggle={setExpandedKey}
          />
          <BackendGroup
            groupKey="personal"
            title="我的连接"
            emptyText="暂无个人连接"
            backends={backendGroups.personalBackends}
            expandedKey={expandedKey}
            onToggle={setExpandedKey}
          />
        </div>
      )}

      {/* 项目同步状态：作为 backend 面板里的元信息行（无独立 card） */}
      <div className="mt-2 flex items-center gap-2 border-t border-border/60 px-2 pt-2">
        <span className={`inline-block h-1.5 w-1.5 rounded-full ${streamDotClass}`} />
        <span className="text-[11px] text-muted-foreground">项目同步 · {streamLabel}</span>
      </div>
    </div>
  );
}

function BackendGroup({
  groupKey,
  title,
  emptyText,
  backends,
  expandedKey,
  onToggle,
}: {
  groupKey: "project" | "personal";
  title: string;
  emptyText: string;
  backends: BackendConfig[];
  expandedKey: string | null;
  onToggle: React.Dispatch<React.SetStateAction<string | null>>;
}) {
  return (
    <section>
      <p className="px-2 pb-1 text-[10px] uppercase tracking-wider text-muted-foreground">{title}</p>
      {backends.length === 0 ? (
        <p className="px-2 py-1 text-[11px] text-muted-foreground/70">{emptyText}</p>
      ) : (
        <div>
          {backends.map((backend) => {
            const rowKey = `${groupKey}:${backend.id}`;
            return (
              <BackendRow
                key={rowKey}
                backend={backend}
                isExpanded={expandedKey === rowKey}
                onToggle={() => onToggle((prev) => (prev === rowKey ? null : rowKey))}
              />
            );
          })}
        </div>
      )}
    </section>
  );
}

function BackendRow({
  backend,
  isExpanded,
  onToggle,
}: {
  backend: BackendConfig;
  isExpanded: boolean;
  onToggle: () => void;
}) {
  const executors = backend.capabilities?.executors ?? [];
  const availableCount = executors.filter((e) => e.available).length;

  return (
    <div>
      <button
        type="button"
        className="flex w-full items-center gap-2 rounded-[8px] px-2 py-1.5 text-left text-sm transition-colors hover:bg-secondary/50"
        onClick={onToggle}
      >
        <span
          className={`inline-block h-2 w-2 shrink-0 rounded-full ${backend.online ? "bg-emerald-500" : "bg-muted-foreground/30"}`}
        />
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-foreground">{backend.name}</span>
        <span className="shrink-0 text-[10px] text-muted-foreground">
          {backend.online
            ? `${availableCount} 执行器`
            : backend.backend_type === "local"
              ? "本机"
              : "远程"}
        </span>
        <svg
          xmlns="http://www.w3.org/2000/svg"
          width="10"
          height="10"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          className={`shrink-0 text-muted-foreground transition-transform ${isExpanded ? "rotate-180" : ""}`}
        >
          <path d="m6 9 6 6 6-6" />
        </svg>
      </button>
      {isExpanded && (
        <div className="px-2 pb-2 pt-1 text-[11px]">
          {backend.online && executors.length > 0 && (
            <div className="flex items-start justify-between gap-2">
              <div className="min-w-0 flex items-center gap-1.5">
                <span className="shrink-0 text-[10px] uppercase tracking-wider text-muted-foreground">执行器</span>
                <div className="flex min-w-0 flex-wrap gap-1">
                  {executors.map((ex) => (
                    <span
                      key={ex.id}
                      className={`inline-block rounded-[6px] px-1.5 py-0.5 text-[10px] ${
                        ex.available
                          ? "bg-emerald-500/10 text-emerald-700 dark:text-emerald-400"
                          : "bg-secondary text-muted-foreground"
                      }`}
                    >
                      {ex.name}
                    </span>
                  ))}
                </div>
              </div>
              <BackendMeta backend={backend} />
            </div>
          )}
          {(!backend.online || executors.length === 0) && <BackendMeta backend={backend} />}
        </div>
      )}
    </div>
  );
}

function BackendMeta({ backend }: { backend: BackendConfig }) {
  return (
    <div className="flex shrink-0 flex-nowrap items-center gap-1.5 whitespace-nowrap text-[10px] text-muted-foreground">
      <span>{backend.backend_type === "local" ? "本机" : "远程"}</span>
      <span>·</span>
      <span>{backend.online ? "在线" : "离线"}</span>
    </div>
  );
}

function ThemePanel() {
  const { theme, setTheme } = useTheme();
  const options: Array<{ value: "light" | "dark" | "system"; label: string }> = [
    { value: "light", label: "浅色" },
    { value: "dark", label: "深色" },
    { value: "system", label: "系统" },
  ];
  return (
    <div className="flex gap-1 px-1 pt-1">
      {options.map((option) => {
        const active = option.value === theme;
        return (
          <button
            key={option.value}
            type="button"
            onClick={() => setTheme(option.value)}
            className={`flex-1 rounded-[8px] px-2 py-1.5 text-xs transition-colors ${
              active
                ? "bg-secondary text-foreground shadow-sm"
                : "text-muted-foreground hover:bg-secondary/50 hover:text-foreground"
            }`}
          >
            {option.label}
          </button>
        );
      })}
    </div>
  );
}
