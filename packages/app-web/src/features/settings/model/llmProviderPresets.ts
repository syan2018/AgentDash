import type { CreateLlmProviderRequest } from "../../../api/llmProviders";
import type { LlmProviderModelConfig } from "./llmProviderModels";

export type LlmProviderProtocol = CreateLlmProviderRequest["protocol"];

export interface LlmProviderPreset {
  name: string;
  slug: string;
  protocol: LlmProviderProtocol;
  base_url: string;
  env_api_key: string;
  default_model?: string;
  models?: LlmProviderModelConfig[];
}

export const LLM_PROVIDER_PRESETS: LlmProviderPreset[] = [
  { name: "Anthropic Claude", slug: "anthropic", protocol: "anthropic", base_url: "", env_api_key: "ANTHROPIC_API_KEY" },
  { name: "Google Gemini", slug: "gemini", protocol: "gemini", base_url: "", env_api_key: "GEMINI_API_KEY" },
  { name: "OpenAI", slug: "openai", protocol: "openai_compatible", base_url: "https://api.openai.com/v1", env_api_key: "OPENAI_API_KEY" },
  {
    name: "ChatGPT Codex",
    slug: "openai-codex",
    protocol: "openai_codex",
    base_url: "",
    env_api_key: "OPENAI_CODEX_OAUTH",
    default_model: "gpt-5.5",
    models: [
      { id: "gpt-5.5", name: "GPT-5.5", context_window: 272000, reasoning: true, supports_image: true },
      { id: "gpt-5.4", name: "GPT-5.4", context_window: 272000, reasoning: true, supports_image: true },
      { id: "gpt-5.4-mini", name: "GPT-5.4 Mini", context_window: 272000, reasoning: true, supports_image: true },
      { id: "gpt-5.3-codex", name: "GPT-5.3 Codex", context_window: 272000, reasoning: true, supports_image: true },
    ],
  },
  { name: "DeepSeek", slug: "deepseek", protocol: "openai_compatible", base_url: "https://api.deepseek.com/v1", env_api_key: "DEEPSEEK_API_KEY" },
  { name: "Groq", slug: "groq", protocol: "openai_compatible", base_url: "https://api.groq.com/openai/v1", env_api_key: "GROQ_API_KEY" },
  { name: "xAI (Grok)", slug: "xai", protocol: "openai_compatible", base_url: "https://api.x.ai/v1", env_api_key: "XAI_API_KEY" },
];

export const CUSTOM_LLM_PROVIDER_PROTOCOLS: LlmProviderProtocol[] = [
  "openai_compatible",
  "anthropic",
  "gemini",
];
