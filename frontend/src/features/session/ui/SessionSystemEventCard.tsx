/**
 * 系统事件卡片
 *
 * 渲染 platform 事件中的系统/hook/companion 事件。
 * badge 是唯一染色点，卡片外框和文字保持中性色。
 */

import type { ReactNode } from "react";
import type { BackboneEvent, HookTraceData } from "../../../generated/backbone-protocol";
import {
  extractPlatformEventType,
  extractPlatformEventData,
  extractPlatformEventMessage,
  isRecord,
} from "../model/platformEvent";
import { EventStripCard, EventFullCard } from "./EventCards";
import { AcpCompanionRequestCard } from "./SessionCompanionRequestCard";
import { getDebugPrefs } from "../../../hooks/use-debug-prefs";

export interface AcpSystemEventCardProps {
  event: BackboneEvent;
  sessionId?: string;
}

// ─── 类型定义 ─────────────────────────────────────────────────────────────────

interface HookEventData {
  trigger?: string;
  decision?: string;
  sequence?: number;
  revision?: number;
  tool_name?: string | null;
  tool_call_id?: string | null;
  subagent_type?: string | null;
  matched_rule_keys?: string[];
  refresh_snapshot?: boolean;
  block_reason?: string | null;
  completion?: {
    mode?: string;
    satisfied?: boolean;
    advanced?: boolean;
    reason?: string;
  } | null;
  diagnostic_codes?: string[];
  diagnostics?: Array<{
    code?: string;
    message?: string;
    summary?: string;
    detail?: string | null;
    source_summary?: string[];
  }>;
  injections?: Array<{
    slot?: string;
    source?: string;
    content?: string;
  }>;
  code?: string;
  severity?: string;
}

// ─── badge 样式 ──────────────────────────────────────────────────────────────

const SEVERITY_BADGE: Record<string, string> = {
  error:   "border-destructive/25 bg-destructive/10 text-destructive",
  warning: "border-warning/25 bg-warning/10 text-warning",
  success: "border-success/25 bg-success/10 text-success",
  info:    "border-border bg-secondary/50 text-muted-foreground",
};
const DEFAULT_BADGE = SEVERITY_BADGE.info!;

// ─── 文案映射 ─────────────────────────────────────────────────────────────────

const EVENT_TYPE_LABELS: Record<string, string> = {
  executor_session_bound:          "会话已绑定",
  turn_interrupted:                "执行已中断",
  turn_failed:                     "执行失败",
  system_message:                  "系统消息",
  error:                           "错误",
  user_feedback:                   "用户反馈",
  user_answered_questions:         "用户回答",
  permission_denied:               "权限拒绝",
  approval_requested:              "等待审批",
  approval_resolved:               "审批结果",
  hook_action_resolved:            "事项已结案",
  companion_dispatch_registered:   "协作 Agent 已派发",
  companion_result_available:      "协作结果可用",
  companion_result_returned:       "协作结果已回传",
  companion_human_request:         "Agent 请求用户回应",
  companion_human_response:        "用户已回应 Agent",
  companion_review_request:        "协作 Agent 提审",
  canvas_presented:                "Canvas 已展示",
  hook_event:                      "流程事件",
  hook_trace:                      "流程事件",
};

const EVENT_TYPE_DEFAULT_MESSAGES: Record<string, string> = {
  executor_session_bound:          "已绑定到执行会话",
  turn_interrupted:                "本轮执行已中断",
  turn_failed:                     "本轮执行失败",
  system_message:                  "系统消息",
  error:                           "执行出现错误",
  user_feedback:                   "已收到用户反馈",
  user_answered_questions:         "已收到用户回答",
  permission_denied:               "操作被权限策略拒绝",
  approval_requested:              "当前工具调用正在等待审批",
  approval_resolved:               "当前工具调用审批已完成",
  hook_action_resolved:            "一项流程干预已被结案",
  companion_dispatch_registered:   "已注册协作 Agent 派发",
  companion_result_available:      "协作 Agent 已回传结果",
  companion_result_returned:       "协作结果已回传到当前会话",
  companion_human_request:         "Agent 正在等待用户回应",
  companion_human_response:        "已将用户回应写入当前会话",
  companion_review_request:        "协作 Agent 请求审阅",
  canvas_presented:                "已请求打开 Canvas 面板",
  hook_event:                      "流程产生新事件",
  hook_trace:                      "流程产生新事件",
};

