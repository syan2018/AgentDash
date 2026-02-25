/**
 * ACP (Agent Client Protocol) 会话功能模块
 *
 * 提供基于 ACP 协议的 Agent 会话绘制功能
 *
 * @example
 * ```tsx
 * import { AcpSessionList, useAcpSession } from "./features/acp-session";
 *
 * function MyComponent() {
 *   return <AcpSessionList sessionId="session-123" />;
 * }
 * ```
 */

// 模型层
export * from "./model";

// UI 层
export * from "./ui";
