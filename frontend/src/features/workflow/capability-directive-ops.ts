// Capability Directive 序列操作 —— 对应后端 `reduce_capability_directives` 的前端视图层工具。
//
// 设计原则：
// - 每条 UI 动作只产出一条 Directive（能力级 Add/Remove 或工具级 Add/Remove）
// - 归约规则（后来者胜）由后端 slot 归约实现；前端仅维护显式的 directive 序列
// - 本模块聚焦序列的 canonicalize / 查询 / 增删，不重复实现 slot 归约

import type { CapabilityDirective } from "../../types/workflow";
import {
  directiveKind,
  directivePath,
  parseCapabilityPath,
  toQualifiedString,
} from "../../types/workflow";

/** 判断两条 directive 是否完全相同（同 kind + 同 path）。 */
export function directiveEquals(a: CapabilityDirective, b: CapabilityDirective): boolean {
  return directiveKind(a) === directiveKind(b) && directivePath(a) === directivePath(b);
}

/** 序列中是否包含指定 directive。 */
export function hasDirective(
  list: CapabilityDirective[],
  target: CapabilityDirective,
): boolean {
  return list.some((d) => directiveEquals(d, target));
}

/**
 * 去重 —— 保留每种 (kind, path) 组合的最后一次出现位置。
 *
 * 后端 slot 归约逻辑是「后来者胜」，所以保留最后出现的那条语义最接近后端计算结果。
 * 这里保留最后一次出现而非第一次，避免 UI 显式反复 add/remove 时产生陈旧残留。
 */
export function normalizeDirectives(list: CapabilityDirective[]): CapabilityDirective[] {
  const seen = new Map<string, number>();
  list.forEach((d, idx) => {
    const key = `${directiveKind(d)}:${directivePath(d)}`;
    seen.set(key, idx);
  });
  return list.filter((d, idx) => {
    const key = `${directiveKind(d)}:${directivePath(d)}`;
    return seen.get(key) === idx;
  });
}

/** 追加一条 directive（若已存在则原样返回）。 */
export function addDirective(
  list: CapabilityDirective[],
  directive: CapabilityDirective,
): CapabilityDirective[] {
  if (hasDirective(list, directive)) return list;
  return [...list, directive];
}

/** 删除所有与 target 相同的 directive。 */
export function removeDirective(
  list: CapabilityDirective[],
  target: CapabilityDirective,
): CapabilityDirective[] {
  return list.filter((d) => !directiveEquals(d, target));
}

/**
 * 序列中是否存在「屏蔽整个能力」的指令（`Remove(cap)` 短 path）。
 *
 * 注意：本函数只检查显式 directive；不代理后端 slot 归约的复杂规则
 * （如 `Add(cap)` 后的 `Remove(cap)` 会覆盖为 Blocked）。UI 层按显式
 * 声明判断即可，因为同一 UI 不会同时发出 Add + Remove 两条相反指令。
 */
export function capabilityBlockedByWorkflow(
  list: CapabilityDirective[],
  capability: string,
): boolean {
  return list.some((d) => {
    if (directiveKind(d) !== "remove") return false;
    try {
      const path = parseCapabilityPath(directivePath(d));
      return path.capability === capability && path.tool === null;
    } catch {
      return false;
    }
  });
}

/**
 * 序列中是否存在「屏蔽某个工具」的指令（`Remove(cap::tool)` 长 path）。
 */
export function toolBlockedByWorkflow(
  list: CapabilityDirective[],
  capability: string,
  tool: string,
): boolean {
  return list.some((d) => {
    if (directiveKind(d) !== "remove") return false;
    try {
      const path = parseCapabilityPath(directivePath(d));
      return path.capability === capability && path.tool === tool;
    } catch {
      return false;
    }
  });
}

/**
 * 序列中是否存在「启用整个能力」的指令（`Add(cap)` 短 path）。
 * 用于区分「基线追加能力」vs「仅工具白名单」。
 */
export function capabilityExplicitlyAdded(
  list: CapabilityDirective[],
  capability: string,
): boolean {
  return list.some((d) => {
    if (directiveKind(d) !== "add") return false;
    try {
      const path = parseCapabilityPath(directivePath(d));
      return path.capability === capability && path.tool === null;
    } catch {
      return false;
    }
  });
}

/** 收集当前序列中显式 Add 的全部 capability key（短 path + 长 path 合并去重）。 */
export function listDeclaredCapabilityKeys(list: CapabilityDirective[]): string[] {
  const keys = new Set<string>();
  for (const d of list) {
    if (directiveKind(d) !== "add") continue;
    try {
      const path = parseCapabilityPath(directivePath(d));
      keys.add(path.capability);
    } catch {
      // 忽略非法 directive
    }
  }
  return Array.from(keys);
}

/** 快捷构造：能力级 Add。 */
export function makeAddCapability(capability: string): CapabilityDirective {
  return { add: toQualifiedString({ capability, tool: null }) };
}

/** 快捷构造：能力级 Remove。 */
export function makeRemoveCapability(capability: string): CapabilityDirective {
  return { remove: toQualifiedString({ capability, tool: null }) };
}

/** 快捷构造：工具级 Add（长 path）。 */
export function makeAddTool(capability: string, tool: string): CapabilityDirective {
  return { add: toQualifiedString({ capability, tool }) };
}

/** 快捷构造：工具级 Remove（长 path）。 */
export function makeRemoveTool(capability: string, tool: string): CapabilityDirective {
  return { remove: toQualifiedString({ capability, tool }) };
}