// ─── Hook 决策分类 ────────────────────────────────────────────────────────────

const HIGH_PRIORITY_HOOK_DECISIONS = new Set(["deny", "ask", "rewrite", "continue"]);

const SUBSTANTIVE_DECISIONS = new Set([
  "deny", "ask", "rewrite", "continue",
  "context_injected", "steering_injected", "step_advanced",
]);

const NON_SUBSTANTIVE_DIAGNOSTIC_CODES = new Set([
  "session_binding_found",
  "active_workflow_resolved",
]);

type HookDiagnosticData = NonNullable<HookEventData["diagnostics"]>[number];

function resolveDiagnosticSummary(diagnostic: HookDiagnosticData): string | null {
  const summary = typeof diagnostic.summary === "string" ? diagnostic.summary.trim() : "";
  if (summary.length > 0) return summary;
  const message = typeof diagnostic.message === "string" ? diagnostic.message.trim() : "";
  return message.length > 0 ? message : null;
}

function isSubstantiveDiagnostic(diagnostic: HookDiagnosticData): boolean {
  const code = typeof diagnostic.code === "string" ? diagnostic.code : "";
  if (code && NON_SUBSTANTIVE_DIAGNOSTIC_CODES.has(code)) {
    return false;
  }
  const summary = resolveDiagnosticSummary(diagnostic);
  const detail = typeof diagnostic.detail === "string" ? diagnostic.detail.trim() : "";
  return summary !== null || detail.length > 0 || code.length > 0;
}

function isHookEventSubstantive(
  decision: string | null | undefined,
  hookData: HookEventData | null,
): boolean {
  if (decision && SUBSTANTIVE_DECISIONS.has(decision)) return true;
  if (hookData?.block_reason) return true;
  if (hookData?.completion) return true;
  if ((hookData?.diagnostics ?? []).some(isSubstantiveDiagnostic)) return true;
  if (hookData?.injections?.length) return true;
  return false;
}

function extractHookDecision(code: string | null | undefined): string | null {
  if (!code) return null;
  const parts = code.split(":");
  return parts.length >= 3 ? (parts[2] ?? null) : null;
}

function isHighPriorityHookEvent(
  hookData: HookEventData | null,
  decision: string | null | undefined,
): boolean {
  if (hookData?.block_reason) return true;
  return typeof decision === "string" && HIGH_PRIORITY_HOOK_DECISIONS.has(decision);
}

// ─── 主组件 ───────────────────────────────────────────────────────────────────

