/**
 * DagNode —— 极简 lifecycle activity 节点。
 *
 * 视觉：executor icon + activity.key + completion_policy 4 字代号；entry ring；
 * 右上 validation 红点；左右 port handles。常驻信息只放 key + 截断 description；
 * description / executor 详情 / iteration / join / validation 详情走 native title tooltip。
 */

import { Handle, Position } from "@xyflow/react";
import type { NodeProps } from "@xyflow/react";

import type {
  ActivityCompletionPolicy,
  ActivityExecutorSpec,
  InputPortDefinition,
  OutputPortDefinition,
} from "../../../types";

export interface DagNodeData {
  activityKey: string;
  description: string;
  executorKind: ActivityExecutorSpec["kind"];
  completionPolicyKind: ActivityCompletionPolicy["kind"];
  isEntryNode: boolean;
  /** 该节点关联的 validation issue 数（来自 WorkflowValidationResult） */
  validationCount: number;
  inputPorts: InputPortDefinition[];
  outputPorts: OutputPortDefinition[];
  /** Tooltip 详情（hover 显示），由 stepsToNodes 装配 */
  tooltip?: string | null;
  [key: string]: unknown;
}

const EXECUTOR_BADGE: Record<DagNodeData["executorKind"], { label: string; color: string; icon: string }> = {
  agent: {
    label: "Agent",
    color: "bg-indigo-500/10 text-indigo-700 dark:text-indigo-300 border-indigo-500/30",
    icon: "◆",
  },
  human: {
    label: "Human",
    color: "bg-amber-500/10 text-amber-700 dark:text-amber-300 border-amber-500/30",
    icon: "✋",
  },
  function: {
    label: "Function",
    color: "bg-emerald-500/10 text-emerald-700 dark:text-emerald-300 border-emerald-500/30",
    icon: "ƒ",
  },
};

const POLICY_BADGE: Record<ActivityCompletionPolicy["kind"], string> = {
  output_ports: "PORT",
  executor_terminal: "TERM",
  human_decision: "DECI",
  hook_gate: "HOOK",
  open_ended: "OPEN",
};

const HANDLE_SIZE = 10;

export function DagNode({ data, selected }: NodeProps) {
  const d = data as DagNodeData;
  const exec = EXECUTOR_BADGE[d.executorKind] ?? EXECUTOR_BADGE.agent;
  const policyLabel = POLICY_BADGE[d.completionPolicyKind] ?? "?";

  const ringClass = selected
    ? "border-primary ring-2 ring-primary/40"
    : d.isEntryNode
      ? "border-primary/60 ring-2 ring-primary/20"
      : "border-border";

  return (
    <div
      className={`relative min-w-[200px] rounded-[10px] border bg-card text-card-foreground shadow-sm transition-all duration-150 ${ringClass}`}
      title={d.tooltip ?? undefined}
    >
      {/* Validation 角标（右上） */}
      {d.validationCount > 0 && (
        <span
          className="absolute -right-1.5 -top-1.5 z-10 flex h-4 min-w-4 items-center justify-center rounded-[8px] bg-destructive px-1 text-[9px] font-semibold text-destructive-foreground shadow"
          title={`${d.validationCount} validation issue${d.validationCount > 1 ? "s" : ""}`}
        >
          {d.validationCount}
        </span>
      )}

      {/* 输入 ports（左侧 handles） */}
      {d.inputPorts.map((port, i) => (
        <Handle
          key={`in-${port.key}`}
          type="target"
          position={Position.Left}
          id={port.key}
          style={{
            top: `${getHandleOffset(i, d.inputPorts.length)}%`,
            width: HANDLE_SIZE,
            height: HANDLE_SIZE,
            background: "hsl(var(--primary))",
            border: "2px solid hsl(var(--background))",
          }}
          title={`${port.key}${port.description ? ` — ${port.description}` : ""}`}
        />
      ))}
      {d.inputPorts.length === 0 && (
        <Handle
          type="target"
          position={Position.Left}
          id="__default_in"
          style={{
            top: "50%",
            width: HANDLE_SIZE,
            height: HANDLE_SIZE,
            background: "hsl(var(--muted-foreground))",
            border: "2px solid hsl(var(--background))",
            opacity: 0.4,
          }}
        />
      )}

      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2">
        <span
          className={`shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[10px] font-mono ${exec.color}`}
          title={exec.label}
        >
          {exec.icon}
        </span>
        <span className="truncate text-sm font-medium text-foreground">
          {d.activityKey || "(no key)"}
        </span>
        <span
          className={`ml-auto shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[9px] font-semibold ${exec.color} opacity-70`}
          title={`completion_policy: ${d.completionPolicyKind}`}
        >
          {policyLabel}
        </span>
      </div>

      {/* Body：description */}
      {d.description && (
        <div className="px-3 pb-2">
          <p className="line-clamp-2 text-[10px] leading-tight text-muted-foreground">
            {d.description}
          </p>
        </div>
      )}

      {/* 输出 ports（右侧 handles） */}
      {d.outputPorts.map((port, i) => (
        <Handle
          key={`out-${port.key}`}
          type="source"
          position={Position.Right}
          id={port.key}
          style={{
            top: `${getHandleOffset(i, d.outputPorts.length)}%`,
            width: HANDLE_SIZE,
            height: HANDLE_SIZE,
            background: "hsl(var(--primary))",
            border: "2px solid hsl(var(--background))",
          }}
          title={`${port.key}${port.description ? ` — ${port.description}` : ""}`}
        />
      ))}
      {d.outputPorts.length === 0 && (
        <Handle
          type="source"
          position={Position.Right}
          id="__default_out"
          style={{
            top: "50%",
            width: HANDLE_SIZE,
            height: HANDLE_SIZE,
            background: "hsl(var(--muted-foreground))",
            border: "2px solid hsl(var(--background))",
            opacity: 0.4,
          }}
        />
      )}
    </div>
  );
}

function getHandleOffset(index: number, total: number): number {
  if (total <= 1) return 50;
  const padding = 20;
  return padding + ((100 - padding * 2) / (total - 1)) * index;
}
