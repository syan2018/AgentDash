import { describe, expect, it } from "vitest";
import {
  isVfsMountBrowsable,
  resolveDefaultMountId,
  type VfsMountBrowsingPolicy,
} from "./vfs-browser-panel-policy";

function mount(
  id: string,
  provider: string,
  browsable: boolean,
): VfsMountBrowsingPolicy {
  return {
    id,
    provider,
    backend_online: provider === "relay_fs" ? browsable : null,
    browsable,
  };
}

describe("VfsBrowserPanel mount browsing policy", () => {
  it("不自动浏览明确离线的 relay_fs mount", () => {
    expect(isVfsMountBrowsable({ provider: "relay_fs", backend_online: false })).toBe(false);
    expect(isVfsMountBrowsable({ provider: "relay_fs", backend_online: true })).toBe(true);
    expect(isVfsMountBrowsable({ provider: "relay_fs" })).toBe(true);
    expect(isVfsMountBrowsable({ provider: "inline_fs", backend_online: false })).toBe(true);
  });

  it("默认选择会跳过离线 mount，避免预览页主动请求离线 backend", () => {
    const mounts = [
      mount("workspace", "relay_fs", false),
      mount("context", "inline_fs", true),
    ];

    expect(resolveDefaultMountId(mounts, undefined, "workspace")).toBe("context");
    expect(resolveDefaultMountId(mounts, "workspace", undefined)).toBe("context");
  });

  it("全部不可浏览时仍保留第一个 mount 作为摘要选择", () => {
    const mounts = [mount("workspace", "relay_fs", false)];

    expect(resolveDefaultMountId(mounts)).toBe("workspace");
  });
});
