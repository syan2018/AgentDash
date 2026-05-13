/**
 * Tab 类型注册表
 *
 * 全局单例，管理所有可用的 Tab 类型描述符。
 * 内置类型在应用初始化时注册，插件/企业定制可在运行时追加注册。
 */

import type { ComponentType, ReactNode } from "react";

// ─── Tab URI ────────────────────────────────────────────

/** Tab 实例的 URI 标识，使用自定义 scheme 如 `canvas://{id}`, `vfs://{ref}/{mount}` */
export type TabURI = string;

// ─── TabTypeDescriptor ──────────────────────────────────

/** Tab 内容区渲染时接收的 props */
export interface TabContentRenderProps {
  uri: string;
  tabId: string;
  sessionId: string | null;
  isActive: boolean;
}

/** Tab 类型描述符 — 插件注册的核心接口 */
export interface TabTypeDescriptor {
  /** 类型唯一标识，如 "canvas", "vfs", "terminal" */
  typeId: string;
  /** 显示名称 */
  label: string;
  /** Tab 栏和菜单中的图标 */
  icon: ComponentType<{ className?: string }>;
  /** 是否允许多实例同时打开 */
  allowMultiple: boolean;
  /** 钉选 Tab：始终存在，不可关闭 */
  pinned: boolean;

  /** 渲染 Tab 内容区域 */
  renderContent: (props: TabContentRenderProps) => ReactNode;

  /** 从 URI 解析出可读标题 */
  resolveTitle: (uri: string) => string;
  /** 从 URI 解析出结构化参数 */
  parseUri: (uri: string) => Record<string, string> | null;
  /** 从参数构建 URI */
  buildUri: (params: Record<string, string>) => string;

  /** "+" 菜单直接创建时使用的默认 URI */
  defaultUri?: string;
  /** "+" 菜单中的排序权重（越小越靠前） */
  menuOrder?: number;
}

// ─── TabInstance ─────────────────────────────────────────

/** Tab 实例 — 运行时打开的每个 Tab */
export interface TabInstance {
  /** 唯一实例 ID */
  id: string;
  /** 引用 TabTypeDescriptor.typeId */
  typeId: string;
  /** 标识此 Tab 目标的 URI */
  uri: string;
  /** 显示标题 */
  title: string;
  /** 是否为钉选 Tab */
  pinned: boolean;
}

// ─── 持久化格式 ─────────────────────────────────────────

/** 存入后端 session meta 的 Tab 布局 */
export interface SessionTabLayout {
  tabs: Array<{
    type_id: string;
    uri: string;
    title: string;
    pinned: boolean;
  }>;
  active_tab_uri: string | null;
}

// ─── Registry ───────────────────────────────────────────

class TabTypeRegistry {
  private types = new Map<string, TabTypeDescriptor>();
  private listeners = new Set<() => void>();

  register(descriptor: TabTypeDescriptor): void {
    this.types.set(descriptor.typeId, descriptor);
    this.notify();
  }

  unregister(typeId: string): void {
    this.types.delete(typeId);
    this.notify();
  }

  getType(typeId: string): TabTypeDescriptor | undefined {
    return this.types.get(typeId);
  }

  /** 返回所有已注册类型（按 menuOrder 排序） */
  listTypes(): TabTypeDescriptor[] {
    return [...this.types.values()].sort(
      (a, b) => (a.menuOrder ?? 100) - (b.menuOrder ?? 100),
    );
  }

  /** 返回可通过 "+" 菜单创建的类型（排除 pinned） */
  listCreatableTypes(): TabTypeDescriptor[] {
    return this.listTypes().filter((t) => !t.pinned);
  }

  /** 订阅注册表变更（用于 React 集成） */
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** 当前快照版本号（用于 useSyncExternalStore） */
  getSnapshot(): TabTypeDescriptor[] {
    return this.listTypes();
  }

  private notify(): void {
    for (const listener of this.listeners) {
      listener();
    }
  }
}

/** 全局注册表单例 */
export const tabTypeRegistry = new TabTypeRegistry();
