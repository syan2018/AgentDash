/**
 * BackboneEvent / PlatformEvent 元信息提取工具
 *
 * 在新的 BackboneEnvelope 协议下，元信息不再嵌套在 _meta.agentdash 内，
 * 而是直接在 envelope 的 trace/source 字段和 PlatformEvent 的结构化 payload 中。
 */

import type { BackboneEvent, PlatformEvent } from "../../../generated/backbone-protocol";

export function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

/**
 * 从 PlatformEvent::SessionMetaUpdate 的 value 中提取 event.type 字段。
 * 对应旧的 AgentDashMetaV1.event.type。
 */
export function extractPlatformEventType(event: BackboneEvent): string | null {
  if (event.type !== "platform") return null;
  const platform: PlatformEvent = event.payload;

  if (platform.kind === "executor_session_bound") return "executor_session_bound";
  if (platform.kind === "hook_trace") return "hook_event";

  if (platform.kind === "session_meta_update") {
    return platform.data.key;
  }

  return null;
}

/**
 * 从 PlatformEvent::SessionMetaUpdate 的 value 中提取事件数据。
 * 对应旧的 AgentDashMetaV1.event.data。
 */
export function extractPlatformEventData(event: BackboneEvent): Record<string, unknown> | null {
  if (event.type !== "platform") return null;
  const platform: PlatformEvent = event.payload;

  if (platform.kind === "executor_session_bound") {
    return { executor_session_id: platform.data.executor_session_id };
  }

  if (platform.kind === "hook_trace") {
    return platform.data as unknown as Record<string, unknown>;
  }

  if (platform.kind === "session_meta_update") {
    const value = platform.data.value;
    if (isRecord(value)) return value;
    if (typeof value === "string") return { message: value };
    return null;
  }

  return null;
}

/**
 * 从 PlatformEvent::SessionMetaUpdate 中提取 event.message。
 */
export function extractPlatformEventMessage(event: BackboneEvent): string | null {
  if (event.type !== "platform") return null;
  const platform: PlatformEvent = event.payload;

  if (platform.kind === "hook_trace") {
    return platform.data.message ?? null;
  }

  if (platform.kind === "session_meta_update") {
    const value = platform.data.value;
    if (isRecord(value) && typeof value.message === "string") {
      return value.message;
    }
    return null;
  }

  return null;
}

/**
 * 从 PlatformEvent::HookTrace 中提取 hook 事件信息。
 */
export function extractHookTraceInfo(event: BackboneEvent): {
  eventType: string | null;
  message: string | null;
  data: unknown;
} | null {
  if (event.type !== "platform") return null;
  const platform: PlatformEvent = event.payload;
  if (platform.kind !== "hook_trace") return null;

  return {
    eventType: platform.data.eventType ?? null,
    message: platform.data.message ?? null,
    data: platform.data.data ?? null,
  };
}
