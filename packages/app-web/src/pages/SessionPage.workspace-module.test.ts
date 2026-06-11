import { describe, expect, it } from "vitest";

import { workspaceModulePresentedTabTarget } from "./SessionPage.workspaceModulePresentation";

describe("workspaceModulePresentedTabTarget", () => {
  it("opens Canvas tabs from presentation_uri", () => {
    expect(workspaceModulePresentedTabTarget({
      renderer_kind: "canvas",
      view_key: "preview",
      presentation_uri: "canvas://dashboard-a",
    })).toEqual({
      typeId: "canvas",
      uri: "canvas://dashboard-a",
      refreshRuntime: true,
    });
  });

  it("does not infer Canvas URI from view_key", () => {
    expect(workspaceModulePresentedTabTarget({
      renderer_kind: "canvas",
      view_key: "preview",
    })).toBeNull();
  });

  it("does not open Canvas tabs from legacy uri fallback", () => {
    expect(workspaceModulePresentedTabTarget({
      renderer_kind: "canvas",
      view_key: "preview",
      uri: "canvas://dashboard-a",
    })).toBeNull();
  });

  it("opens non-Canvas module views by view_key", () => {
    expect(workspaceModulePresentedTabTarget({
      renderer_kind: "webview",
      view_key: "inspector",
      presentation_uri: "ext-demo://panel",
    })).toEqual({
      typeId: "inspector",
      uri: "ext-demo://panel",
      refreshRuntime: false,
    });
  });
});
