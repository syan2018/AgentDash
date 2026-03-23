/**
 * ACP 系统事件卡片
 *
 * 渲染 session_info_update 事件，按事件重要性分三路：
 *
 * 1. 高优先级干预型（deny/ask/rewrite/continue/block_reason）
 *    → 完整大卡片，error/warning 色调，用户必须注意
 *
 * 2. 信息展示型（context_injected/steering_injected/phase_advanced 及其他 info hook_event）
 *    → 可展开细条（ExpandableStrip），默认折叠，展开显示 fragments/constraints/诊断
 *
 * 3. 通用系统事件（approval_requested/companion/error/permission 等）
 *    → 完整卡片，按 severity 着色
 */

import { useState } from "react";
import type { SessionUpdate } from "@agentclientprotocol/sdk";
import { extractAgentDashMetaFromUpdate, isRecord } from "../model/agentdashMeta";

export interface AcpSystemEventCardProps {
  update: SessionUpdate;
}

// ─── 类型定义 ────────────────────────────────────────────────────────────────

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

// ─── 样式常量 ────────────────────────────────────────────────────────────────

const SEVERITY_STYLES: Record<string, { border: string; tone: string; badge: string }> = {
  error: {
    border: "border-destructive/35",
    tone: "text-destructive",
    badge: "border-destructive/25 bg-destructive/10 text-destructive",
  },
  warning: {
    border: "border-warning/35",
    tone: "text-warning",
    badge: "border-warning/25 bg-warning/10 text-warning",
  },
  success: {
    border: "border-success/35",
    tone: "text-success",
    badge: "border-success/25 bg-success/10 text-success",
  },
  info: {
    border: "border-border",
    tone: "text-muted-foreground",
    badge: "border-border bg-secondary/50 text-muted-foreground",
  },
};

const DEFAULT_STYLE = SEVERITY_STYLES.info!;

const EVENT_TYPE_LABELS: Record<string, string> = {
  executor_session_bound: "会话已绑定",
  turn_interrupted: "执行已中断",
  turn_failed: "执行失败",
  system_message: "系统消息",
  error: "错误",
  user_feedback: "用户反馈",
  user_answered_questions: "用户回答",
  permission_denied: "权限拒绝",
  approval_requested: "等待审批",
  approval_resolved: "审批结果",
  hook_action_resolved: "事项已结案",
  companion_dispatch_registered: "协作 Agent 已派发",
  companion_result_available: "协作结果可用",
  companion_result_returned: "协作结果已回传",
  hook_event: "流程事件",
};

const EVENT_TYPE_DEFAULT_MESSAGES: Record<string, string> = {
  executor_session_bound: "已绑定到执行会话",
  turn_interrupted: "本轮执行已中断",
  turn_failed: "本轮执行失败",
  system_message: "系统消息",
  error: "执行出现错误",
  user_feedback: "已收到用户反馈",
  user_answered_questions: "已收到用户回答",
  permission_denied: "操作被权限策略拒绝",
  approval_requested: "当前工具调用正在等待审批",
  approval_resolved: "当前工具调用审批已完成",
  hook_action_resolved: "一项流程干预已被结案",
  companion_dispatch_registered: "已注册协作 Agent 派发",
  companion_result_available: "协作 Agent 已回传结果",
  companion_result_returned: "协作结果已回传到当前会话",
  hook_event: "流程产生新事件",
};

// ─── Hook 决策分类 ───────────────────────────────────────────────────────────

/** 高优先级干预型决策——需要完整大卡片，用户必须注意 */
const HIGH_PRIORITY_HOOK_DECISIONS = new Set([
  "deny",
  "ask",
  "rewrite",
  "continue",
]);

/** 从 event.code（格式：hook:{trigger}:{decision}）中提取 decision */
function extractHookDecision(code: string | null | undefined): string | null {
  if (!code) return null;
  const parts = code.split(":");
  return parts.length >= 3 ? (parts[2] ?? null) : null;
}

/** 判断 hook_event 是否为高优先级干预（需要完整卡片） */
function isHighPriorityHookEvent(hookData: HookEventData | null, code: string | null | undefined): boolean {
  if (hookData?.block_reason) return true;
  const decision = extractHookDecision(code);
  return decision !== null && HIGH_PRIORITY_HOOK_DECISIONS.has(decision);
}

// ─── 主组件入口 ──────────────────────────────────────────────────────────────