export function AcpSystemEventCard({ event, sessionId }: AcpSystemEventCardProps) {
  if (event.type !== "platform") return null;

  const eventType = extractPlatformEventType(event) ?? "system";
  const eventData = extractPlatformEventData(event);
  const eventMessage = extractPlatformEventMessage(event);

  // ── companion_human_request → 交互卡片 ──
  if (eventType === "companion_human_request") {
    return <AcpCompanionRequestCard event={event} sessionId={sessionId} />;
  }

  // ── hook_trace / hook_event → hook 卡片逻辑 ──
  const isHook = eventType === "hook_event" || eventType === "hook_trace";
  const hookData = isHook ? extractHookEventData(event, eventData) : null;

  if (isHook) {
    const code = hookData?.code ?? null;
    const decision = extractHookDecision(code) ?? hookData?.decision ?? null;
    const hasSubstance = isHookEventSubstantive(decision, hookData);
    if (!hasSubstance && !getDebugPrefs().hookVerbose) {
      return null;
    }

    const severity = hookData?.severity ?? "info";
    const badge = hasSubstance ? (SEVERITY_BADGE[severity] ?? DEFAULT_BADGE) : SEVERITY_BADGE.info!;
    const token = resolveDecisionToken(decision);

    const { completionLine, diagnostics } = buildHookExpandData(hookData);
    const diagBody = buildDiagnosticsNode(diagnostics);

    const verboseWrapper = (node: ReactNode) =>
      hasSubstance ? node : <div className="opacity-50">{node}</div>;

    if (isHighPriorityHookEvent(hookData, decision)) {
      const detailLines: string[] = [];
      if (hookData?.block_reason) detailLines.push(`阻塞原因：${hookData.block_reason}`);
      if (completionLine) detailLines.push(completionLine);

      return verboseWrapper(
        <EventFullCard
          badgeToken={token}
          badgeClass={badge}
          subtitle={resolveDecisionLabel(decision, hookData)}
          message={eventMessage ?? EVENT_TYPE_DEFAULT_MESSAGES.hook_event ?? "流程干预"}
          detailLines={detailLines}
          debugChips={buildHookDebugChips(hookData, null)}
          debugLines={buildHookDebugMeta(hookData)}
          debugBody={diagBody}
        />
      );
    }

    const expandSections: Array<{ title?: string; lines: string[] }> = [];
    if (completionLine) {
      expandSections.push({ lines: [completionLine] });
    }
    if (hookData?.injections?.length) {
      for (const inj of hookData.injections) {
        const title = inj.source
          ? `${inj.slot ?? "injection"} (${inj.source})`
          : (inj.slot ?? "injection");
        const lines = (inj.content ?? "").split("\n").filter((l) => l.trim().length > 0);
        if (lines.length > 0) {
          expandSections.push({ title, lines });
        }
      }
    }
    const expandContent = expandSections.length > 0
      ? { sections: expandSections }
      : undefined;
    const rightHint = hookData?.completion
      ? (hookData.completion.satisfied ? "已满足" : "未满足")
      : hookData?.injections?.length
        ? `${hookData.injections.length} 项注入`
        : hookData?.matched_rule_keys?.length
          ? `${hookData.matched_rule_keys.length} 条规则`
          : null;
    return verboseWrapper(
      <EventStripCard
        badgeToken={token}
        badgeClass={badge}
        label={resolveStripLabel(decision, hookData)}
        rightHint={rightHint}
        expandContent={expandContent}
        bodyExtra={diagBody}
      />
    );
  }

  // ── 通用系统事件 ──
  const severity = "info";
  const badge = SEVERITY_BADGE[severity] ?? DEFAULT_BADGE;
  const typeLabel = EVENT_TYPE_LABELS[eventType] ?? eventType;
  const message = eventMessage ?? EVENT_TYPE_DEFAULT_MESSAGES[eventType] ?? "系统事件";
  const detailLines = buildGenericDetailLines(eventType, eventData);
  const extraData = formatExtraData(eventData);
  return (
    <EventFullCard
      badgeToken={typeLabel}
      badgeClass={badge}
      message={message}
      detailLines={detailLines}
      debugRaw={extraData ?? undefined}
    />
  );
}

// ─── 辅助函数 ─────────────────────────────────────────────────────────────────

function resolveDecisionToken(decision: string | null | undefined): string {
  switch (decision) {
    case "context_injected":  return "CTX";
    case "steering_injected": return "STEER";
    case "step_advanced":     return "STEP";
    case "continue":          return "HOLD";
    case "deny":              return "DENY";
    case "ask":               return "ASK";
    case "rewrite":           return "REWRITE";
    default:                  return "HOOK";
  }
}

function resolveDecisionLabel(
  decision: string | null | undefined,
  hookData: HookEventData | null,
): string {
  switch (decision) {
    case "deny":    return "工具调用已阻止";
    case "ask":     return "等待审批";
    case "rewrite": return "参数已改写";
    case "continue":
      return hookData?.completion?.satisfied
        ? "条件满足，仍需处理约束"
        : "阻止结束，要求继续执行";
    default: return decision ?? "hook";
  }
}

