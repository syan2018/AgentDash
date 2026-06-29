import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const originalFetch = globalThis.fetch;

describe("desktop defaults", () => {
  beforeEach(() => {
    vi.resetModules();
    vi.unstubAllEnvs();
    globalThis.fetch = originalFetch;
  });

  afterEach(() => {
    vi.unstubAllEnvs();
    globalThis.fetch = originalFetch;
  });

  it("本机运行时默认 server URL 优先使用当前 Dashboard API origin", async () => {
    vi.stubEnv("VITE_API_ORIGIN", "http://127.0.0.1:3001");
    globalThis.fetch = vi.fn(async () => new Response(JSON.stringify({
      default_cloud_origin: "http://10.22.71.7:8080",
    }), {
      status: 200,
      headers: { "Content-Type": "application/json" },
    }));

    const {
      ensureDesktopDefaultsLoaded,
      resolveDefaultLocalRuntimeServerUrl,
    } = await import("./defaults");

    await ensureDesktopDefaultsLoaded();

    expect(resolveDefaultLocalRuntimeServerUrl()).toBe("http://127.0.0.1:3001");
  });
});
