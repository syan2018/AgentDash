import type { Routine, RoutineTriggerType, RoutineSessionMode } from "../../types";

export interface RoutineFormState {
  name: string;
  prompt_template: string;
  project_agent_id: string;
  trigger_type: RoutineTriggerType;
  cron_expression: string;
  provider_key: string;
  provider_config_json: string;
  session_mode: RoutineSessionMode;
  entity_key_path: string;
}

export const INITIAL_FORM: RoutineFormState = {
  name: "",
  prompt_template: "",
  project_agent_id: "",
  trigger_type: "scheduled",
  cron_expression: "0 9 * * *",
  provider_key: "",
  provider_config_json: "{}",
  session_mode: "fresh",
  entity_key_path: "",
};

export function routineToForm(r: Routine): RoutineFormState {
  return {
    name: r.name,
    prompt_template: r.prompt_template,
    project_agent_id: r.project_agent_id,
    trigger_type: r.trigger_config.type,
    cron_expression: r.trigger_config.cron_expression ?? "0 9 * * *",
    provider_key: r.trigger_config.provider_key ?? "",
    provider_config_json: r.trigger_config.provider_config
      ? JSON.stringify(r.trigger_config.provider_config, null, 2)
      : "{}",
    session_mode: r.session_strategy.mode,
    entity_key_path: r.session_strategy.entity_key_path ?? "",
  };
}

export function formToPayload(form: RoutineFormState): {
  name: string;
  prompt_template: string;
  project_agent_id: string;
  trigger_config: Record<string, unknown>;
  session_strategy: Record<string, unknown>;
} {
  let trigger_config: Record<string, unknown>;
  switch (form.trigger_type) {
    case "scheduled":
      trigger_config = { type: "scheduled", cron_expression: form.cron_expression };
      break;
    case "webhook":
      trigger_config = { type: "webhook" };
      break;
    case "plugin":
      trigger_config = {
        type: "plugin",
        provider_key: form.provider_key,
        provider_config: JSON.parse(form.provider_config_json || "{}"),
      };
      break;
  }

  const session_strategy: Record<string, unknown> = { mode: form.session_mode };
  if (form.session_mode === "per_entity" && form.entity_key_path.trim()) {
    session_strategy.entity_key_path = form.entity_key_path.trim();
  }

  return {
    name: form.name,
    prompt_template: form.prompt_template,
    project_agent_id: form.project_agent_id,
    trigger_config,
    session_strategy,
  };
}

export function validateForm(form: RoutineFormState): string | null {
  if (!form.name.trim()) return "名称不能为空";
  if (!form.prompt_template.trim()) return "执行指令不能为空";
  if (!form.project_agent_id) return "请选择执行 Agent";
  if (form.trigger_type === "scheduled" && !form.cron_expression.trim()) return "请配置定时表达式";
  if (form.trigger_type === "plugin" && !form.provider_key.trim()) return "请输入 provider_key";
  if (form.session_mode === "per_entity" && !form.entity_key_path.trim()) return "Per-Entity 模式需要指定 entity_key_path";
  return null;
}
