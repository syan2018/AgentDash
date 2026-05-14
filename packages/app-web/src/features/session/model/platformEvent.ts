/**
 * Backbone Platform 事件提取工具
 *
 * 统一从 BackboneEvent::Platform 提取展示层关心的
 * event type / message / data，避免散落在 UI 组件里重复判断。
 */

import type { BackboneEvent, PlatformEvent } from "../../../generated/backbone-protocol";

export function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

/** 从 PlatformEvent 提取可渲染事件类型。 */
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

/** 从 PlatformEvent 提取可渲染数据体。 */
export function extractPlatformEventData(event: BackboneEvent): Record<string, unknown> | null {
  if (event.type !== "platform") return null;
  const platform: PlatformEvent = event.payload;

  if (platform.kind === "executor_session_bound") {
    return { executor_session_id: platform.data.executor_session_id };
  }

  if (platform.kind === "hook_trace") {
    const traceData = platform.data.data;
    if (traceData) {
      return {
        ...traceData,
        event_type: platform.data.eventType ?? null,
      };
    }
    return {
      event_type: platform.data.eventType ?? null,
      message: platform.data.message ?? null,
    };
  }

  if (platform.kind === "session_meta_update") {
    const value = platform.data.value;
    if (isRecord(value)) return value;
    if (typeof value === "string") return { message: value };
    return null;
  }

  return null;
}

/** 从 PlatformEvent 提取可渲染 message。 */
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
