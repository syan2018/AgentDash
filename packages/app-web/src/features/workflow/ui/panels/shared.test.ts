import { describe, expect, it } from "vitest";

import {
  AUTO_GRANTED_BASELINE,
  CAP_EDITOR_WELL_KNOWN_KEYS,
  isWellKnownCapability,
} from "./shared";

describe("workflow capability editor shared constants", () => {
  it("uses workspace_module as the well-known Canvas-capable entry", () => {
    expect(CAP_EDITOR_WELL_KNOWN_KEYS).toContain("workspace_module");
    expect(CAP_EDITOR_WELL_KNOWN_KEYS).not.toContain("canvas");
    expect(isWellKnownCapability("workspace_module")).toBe(true);
    expect(isWellKnownCapability("canvas")).toBe(false);
  });

  it("auto-grants workspace_module for project workflows", () => {
    expect(AUTO_GRANTED_BASELINE.project).toContain("workspace_module");
    expect(AUTO_GRANTED_BASELINE.project).not.toContain("canvas");
  });

  it("auto-grants workspace_module for story workflows", () => {
    expect(AUTO_GRANTED_BASELINE.story).toContain("workspace_module");
    expect(AUTO_GRANTED_BASELINE.story).not.toContain("canvas");
  });
});
