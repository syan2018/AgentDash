import { useMemo } from "react";
import type { AgentBinding, ProjectConfig } from "../../types";
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
  binding: AgentBinding,
  projectConfig: ProjectConfig | undefined,
  discovered: Array<{ id: string; name: string; available: boolean }>,
): AgentTypeOption[] {
  const options = new Map<string, AgentTypeOption>();

  for (const executor of discovered) {
    const suffix = executor.available ? "" : "（不可用）";
    options.set(executor.id, {
      value: executor.id,
      label: `${executor.name}${suffix}`,
    });
  }

  const appendRawOption = (raw: string | null | undefined, labelPrefix?: string) => {
    const value = raw?.trim() ?? "";
    if (!value || options.has(value)) return;
    const label = labelPrefix ? `${labelPrefix}: ${value}` : value;
    options.set(value, { value, label });
  };

  appendRawOption(projectConfig?.default_agent_type, "项目默认");
  for (const preset of projectConfig?.agent_presets ?? []) {
    appendRawOption(preset.agent_type, `预设 ${preset.name}`);
  }
  appendRawOption(binding.agent_type, "当前值");

  return Array.from(options.values()).sort((a, b) => a.label.localeCompare(b.label));
}

export function AgentBindingFields({ value, projectConfig, onChange }: AgentBindingFieldsProps) {
  const { executors, isLoading, error } = useExecutorDiscovery();

  const agentTypeOptions = useMemo(
    () => buildAgentTypeOptions(value, projectConfig, executors),
    [value, projectConfig, executors],
  );
  const presets = projectConfig?.agent_presets ?? [];

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
      agent_type: preset?.agent_type ?? value.agent_type ?? null,
    });
  };

  return (
    <div className="space-y-3">
      <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
        <div>
          <label className="mb-1 block text-xs text-muted-foreground">Agent 类型</label>
          <select
            value={value.agent_type ?? ""}
            onChange={(event) => updateBinding({ agent_type: event.target.value || null })}
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
          >
            <option value="">
              {isLoading ? "加载 Agent 类型中..." : "使用项目默认 / 预设推导"}
            </option>
            {agentTypeOptions.map((option) => (
              <option key={option.value} value={option.value}>
                {option.label}
              </option>
            ))}
          </select>
        </div>

        <div>
          <label className="mb-1 block text-xs text-muted-foreground">Agent 预设</label>
          <select
            value={value.preset_name ?? ""}
            onChange={(event) => handlePresetChange(event.target.value)}
            className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
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

      <div>
        <label className="mb-1 block text-xs text-muted-foreground">Prompt 模板</label>
        <textarea
          value={value.prompt_template ?? ""}
          onChange={(event) => updateBinding({ prompt_template: event.target.value || null })}
          rows={3}
          placeholder="留空则使用系统默认模板"
          className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
        />
      </div>

      <div>
        <label className="mb-1 block text-xs text-muted-foreground">Initial Context</label>
        <textarea
          value={value.initial_context ?? ""}
          onChange={(event) => updateBinding({ initial_context: event.target.value || null })}
          rows={3}
          placeholder="可补充执行前置约束、上下文说明"
          className="w-full rounded border border-border bg-background px-2 py-1.5 text-sm outline-none ring-ring focus:ring-1"
        />
      </div>

      {error && (
        <p className="text-xs text-amber-600">
          Agent 枚举加载失败：{error.message}
        </p>
      )}
    </div>
  );
}
