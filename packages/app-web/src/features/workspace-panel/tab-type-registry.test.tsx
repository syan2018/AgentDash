import { describe, expect, it } from "vitest";

import { tabTypeRegistry, type TabTypeDescriptor } from "./tab-type-registry";

function TestIcon() {
  return null;
}

function descriptor(typeId: string): TabTypeDescriptor {
  return {
    typeId,
    label: typeId,
    icon: TestIcon,
    allowMultiple: true,
    pinned: false,
    renderContent: () => null,
    resolveTitle: () => typeId,
    parseUri: () => null,
    buildUri: () => `${typeId}://panel`,
  };
}

describe("tabTypeRegistry contribution lifecycle", () => {
  it("按 owner 替换 extension-owned descriptors", () => {
    const ownerKey = "test-extension-runtime:project-1";
    tabTypeRegistry.registerContribution(ownerKey, [
      descriptor("test-extension.first"),
    ]);
    expect(tabTypeRegistry.getType("test-extension.first")?.label).toBe("test-extension.first");

    tabTypeRegistry.registerContribution(ownerKey, [
      descriptor("test-extension.second"),
    ]);

    expect(tabTypeRegistry.getType("test-extension.first")).toBeUndefined();
    expect(tabTypeRegistry.getType("test-extension.second")?.label).toBe("test-extension.second");

    tabTypeRegistry.unregisterContribution(ownerKey);
    expect(tabTypeRegistry.getType("test-extension.second")).toBeUndefined();
  });
});
