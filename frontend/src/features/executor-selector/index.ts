/**
 * 执行器/模型选择器功能模块
 *
 * 提供执行器发现、配置管理和选择器 UI。
 *
 * @example
 * ```tsx
 * import { useExecutorDiscovery, useExecutorConfig, ExecutorSelector } from "../features/executor-selector";
 *
 * function MyComponent() {
 *   const discovery = useExecutorDiscovery();
 *   const config = useExecutorConfig();
 *   return <ExecutorSelector {...discovery} {...config} />;
 * }
 * ```
 */

export * from "./model";
export * from "./ui";
