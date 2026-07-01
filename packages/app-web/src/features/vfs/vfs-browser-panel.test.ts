import { describe, expect, it } from "vitest";
import {
  isVfsMountBrowsable,
  resolveDefaultMountId,
  selectVfsBackendTarget,
  type VfsMountBrowsingPolicy,
} from "./vfs-browser-panel-policy";
import { formatBytes } from "./vfs-format";

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

  it("默认选择会避开外部服务 mount，除非用户明确指定", () => {
    const mounts = [
      mount("knowledge", "external_docs", true),
      mount("main", "relay_fs", true),
      mount("context", "inline_fs", true),
    ];

    expect(resolveDefaultMountId(mounts, undefined, "knowledge")).toBe("main");
    expect(resolveDefaultMountId(mounts, "knowledge", undefined)).toBe("knowledge");
  });

  it("只有外部服务可浏览时仍允许作为兜底选择", () => {
    const mounts = [mount("knowledge", "external_docs", true)];

    expect(resolveDefaultMountId(mounts, undefined, "knowledge")).toBe("knowledge");
    expect(resolveDefaultMountId(mounts)).toBe("knowledge");
  });

  it("skill_asset_fs 不作为运行时资源浏览的优先自动选择", () => {
    const mounts = [
      mount("skill-assets", "skill_asset_fs", true),
      mount("context", "inline_fs", true),
    ];

    expect(resolveDefaultMountId(mounts, undefined, "skill-assets")).toBe("context");
    expect(resolveDefaultMountId([mount("skill-assets", "skill_asset_fs", true)])).toBe("skill-assets");
  });

  it("全部不可浏览时仍保留第一个 mount 作为摘要选择", () => {
    const mounts = [mount("workspace", "relay_fs", false)];

    expect(resolveDefaultMountId(mounts)).toBe("workspace");
  });

  it("backend target 选择复用浏览策略并跳过离线 relay_fs", () => {
    const target = selectVfsBackendTarget([
      {
        id: "workspace",
        provider: "relay_fs",
        backend_id: "backend-offline",
        display_name: "Offline",
        backend_online: false,
      },
      {
        id: "backup",
        provider: "relay_fs",
        backend_id: "backend-2",
        display_name: "Backup",
        backend_online: true,
      },
    ], { defaultMountId: "workspace" });

    expect(target).toEqual({
      mountId: "backup",
      backend_id: "backend-2",
      label: "Backup",
      online: true,
    });
  });

  it("统一格式化文件大小", () => {
    expect(formatBytes(34)).toBe("34 B");
    expect(formatBytes(1536)).toBe("1.5 KB");
    expect(formatBytes(16 * 1024 * 1024)).toBe("16.0 MB");
  });
});
