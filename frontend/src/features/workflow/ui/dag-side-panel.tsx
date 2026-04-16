import { useState, useMemo, useCallback } from "react";

import type {
  LifecycleStepDefinition,
  LifecycleNodeType,
  WorkflowDefinition,
  OutputPortDefinition,
  InputPortDefinition,
  GateStrategy,
  ContextStrategy,
} from "../../../types";

// ─── Tab 枚举 ───

type TabKey = "basic" | "output_ports" | "input_ports";

const TAB_LABELS: Record<TabKey, string> = {
  basic: "基本信息",
  output_ports: "Output Ports",
  input_ports: "Input Ports",
};

// ─── Props ───

export interface DagSidePanelProps {
  step: LifecycleStepDefinition;
  /** 是否为入口节点 */
  isEntry: boolean;
  /** 可选择的 workflow 列表 */
  availableWorkflows: WorkflowDefinition[];
  /** 当前绑定的 workflow 的 contract ports */
  outputPorts: OutputPortDefinition[];
  inputPorts: InputPortDefinition[];
  onChange: (patch: Partial<LifecycleStepDefinition>) => void;
  onRemove: () => void;
  onClose: () => void;
  /** 设为入口节点 */
  onSetEntry: () => void;
  /** port 变更回调 — 编辑器负责同步到 WorkflowDefinition draft */
  onOutputPortsChange: (ports: OutputPortDefinition[]) => void;
  onInputPortsChange: (ports: InputPortDefinition[]) => void;
}

/**
 * DAG 编辑器的节点配置侧面板。
 * 包含三个 Tab：基本信息、Output Ports、Input Ports。
 */
export function DagSidePanel({
  step,
  isEntry,
  availableWorkflows,
  outputPorts,
  inputPorts,
  onChange,
  onRemove,
  onClose,
  onSetEntry,
  onOutputPortsChange,
  onInputPortsChange,
}: DagSidePanelProps) {
  const [activeTab, setActiveTab] = useState<TabKey>("basic");
  const isAgentNode = (step.node_type ?? "agent_node") === "agent_node";

  const tabKeys = useMemo<TabKey[]>(() => {
    if (isAgentNode) return ["basic", "output_ports", "input_ports"];
    return ["basic"];
  }, [isAgentNode]);

  return (
    <div className="flex h-full flex-col border-l border-border bg-background">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-border px-4 py-3">
        <div className="overflow-hidden">
          <p className="truncate text-sm font-semibold text-foreground">
            {step.key || "(no key)"}
          </p>
          {isEntry ? (
            <p className="text-[10px] text-emerald-600">入口节点</p>
          ) : (
            <button
              type="button"
              onClick={onSetEntry}
              className="mt-0.5 rounded-[6px] px-1.5 py-0.5 text-[10px] text-primary transition-colors hover:bg-primary/10"
            >
              设为入口
            </button>
          )}
        </div>
        <button
          type="button"
          onClick={onClose}
          className="rounded-[8px] p-1 text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
          title="关闭面板"
        >
          <svg xmlns="http://www.w3.org/2000/svg" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M18 6 6 18"/><path d="m6 6 12 12"/></svg>
        </button>
      </div>

      {/* Tabs */}
      <div className="flex border-b border-border">
        {tabKeys.map((key) => (
          <button
            key={key}
            type="button"
            onClick={() => setActiveTab(key)}
            className={`flex-1 px-3 py-2 text-xs font-medium transition-colors ${
              activeTab === key
                ? "border-b-2 border-primary text-foreground"
                : "text-muted-foreground hover:text-foreground"
            }`}
          >
            {TAB_LABELS[key]}
          </button>
        ))}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4">
        {activeTab === "basic" && (
          <BasicInfoTab
            step={step}
            availableWorkflows={availableWorkflows}
            onChange={onChange}
          />
        )}
        {activeTab === "output_ports" && isAgentNode && (
          <OutputPortsTab
            ports={outputPorts}
            onChange={onOutputPortsChange}
          />
        )}
        {activeTab === "input_ports" && isAgentNode && (
          <InputPortsTab
            ports={inputPorts}
            onChange={onInputPortsChange}
          />
        )}
      </div>

      {/* Footer — 删除按钮 */}
      <div className="border-t border-border px-4 py-3">
        <button
          type="button"
          onClick={onRemove}
          className="w-full rounded-[8px] border border-destructive/30 px-3 py-2 text-xs text-destructive transition-colors hover:bg-destructive/5"
        >
          删除此节点
        </button>
      </div>
    </div>
  );
}

// ─── Basic Info Tab ─────────────────────────────────────