export function AcpSystemEventCard({ update }: AcpSystemEventCardProps) {
  if (update.sessionUpdate !== "session_info_update") return null;

  const meta = extractAgentDashMetaFromUpdate(update);
  const event = meta?.event;
  const eventType = event?.type ?? "system";
  const hookData = eventType === "hook_event" ? extractHookEventData(event?.data) : null;

  // hook_event 路由：高优先级 → 完整卡片；其余 → 细条
  if (eventType === "hook_event") {
    if (isHighPriorityHookEvent(hookData, event?.code)) {
      return <HookEventFullCard event={event} hookData={hookData} turnId={meta?.trace?.turnId ?? null} />;
    }
    return <HookEventStripCard event={event} hookData={hookData} />;
  }

  // 通用系统事件 → 完整卡片
  return <SystemEventFullCard update={update} event={event} eventType={eventType} hookData={null} turnId={meta?.trace?.turnId ?? null} />;
}

// ─── 可展开细条（Hook 信息型事件）────────────────────────────────────────────

/**
 * HookEventStripCard
 *
 * 用于 context_injected / steering_injected / phase_advanced 等信息型 hook 事件。
 * 默认折叠为一行细条，点击展开显示注入的 fragments、constraints、diagnostics。
 */
function HookEventStripCard({
  event,
  hookData,
}: {
  event: { severity?: string | null; code?: string | null; message?: string | null; data?: unknown } | null | undefined;
  hookData: HookEventData | null;
}) {
  const [expanded, setExpanded] = useState(false);

  const decision = extractHookDecision(event?.code);
  const severity = event?.severity ?? "info";
  const style = SEVERITY_STYLES[severity] ?? DEFAULT_STYLE;

  const stripLabel = resolveStripLabel(decision, hookData);
  const expandContent = buildStripExpandContent(hookData);
  const hasExpandContent = expandContent.fragments.length > 0
    || expandContent.constraints.length > 0
    || expandContent.diagnostics.length > 0
    || expandContent.completionLine !== null;

  return (
    <div className={`rounded-[12px] border ${style.border} bg-background overflow-hidden`}>
      <button
        type="button"
        onClick={() => hasExpandContent && setExpanded((prev) => !prev)}
        className={`flex w-full items-center gap-2.5 px-3 py-2 text-left transition-colors ${hasExpandContent ? "hover:bg-secondary/35 cursor-pointer" : "cursor-default"}`}
      >
        {/* 左侧决策类型 badge */}
        <span className={`inline-flex shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] ${style.badge}`}>
          {resolveDecisionToken(decision)}
        </span>

        {/* 主要文本 */}
        <span className="min-w-0 flex-1 truncate text-xs text-foreground/80">
          {stripLabel}
        </span>

        {/* 右侧附加信息 */}
        {hookData?.completion && (
          <span className={`shrink-0 text-[10px] ${hookData.completion.satisfied ? "text-success" : style.tone}`}>
            {hookData.completion.satisfied ? "已满足" : "未满足"}
          </span>
        )}
        {hookData?.matched_rule_keys && hookData.matched_rule_keys.length > 0 && (
          <span className="shrink-0 text-[10px] text-muted-foreground/50">
            {hookData.matched_rule_keys.length} 条规则
          </span>
        )}

        {/* 展开箭头 */}
        {hasExpandContent && (
          <span className="shrink-0 text-[10px] text-muted-foreground/40">
            {expanded ? "▲" : "▼"}
          </span>
        )}
      </button>

      {expanded && hasExpandContent && (
        <div className="border-t border-border px-3 py-2.5 space-y-2">
          {/* 完成判定 */}
          {expandContent.completionLine && (
            <p className={`text-xs leading-5 ${hookData?.completion?.satisfied ? "text-success" : "text-muted-foreground"}`}>
              {expandContent.completionLine}
            </p>
          )}

          {/* 注入的 context fragments */}
          {expandContent.fragments.length > 0 && (
            <div className="space-y-1.5">
              <p className="text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60">注入片段</p>
              {expandContent.fragments.map((frag) => (
                <FragmentRow key={frag.slot} slot={frag.slot} label={frag.label} content={frag.content} />
              ))}
            </div>
          )}

          {/* 约束列表 */}
          {expandContent.constraints.length > 0 && (
            <div className="space-y-1">
              <p className="text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60">生效约束</p>
              {expandContent.constraints.map((c, i) => (
                <p key={i} className="text-xs leading-5 text-foreground/75">— {c}</p>
              ))}
            </div>
          )}

          {/* 诊断信息 */}
          {expandContent.diagnostics.length > 0 && (
            <div className="space-y-1">
              <p className="text-[10px] font-medium uppercase tracking-[0.12em] text-muted-foreground/60">诊断</p>
              {expandContent.diagnostics.map((d, i) => (
                <p key={i} className="text-xs leading-5 text-muted-foreground">
                  {d.code ? <span className="font-mono text-[10px] text-muted-foreground/60 mr-1">{d.code}</span> : null}
                  {d.summary}
                  {d.detail ? <span className="text-muted-foreground/60">；{d.detail}</span> : null}
                </p>
              ))}
            </div>
          )}

          {/* 调试 chips */}
          <DebugChips hookData={hookData} />
        </div>
      )}
    </div>
  );
}

