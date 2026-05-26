import { describe, expect, it } from "vitest";

import { parseExtensionBridgeMessage, toJsonValue } from "./bridge";

describe("extension bridge message validation", () => {
  it("只接受 agentdash extension request message", () => {
    const message = parseExtensionBridgeMessage({
      channel: "agentdash.extension",
      kind: "request",
      request_id: "request-1",
      method: "runtime.invoke_action",
      params: {
        action_key: "local-hello.profile",
      },
    });

    expect(message).toEqual({
      channel: "agentdash.extension",
      kind: "request",
      request_id: "request-1",
      method: "runtime.invoke_action",
      params: {
        action_key: "local-hello.profile",
      },
    });
    expect(parseExtensionBridgeMessage({ channel: "other" })).toBeNull();
    expect(parseExtensionBridgeMessage({
      channel: "agentdash.extension",
      kind: "request",
      request_id: "",
      method: "runtime.invoke_action",
    })).toBeNull();
  });

  it("把 bridge payload 归一化为 JSON value", () => {
    expect(toJsonValue({
      ok: true,
      value: Number.NaN,
      list: [1, undefined],
    })).toEqual({
      ok: true,
      value: null,
      list: [1, null],
    });
  });
});
