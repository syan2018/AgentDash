import { describe, expect, it } from "vitest";

import { sha256OfBlob } from "./sha256";

// 已知 sha256:
//  - 空字节: e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
//  - [0x01, 0x02, 0x03]: 039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81
describe("sha256OfBlob", () => {
  it("hashes [1,2,3] to known digest", async () => {
    const blob = new Blob([new Uint8Array([1, 2, 3])]);
    const digest = await sha256OfBlob(blob);
    expect(digest).toBe(
      "sha256:039058c6f2c0cb492c533b0a4d14ef77cc0f78abccced5287d84a1a2011cfb81",
    );
  });

  it("hashes empty blob to known digest", async () => {
    const blob = new Blob([new Uint8Array([])]);
    const digest = await sha256OfBlob(blob);
    expect(digest).toBe(
      "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
  });

  it("hashes string blob using utf-8 bytes", async () => {
    const blob = new Blob(["abc"]);
    const digest = await sha256OfBlob(blob);
    expect(digest).toBe(
      "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    );
  });
});
