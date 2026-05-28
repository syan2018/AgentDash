import { useCallback, useEffect, useMemo, useState } from "react";
import { llmProvidersApi, type EffectiveLlmProvider, type JsonValue } from "../../../api/llmProviders";
import type { ModelInfo, ModelSelectorConfig } from "./types";

const DEFAULT_CONTEXT_WINDOW = 200_000;

interface UseEffectiveLlmModelSelectorResult {
  modelSelector: ModelSelectorConfig;
  loading: boolean;
  error: Error | null;
  refetch: () => void;
}

const EMPTY_MODEL_SELECTOR: ModelSelectorConfig = {
  providers: [],
  models: [],
  default_model: null,
  agents: [],
  permissions: [],
};

export function useEffectiveLlmModelSelector(enabled: boolean): UseEffectiveLlmModelSelectorResult {
  const [providers, setProviders] = useState<EffectiveLlmProvider[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);

  useEffect(() => {
    if (!enabled) {
      setProviders([]);
      setLoading(false);
      setError(null);
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);

    void (async () => {
      try {
        const next = await llmProvidersApi.listEffective();
        if (!cancelled) setProviders(next);
      } catch (e) {
        if (!cancelled) {
          setProviders([]);
          setError(e instanceof Error ? e : new Error(String(e)));
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [enabled, refreshKey]);

  const modelSelector = useMemo(() => {
    if (!enabled) return EMPTY_MODEL_SELECTOR;
    return buildEffectiveModelSelector(providers);
  }, [enabled, providers]);

  const refetch = useCallback(() => setRefreshKey((value) => value + 1), []);

  return { modelSelector, loading, error, refetch };
}

export function buildEffectiveModelSelector(providers: EffectiveLlmProvider[]): ModelSelectorConfig {
  const executableProviders = providers.filter((provider) => provider.enabled && provider.executable);
  const modelProviders = executableProviders.map((provider) => ({
    id: provider.slug,
    name: provider.name,
  }));
  const models = executableProviders.flatMap((provider) => modelsFromProviderConfig(provider));
  const defaultModel = executableProviders
    .map((provider) => provider.default_model.trim())
    .find((modelId) => modelId.length > 0) ?? null;

  return {
    providers: modelProviders,
    models,
    default_model: defaultModel,
    agents: [],
    permissions: [],
  };
}

function modelsFromProviderConfig(provider: EffectiveLlmProvider): ModelInfo[] {
  const blockedModels = parseStringList(provider.blocked_models);
  const models = parseModelList(provider.models, provider.slug);
  const defaultModel = provider.default_model.trim();

  if (models.length === 0 && defaultModel.length > 0) {
    models.push(modelFromId(defaultModel, provider.slug, false));
  }

  if (defaultModel.length > 0 && !models.some((model) => model.id === defaultModel)) {
    models.unshift(modelFromId(defaultModel, provider.slug, false));
  }

  const seen = new Set<string>();
  return models.filter((model) => {
    if (seen.has(model.id)) return false;
    seen.add(model.id);
    model.blocked = blockedModels.has(model.id);
    return true;
  });
}

function parseModelList(value: JsonValue, providerId: string): ModelInfo[] {
  const normalized = normalizeJsonString(value);
  if (!Array.isArray(normalized)) return [];

  const models: ModelInfo[] = [];
  for (const item of normalized) {
    if (typeof item === "string") {
      const id = item.trim();
      if (id.length > 0) models.push(modelFromId(id, providerId, false));
      continue;
    }

    if (!isJsonObject(item)) continue;
    const id = readString(item.id).trim();
    if (id.length === 0) continue;
    models.push({
      id,
      name: readString(item.name).trim() || formatModelName(id),
      provider_id: providerId,
      reasoning: readBoolean(item.reasoning, true),
      supports_image: readBoolean(item.supports_image, true),
      context_window: readPositiveNumber(item.context_window, DEFAULT_CONTEXT_WINDOW),
      blocked: false,
      discovered: false,
    });
  }

  return models;
}

function parseStringList(value: JsonValue): Set<string> {
  const normalized = normalizeJsonString(value);
  if (Array.isArray(normalized)) {
    return new Set(
      normalized
        .filter((item): item is string => typeof item === "string")
        .map((item) => item.trim())
        .filter((item) => item.length > 0),
    );
  }

  if (typeof normalized === "string") {
    return new Set(
      normalized
        .split(/[\n,]/)
        .map((item) => item.trim())
        .filter((item) => item.length > 0),
    );
  }

  return new Set();
}

function normalizeJsonString(value: JsonValue): JsonValue {
  if (typeof value !== "string") return value;
  const trimmed = value.trim();
  if (trimmed.length === 0) return [];
  if (!trimmed.startsWith("[") && !trimmed.startsWith("{")) return value;
  try {
    const parsed: unknown = JSON.parse(trimmed);
    return isJsonValue(parsed) ? parsed : value;
  } catch {
    return value;
  }
}

function modelFromId(id: string, providerId: string, discovered: boolean): ModelInfo {
  return {
    id,
    name: formatModelName(id),
    provider_id: providerId,
    reasoning: true,
    supports_image: true,
    context_window: DEFAULT_CONTEXT_WINDOW,
    blocked: false,
    discovered,
  };
}

function formatModelName(modelId: string): string {
  return modelId
    .split(/[-_]/)
    .filter((word) => word.length > 0)
    .map((word) => `${word.charAt(0).toUpperCase()}${word.slice(1).toLowerCase()}`)
    .join(" ");
}

function isJsonObject(value: JsonValue): value is { [key: string]: JsonValue | undefined } {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isJsonValue(value: unknown): value is JsonValue {
  if (value === null) return true;
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") return true;
  if (Array.isArray(value)) return value.every(isJsonValue);
  if (typeof value === "object") {
    return Object.values(value).every(isJsonValue);
  }
  return false;
}

function readString(value: JsonValue | undefined): string {
  return typeof value === "string" ? value : "";
}

function readBoolean(value: JsonValue | undefined, fallback: boolean): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function readPositiveNumber(value: JsonValue | undefined, fallback: number): number {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? value : fallback;
}
