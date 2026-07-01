import { afterEach, describe, expect, it, vi } from "vitest";
import { consumeTokenFromLocationHash, getStoredToken } from "./client";

function installBrowserStubs(hash: string, store = new Map<string, string>()) {
  const localStorageMock: Storage = {
    get length() {
      return store.size;
    },
    clear: () => store.clear(),
    getItem: (key: string) => store.get(key) ?? null,
    key: (index: number) => Array.from(store.keys())[index] ?? null,
    removeItem: (key: string) => {
      store.delete(key);
    },
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
  };
  const documentMock = {
    cookie: "",
    title: "AgentDash",
  };
  const historyMock = {
    state: { source: "test" },
    replaceState: vi.fn(),
  };

  vi.stubGlobal("localStorage", localStorageMock);
  vi.stubGlobal("document", documentMock);
  vi.stubGlobal("window", {
    location: {
      hash,
      pathname: "/dashboard/agent",
      search: "?tab=story",
    },
    history: historyMock,
  });

  return { documentMock, historyMock, store };
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("consumeTokenFromLocationHash", () => {
  it("把登录回跳 fragment 中的 token 写入本地认证存储并清理地址", () => {
    const { documentMock, historyMock } = installBrowserStubs(
      "#agentdash_access_token=agd_token&view=main",
    );

    expect(consumeTokenFromLocationHash()).toBe("agd_token");
    expect(getStoredToken()).toBe("agd_token");
    expect(documentMock.cookie).toContain("agentdash_access_token=agd_token");
    expect(historyMock.replaceState).toHaveBeenCalledWith(
      historyMock.state,
      "AgentDash",
      "/dashboard/agent?tab=story#view=main",
    );
  });

  it("没有 token fragment 时不改写地址", () => {
    const { historyMock } = installBrowserStubs("#view=main");

    expect(consumeTokenFromLocationHash()).toBeNull();
    expect(getStoredToken()).toBeNull();
    expect(historyMock.replaceState).not.toHaveBeenCalled();
  });

  it("重新打开应用时可从本机 storage 继续读取上次 OIDC token", () => {
    const firstOpen = installBrowserStubs("#agentdash_access_token=agd_persisted");
    expect(consumeTokenFromLocationHash()).toBe("agd_persisted");
    expect(getStoredToken()).toBe("agd_persisted");

    vi.unstubAllGlobals();
    const secondOpen = installBrowserStubs("", firstOpen.store);

    expect(consumeTokenFromLocationHash()).toBeNull();
    expect(getStoredToken()).toBe("agd_persisted");
    expect(secondOpen.historyMock.replaceState).not.toHaveBeenCalled();
  });
});
