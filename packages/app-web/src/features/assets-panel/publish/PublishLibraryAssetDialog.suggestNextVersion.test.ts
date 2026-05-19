import { describe, it, expect } from "vitest";
import { suggestNextVersion } from "./version";

describe("suggestNextVersion", () => {
  it("bumps semver patch", () => {
    expect(suggestNextVersion("1.0.0")).toBe("1.0.1");
    expect(suggestNextVersion("0.2.9")).toBe("0.2.10");
    expect(suggestNextVersion("12.34.56")).toBe("12.34.57");
  });

  it("preserves pre-release / build suffix", () => {
    expect(suggestNextVersion("1.0.0-beta")).toBe("1.0.1-beta");
    expect(suggestNextVersion("1.0.0+build.5")).toBe("1.0.1+build.5");
  });

  it("falls back to '<input>.1' for non-semver", () => {
    expect(suggestNextVersion("v1")).toBe("v1.1");
    expect(suggestNextVersion("alpha")).toBe("alpha.1");
  });

  it("trims surrounding whitespace", () => {
    expect(suggestNextVersion("  1.0.0  ")).toBe("1.0.1");
  });
});
