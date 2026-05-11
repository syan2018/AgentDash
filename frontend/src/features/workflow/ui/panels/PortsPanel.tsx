/**
 * Ports Panel —— output_ports + input_ports 编辑。
 *
 * 视觉语言对齐 Overview：线性 section heading + 控件；无 DetailSection 边框灰底，
 * 无平铺 description 注释。每个 port 展开为"key + 描述 + 策略"三字段的子卡片，
 * 字段使用 agentdash-form-label / input / select 标准组件。
 */

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

function PortCard({
  children,
  onRemove,
}: {
  children: React.ReactNode;
  onRemove: () => void;
}) {
  return (
    <div className="relative space-y-2.5 rounded-[10px] border border-border bg-background p-3 pr-9">
      {children}
      <button
        type="button"
        onClick={onRemove}
        className="absolute right-2 top-2 rounded-[6px] p-1 text-destructive/60 hover:bg-destructive/5 hover:text-destructive"
        aria-label="删除"
      >
        <TrashIcon />
      </button>
    </div>
  );
}

function OutputPortCard({
  port,
  onChange,
  onRemove,
}: {
  port: OutputPortDefinition;
  onChange: (next: OutputPortDefinition) => void;
  onRemove: () => void;
}) {
  return (
    <PortCard onRemove={onRemove}>
      <div>
        <label className="agentdash-form-label">Key</label>
        <input
          value={port.key}
          onChange={(e) => onChange({ ...port, key: e.target.value })}
          className="agentdash-form-input font-mono text-xs"
          placeholder="port_key"
        />
      </div>
      <div>
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={port.description}
          onChange={(e) => onChange({ ...port, description: e.target.value })}
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
            onChange({ ...port, gate_strategy: e.target.value as GateStrategy })
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
    </PortCard>
  );
}

function InputPortCard({
  port,
  onChange,
  onRemove,
}: {
  port: InputPortDefinition;
  onChange: (next: InputPortDefinition) => void;
  onRemove: () => void;
}) {
  return (
    <PortCard onRemove={onRemove}>
      <div>
        <label className="agentdash-form-label">Key</label>
        <input
          value={port.key}
          onChange={(e) => onChange({ ...port, key: e.target.value })}
          className="agentdash-form-input font-mono text-xs"
          placeholder="port_key"
        />
      </div>
      <div>
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={port.description}
          onChange={(e) => onChange({ ...port, description: e.target.value })}
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
            onChange({ ...port, context_strategy: e.target.value as ContextStrategy })
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
    </PortCard>
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
            <OutputPortCard
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
            <InputPortCard
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
