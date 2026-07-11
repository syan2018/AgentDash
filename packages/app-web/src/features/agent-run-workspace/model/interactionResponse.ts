import type {
  InteractionResponse,
  JsonValue,
  RuntimeInteractionKind,
} from "../../../generated/agent-runtime-contracts";

function isJsonValue(value: unknown): value is JsonValue {
  if (value == null || typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return true;
  }
  if (Array.isArray(value)) return value.every(isJsonValue);
  if (typeof value !== "object") return false;
  return Object.values(value).every(isJsonValue);
}

export type InteractionResponseInputResult =
  | { ok: true; response: InteractionResponse }
  | { ok: false; error: string };

export function interactionResponseFromText(
  kind: RuntimeInteractionKind,
  input: string,
): InteractionResponseInputResult {
  const trimmed = input.trim();
  if (kind === "user_input_request") {
    if (!trimmed) return { ok: false, error: "请输入要提交给 Runtime 的内容。" };
    return {
      ok: true,
      response: { kind: "user_input", input: [{ kind: "text", text: trimmed }] },
    };
  }
  if (kind !== "mcp_elicitation" && kind !== "dynamic_tool_execution") {
    return { ok: false, error: "该 interaction 不接收文本 payload。" };
  }
  if (!trimmed) return { ok: false, error: "请输入明确的 JSON payload。" };
  try {
    const value: unknown = JSON.parse(trimmed);
    if (!isJsonValue(value)) return { ok: false, error: "JSON payload 包含不支持的值。" };
    return kind === "mcp_elicitation"
      ? { ok: true, response: { kind: "mcp_elicitation", value } }
      : { ok: true, response: { kind: "dynamic_tool_result", output: value } };
  } catch {
    return { ok: false, error: "请输入有效 JSON。" };
  }
}