/** 可展开的单个 fragment 行 */
function FragmentRow({ slot, label, content }: { slot: string; label: string; content: string }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="rounded-[8px] border border-border/70 bg-secondary/20 overflow-hidden">
      <button
        type="button"
        onClick={() => content && setOpen((v) => !v)}
        className={`flex w-full items-center gap-2 px-2.5 py-1.5 text-left ${content ? "hover:bg-secondary/40 cursor-pointer" : "cursor-default"}`}
      >
        <span className="inline-flex rounded-[4px] border border-border bg-secondary/60 px-1 py-0 text-[9px] font-mono text-muted-foreground/70">
          {slot}
        </span>
        <span className="min-w-0 flex-1 truncate text-[11px] text-foreground/80">{label}</span>
        {content && (
          <span className="shrink-0 text-[10px] text-muted-foreground/40">{open ? "▲" : "▼"}</span>
        )}
      </button>
      {open && content && (
        <div className="border-t border-border/50 px-2.5 py-2">
          <pre className="max-h-48 overflow-auto whitespace-pre-wrap text-[11px] leading-relaxed text-foreground/75">
            {content}
          </pre>
        </div>
      )}
    </div>
  );
}

// ─── 高优先级干预 Hook 完整卡片 ──────────────────────────────────────────────

