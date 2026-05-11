/**
 * Ports Panel —— output_ports + input_ports 编辑。
 *
 * Output ports 同时作为完成门禁（全部交付才可推进）；
 * Input ports 声明本 workflow 所需的外部数据。
 *
 * 受控组件：接收数组 + 两个 onChange callback，不自带 store 依赖。
 */

import type {
  ContextStrategy,
  GateStrategy,
  InputPortDefinition,
  OutputPortDefinition,
} from "../../../../types";
import { DetailSection } from "../../../../components/ui/detail-panel";

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

function PortsEditor({
  outputPorts,
  inputPorts,
  onOutputChange,
  onInputChange,
}: {
  outputPorts: OutputPortDefinition[];
  inputPorts: InputPortDefinition[];
  onOutputChange: (ports: OutputPortDefinition[]) => void;
  onInputChange: (ports: InputPortDefinition[]) => void;
}) {
  return (
    <div className="space-y-4">
      {/* Output */}
      <div>
        <div className="flex items-center justify-between">
          <p className="text-xs font-medium text-muted-foreground">
            Output Ports ({outputPorts.length})
          </p>
          <button
            type="button"
            onClick={() =>
              onOutputChange([
                ...outputPorts,
                { key: "", description: "", gate_strategy: "existence" },
              ])
            }
            className="agentdash-button-secondary px-2 py-1 text-xs"
          >
            + 添加
          </button>
        </div>
        <div className="mt-2 space-y-2">
          {outputPorts.map((p, idx) => (
            <div key={idx} className="flex items-center gap-2">
              <input
                value={p.key}
                onChange={(e) => {
                  const n = [...outputPorts];
                  n[idx] = { ...p, key: e.target.value };
                  onOutputChange(n);
                }}
                className="agentdash-form-input flex-1"
                placeholder="port key"
              />
              <input
                value={p.description}
                onChange={(e) => {
                  const n = [...outputPorts];
                  n[idx] = { ...p, description: e.target.value };
                  onOutputChange(n);
                }}
                className="agentdash-form-input flex-1"
                placeholder="描述"
              />
              <select
                value={p.gate_strategy ?? "existence"}
                onChange={(e) => {
                  const n = [...outputPorts];
                  n[idx] = { ...p, gate_strategy: e.target.value as GateStrategy };
                  onOutputChange(n);
                }}
                className="agentdash-form-select w-28"
              >
                {(Object.entries(GATE_LABEL) as [GateStrategy, string][]).map(([k, v]) => (
                  <option key={k} value={k}>
                    {v}
                  </option>
                ))}
              </select>
              <button
                type="button"
                onClick={() => onOutputChange(outputPorts.filter((_, i) => i !== idx))}
                className="shrink-0 rounded-[6px] p-1 text-destructive/60 hover:bg-destructive/5 hover:text-destructive"
              >
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
                >
                  <path d="M3 6h18" />
                  <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
                  <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
                </svg>
              </button>
            </div>
          ))}
          {outputPorts.length === 0 && (
            <p className="py-2 text-center text-xs text-muted-foreground">暂无 output port</p>
          )}
        </div>
      </div>

      {/* Input */}
      <div>
        <div className="flex items-center justify-between">
          <p className="text-xs font-medium text-muted-foreground">
            Input Ports ({inputPorts.length})
          </p>
          <button
            type="button"
            onClick={() =>
              onInputChange([
                ...inputPorts,
                { key: "", description: "", context_strategy: "full" },
              ])
            }
            className="agentdash-button-secondary px-2 py-1 text-xs"
          >
            + 添加
          </button>
        </div>
        <div className="mt-2 space-y-2">
          {inputPorts.map((p, idx) => (
            <div key={idx} className="flex items-center gap-2">
              <input
                value={p.key}
                onChange={(e) => {
                  const n = [...inputPorts];
                  n[idx] = { ...p, key: e.target.value };
                  onInputChange(n);
                }}
                className="agentdash-form-input flex-1"
                placeholder="port key"
              />
              <input
                value={p.description}
                onChange={(e) => {
                  const n = [...inputPorts];
                  n[idx] = { ...p, description: e.target.value };
                  onInputChange(n);
                }}
                className="agentdash-form-input flex-1"
                placeholder="描述"
              />
              <select
                value={p.context_strategy ?? "full"}
                onChange={(e) => {
                  const n = [...inputPorts];
                  n[idx] = { ...p, context_strategy: e.target.value as ContextStrategy };
                  onInputChange(n);
                }}
                className="agentdash-form-select w-28"
              >
                {(Object.entries(CTX_LABEL) as [ContextStrategy, string][]).map(([k, v]) => (
                  <option key={k} value={k}>
                    {v}
                  </option>
                ))}
              </select>
              <button
                type="button"
                onClick={() => onInputChange(inputPorts.filter((_, i) => i !== idx))}
                className="shrink-0 rounded-[6px] p-1 text-destructive/60 hover:bg-destructive/5 hover:text-destructive"
              >
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
                >
                  <path d="M3 6h18" />
                  <path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6" />
                  <path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2" />
                </svg>
              </button>
            </div>
          ))}
          {inputPorts.length === 0 && (
            <p className="py-2 text-center text-xs text-muted-foreground">暂无 input port</p>
          )}
        </div>
      </div>
    </div>
  );
}

// ─── Panel 外壳 ───────────────────────────────────────

export interface PortsPanelProps {
  outputPorts: OutputPortDefinition[];
  inputPorts: InputPortDefinition[];
  onOutputChange: (ports: OutputPortDefinition[]) => void;
  onInputChange: (ports: InputPortDefinition[]) => void;
}

export function PortsPanel({
  outputPorts,
  inputPorts,
  onOutputChange,
  onInputChange,
}: PortsPanelProps) {
  return (
    <DetailSection
      title={`Ports (${outputPorts.length + inputPorts.length})`}
      description="Output ports 同时作为完成门禁（全部交付才可推进）；Input ports 声明所需的外部数据。"
    >
      <PortsEditor
        outputPorts={outputPorts}
        inputPorts={inputPorts}
        onOutputChange={onOutputChange}
        onInputChange={onInputChange}
      />
    </DetailSection>
  );
}