function BasicInfoTab({
  step,
  availableWorkflows,
  onChange,
}: {
  step: LifecycleStepDefinition;
  availableWorkflows: WorkflowDefinition[];
  onChange: (patch: Partial<LifecycleStepDefinition>) => void;
}) {
  const nodeType = step.node_type ?? "agent_node";

  return (
    <div className="space-y-4">
      <div>
        <label className="agentdash-form-label">Node Key</label>
        <input
          value={step.key}
          onChange={(e) => onChange({ key: e.target.value })}
          className="agentdash-form-input"
          placeholder="implement"
        />
        <p className="mt-1 text-[10px] text-muted-foreground">
          lifecycle 内唯一标识，用作 edge 连接引用
        </p>
      </div>

      <div>
        <label className="agentdash-form-label">描述</label>
        <textarea
          value={step.description}
          onChange={(e) => onChange({ description: e.target.value })}
          rows={3}
          className="agentdash-form-textarea"
          placeholder="当前节点的职责与边界"
        />
      </div>

      <div>
        <label className="agentdash-form-label">节点类型</label>
        <select
          value={nodeType}
          onChange={(e) => onChange({ node_type: e.target.value as LifecycleNodeType })}
          className="agentdash-form-select"
        >
          <option value="agent_node">Agent Node</option>
          <option value="phase_node">Phase Node（运行时待完善）</option>
        </select>
      </div>

      <div>
        <label className="agentdash-form-label">Workflow</label>
        <select
          value={step.workflow_key ?? ""}
          onChange={(e) => onChange({ workflow_key: e.target.value || null })}
          className="agentdash-form-select"
        >
          <option value="">— 未绑定 —</option>
          {availableWorkflows.map((wf) => (
            <option key={wf.id} value={wf.key}>
              {wf.name} ({wf.key})
            </option>
          ))}
        </select>
        <p className="mt-1 text-[10px] text-muted-foreground">
          绑定已发布的 Workflow 定义以驱动该步。
        </p>
      </div>
    </div>
  );
}

// ─── Output Ports Tab ───────────────────────────────────

const GATE_STRATEGY_LABEL: Record<GateStrategy, string> = {
  existence: "文件存在即通过",
  schema: "Schema 校验（预留）",
  llm_judge: "LLM 评估（预留）",
};

function OutputPortsTab({
  ports,
  onChange,
}: {
  ports: OutputPortDefinition[];
  onChange: (ports: OutputPortDefinition[]) => void;
}) {
  const handleAdd = useCallback(() => {
    onChange([...ports, { key: "", description: "", gate_strategy: "existence" }]);
  }, [ports, onChange]);

  const handleRemove = useCallback(
    (idx: number) => onChange(ports.filter((_, i) => i !== idx)),
    [ports, onChange],
  );

  const handleUpdate = useCallback(
    (idx: number, patch: Partial<OutputPortDefinition>) =>
      onChange(ports.map((p, i) => (i === idx ? { ...p, ...patch } : p))),
    [ports, onChange],
  );

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <p className="text-xs font-medium text-muted-foreground">
          Output Ports ({ports.length})
        </p>
        <button
          type="button"
          onClick={handleAdd}
          className="agentdash-button-secondary px-2 py-1 text-xs"
        >
          + 添加
        </button>
      </div>

      <p className="text-[10px] text-muted-foreground">
        port_key 在整个 lifecycle 内必须全局唯一。
      </p>

      {ports.map((port, idx) => (
        <div key={idx} className="space-y-2 rounded-[10px] border border-border bg-secondary/20 p-3">
          <div className="flex items-start justify-between gap-2">
            <div className="flex-1 space-y-2">
              <div>
                <label className="agentdash-form-label">Port Key</label>
                <input
                  value={port.key}
                  onChange={(e) => handleUpdate(idx, { key: e.target.value })}
                  className="agentdash-form-input"
                  placeholder="summary_output"
                />
              </div>
              <div>
                <label className="agentdash-form-label">描述</label>
                <input
                  value={port.description}
                  onChange={(e) => handleUpdate(idx, { description: e.target.value })}
                  className="agentdash-form-input"
                  placeholder="产出摘要"
                />
              </div>
              <div>
                <label className="agentdash-form-label">门禁策略</label>
                <select
                  value={port.gate_strategy ?? "existence"}
                  onChange={(e) => handleUpdate(idx, { gate_strategy: e.target.value as GateStrategy })}
                  className="agentdash-form-select"
                >
                  {(Object.entries(GATE_STRATEGY_LABEL) as [GateStrategy, string][]).map(([k, v]) => (
                    <option key={k} value={k} disabled={k !== "existence"}>
                      {v}
                    </option>
                  ))}
                </select>
              </div>
            </div>
            <button
              type="button"
              onClick={() => handleRemove(idx)}
              className="mt-5 shrink-0 rounded-[6px] p-1 text-destructive/60 transition-colors hover:bg-destructive/5 hover:text-destructive"
              title="删除此 port"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>
            </button>
          </div>
        </div>
      ))}

      {ports.length === 0 && (
        <p className="py-3 text-center text-xs text-muted-foreground">暂无 output port</p>
      )}
    </div>
  );
}

