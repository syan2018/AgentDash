/**
 * ACP 系统事件卡片
 *
 * 三类渲染路径，均复用 EventStripCard / EventFullCard 模板：
 *
 * 1. 信息型 hook 事件（context_injected / steering_injected / step_advanced 等）
 *    → EventStripCard：单行折叠，展开显示 diagnostics / completion
 *
 * 2. 高优先级干预 hook 事件（deny / ask / rewrite / continue / block_reason）
 *    → EventFullCard：badge + subtitle + message + detail lines + debug
 *
 * 3. 通用系统事件（approval_requested / companion / error / permission 等）
 *    → EventFullCard：badge + message + detail lines + debug
 *
 * 样式原则：badge 是唯一染色点，卡片外框和文字保持中性色。
 */

import type { ReactNode } from "react";
import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { extractAgentDashMetaFromUpdate, isRecord } from "../model/agentdashMeta";
import { EventStripCard, EventFullCard } from "./EventCards";

export interface AcpSystemEventCardProps {
  update: SessionUpdate;
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
    summary?: string;
    detail?: string | null;
    source_summary?: string[];
  }>;
}

// ─── badge 样式（唯一颜色分叉点）─────────────────────────────────────────────

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
  hook_event:                      "流程事件",
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
  hook_event:                      "流程产生新事件",
};

// ─── Hook 决策分类 ────────────────────────────────────────────────────────────

const HIGH_PRIORITY_HOOK_DECISIONS = new Set(["deny", "ask", "rewrite", "continue"]);

function extractHookDecision(code: string | null | undefined): string | null {
  if (!code) return null;
  const parts = code.split(":");
  return parts.length >= 3 ? (parts[2] ?? null) : null;
}

function isHighPriorityHookEvent(
  hookData: HookEventData | null,
  code: string | null | undefined,
): boolean {
  if (hookData?.block_reason) return true;
  const decision = extractHookDecision(code);
  return decision !== null && HIGH_PRIORITY_HOOK_DECISIONS.has(decision);
}

// ─── 主组件 ───────────────────────────────────────────────────────────────────

export function AcpSystemEventCard({ update }: AcpSystemEventCardProps) {
  if (update.sessionUpdate !== "session_info_update") return null;

  const meta = extractAgentDashMetaFromUpdate(update);
  const event = meta?.event;
  const eventType = event?.type ?? "system";
  const hookData = eventType === "hook_event" ? extractHookEventData(event?.data) : null;

  // ── hook_event 路由 ──────────────────────────────────────────────────────
  if (eventType === "hook_event") {
    const decision = extractHookDecision(event?.code);
    const severity = event?.severity ?? "info";
    const badge = SEVERITY_BADGE[severity] ?? DEFAULT_BADGE;
    const token = resolveDecisionToken(decision);

    // 两条路径共用同一份数据构建
    const { completionLine, diagnostics } = buildHookExpandData(hookData);
    const diagBody = buildDiagnosticsNode(diagnostics);

    if (isHighPriorityHookEvent(hookData, event?.code)) {
      // 高优先级 → FullCard
      // detailLines 只放 block_reason + completion 判定（主要说明）
      // diagnostics 和 debug meta 一起放进折叠区（次要详情）
      const detailLines: string[] = [];
      if (hookData?.block_reason) detailLines.push(`阻塞原因：${hookData.block_reason}`);
      if (completionLine) detailLines.push(completionLine);

      return (
        <EventFullCard
          badgeToken={token}
          badgeClass={badge}
          subtitle={resolveDecisionLabel(decision, hookData)}
          message={event?.message ?? EVENT_TYPE_DEFAULT_MESSAGES.hook_event ?? "流程干预"}
          detailLines={detailLines}
          debugChips={buildHookDebugChips(hookData, meta?.trace?.turnId ?? null)}
          debugLines={buildHookDebugMeta(hookData)}
          debugBody={diagBody}
        />
      );
    }

    // 信息型 → StripCard
    // expandContent 放 completion，diagnostics 用 bodyExtra 渲染
    const expandContent = completionLine
      ? { sections: [{ lines: [completionLine] }] }
      : undefined;
    const rightHint = hookData?.completion
      ? (hookData.completion.satisfied ? "已满足" : "未满足")
      : hookData?.matched_rule_keys?.length
        ? `${hookData.matched_rule_keys.length} 条规则`
        : null;
    return (
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

  // ── 通用系统事件 ─────────────────────────────────────────────────────────
  const severity = event?.severity ?? "info";
  const badge = SEVERITY_BADGE[severity] ?? DEFAULT_BADGE;
  const typeLabel = EVENT_TYPE_LABELS[eventType] ?? eventType;
  const message = event?.message ?? EVENT_TYPE_DEFAULT_MESSAGES[eventType] ?? "系统事件";
  const detailLines = buildGenericDetailLines(eventType, event?.data);
  const turnId = meta?.trace?.turnId ?? null;
  const debugChips: string[] = turnId ? [`turn: ${turnId.slice(0, 8)}`] : [];
  const extraData = formatExtraData(event?.data);
  return (
    <EventFullCard
      badgeToken={typeLabel}
      badgeClass={badge}
      message={message}
      detailLines={detailLines}
      debugChips={debugChips}
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
      const count = hookData?.matched_rule_keys?.length ?? 0;
      return count > 0 ? `已注入动态上下文（${count} 条规则生效）` : "已注入动态上下文";
    }
    case "steering_injected": return "已追加流程约束（steering）";
    case "step_advanced":     return "Workflow Step 已推进";
    default:
      return hookData?.completion
        ? `流程决策：${decision ?? "unknown"}`
        : `Hook 决策：${decision ?? "unknown"}`;
  }
}

/** 两条路径共用：从 hookData 提取 completionLine + 结构化 diagnostics */
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

  const diagnostics = (hookData.diagnostics ?? [])
    .filter((d) => d.summary)
    .map((d) => ({ code: d.code, summary: d.summary!, detail: d.detail }));

  return { completionLine, diagnostics };
}

