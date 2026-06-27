import { describe, expect, it } from "vitest";

import {
  modelBelongsToProviderSlug,
  parseLlmProviderBlockedModels,
  parseLlmProviderModelConfigs,
  serializeLlmProviderBlockedModels,
  serializeLlmProviderModelConfigs,
} from "./llmProviderModels";

describe("llm provider model config helpers", () => {
  it("解析模型配置并过滤无效 id", () => {
    expect(parseLlmProviderModelConfigs([
      { id: " gpt-5 ", name: " GPT-5 ", context_window: 128000, reasoning: false, supports_image: false },
      { id: "fallback", context_window: 0 },
      { id: " " },
      "invalid",
    ])).toEqual([
      { id: "gpt-5", name: "GPT-5", context_window: 128000, reasoning: false, supports_image: false },
      { id: "fallback", name: "", context_window: 200000, reasoning: true, supports_image: true },
    ]);
  });

  it("序列化模型配置为 JsonValue 形态", () => {
    expect(serializeLlmProviderModelConfigs([
      { id: "gpt-5", name: "GPT-5", context_window: 128000, reasoning: true, supports_image: false },
    ])).toEqual([
      { id: "gpt-5", name: "GPT-5", context_window: 128000, reasoning: true, supports_image: false },
    ]);
  });

  it("解析和序列化 blocked models", () => {
    expect(parseLlmProviderBlockedModels([" a ", "", 42])).toEqual(["a", "42"]);
    expect(parseLlmProviderBlockedModels("a, b\nc\r\nd")).toEqual(["a", "b", "c", "d"]);
    expect(serializeLlmProviderBlockedModels(["a", "b"])).toEqual(["a", "b"]);
  });

  it("用 provider slug 判断模型归属", () => {
    expect(modelBelongsToProviderSlug({ provider_id: "openai" }, { slug: "openai" })).toBe(true);
    expect(modelBelongsToProviderSlug({ provider_id: "provider-id" }, { slug: "openai" })).toBe(false);
    expect(modelBelongsToProviderSlug({}, { slug: "openai" })).toBe(false);
  });
});
