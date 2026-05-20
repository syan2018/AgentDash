import { Handle, Position } from "@xyflow/react";
import type { NodeProps } from "@xyflow/react";

import type { AgentSessionPolicy, InputPortDefinition, OutputPortDefinition } from "../../../types";

export interface DagNodeData {
  stepKey: string;
  description: string;
  executorKind: "agent" | "function" | "human";
  sessionPolicy: AgentSessionPolicy | null;
  workflowKey: string | null;
  workflowName: string | null;
  inputPorts: InputPortDefinition[];
  outputPorts: OutputPortDefinition[];
  isEntryNode: boolean;
  /** 运行时状态叠加（Phase 4） */
  runtimeStatus?: string;
  [key: string]: unknown;
}

const NODE_TYPE_LABEL: Record<DagNodeData["executorKind"], string> = {
  agent: "Agent",
  function: "Function",
  human: "Human",
};

const NODE_TYPE_COLOR: Record<DagNodeData["executorKind"], string> = {
  agent: "bg-primary/10 text-primary border-primary/30",
  function: "bg-info/10 text-info border-info/30",
  human: "bg-warning/10 text-warning border-warning/40",
};

const RUNTIME_STATUS_RING: Record<string, string> = {
  pending: "ring-muted-foreground/30",
  ready: "ring-info/50",
  running: "ring-primary/60 animate-pulse",
  completed: "ring-success/50",
  failed: "ring-destructive/50",
};

const HANDLE_SIZE = 10;

/**
 * DAG 图中的自定义节点组件。
 * 显示 node key、类型 badge、workflow 名称，以及 input/output port handles。
 */
export function DagNode({ data, selected }: NodeProps) {
  const d = data as DagNodeData;
  const nodeType = d.executorKind ?? "agent";
  const runtimeRing = d.runtimeStatus ? RUNTIME_STATUS_RING[d.runtimeStatus] ?? "" : "";

  return (
    <div
      className={`
        relative min-w-[220px] rounded-[12px] border bg-card text-card-foreground shadow-sm
        transition-all duration-150
        ${selected ? "border-primary ring-2 ring-primary/25" : "border-border"}
        ${runtimeRing ? `ring-2 ${runtimeRing}` : ""}
      `}
    >
      {/* 输入 Port Handles（左侧） */}
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
      {/* 无 port 时提供默认 handle */}
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
      <div className="flex items-center justify-between gap-2 border-b border-border/60 px-3 py-2">
        <div className="flex items-center gap-2 overflow-hidden">
          {d.isEntryNode && (
            <span className="shrink-0 rounded-[8px] bg-success/15 px-1.5 py-0.5 text-[9px] font-semibold text-success">
              ENTRY
            </span>
          )}
          <span className="truncate text-sm font-medium text-foreground">
            {d.stepKey || "(no key)"}
          </span>
        </div>
        <span
          className={`shrink-0 rounded-[6px] border px-1.5 py-0.5 text-[9px] font-semibold ${NODE_TYPE_COLOR[nodeType]}`}
        >
          {NODE_TYPE_LABEL[nodeType]}
        </span>
      </div>

      {/* Body */}
      <div className="space-y-1 px-3 py-2">
        {d.workflowName ? (
          <p className="truncate text-xs text-muted-foreground">
            <span className="text-foreground/70">{d.workflowName}</span>
          </p>
        ) : d.executorKind === "human" ? (
          <p className="text-xs text-muted-foreground/70">Human approval</p>
        ) : d.executorKind === "function" ? (
          <p className="text-xs text-muted-foreground/70">Function executor</p>
        ) : (
          <p className="text-xs italic text-muted-foreground/60">未绑定 workflow</p>
        )}
        {(d.inputPorts.length > 0 || d.outputPorts.length > 0) && (
          <p className="text-[10px] text-muted-foreground">
            {d.inputPorts.length > 0 && <span>{d.inputPorts.length} in</span>}
            {d.inputPorts.length > 0 && d.outputPorts.length > 0 && <span> · </span>}
            {d.outputPorts.length > 0 && <span>{d.outputPorts.length} out</span>}
          </p>
        )}
      </div>

      {/* 输出 Port Handles（右侧） */}
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
      {/* 无 port 时提供默认 handle */}
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

/** 计算第 i 个 handle 在节点高度中的百分比偏移 */
function getHandleOffset(index: number, total: number): number {
  if (total <= 1) return 50;
  const padding = 20; // 上下各留 20% 空间
  return padding + ((100 - padding * 2) / (total - 1)) * index;
}