/** 两条路径共用：将结构化 diagnostics 渲染为带样式的 JSX（或 null） */
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

/** debug 折叠区的 meta chips（turnId + hookData 字段，不含 diagnostics） */
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

/** debug 折叠区的文本行（matched_rule_keys + tool_call_id，不含 diagnostics） */
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

function buildGenericDetailLines(eventType: string, value: unknown): string[] {
  if (!isRecord(value)) return [];
  const lines: string[] = [];

  if (eventType === "approval_requested" || eventType === "approval_resolved") {
    const toolName = typeof value.tool_name === "string" ? value.tool_name : null;
    const reason = typeof value.reason === "string" ? value.reason : null;
    const approved = typeof value.approved === "boolean" ? value.approved : null;
    if (toolName) lines.push(`工具：${toolName}`);
    if (approved != null) lines.push(`审批结果：${approved ? "已批准" : "已拒绝"}`);
    if (reason) lines.push(`原因：${reason}`);
    return lines;
  }

  if (eventType === "hook_action_resolved") {
    const summary = typeof value.summary === "string" ? value.summary : null;
    const resolutionNote = typeof value.resolution_note === "string" ? value.resolution_note : null;
    if (summary) lines.push(`摘要：${summary}`);
    if (resolutionNote) lines.push(`说明：${resolutionNote}`);
    return lines;
  }

  if (
    eventType === "companion_dispatch_registered" ||
    eventType === "companion_result_available" ||
    eventType === "companion_result_returned"
  ) {
    const label = typeof value.companion_label === "string" ? value.companion_label : null;
    const summary = typeof value.summary === "string" ? value.summary : null;
    const status = typeof value.status === "string" ? value.status : null;
    if (label) lines.push(`协作 Agent：${label}`);
    if (status) lines.push(`状态：${status}`);
    if (summary) lines.push(`摘要：${summary}`);
    return lines;
  }

  return lines;
}

function extractHookEventData(value: unknown): HookEventData | null {
  return isRecord(value) ? (value as HookEventData) : null;
}

function formatExtraData(value: unknown): string | null {
  if (value == null) return null;
  if (typeof value === "string") return value;
  try { return JSON.stringify(value, null, 2); } catch { return null; }
}

export default AcpSystemEventCard;
