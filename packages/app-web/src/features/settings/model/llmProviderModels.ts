import type { JsonValue } from "../../../api/llmProviders";

export interface LlmProviderModelConfig {
  id: string;
  name: string;
  context_window: number;
  reasoning: boolean;
  supports_image: boolean;
}

interface ProviderSlugRef {
  slug: string;
}

interface ProviderModelSlugRef {
  provider_id?: string | null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function parseLlmProviderModelConfigs(value: unknown): LlmProviderModelConfig[] {
  if (!Array.isArray(value)) return [];
  return value.flatMap((item) => {
    if (!isRecord(item)) return [];
    const id = String(item.id ?? "").trim();
    if (!id) return [];
    return [{
      id,
      name: String(item.name ?? "").trim(),
      context_window: Number(item.context_window ?? 200000) || 200000,
      reasoning: item.reasoning !== false,
      supports_image: item.supports_image !== false,
    }];
  });
}

export function parseLlmProviderBlockedModels(value: unknown): string[] {
  if (Array.isArray(value)) {
    return value
      .map((item) => String(item).trim())
      .filter((item) => item.length > 0);
  }
  if (typeof value === "string") {
    return value
      .split(/\r?\n|,/)
      .map((item) => item.trim())
      .filter((item) => item.length > 0);
  }
  return [];
}

export function serializeLlmProviderModelConfigs(models: LlmProviderModelConfig[]): JsonValue {
  return models.map((model): JsonValue => ({
    id: model.id,
    name: model.name,
    context_window: model.context_window,
    reasoning: model.reasoning,
    supports_image: model.supports_image,
  }));
}

export function serializeLlmProviderBlockedModels(blockedModels: string[]): JsonValue {
  return blockedModels;
}

export function modelBelongsToProviderSlug(
  model: ProviderModelSlugRef,
  provider: ProviderSlugRef,
): boolean {
  return (model.provider_id ?? "") === provider.slug;
}
