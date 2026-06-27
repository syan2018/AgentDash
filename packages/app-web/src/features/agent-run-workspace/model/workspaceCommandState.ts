import type { JsonValue } from "../../../generated/common-contracts";
import type { ProjectAgentExecutor } from "../../../types";

export interface InFlightAgentRunCommand {
  key: string;
  id: string;
}

export interface ResolvedAgentRunClientCommand {
  clientCommandId: string;
  inFlightCommand: InFlightAgentRunCommand;
}

export function resolveAgentRunClientCommandId(
  current: InFlightAgentRunCommand | null,
  commandKey: string,
  createId: () => string,
): ResolvedAgentRunClientCommand {
  if (current?.key === commandKey) {
    return {
      clientCommandId: current.id,
      inFlightCommand: current,
    };
  }
  const id = createId();
  return {
    clientCommandId: id,
    inFlightCommand: { key: commandKey, id },
  };
}

function stringField(record: { [key in string]?: JsonValue }, key: string): string | undefined {
  const value = record[key];
  return typeof value === "string" ? value : undefined;
}

function thinkingLevelField(
  record: { [key in string]?: JsonValue },
  key: string,
): ProjectAgentExecutor["thinking_level"] {
  const value = record[key];
  if (typeof value !== "string") return undefined;
  switch (value) {
    case "off":
    case "minimal":
    case "low":
    case "medium":
    case "high":
    case "xhigh":
      return value;
    default:
      return undefined;
  }
}

export function executorSourceFromExecutionProfile(value: JsonValue | undefined): ProjectAgentExecutor | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const record: { [key in string]?: JsonValue } = value;
  return {
    executor: stringField(record, "executor") ?? "",
    provider_id: stringField(record, "provider_id"),
    model_id: stringField(record, "model_id"),
    agent_id: stringField(record, "agent_id"),
    thinking_level: thinkingLevelField(record, "thinking_level"),
    permission_policy: stringField(record, "permission_policy"),
  };
}