function resolveStripLabel(
  decision: string | null,
  hookData: HookEventData | null,
): string {
  switch (decision) {
    case "context_injected": {
      const ruleCount = hookData?.matched_rule_keys?.length ?? 0;
      const injCount = hookData?.injections?.length ?? 0;
      const parts: string[] = [];
      if (ruleCount > 0) parts.push(`${ruleCount} 条规则`);
      if (injCount > 0) parts.push(`${injCount} 项注入`);
      return parts.length > 0
        ? `已注入动态上下文（${parts.join("，")}）`
        : "已注入动态上下文";
    }
    case "steering_injected": return "已追加流程约束（steering）";
    case "step_advanced":     return "Workflow Step 已推进";
    default:
      return hookData?.completion
        ? `流程决策：${decision ?? "unknown"}`
        : `Hook 决策：${decision ?? "unknown"}`;
  }
}

function buildHookExpandData(hookData: HookEventData | null): {
  completionLine: string | null;
  diagnostics: Array<{ code?: string; summary: string; detail?: string | null }>;
} {
  if (!hookData) return { completionLine: null, diagnostics: [] };

  let completionLine: string | null = null;
  if (hookData.completion) {
    const { satisfied, advanced, reason, mode } = hookData.completion;
    const statusText = satisfied ? "已满足" : "未满足";
    const modeText = mode ? ` [${mode}]` : "";
    const advancedText = advanced ? "；已推进阶段" : "";
    const reasonText = reason ? `；${reason}` : "";
    completionLine = `完成判定：${statusText}${modeText}${advancedText}${reasonText}`;
  }

  const diagnostics: Array<{ code?: string; summary: string; detail?: string | null }> = [];
  for (const diagnostic of hookData.diagnostics ?? []) {
    if (!isSubstantiveDiagnostic(diagnostic)) continue;
    const summary =
      resolveDiagnosticSummary(diagnostic) ??
      (typeof diagnostic.code === "string" && diagnostic.code.trim().length > 0
        ? diagnostic.code
        : null);
    if (!summary) continue;
    const detail = typeof diagnostic.detail === "string" ? diagnostic.detail.trim() : "";
    diagnostics.push({
      code: diagnostic.code,
      summary,
      detail: detail.length > 0 ? detail : null,
    });
  }

  return { completionLine, diagnostics };
}

function buildDiagnosticsNode(
  diagnostics: Array<{ code?: string; summary: string; detail?: string | null }>,
): ReactNode {
  if (diagnostics.length === 0) return null;
  return (
    <div className="space-y-1">
      <p className="text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60">诊断</p>
      {diagnostics.map((d, i) => (
        <p key={i} className="flex flex-wrap items-baseline gap-1.5 text-xs leading-5 text-foreground/75">
          {d.code && (
            <span className="inline-flex rounded-[4px] border border-border bg-secondary/60 px-1 py-0 font-mono text-[9px] text-muted-foreground/70">
              {d.code}
            </span>
          )}
          <span>{d.summary}</span>
          {d.detail && <span className="text-muted-foreground/60">；{d.detail}</span>}
        </p>
      ))}
    </div>
  );
}

function buildHookDebugChips(
  hookData: HookEventData | null,
  turnId: string | null,
): string[] {
  const chips: string[] = [];
  if (turnId) chips.push(`turn: ${turnId.slice(0, 8)}`);
  if (!hookData) return chips;
  if (hookData.trigger) chips.push(`trigger: ${hookData.trigger}`);
  if (hookData.decision) chips.push(`decision: ${hookData.decision}`);
  if (hookData.completion?.mode) chips.push(`completion: ${hookData.completion.mode}`);
  if (typeof hookData.revision === "number") chips.push(`rev: ${hookData.revision}`);
  if (typeof hookData.sequence === "number") chips.push(`seq: #${hookData.sequence}`);
  if (hookData.tool_name) chips.push(`tool: ${hookData.tool_name}`);
  if (hookData.subagent_type) chips.push(`subagent: ${hookData.subagent_type}`);
  if (hookData.matched_rule_keys?.length) chips.push(`rules: ${hookData.matched_rule_keys.length}`);
  if (hookData.diagnostic_codes?.length) chips.push(`diag: ${hookData.diagnostic_codes.length}`);
  return chips;
}

