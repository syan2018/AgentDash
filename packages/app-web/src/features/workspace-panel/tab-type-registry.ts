/**
 * Tab 类型注册表
 *
 * 全局单例，管理所有可用的 Tab 类型描述符。
 * 内置类型在应用初始化时注册，插件/企业定制可在运行时追加注册。
 */

import { useSyncExternalStore } from "react";
import type { TabTypeDescriptor } from "../workspace-runtime";
export type {
  TabContentRenderProps,
  TabInstance,
  TabTypeDescriptor,
  TabURI,
  WorkspaceTabLayout,
} from "../workspace-runtime";

// ─── Registry ───────────────────────────────────────────

class TabTypeRegistry {
  private types = new Map<string, TabTypeDescriptor>();
  private contributionTypeIds = new Map<string, Set<string>>();
  private listeners = new Set<() => void>();
  private snapshot: TabTypeDescriptor[] = [];

  register(descriptor: TabTypeDescriptor): void {
    this.types.set(descriptor.typeId, descriptor);
    this.commit();
  }

  registerContribution(ownerKey: string, descriptors: TabTypeDescriptor[]): void {
    this.removeContribution(ownerKey);
    const typeIds = new Set<string>();
    for (const descriptor of descriptors) {
      this.types.set(descriptor.typeId, descriptor);
      typeIds.add(descriptor.typeId);
    }
    if (typeIds.size > 0) {
      this.contributionTypeIds.set(ownerKey, typeIds);
    }
    this.commit();
  }

  unregisterContribution(ownerKey: string): void {
    this.removeContribution(ownerKey);
    this.commit();
  }

  unregister(typeId: string): void {
    this.types.delete(typeId);
    for (const typeIds of this.contributionTypeIds.values()) {
      typeIds.delete(typeId);
    }
    this.commit();
  }

  getType(typeId: string): TabTypeDescriptor | undefined {
    return this.types.get(typeId);
  }

  /** 返回所有已注册类型（按 menuOrder 排序） */
  listTypes(): TabTypeDescriptor[] {
    return this.snapshot;
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
    return this.snapshot;
  }

  private removeContribution(ownerKey: string): void {
    const previous = this.contributionTypeIds.get(ownerKey);
    if (!previous) return;
    for (const typeId of previous) {
      this.types.delete(typeId);
    }
    this.contributionTypeIds.delete(ownerKey);
  }

  private commit(): void {
    this.snapshot = [...this.types.values()].sort(
      (a, b) => (a.menuOrder ?? 100) - (b.menuOrder ?? 100),
    );
    this.notify();
  }

  private notify(): void {
    for (const listener of this.listeners) {
      listener();
    }
  }
}

/** 全局注册表单例 */
export const tabTypeRegistry = new TabTypeRegistry();

export function useTabTypeRegistrySnapshot(): TabTypeDescriptor[] {
  return useSyncExternalStore(
    (listener) => tabTypeRegistry.subscribe(listener),
    () => tabTypeRegistry.getSnapshot(),
    () => tabTypeRegistry.getSnapshot(),
  );
}
