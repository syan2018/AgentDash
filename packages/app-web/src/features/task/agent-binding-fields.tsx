import { useMemo } from "react";
import type { AgentBinding, ProjectConfig, ThinkingLevel } from "../../types";
import { THINKING_LEVEL_OPTIONS } from "../../types";
import { useExecutorDiscovery } from "../executor-selector";

export interface AgentBindingFieldsProps {
  value: AgentBinding;
  projectConfig?: ProjectConfig;
  onChange: (value: AgentBinding) => void;
}

interface AgentTypeOption {
  value: string;
  label: string;
}

function buildAgentTypeOptions(
  discovered: Array<{ id: string; name: string; available: boolean; backend_ids?: string[] }>,
): AgentTypeOption[] {
  const options = new Map<string, AgentTypeOption>();

  for (const executor of discovered) {
    const tags: string[] = [];
    if (!executor.available) tags.push("不可用");
    if (executor.backend_ids && executor.backend_ids.length > 0) {
      tags.push(`远程: ${executor.backend_ids.join(", ")}`);
    }
    const suffix = tags.length > 0 ? `（${tags.join(" · ")}）` : "";
    options.set(executor.id, {
      value: executor.id,
      label: `${executor.name}${suffix}`,
    });
  }

  return Array.from(options.values());
}

function normalizeOptionalText(value: string | null | undefined): string | null {
  const trimmed = value?.trim() ?? "";
  return trimmed.length > 0 ? trimmed : null;
}

export function AgentBindingFields({ value, projectConfig, onChange }: AgentBindingFieldsProps) {
  const { executors, isLoading, error } = useExecutorDiscovery();

  const agentTypeOptions = useMemo(
    () => buildAgentTypeOptions(executors),
    [executors],
  );
  const presets = projectConfig?.agent_presets ?? [];
  const projectDefaultAgentType = normalizeOptionalText(projectConfig?.default_agent_type);
  const hasSelectedAgentType = agentTypeOptions.some((option) => option.value === value.agent_type);

  const updateBinding = (patch: Partial<AgentBinding>) => {
    onChange({ ...value, ...patch });
  };

  const handlePresetChange = (presetName: string) => {
    if (!presetName) {
      updateBinding({ preset_name: null });
      return;
    }
    const preset = presets.find((item) => item.name === presetName);
    updateBinding({
      preset_name: presetName,
      agent_type: normalizeOptionalText(preset?.agent_type),
    });
  };

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
        <div>
          <label className="agentdash-form-label">Agent 类型</label>
          <select
            value={value.agent_type ?? ""}
            onChange={(event) => updateBinding({ agent_type: event.target.value || null })}
            className="agentdash-form-select"
          >
            <option value="">
              {isLoading ? "加载 Agent 类型中..." : "请选择显式 Agent 类型"}
            </option>
            {agentTypeOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>

        <div>
          <label className="agentdash-form-label">Agent 预设</label>
          <select
            value={value.preset_name ?? ""}
            onChange={(event) => handlePresetChange(event.target.value)}
            className="agentdash-form-select"
          >
            <option value="">不使用预设</option>
            {presets.map((preset) => (
              <option key={preset.name} value={preset.name}>
                {preset.name}
              </option>
            ))}
          </select>
        </div>
      </div>

      {projectDefaultAgentType && (
        <p className="text-xs text-muted-foreground">
          Project 默认 Agent 为 <span className="font-mono">{projectDefaultAgentType}</span>，但这里不会再自动代填；如需绑定，请显式选择。
        </p>
      )}

      {value.agent_type && !hasSelectedAgentType && (
        <p className="text-xs text-amber-600">
          当前绑定的 Agent 类型 <span className="font-mono">{value.agent_type}</span> 不在执行器发现结果中，请修正配置。
        </p>
      )}

      <div>
        <label className="agentdash-form-label">Prompt 模板</label>
        <textarea
          value={value.prompt_template ?? ""}
          onChange={(event) => updateBinding({ prompt_template: event.target.value || null })}
          rows={3}
          placeholder="留空则使用系统默认模板"
          className="agentdash-form-textarea"
        />
      </div>

      <div>
        <label className="agentdash-form-label">Initial Context</label>
        <textarea
          value={value.initial_context ?? ""}
          onChange={(event) => updateBinding({ initial_context: event.target.value || null })}
          rows={3}
          placeholder="可补充执行前置约束、上下文说明"
          className="agentdash-form-textarea"
        />
      </div>

      <div>
        <label className="agentdash-form-label">推理级别</label>
        <select
          value={value.thinking_level ?? ""}
          onChange={(event) =>
            updateBinding({
              thinking_level: (event.target.value as ThinkingLevel) || undefined,
            })
          }
          className="agentdash-form-select"
        >
          <option value="">默认（不指定）</option>
          {THINKING_LEVEL_OPTIONS.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      </div>

      {error && (
        <p className="text-xs text-amber-600">
          Agent 枚举加载失败：{error.message}
        </p>
      )}
    </div>
  );
}