function HookEventFullCard({
  event,
  hookData,
  turnId,
}: {
  event: { severity?: string | null; code?: string | null; message?: string | null; data?: unknown } | null | undefined;
  hookData: HookEventData | null;
  turnId: string | null;
}) {
  const [showDebug, setShowDebug] = useState(false);

  const severity = event?.severity ?? "warning";
  const style = SEVERITY_STYLES[severity] ?? SEVERITY_STYLES.warning!;
  const decision = extractHookDecision(event?.code);
  const message = event?.message ?? EVENT_TYPE_DEFAULT_MESSAGES.hook_event ?? "流程干预";

  const userDetailLines = buildHookUserDetailLines(hookData);
  const debugChips = buildHookDebugChips(hookData, turnId);
  const debugLines = buildHookDebugLines(hookData);
  const hasDebugContent = debugChips.length > 0 || debugLines.length > 0;

  return (
    <div className={`rounded-[12px] border ${style.border} bg-background px-4 py-3`}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className={`rounded-[6px] border px-1.5 py-0.5 text-[10px] font-semibold uppercase tracking-[0.1em] ${style.badge}`}>
              {resolveDecisionToken(decision) ?? "HOOK"}
            </span>
            <span className={`text-[10px] font-medium ${style.tone}`}>
              {resolveDecisionLabel(decision, hookData)}
            </span>
          </div>
          <p className="mt-2 text-sm leading-6 text-foreground/90">{message}</p>
        </div>
        <span className={`shrink-0 text-[11px] font-medium ${style.tone}`}>
          {resolveSeverityLabel(severity)}
        </span>
      </div>

      {userDetailLines.length > 0 && (
        <div className="mt-3 space-y-2 rounded-[10px] border border-border/70 bg-secondary/20 px-3 py-2.5">
          {userDetailLines.map((line) => (
            <p key={line} className="text-xs leading-5 text-muted-foreground">{line}</p>
          ))}
        </div>
      )}

      {hasDebugContent && (
        <div className="mt-2">
          <button
            type="button"
            onClick={() => setShowDebug((v) => !v)}
            className="text-[10px] text-muted-foreground/40 transition-colors hover:text-muted-foreground"
          >
            {showDebug ? "▲ 收起详情" : "▶ 调试详情"}
          </button>
          {showDebug && (
            <div className="mt-1.5 space-y-2">
              {debugChips.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                  {debugChips.map((chip) => (
                    <span
                      key={chip}
                      className="rounded-full border border-border bg-secondary/35 px-2 py-1 text-[10px] text-muted-foreground"
                    >
                      {chip}
                    </span>
                  ))}
                </div>
              )}
              {debugLines.length > 0 && (
                <div className="space-y-1 rounded-[10px] border border-border/70 bg-secondary/20 px-3 py-2.5">
                  {debugLines.map((line) => (
                    <p key={line} className="text-xs leading-5 text-muted-foreground">{line}</p>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─── 通用系统事件完整卡片 ────────────────────────────────────────────────────

function SystemEventFullCard({
  event,
  eventType,
  turnId,
}: {
  update: SessionUpdate;
  event: { severity?: string | null; type?: string | null; message?: string | null; data?: unknown } | null | undefined;
  eventType: string;
  hookData: null;
  turnId: string | null;
}) {
  const [showDebug, setShowDebug] = useState(false);

  const severity = event?.severity ?? "info";
  const style = SEVERITY_STYLES[severity] ?? DEFAULT_STYLE;
  const typeLabel = EVENT_TYPE_LABELS[eventType] ?? eventType;
  const message = event?.message ?? EVENT_TYPE_DEFAULT_MESSAGES[eventType] ?? "系统事件";
  const userDetailLines = buildGenericDetailLines(eventType, event?.data);
  const debugChips: string[] = turnId ? [`turn: ${turnId.slice(0, 8)}`] : [];
  const extraData = formatExtraData(event?.data);
  const hasDebugContent = debugChips.length > 0 || Boolean(extraData);

  return (
    <div className={`rounded-[12px] border ${style.border} bg-background px-4 py-3`}>
      <div className="flex items-start justify-between gap-3">
        <div className="min-w-0 flex-1">
          <div className="flex flex-wrap items-center gap-2">
            <span className={`rounded-full border px-2 py-1 text-[10px] font-semibold uppercase tracking-[0.14em] ${style.badge}`}>
              {typeLabel}
            </span>
          </div>
          <p className="mt-2 text-sm leading-6 text-foreground/90">{message}</p>
        </div>
        <span className={`shrink-0 text-[11px] font-medium ${style.tone}`}>
          {resolveSeverityLabel(severity)}
        </span>
      </div>

      {userDetailLines.length > 0 && (
        <div className="mt-3 space-y-2 rounded-[10px] border border-border/70 bg-secondary/20 px-3 py-2.5">
          {userDetailLines.map((line) => (
            <p key={line} className="text-xs leading-5 text-muted-foreground">{line}</p>
          ))}
        </div>
      )}

      {hasDebugContent && (
        <div className="mt-2">
          <button
            type="button"
            onClick={() => setShowDebug((v) => !v)}
            className="text-[10px] text-muted-foreground/40 transition-colors hover:text-muted-foreground"
          >
            {showDebug ? "▲ 收起详情" : "▶ 调试详情"}
          </button>
          {showDebug && (
            <div className="mt-1.5 space-y-2">
              {debugChips.length > 0 && (
                <div className="flex flex-wrap gap-1.5">
                  {debugChips.map((chip) => (
                    <span
                      key={chip}
                      className="rounded-full border border-border bg-secondary/35 px-2 py-1 text-[10px] text-muted-foreground"
                    >
                      {chip}
                    </span>
                  ))}
                </div>
              )}
              {extraData && (
                <pre className="overflow-auto rounded-[10px] border border-border/70 bg-secondary/20 p-3 text-xs text-muted-foreground">
                  {extraData}
                </pre>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─── DebugChips 子组件 ───────────────────────────────────────────────────────

function DebugChips({ hookData }: { hookData: HookEventData | null }) {
  const chips = buildHookDebugChips(hookData, null);
  if (chips.length === 0) return null;
  return (
    <div className="flex flex-wrap gap-1.5 pt-1">
      {chips.map((chip) => (
        <span
          key={chip}
          className="rounded-full border border-border bg-secondary/35 px-1.5 py-0.5 text-[10px] text-muted-foreground/60"
        >
          {chip}
        </span>
      ))}
    </div>
  );
}

// ─── 辅助函数 ────────────────────────────────────────────────────────────────

function resolveStripLabel(decision: string | null, hookData: HookEventData | null): string {
  switch (decision) {
    case "context_injected": {
      const count = hookData?.matched_rule_keys?.length ?? 0;
      return count > 0 ? `已注入动态上下文（${count} 条规则生效）` : "已注入动态上下文";
    }
    case "steering_injected":
      return "已追加流程约束（steering）";
    case "phase_advanced":
      return "Workflow 阶段已推进";
    default:
      return hookData?.completion
        ? `流程决策：${decision ?? "unknown"}`
        : `Hook 决策：${decision ?? "unknown"}`;
  }
}

function resolveDecisionToken(decision: string | null | undefined): string {
  switch (decision) {
    case "context_injected": return "CTX";
    case "steering_injected": return "STEER";
    case "phase_advanced": return "PHASE";
    case "continue": return "HOLD";
    case "deny": return "DENY";
    case "ask": return "ASK";
    case "rewrite": return "REWRITE";
    default: return "HOOK";
  }
}

function resolveDecisionLabel(decision: string | null | undefined, hookData: HookEventData | null): string {
  switch (decision) {
    case "deny": return "工具调用已阻止";
    case "ask": return "等待审批";
    case "rewrite": return "参数已改写";
    case "continue": return hookData?.completion?.satisfied ? "条件满足，仍需处理约束" : "阻止结束，要求继续执行";
    default: return decision ?? "hook";
  }
}

function resolveSeverityLabel(severity: string): string {
  switch (severity) {
    case "error": return "错误";
    case "warning": return "注意";
    case "success": return "完成";
    default: return "信息";
  }
}

interface StripExpandContent {
  fragments: Array<{ slot: string; label: string; content: string }>;
  constraints: string[];
  diagnostics: Array<{ code?: string; summary: string; detail?: string | null }>;
  completionLine: string | null;
}

function buildStripExpandContent(hookData: HookEventData | null): StripExpandContent {
  if (!hookData) return { fragments: [], constraints: [], diagnostics: [], completionLine: null };

  // fragments 来自 event.data 中的扩展字段（后端目前不在 hook_event data 中放 fragments，
  // 但为未来扩展保留接口；当前仅展示 diagnostics 和 completion）
  const fragments: StripExpandContent["fragments"] = [];

  const constraints: string[] = [];

  const diagnostics = (hookData.diagnostics ?? [])
    .filter((d) => d.summary)
    .map((d) => ({ code: d.code, summary: d.summary!, detail: d.detail }));

  let completionLine: string | null = null;
  if (hookData.completion) {
    const { satisfied, advanced, reason, mode } = hookData.completion;
    const statusText = satisfied ? "✓ 条件已满足" : "✗ 条件未满足";
    const advancedText = advanced ? "，已推进阶段" : "";
    const modeText = mode ? ` [${mode}]` : "";
    const reasonText = reason ? `：${reason}` : "";
    completionLine = `${statusText}${advancedText}${modeText}${reasonText}`;
  }

  return { fragments, constraints, diagnostics, completionLine };
}

function buildHookUserDetailLines(hookData: HookEventData | null): string[] {
  if (!hookData) return [];
  const lines: string[] = [];
  if (hookData.block_reason) {
    lines.push(`阻塞原因：${hookData.block_reason}`);
  }
  if (hookData.completion) {
    const status = hookData.completion.satisfied ? "已满足" : "未满足";
    const advanced = hookData.completion.advanced ? "，已推进后续阶段" : "";
    const reason = hookData.completion.reason ? `：${hookData.completion.reason}` : "";
    lines.push(`完成判定：${status}${advanced}${reason}`);
  }
  return lines;
}

function buildHookDebugChips(hookData: HookEventData | null, turnId: string | null): string[] {
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

function buildHookDebugLines(hookData: HookEventData | null): string[] {
  if (!hookData) return [];
  const lines: string[] = [];
  if (hookData.matched_rule_keys?.length) {
    lines.push(`命中规则：${hookData.matched_rule_keys.join("，")}`);
  }
  if (hookData.tool_call_id) {
    lines.push(`tool_call_id：${hookData.tool_call_id}`);
  }
  if (hookData.diagnostics?.length) {
    for (const d of hookData.diagnostics) {
      const summary = typeof d?.summary === "string" ? d.summary : null;
      if (!summary) continue;
      const code = typeof d?.code === "string" ? d.code : null;
      const detail = typeof d?.detail === "string" ? d.detail : null;
      lines.push(`诊断${code ? ` ${code}` : ""}：${summary}${detail ? `；${detail}` : ""}`);
    }
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
  try {
    return JSON.stringify(value, null, 2);
  } catch {
    return null;
  }
}

export default AcpSystemEventCard;
