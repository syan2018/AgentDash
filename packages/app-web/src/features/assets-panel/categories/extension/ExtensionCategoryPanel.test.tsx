import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const projectStoreState = vi.hoisted(() => ({
  currentProjectId: "project-1" as string | null,
}));

vi.mock("../../../../stores/projectStore", () => ({
  useProjectStore: (selector: (state: { currentProjectId: string | null }) => unknown) =>
    selector({ currentProjectId: projectStoreState.currentProjectId }),
}));

vi.mock("../../../../stores/currentUserStore", () => ({
  useCurrentUserStore: (
    selector: (state: { currentUser: { user_id: string } | null }) => unknown,
  ) => selector({ currentUser: { user_id: "user-1" } }),
}));

vi.mock("../../../../services/extensionManagement", () => ({
  fetchProjectExtensions: vi.fn(async () => ({ extensions: [] })),
}));

vi.mock("../../../../services/sharedLibrary", () => ({
  fetchLibraryAssets: vi.fn(async () => []),
  publishLibraryAsset: vi.fn(),
}));

vi.mock("../../../../services/extensionPackage", () => ({
  downloadExtensionArtifact: vi.fn(),
  importExtensionPackage: vi.fn(),
}));

vi.mock("../../../../services/extensionRuntime", () => ({
  uninstallExtensionInstallation: vi.fn(),
}));

import { ExtensionCategoryPanel } from "../ExtensionCategoryPanel";

describe("ExtensionCategoryPanel", () => {
  beforeEach(() => {
    projectStoreState.currentProjectId = "project-1";
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("renders project selection placeholder", () => {
    projectStoreState.currentProjectId = null;

    const html = renderToStaticMarkup(<ExtensionCategoryPanel />);

    expect(html).toContain("请选择项目");
  });

  it("renders management-first empty state without archive-library language", () => {
    const html = renderToStaticMarkup(<ExtensionCategoryPanel />);

    expect(html).toContain("Extension 资产");
    expect(html).toContain("本地包");
    expect(html).toContain("暂无 Extension 资产");
  });
});
