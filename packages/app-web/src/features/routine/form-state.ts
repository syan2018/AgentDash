import type { Routine, RoutineTriggerType, RoutineDispatchMode } from "../../types";
import type { CreateRoutineRequest } from "../../generated/routine-contracts";

export interface RoutineFormState {
  name: string;
  prompt_template: string;
  project_agent_id: string;
  trigger_type: RoutineTriggerType;
  cron_expression: string;
  provider_key: string;
  provider_config_json: string;
  dispatch_mode: RoutineDispatchMode;
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
  dispatch_mode: "fresh",
  entity_key_path: "",
};

export function routineToForm(r: Routine): RoutineFormState {
  const triggerConfig = r.trigger_config;
  const dispatchStrategy = r.dispatch_strategy;

  return {
    name: r.name,
    prompt_template: r.prompt_template,
    project_agent_id: r.project_agent_id,
    trigger_type: triggerConfig.type,
    cron_expression: triggerConfig.type === "scheduled" ? triggerConfig.cron_expression : "0 9 * * *",
    provider_key: triggerConfig.type === "plugin" ? triggerConfig.provider_key : "",
    provider_config_json: triggerConfig.type === "plugin"
      ? JSON.stringify(triggerConfig.provider_config, null, 2)
      : "{}",
    dispatch_mode: dispatchStrategy.mode,
    entity_key_path: dispatchStrategy.mode === "per_entity" ? dispatchStrategy.entity_key_path : "",
  };
}

export function formToPayload(form: RoutineFormState): CreateRoutineRequest {
  let trigger_config: CreateRoutineRequest["trigger_config"];
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

  const dispatch_strategy: CreateRoutineRequest["dispatch_strategy"] =
    form.dispatch_mode === "per_entity"
      ? { mode: "per_entity", entity_key_path: form.entity_key_path.trim() }
      : { mode: form.dispatch_mode };

  return {
    name: form.name,
    prompt_template: form.prompt_template,
    project_agent_id: form.project_agent_id,
    trigger_config,
    dispatch_strategy,
  };
}

export function validateForm(form: RoutineFormState): string | null {
  if (!form.name.trim()) return "名称不能为空";
  if (!form.prompt_template.trim()) return "执行指令不能为空";
  if (!form.project_agent_id) return "请选择执行 Agent";
  if (form.trigger_type === "scheduled" && !form.cron_expression.trim()) return "请配置定时表达式";
  if (form.trigger_type === "plugin" && !form.provider_key.trim()) return "请输入 provider_key";
  if (form.dispatch_mode === "per_entity" && !form.entity_key_path.trim()) return "Per-Entity 模式需要指定 entity_key_path";
  return null;
}
