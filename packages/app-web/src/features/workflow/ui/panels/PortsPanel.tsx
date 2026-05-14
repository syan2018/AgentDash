/**
 * Ports Panel —— output_ports + input_ports 编辑.
 *
 * 每个 port 支持 view ↔ edit 双态：
 *   - 新增（key 为空）默认 edit 态
 *   - view 态展示 key + 描述 + 策略标签，并提供编辑/删除按钮
 *   - edit 态展示完整表单 + 完成按钮回到 view 态
 *
 * 导出 `OutputPortItem` / `InputPortItem` / `PortViewCard`
 * 供 step-inspector Overview 复用，避免重复实现端口卡片。
 */

import { useState } from "react";

import type {
  ContextStrategy,
  GateStrategy,
  InputPortDefinition,
  OutputPortDefinition,
} from "../../../../types";

const GATE_LABEL: Record<GateStrategy, string> = {
  existence: "文件存在",
  schema: "Schema（预留）",
  llm_judge: "LLM（预留）",
};
const CTX_LABEL: Record<ContextStrategy, string> = {
  full: "完整",
  summary: "摘要（预留）",
  metadata_only: "元信息（预留）",
  custom: "自定义（预留）",
};

function TrashIcon() {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M3 6h18" />
      <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
      <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
    </svg>
  );
}

function PencilIcon() {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M12 20h9" />
      <path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4Z" />
    </svg>
  );
}

/**
 * 端口的只读展示卡片（view 态）。
 *
 * onEdit / onRemove 都可选：传入即渲染对应按钮，不传则纯只读。
 */
export function PortViewCard({
  portKey,
  description,
  strategyLabel,
  badge,
  onEdit,
  onRemove,
}: {
  portKey: string;
  description: string;
  strategyLabel: string;
  badge?: string;
  onEdit?: () => void;
  onRemove?: () => void;
}) {
  return (
    <div className="flex items-start gap-2 rounded-[10px] border border-border bg-background/60 p-2.5">
      <div className="min-w-0 flex-1 space-y-1">
        <div className="flex items-center gap-1.5">
          <code className="truncate font-mono text-xs text-foreground">
            {portKey || <span className="text-muted-foreground">(no key)</span>}
          </code>
          {badge && (
            <span className="rounded-[4px] bg-secondary px-1.5 py-px text-[10px] font-medium text-muted-foreground">
              {badge}
            </span>
          )}
        </div>
        {description && (
          <p className="text-[11px] leading-snug text-muted-foreground">{description}</p>
        )}
        <p className="text-[10px] text-muted-foreground/80">{strategyLabel}</p>
      </div>
      {(onEdit || onRemove) && (
        <div className="flex shrink-0 gap-0.5">
          {onEdit && (
            <button
              type="button"
              onClick={onEdit}
              className="rounded-[6px] p-1 text-muted-foreground hover:bg-secondary hover:text-foreground"
              aria-label="编辑"
              title="编辑"
            >
              <PencilIcon />
            </button>
          )}
          {onRemove && (
            <button
              type="button"
              onClick={onRemove}
              className="rounded-[6px] p-1 text-destructive/60 hover:bg-destructive/5 hover:text-destructive"
              aria-label="删除"
              title="删除"
            >
              <TrashIcon />
            </button>
          )}
        </div>
      )}
    </div>
  );
}

function PortEditCard({
  children,
  onDone,
  onRemove,
}: {
  children: React.ReactNode;
  onDone: () => void;
  onRemove?: () => void;
}) {
  return (
    <div className="space-y-2.5 rounded-[10px] border border-primary/40 bg-background p-3">
      {children}
      <div className="flex justify-end gap-1.5 pt-0.5">
        {onRemove && (
          <button
            type="button"
            onClick={onRemove}
            className="rounded-[6px] border border-destructive/30 px-2 py-1 text-[11px] text-destructive transition-colors hover:bg-destructive/5"
          >
            删除
          </button>
        )}
        <button
          type="button"
          onClick={onDone}
          className="rounded-[6px] border border-border bg-background px-2 py-1 text-[11px] text-foreground transition-colors hover:bg-secondary"
        >
          完成
        </button>
      </div>
    </div>
  );
}