function buildHookDebugMeta(hookData: HookEventData | null): string[] {
  if (!hookData) return [];
  const lines: string[] = [];
  if (hookData.matched_rule_keys?.length) {
    lines.push(`命中规则：${hookData.matched_rule_keys.join("，")}`);
  }
  if (hookData.tool_call_id) {
    lines.push(`tool_call_id：${hookData.tool_call_id}`);
  }
  return lines;
}

function buildGenericDetailLines(eventType: string, data: Record<string, unknown> | null): string[] {
  if (!data) return [];
  const lines: string[] = [];

  if (eventType === "approval_requested" || eventType === "approval_resolved") {
    const toolName = typeof data.tool_name === "string" ? data.tool_name : null;
    const reason = typeof data.reason === "string" ? data.reason : null;
    const approved = typeof data.approved === "boolean" ? data.approved : null;
    if (toolName) lines.push(`工具：${toolName}`);
    if (approved != null) lines.push(`审批结果：${approved ? "已批准" : "已拒绝"}`);
    if (reason) lines.push(`原因：${reason}`);
    return lines;
  }

  if (eventType === "hook_action_resolved") {
    const summary = typeof data.summary === "string" ? data.summary : null;
    const resolutionNote = typeof data.resolution_note === "string" ? data.resolution_note : null;
    if (summary) lines.push(`摘要：${summary}`);
    if (resolutionNote) lines.push(`说明：${resolutionNote}`);
    return lines;
  }

  if (eventType === "companion_dispatch_registered" ||
      eventType === "companion_result_available" ||
      eventType === "companion_result_returned") {
    const label = typeof data.companion_label === "string" ? data.companion_label : null;
    const agentName = typeof data.agent_name === "string" ? data.agent_name : null;
    const summary = typeof data.summary === "string" ? data.summary : null;
    const status = typeof data.status === "string" ? data.status : null;
    if (agentName) lines.push(`协作 Agent：${agentName}`);
    else if (label) lines.push(`协作 Agent：${label}`);
    if (status) lines.push(`状态：${status}`);
    if (summary) lines.push(`摘要：${summary}`);
    return lines;
  }

  if (eventType === "companion_review_request") {
    const label = typeof data.companion_label === "string" ? data.companion_label : null;
    const prompt = typeof data.prompt === "string" ? data.prompt : null;
    const wait = typeof data.wait === "boolean" ? data.wait : null;
    if (label) lines.push(`协作 Agent：${label}`);
    if (prompt) lines.push(`提审内容：${prompt}`);
    if (wait != null) lines.push(`等待回应：${wait ? "是" : "否"}`);
    return lines;
  }

  if (eventType === "companion_human_response") {
    const status = typeof data.status === "string" ? data.status : null;
    const summary = typeof data.summary === "string" ? data.summary : null;
    const resumed = typeof data.resumed_waiting_tool === "boolean" ? data.resumed_waiting_tool : null;
    if (status) lines.push(`状态：${status}`);
    if (summary) lines.push(`摘要：${summary}`);
    if (resumed != null) lines.push(`挂起工具：${resumed ? "已恢复" : "未挂起 / 已离线"}`);
    return lines;
  }

  if (eventType === "executor_session_bound") {
    const esId = typeof data.executor_session_id === "string" ? data.executor_session_id : null;
    if (esId) lines.push(`执行器会话：${esId.slice(0, 12)}...`);
    return lines;
  }

  return lines;
}

