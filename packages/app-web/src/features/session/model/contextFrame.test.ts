import { describe, expect, it } from "vitest";
import { parseContextFrame } from "./contextFrame";

describe("parseContextFrame", () => {
  it("允许 rendered_text 为空但保留结构化 sections", () => {
    const frame = parseContextFrame({
      id: "ctx-1",
      kind: "identity",
      source: "runtime",
      delivery_status: "delivered",
      delivery_channel: "system",
      message_role: "system",
      rendered_text: "",
      created_at_ms: 123,
      sections: [
        {
          kind: "identity",
          title: "Identity",
          summary: "当前身份",
          base_prompt: "base",
          mode: "append",
          effective_prompt: "effective",
        },
      ],
    });

    expect(frame).not.toBeNull();
    expect(frame?.rendered_text).toBe("");
    expect(frame?.sections).toHaveLength(1);
  });
});