export function OutputPortItem({
  port,
  badge,
  readOnly = false,
  onChange,
  onRemove,
}: {
  port: OutputPortDefinition;
  badge?: string;
  readOnly?: boolean;
  onChange?: (next: OutputPortDefinition) => void;
  onRemove?: () => void;
}) {
  const [editing, setEditing] = useState(() => !readOnly && port.key === "");
  const strategyLabel = `门禁：${GATE_LABEL[port.gate_strategy ?? "existence"]}`;

  if (readOnly || !editing) {
    return (
      <PortViewCard
        portKey={port.key}
        description={port.description}
        strategyLabel={strategyLabel}
        badge={badge}
        onEdit={readOnly ? undefined : () => setEditing(true)}
        onRemove={readOnly ? undefined : onRemove}
      />
    );
  }

  return (
    <PortEditCard onDone={() => setEditing(false)} onRemove={onRemove}>
      <div>
        <label className="agentdash-form-label">Key</label>
        <input
          value={port.key}
          onChange={(e) => onChange?.({ ...port, key: e.target.value })}
          className="agentdash-form-input font-mono text-xs"
          placeholder="port_key"
        />
      </div>
      <div>
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={port.description}
          onChange={(e) => onChange?.({ ...port, description: e.target.value })}
          rows={2}
          className="agentdash-form-textarea text-xs leading-[1.5]"
          placeholder="该 port 产出什么"
        />
      </div>
      <div>
        <label className="agentdash-form-label">完成门禁</label>
        <select
          value={port.gate_strategy ?? "existence"}
          onChange={(e) =>
            onChange?.({ ...port, gate_strategy: e.target.value as GateStrategy })
          }
          className="agentdash-form-select text-xs"
        >
          {(Object.entries(GATE_LABEL) as [GateStrategy, string][]).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </select>
      </div>
    </PortEditCard>
  );
}

export function InputPortItem({
  port,
  badge,
  readOnly = false,
  onChange,
  onRemove,
}: {
  port: InputPortDefinition;
  badge?: string;
  readOnly?: boolean;
  onChange?: (next: InputPortDefinition) => void;
  onRemove?: () => void;
}) {
  const [editing, setEditing] = useState(() => !readOnly && port.key === "");
  const strategyLabel = `上下文：${CTX_LABEL[port.context_strategy ?? "full"]}`;

  if (readOnly || !editing) {
    return (
      <PortViewCard
        portKey={port.key}
        description={port.description}
        strategyLabel={strategyLabel}
        badge={badge}
        onEdit={readOnly ? undefined : () => setEditing(true)}
        onRemove={readOnly ? undefined : onRemove}
      />
    );
  }

  return (
    <PortEditCard onDone={() => setEditing(false)} onRemove={onRemove}>
      <div>
        <label className="agentdash-form-label">Key</label>
        <input
          value={port.key}
          onChange={(e) => onChange?.({ ...port, key: e.target.value })}
          className="agentdash-form-input font-mono text-xs"
          placeholder="port_key"
        />
      </div>
      <div>
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={port.description}
          onChange={(e) => onChange?.({ ...port, description: e.target.value })}
          rows={2}
          className="agentdash-form-textarea text-xs leading-[1.5]"
          placeholder="该 port 需要的外部数据"
        />
      </div>
      <div>
        <label className="agentdash-form-label">上下文策略</label>
        <select
          value={port.context_strategy ?? "full"}
          onChange={(e) =>
            onChange?.({ ...port, context_strategy: e.target.value as ContextStrategy })
          }
          className="agentdash-form-select text-xs"
        >
          {(Object.entries(CTX_LABEL) as [ContextStrategy, string][]).map(([k, v]) => (
            <option key={k} value={k}>
              {v}
            </option>
          ))}
        </select>
      </div>
    </PortEditCard>
  );
}

export interface PortsPanelProps {
  outputPorts: OutputPortDefinition[];
  inputPorts: InputPortDefinition[];
  /** @deprecated 视觉语言统一后不再需要；保留 prop 以兼容调用方。 */
  compact?: boolean;
  onOutputChange: (ports: OutputPortDefinition[]) => void;
  onInputChange: (ports: InputPortDefinition[]) => void;
}

export function PortsPanel({
  outputPorts,
  inputPorts,
  onOutputChange,
  onInputChange,
}: PortsPanelProps) {
  const addBtnClass =
    "rounded-[8px] border border-border bg-background px-2 py-1 text-[11px] text-foreground transition-colors hover:bg-secondary";

  return (
    <section className="space-y-4">
      <div>
        <div className="mb-1.5 flex items-center justify-between gap-2">
          <label className="agentdash-form-label m-0">
            Output Ports ({outputPorts.length})
          </label>
          <button
            type="button"
            onClick={() =>
              onOutputChange([
                ...outputPorts,
                { key: "", description: "", gate_strategy: "existence" },
              ])
            }
            className={addBtnClass}
          >
            + 添加
          </button>
        </div>
        <div className="space-y-2">
          {outputPorts.map((p, idx) => (
            <OutputPortItem
              key={idx}
              port={p}
              onChange={(next) => {
                const n = [...outputPorts];
                n[idx] = next;
                onOutputChange(n);
              }}
              onRemove={() => onOutputChange(outputPorts.filter((_, i) => i !== idx))}
            />
          ))}
          {outputPorts.length === 0 && (
            <p className="py-2 text-center text-xs text-muted-foreground">暂无</p>
          )}
        </div>
      </div>

      <div>
        <div className="mb-1.5 flex items-center justify-between gap-2">
          <label className="agentdash-form-label m-0">
            Input Ports ({inputPorts.length})
          </label>
          <button
            type="button"
            onClick={() =>
              onInputChange([
                ...inputPorts,
                { key: "", description: "", context_strategy: "full" },
              ])
            }
            className={addBtnClass}
          >
            + 添加
          </button>
        </div>
        <div className="space-y-2">
          {inputPorts.map((p, idx) => (
            <InputPortItem
              key={idx}
              port={p}
              onChange={(next) => {
                const n = [...inputPorts];
                n[idx] = next;
                onInputChange(n);
              }}
              onRemove={() => onInputChange(inputPorts.filter((_, i) => i !== idx))}
            />
          ))}
          {inputPorts.length === 0 && (
            <p className="py-2 text-center text-xs text-muted-foreground">暂无</p>
          )}
        </div>
      </div>
    </section>
  );
}