function extractHookEventData(
  event: BackboneEvent,
  value: Record<string, unknown> | null,
): HookEventData | null {
  if (event.type === "platform" && event.payload.kind === "hook_trace") {
    const payload = event.payload.data;
    if (payload.data) {
      return extractHookEventDataFromTrace(payload.data, payload.eventType);
    }
  }
  return extractHookEventDataFromRecord(value);
}

function extractHookEventDataFromTrace(
  trace: HookTraceData,
  eventTypeCode: string | null,
): HookEventData {
  const sequence = readOptionalNumber(trace.sequence);
  const revision = readOptionalNumber(trace.revision);
  const matchedRuleKeys = trace.matched_rule_keys.filter((item): item is string => item.trim().length > 0);
  const diagnosticCodes = trace.diagnostic_codes.filter((item): item is string => item.trim().length > 0);
  const diagnostics = trace.diagnostics
    .map((item) => {
      const code = readOptionalString(item.code) ?? undefined;
      const message = readOptionalString(item.message) ?? undefined;
      if (!code && !message) return null;
      return { code, message };
    })
    .filter((item): item is NonNullable<typeof item> => item != null);
  const injections = trace.injections
    .map((item) => {
      const slot = readOptionalString(item.slot) ?? undefined;
      const source = readOptionalString(item.source) ?? undefined;
      const content = readOptionalString(item.content) ?? undefined;
      if (!slot && !source && !content) return null;
      return { slot, source, content };
    })
    .filter((item): item is NonNullable<typeof item> => item != null);

  const data: HookEventData = {
    trigger: trace.trigger,
    decision: trace.decision,
    severity: trace.severity,
    refresh_snapshot: trace.refresh_snapshot,
    tool_name: trace.tool_name,
    tool_call_id: trace.tool_call_id,
    subagent_type: trace.subagent_type,
    block_reason: trace.block_reason,
    completion: trace.completion
      ? {
        mode: trace.completion.mode,
        satisfied: trace.completion.satisfied,
        advanced: trace.completion.advanced,
        reason: trace.completion.reason,
      }
      : null,
    matched_rule_keys: matchedRuleKeys.length > 0 ? matchedRuleKeys : undefined,
    diagnostic_codes: diagnosticCodes.length > 0 ? diagnosticCodes : undefined,
    diagnostics: diagnostics.length > 0 ? diagnostics : undefined,
    injections: injections.length > 0 ? injections : undefined,
  };
  if (sequence != null) data.sequence = sequence;
  if (revision != null) data.revision = revision;
  if (eventTypeCode) data.code = eventTypeCode;
  if (!data.decision) {
    const inferred = extractHookDecision(data.code);
    if (inferred) data.decision = inferred;
  }
  return data;
}

function extractHookEventDataFromRecord(value: Record<string, unknown> | null): HookEventData | null {
  if (!isRecord(value)) return null;

  const data: HookEventData = {};
  const trigger = readOptionalString(value.trigger);
  if (trigger) data.trigger = trigger;

  const decision = readOptionalString(value.decision);
  if (decision) data.decision = decision;

  const sequence = readOptionalNumber(value.sequence);
  if (sequence != null) data.sequence = sequence;

  const revision = readOptionalNumber(value.revision);
  if (revision != null) data.revision = revision;

  const toolName = readNullableString(value.tool_name);
  if (toolName !== undefined) data.tool_name = toolName;

  const toolCallId = readNullableString(value.tool_call_id);
  if (toolCallId !== undefined) data.tool_call_id = toolCallId;

  const subagentType = readNullableString(value.subagent_type);
  if (subagentType !== undefined) data.subagent_type = subagentType;

  const matchedRuleKeys = readStringArray(value.matched_rule_keys);
  if (matchedRuleKeys) data.matched_rule_keys = matchedRuleKeys;

  const refreshSnapshot = readOptionalBoolean(value.refresh_snapshot);
  if (refreshSnapshot != null) data.refresh_snapshot = refreshSnapshot;

  const blockReason = readNullableString(value.block_reason);
  if (blockReason !== undefined) data.block_reason = blockReason;

  const completion = normalizeHookCompletion(value.completion);
  if (completion !== undefined) data.completion = completion;

  const diagnosticCodes = readStringArray(value.diagnostic_codes);
  if (diagnosticCodes) data.diagnostic_codes = diagnosticCodes;

  const diagnostics = normalizeHookDiagnostics(value.diagnostics);
  if (diagnostics) data.diagnostics = diagnostics;

  const injections = normalizeHookInjections(value.injections);
  if (injections) data.injections = injections;

  const code = readOptionalString(value.code);
  if (code) data.code = code;

  const eventTypeCode = readOptionalString(value.event_type ?? value.eventType);
  if (eventTypeCode && !data.code) data.code = eventTypeCode;

  const severity = readOptionalString(value.severity);
  if (severity) data.severity = severity;

  if (!data.decision) {
    const inferred = extractHookDecision(data.code);
    if (inferred) data.decision = inferred;
  }

  return Object.keys(data).length > 0 ? data : null;
}

