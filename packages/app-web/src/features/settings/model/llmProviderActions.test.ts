import { afterEach, describe, expect, it, vi } from "vitest";

import { createAdminCodexOAuthActions } from "./llmProviderActions";

function installDesktopBridge(accessToken?: string) {
  const startCodexOAuth = vi.fn(async () => ({
    flow_id: "flow-1",
    auth_url: "https://auth.example.test",
    expires_at: "2026-07-11T00:00:00Z",
  }));
  const store = new Map<string, string>();
  if (accessToken) store.set("agentdash_access_token", accessToken);

  vi.stubGlobal("localStorage", {
    getItem: (key: string) => store.get(key) ?? null,
  });
  vi.stubGlobal("document", { cookie: "" });
  vi.stubGlobal("window", {
    __AGENTDASH_DESKTOP_APP__: {
      startCodexOAuth,
      getDesktopApiSnapshot: vi.fn(async () => ({ origin: "http://127.0.0.1:3000" })),
    },
  });

  return startCodexOAuth;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("createAdminCodexOAuthActions", () => {
  it("Personal 模式没有平台 token 时仍交给服务端解析身份", async () => {
    const startCodexOAuth = installDesktopBridge();

    await createAdminCodexOAuthActions("provider-1").start();

    expect(startCodexOAuth).toHaveBeenCalledWith({
      api_origin: "http://127.0.0.1:3000",
      provider_id: "provider-1",
      target: "global_provider",
    });
  });

  it("存在平台 token 时透传给桌面宿主", async () => {
    const startCodexOAuth = installDesktopBridge("token-1");

    await createAdminCodexOAuthActions("provider-1").start();

    expect(startCodexOAuth).toHaveBeenCalledWith({
      api_origin: "http://127.0.0.1:3000",
      access_token: "token-1",
      provider_id: "provider-1",
      target: "global_provider",
    });
  });
});
