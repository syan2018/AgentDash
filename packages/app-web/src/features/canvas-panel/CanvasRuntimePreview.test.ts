import { describe, expect, it, vi } from "vitest";
import type { CanvasRuntimeSnapshot } from "../../types";
import {
  buildPreviewDocument,
  createRuntimeAssetUrlCache,
  parseVfsAssetUri,
  resolveRuntimeAssetUrl,
  revokeAllRuntimeAssetUrls,
  revokeRuntimeAssetUrl,
} from "./CanvasRuntimePreview.runtime";

function snapshot(): CanvasRuntimeSnapshot {
  return {
    canvas_id: "canvas-1",
    session_id: "session-1",
    resource_surface_ref: "session-runtime:session-1",
    entry: "src/main.tsx",
    files: [
      {
        path: "src/main.tsx",
        content: "console.log('ready');",
        file_type: "code",
      },
    ],
    bindings: [],
    import_map: { imports: {} },
    libraries: [],
    runtime_bridge: {
      enabled: false,
      surface: null,
      disabled_reason: "test",
    },
  };
}

describe("CanvasRuntimePreview VFS image assets", () => {
  it("parses safe VFS image mount URIs", () => {
    expect(parseVfsAssetUri("ld-km://assets/doc-1/source.png")).toEqual({
      mountId: "ld-km",
      path: "assets/doc-1/source.png",
    });
    expect(parseVfsAssetUri("skill-assets://skills/demo/assets/logo.png")).toEqual({
      mountId: "skill-assets",
      path: "skills/demo/assets/logo.png",
    });
  });

  it("rejects non-VFS or escaping image URIs", () => {
    expect(parseVfsAssetUri("https://example.test/image.png")).toBe("无效的 VFS 图片 URI");
    expect(parseVfsAssetUri("ld-km:///absolute.png")).toBe("VFS 图片路径必须是 mount 相对路径");
    expect(parseVfsAssetUri("ld-km://assets/../secret.png")).toBe("VFS 图片路径不能包含 ..");
    expect(parseVfsAssetUri("ld-km://assets/image.png?token=1")).toBe(
      "VFS 图片 URI 不支持 query 或 fragment",
    );
  });

  it("injects the agentdash assets SDK into the preview document", () => {
    const createObjectUrl = vi.spyOn(URL, "createObjectURL").mockReturnValue("blob:canvas-module");
    const revokeObjectUrl = vi.spyOn(URL, "revokeObjectURL").mockImplementation(() => {});

    const built = buildPreviewDocument(snapshot(), "frame-1");

    expect(built.srcDoc).toContain("assets: Object.freeze");
    expect(built.srcDoc).toContain("canvas-asset-url-request");
    expect(built.srcDoc).toContain("canvas-asset-url-result");

    built.dispose();
    expect(revokeObjectUrl).toHaveBeenCalledWith("blob:canvas-module");

    createObjectUrl.mockRestore();
    revokeObjectUrl.mockRestore();
  });

  it("resolves image blobs through the runtime asset cache", async () => {
    const cache = createRuntimeAssetUrlCache();
    const readBlob = vi.fn(async () => new Blob(["image"], { type: "image/png" }));
    const createObjectUrl = vi.fn(() => "blob:asset-1");

    const firstUrl = await resolveRuntimeAssetUrl({
      surfaceRef: "session-runtime:session-1",
      uri: "ld-km://assets/doc-1/source.png",
      cache,
      readBlob,
      createObjectUrl,
    });
    const secondUrl = await resolveRuntimeAssetUrl({
      surfaceRef: "session-runtime:session-1",
      uri: "ld-km://assets/doc-1/source.png",
      cache,
      readBlob,
      createObjectUrl,
    });

    expect(firstUrl).toBe("blob:asset-1");
    expect(secondUrl).toBe("blob:asset-1");
    expect(readBlob).toHaveBeenCalledTimes(1);
    expect(readBlob).toHaveBeenCalledWith({
      surfaceRef: "session-runtime:session-1",
      mountId: "ld-km",
      path: "assets/doc-1/source.png",
    });
    expect(createObjectUrl).toHaveBeenCalledTimes(1);
  });

  it("rejects invalid or non-image runtime assets", async () => {
    const cache = createRuntimeAssetUrlCache();
    const readBlob = vi.fn(async () => new Blob(["{}"], { type: "application/json" }));

    await expect(resolveRuntimeAssetUrl({
      surfaceRef: "session-runtime:session-1",
      uri: "ld-km://assets/doc-1/source.json",
      cache,
      readBlob,
      createObjectUrl: () => "blob:asset-json",
    })).rejects.toThrow("资源不是图片 MIME: application/json");
    await expect(resolveRuntimeAssetUrl({
      surfaceRef: "session-runtime:session-1",
      uri: "https://example.test/image.png",
      cache,
      readBlob,
      createObjectUrl: () => "blob:asset-http",
    })).rejects.toThrow("无效的 VFS 图片 URI");

    expect(cache.urls.size).toBe(0);
    expect(readBlob).toHaveBeenCalledTimes(1);
  });

  it("revokes runtime asset object URLs and clears cached mappings", async () => {
    const cache = createRuntimeAssetUrlCache();
    const readBlob = vi.fn(async () => new Blob(["image"], { type: "image/png" }));
    const createObjectUrl = vi.fn()
      .mockReturnValueOnce("blob:asset-1")
      .mockReturnValueOnce("blob:asset-2");
    const revokeObjectUrl = vi.fn();

    const firstUrl = await resolveRuntimeAssetUrl({
      surfaceRef: "session-runtime:session-1",
      uri: "ld-km://assets/doc-1/source.png",
      cache,
      readBlob,
      createObjectUrl,
    });
    revokeRuntimeAssetUrl(cache, firstUrl, revokeObjectUrl);
    const secondUrl = await resolveRuntimeAssetUrl({
      surfaceRef: "session-runtime:session-1",
      uri: "ld-km://assets/doc-1/source.png",
      cache,
      readBlob,
      createObjectUrl,
    });
    revokeAllRuntimeAssetUrls(cache, revokeObjectUrl);

    expect(firstUrl).toBe("blob:asset-1");
    expect(secondUrl).toBe("blob:asset-2");
    expect(readBlob).toHaveBeenCalledTimes(2);
    expect(revokeObjectUrl).toHaveBeenCalledWith("blob:asset-1");
    expect(revokeObjectUrl).toHaveBeenCalledWith("blob:asset-2");
    expect(cache.urls.size).toBe(0);
    expect(cache.uriCache.size).toBe(0);
    expect(cache.pending.size).toBe(0);
  });
});
