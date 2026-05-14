import { Handle, Position } from "@xyflow/react";
import type { NodeProps } from "@xyflow/react";

import type { InputPortDefinition, OutputPortDefinition, LifecycleNodeType } from "../../../types";

export interface DagNodeData {
  stepKey: string;
  description: string;
  nodeType: LifecycleNodeType;
  workflowKey: string | null;
  workflowName: string | null;
  inputPorts: InputPortDefinition[];
  outputPorts: OutputPortDefinition[];
  isEntryNode: boolean;
  /** 运行时状态叠加（Phase 4） */
  runtimeStatus?: string;
  [key: string]: unknown;
}

const NODE_TYPE_LABEL: Record<LifecycleNodeType, string> = {
  agent_node: "Agent",
  phase_node: "Phase",
};

const NODE_TYPE_COLOR: Record<LifecycleNodeType, string> = {
  agent_node: "bg-primary/10 text-primary border-primary/30",
  phase_node: "bg-amber-500/10 text-amber-700 border-amber-300/40",
};

const RUNTIME_STATUS_RING: Record<string, string> = {
  pending: "ring-muted-foreground/30",
  ready: "ring-blue-400/50",
  running: "ring-primary/60 animate-pulse",
  completed: "ring-emerald-500/50",
  failed: "ring-destructive/50",
};

const HANDLE_SIZE = 10;

/**
 * DAG 图中的自定义节点组件。
 * 显示 node key、类型 badge、workflow 名称，以及 input/output port handles。
 */
export function DagNode({ data, selected }: NodeProps) {
  const d = data as DagNodeData;
  const nodeType = d.nodeType ?? "agent_node";
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
            <span className="shrink-0 rounded-full bg-emerald-500/15 px-1.5 py-0.5 text-[9px] font-semibold text-emerald-700">
              ENTRY
            </span>
          )}
          <span className="truncate text-sm font-medium text-foreground">
            {d.stepKey || "(no key)"}
          </span>
        </div>
        <span
          className={`shrink-0 rounded-full border px-1.5 py-0.5 text-[9px] font-semibold ${NODE_TYPE_COLOR[nodeType]}`}
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