// ─── Input Ports Tab ────────────────────────────────────

const CONTEXT_STRATEGY_LABEL: Record<ContextStrategy, string> = {
  full: "完整内容注入",
  summary: "摘要注入（预留）",
  metadata_only: "仅元信息（预留）",
  custom: "自定义模板（预留）",
};

function InputPortsTab({
  ports,
  onChange,
}: {
  ports: InputPortDefinition[];
  onChange: (ports: InputPortDefinition[]) => void;
}) {
  const handleAdd = useCallback(() => {
    onChange([...ports, { key: "", description: "", context_strategy: "full" }]);
  }, [ports, onChange]);

  const handleRemove = useCallback(
    (idx: number) => onChange(ports.filter((_, i) => i !== idx)),
    [ports, onChange],
  );

  const handleUpdate = useCallback(
    (idx: number, patch: Partial<InputPortDefinition>) =>
      onChange(ports.map((p, i) => (i === idx ? { ...p, ...patch } : p))),
    [ports, onChange],
  );

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between">
        <p className="text-xs font-medium text-muted-foreground">
          Input Ports ({ports.length})
        </p>
        <button
          type="button"
          onClick={handleAdd}
          className="agentdash-button-secondary px-2 py-1 text-xs"
        >
          + 添加
        </button>
      </div>

      <p className="text-[10px] text-muted-foreground">
        每个 input port 只能接收一条 edge（单一数据源）。
      </p>

      {ports.map((port, idx) => (
        <div key={idx} className="space-y-2 rounded-[10px] border border-border bg-secondary/20 p-3">
          <div className="flex items-start justify-between gap-2">
            <div className="flex-1 space-y-2">
              <div>
                <label className="agentdash-form-label">Port Key</label>
                <input
                  value={port.key}
                  onChange={(e) => handleUpdate(idx, { key: e.target.value })}
                  className="agentdash-form-input"
                  placeholder="context_input"
                />
              </div>
              <div>
                <label className="agentdash-form-label">描述</label>
                <input
                  value={port.description}
                  onChange={(e) => handleUpdate(idx, { description: e.target.value })}
                  className="agentdash-form-input"
                  placeholder="接收上游产出的上下文"
                />
              </div>
              <div>
                <label className="agentdash-form-label">上下文策略</label>
                <select
                  value={port.context_strategy ?? "full"}
                  onChange={(e) => handleUpdate(idx, { context_strategy: e.target.value as ContextStrategy })}
                  className="agentdash-form-select"
                >
                  {(Object.entries(CONTEXT_STRATEGY_LABEL) as [ContextStrategy, string][]).map(([k, v]) => (
                    <option key={k} value={k} disabled={k !== "full"}>
                      {v}
                    </option>
                  ))}
                </select>
              </div>
              {port.context_strategy === "custom" && (
                <div>
                  <label className="agentdash-form-label">Prompt 模板</label>
                  <textarea
                    value={port.context_template ?? ""}
                    onChange={(e) => handleUpdate(idx, { context_template: e.target.value || null })}
                    rows={4}
                    className="agentdash-form-textarea font-mono text-xs"
                    placeholder={"请基于以下内容进行分析：\n\n{{artifact.content}}"}
                  />
                  <p className="mt-1 text-[10px] text-muted-foreground">
                    {"可用变量：{{artifact.content}}、{{artifact.title}}"}
                  </p>
                </div>
              )}
            </div>
            <button
              type="button"
              onClick={() => handleRemove(idx)}
              className="mt-5 shrink-0 rounded-[6px] p-1 text-destructive/60 transition-colors hover:bg-destructive/5 hover:text-destructive"
              title="删除此 port"
            >
              <svg xmlns="http://www.w3.org/2000/svg" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round"><path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/></svg>
            </button>
          </div>
        </div>
      ))}

      {ports.length === 0 && (
        <p className="py-3 text-center text-xs text-muted-foreground">暂无 input port</p>
      )}
    </div>
  );
}