function readOptionalString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function readNullableString(value: unknown): string | null | undefined {
  if (value == null) return null;
  if (typeof value !== "string") return undefined;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function readOptionalNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "bigint") {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

function readOptionalBoolean(value: unknown): boolean | null {
  if (typeof value !== "boolean") return null;
  return value;
}

function readStringArray(value: unknown): string[] | undefined {
  if (!Array.isArray(value)) return undefined;
  const normalized = value
    .map((item) => readOptionalString(item))
    .filter((item): item is string => item != null);
  return normalized.length > 0 ? normalized : undefined;
}

function normalizeHookCompletion(
  value: unknown,
): HookEventData["completion"] | undefined {
  if (value == null) return null;
  if (!isRecord(value)) return undefined;

  const mode = readOptionalString(value.mode) ?? undefined;
  const satisfied = readOptionalBoolean(value.satisfied) ?? undefined;
  const advanced = readOptionalBoolean(value.advanced) ?? undefined;
  const reason = readOptionalString(value.reason) ?? undefined;

  if (!mode && satisfied == null && advanced == null && !reason) {
    return null;
  }

  return {
    mode,
    satisfied,
    advanced,
    reason,
  };
}

function normalizeHookDiagnostics(
  value: unknown,
): HookEventData["diagnostics"] | undefined {
  if (!Array.isArray(value)) return undefined;
  const diagnostics = value
    .map((item) => {
      if (!isRecord(item)) return null;
      const code = readOptionalString(item.code) ?? undefined;
      const message = readOptionalString(item.message) ?? undefined;
      const summary = readOptionalString(item.summary) ?? undefined;
      const detail = readNullableString(item.detail);
      const sourceSummary = readStringArray(item.source_summary);

      if (!code && !message && !summary && detail == null && !sourceSummary) {
        return null;
      }
      return {
        code,
        message,
        summary,
        detail: detail ?? null,
        source_summary: sourceSummary,
      };
    })
    .filter((item): item is NonNullable<typeof item> => item != null);

  return diagnostics.length > 0 ? diagnostics : undefined;
}

function normalizeHookInjections(
  value: unknown,
): HookEventData["injections"] | undefined {
  if (!Array.isArray(value)) return undefined;
  const injections = value
    .map((item) => {
      if (!isRecord(item)) return null;
      const slot = readOptionalString(item.slot) ?? undefined;
      const source = readOptionalString(item.source) ?? undefined;
      const content = readOptionalString(item.content) ?? undefined;
      if (!slot && !source && !content) return null;
      return { slot, source, content };
    })
    .filter((item): item is NonNullable<typeof item> => item != null);
  return injections.length > 0 ? injections : undefined;
}

function formatExtraData(value: unknown): string | null {
  if (value == null) return null;
  if (typeof value === "string") return value;
  try { return JSON.stringify(value, null, 2); } catch { return null; }
}

export default AcpSystemEventCard;
