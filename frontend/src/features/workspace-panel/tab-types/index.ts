/**
 * 内置 Tab 类型注册
 *
 * 在应用初始化时调用 registerBuiltinTabTypes() 完成内置类型注册。
 */

import { tabTypeRegistry } from "../tab-type-registry";
import { contextTabType } from "./context-tab";
import { inspectorTabType } from "./inspector-tab";
import { canvasTabType } from "./canvas-tab";
import { vfsTabType } from "./vfs-tab";
import { terminalTabType } from "./terminal-tab";

let registered = false;

export function registerBuiltinTabTypes(): void {
  if (registered) return;
  registered = true;

  tabTypeRegistry.register(contextTabType);
  tabTypeRegistry.register(inspectorTabType);
  tabTypeRegistry.register(canvasTabType);
  tabTypeRegistry.register(vfsTabType);
  tabTypeRegistry.register(terminalTabType);
}

export { contextTabType } from "./context-tab";
export { inspectorTabType } from "./inspector-tab";
export { canvasTabType } from "./canvas-tab";
export { vfsTabType } from "./vfs-tab";
export { terminalTabType } from "./terminal-tab";
