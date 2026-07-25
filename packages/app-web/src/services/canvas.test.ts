import { describe, expect, it } from "vitest";

import { canonicalSourceBundle, createDefaultCanvasSourceBundle } from "./canvas";

describe("Interaction Canvas source bundle", () => {
  it("按 path 与 sandbox key 生成稳定 V1 digest", async () => {
    const first = await canonicalSourceBundle({
      format_version: 1,
      entry_file: "src/main.html",
      files: [
        { path: "src/z.js", content: "z", media_type: "text/javascript" },
        { path: "src/main.html", content: "main", media_type: "text/html" },
      ],
      sandbox: {
        libraries: ["z", "a", "a"],
        import_map: { z: "/z.js", a: "/a.js" },
      },
    });
    const second = await canonicalSourceBundle({
      format_version: 1,
      entry_file: "src/main.html",
      files: [...first.files].reverse(),
      sandbox: {
        libraries: ["a", "z"],
        import_map: { a: "/a.js", z: "/z.js" },
      },
    });
    expect(first.files.map((file) => file.path)).toEqual(["src/main.html", "src/z.js"]);
    expect(first.digest).toBe(second.digest);
    expect(first.digest).toMatch(/^sha256:[a-f0-9]{64}$/);
  });

  it("默认 Canvas 从可直接预览的 immutable source bundle 开始", async () => {
    const bundle = await createDefaultCanvasSourceBundle();
    expect(bundle.entry_file).toBe("index.html");
    expect(bundle.files[0].content).toContain("New Canvas");
  });
});
