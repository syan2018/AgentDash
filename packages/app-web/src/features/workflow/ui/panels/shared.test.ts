import { describe, expect, it } from "vitest";
import type { CapabilityCatalogEntryDto } from "../../../../types";

import {
  capabilityAutoGrantedForTargetKind,
  capabilityKnownInCatalog,
  capabilityVisibleForTargetKind,
} from "./shared";

const workspaceModuleEntry: CapabilityCatalogEntryDto = {
  key: "workspace_module",
  label: "Workspace Module",
  description: "模块创建、调用与展示，包含 Canvas",
  allowed_scopes: ["project", "story", "task"],
  auto_granted: true,
  agent_can_grant: false,
  workflow_can_grant: false,
  tools: [],
};

describe("workflow capability editor catalog helpers", () => {
  it("detects catalog entries by key", () => {
    expect(capabilityKnownInCatalog([workspaceModuleEntry], "workspace_module")).toBe(true);
    expect(capabilityKnownInCatalog([workspaceModuleEntry], "canvas")).toBe(false);
  });

  it("uses catalog visibility and auto-grant metadata for target kinds", () => {
    expect(capabilityVisibleForTargetKind(workspaceModuleEntry, "project")).toBe(true);
    expect(capabilityVisibleForTargetKind(workspaceModuleEntry, "story")).toBe(true);
    expect(capabilityAutoGrantedForTargetKind(workspaceModuleEntry, "project")).toBe(true);
    expect(capabilityAutoGrantedForTargetKind(workspaceModuleEntry, "story")).toBe(true);
  });
});
