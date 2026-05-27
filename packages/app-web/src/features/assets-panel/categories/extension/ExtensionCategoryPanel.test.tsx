/**
 * ExtensionCategoryPanel 静态渲染测试。
 *
 * 项目未引入 @testing-library/react，沿用 renderToStaticMarkup +
 * 受控的 store / service mock 来覆盖关键渲染态：
 * - 空 project：占位文案
 * - 仅有归档：归档行渲染 + 「从归档安装」按钮可见
 * - 仅有已安装：已安装行渲染 + marketplace 来源不显示「下载归档」
 * - 有 notice：notice 文案渲染
 */

import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import type {
  ExtensionInstallationProjectionResponse,
  ExtensionRuntimeProjectionResponse,
} from "../../../../generated/extension-runtime-contracts";
import type { ExtensionPackageArtifactResponse } from "../../../../generated/extension-package-contracts";

const projectStoreState = vi.hoisted(() => ({
  currentProjectId: "project-1" as string | null,
}));

const runtimeState = vi.hoisted(() => ({
  status: "ready" as "ready" | "loading" | "error" | "idle",
  projection: {
    installations: [],
    commands: [],
    flags: [],
    message_renderers: [],
    runtime_actions: [],
    workspace_tabs: [],
    permissions: [],
    bundles: [],
  } as ExtensionRuntimeProjectionResponse,
  error: null as string | null,
}));

const artifactsState = vi.hoisted(() => ({
  list: [] as ExtensionPackageArtifactResponse[],
}));

vi.mock("../../../../stores/projectStore", () => ({
  useProjectStore: (selector: (state: { currentProjectId: string | null }) => unknown) =>
    selector({ currentProjectId: projectStoreState.currentProjectId }),
}));

vi.mock("../../../extension-runtime", () => ({
  useProjectExtensionRuntime: () => ({
    project_id: projectStoreState.currentProjectId,
    status: runtimeState.status,
    projection: runtimeState.projection,
    error: runtimeState.error,
  }),
}));

vi.mock("../../../extension-runtime/model/extensionRuntimeStore", () => ({
  useExtensionRuntimeStore: (
    selector: (state: { fetchProject: (projectId: string) => Promise<void> }) => unknown,
  ) => selector({ fetchProject: async () => undefined }),
}));

vi.mock("../../../../services/extensionPackage", () => ({
  listExtensionArtifacts: vi.fn(async () => artifactsState.list),
  downloadExtensionArtifact: vi.fn(),
  installExtensionArtifact: vi.fn(),
  uploadExtensionArtifact: vi.fn(),
}));

vi.mock("../../../../services/extensionRuntime", () => ({
  uninstallExtensionInstallation: vi.fn(),
}));

import { ExtensionCategoryPanel } from "../ExtensionCategoryPanel";
import { ExtensionArtifactRow } from "./ExtensionArtifactRow";

function emptyProjection(
  installations: ExtensionInstallationProjectionResponse[],
): ExtensionRuntimeProjectionResponse {
  return {
    installations,
    commands: [],
    flags: [],
    message_renderers: [],
    runtime_actions: [],
    workspace_tabs: [],
    permissions: [],
    bundles: [],
  };
}

function localArchiveInstallation(): ExtensionInstallationProjectionResponse {
  return {
    installation_id: "install-local",
    extension_key: "local-hello",
    extension_id: "local-hello",
    display_name: "Local Hello",
    installed_source: null,
    package_artifact: {
      artifact_id: "artifact-1",
      package_name: "@agentdash/local-hello",
      package_version: "0.1.0",
      asset_version: "v1",
      source_version: "0.1.0",
      storage_ref: "ref",
      archive_digest: "sha256:abc",
      manifest_digest: "sha256:def",
    },
  };
}

function marketplaceInstallation(): ExtensionInstallationProjectionResponse {
  return {
    installation_id: "install-mp",
    extension_key: "mp-only",
    extension_id: "mp-only",
    display_name: "Marketplace Only",
    installed_source: {
      library_asset_id: "asset-mp",
      source_ref: "plugin:mp",
      source_version: "0.5.0",
      source_digest: "sha256:digest",
      installed_at: "2026-05-26T00:00:00Z",
    },
    package_artifact: null,
  };
}

function sampleArtifact(): ExtensionPackageArtifactResponse {
  return {
    id: "artifact-1",
    project_id: "project-1",
    extension_id: "local-hello",
    package_name: "@agentdash/local-hello",
    package_version: "0.1.0",
    asset_version: "v1",
    source_version: "0.1.0",
    storage_ref: "ref",
    archive_digest: "sha256:abcdef0123456789abcdef0123456789",
    manifest_digest: "sha256:def",
    manifest: {},
    byte_size: 12345n,
    created_at: "2026-05-27T00:00:00Z",
    updated_at: "2026-05-27T00:01:00Z",
  };
}

describe("ExtensionCategoryPanel 渲染态", () => {
  beforeEach(() => {
    projectStoreState.currentProjectId = "project-1";
    runtimeState.status = "ready";
    runtimeState.projection = emptyProjection([]);
    runtimeState.error = null;
    artifactsState.list = [];
  });

  afterEach(() => {
    vi.clearAllMocks();
  });

  it("空 project 渲染占位", () => {
    projectStoreState.currentProjectId = null;
    const html = renderToStaticMarkup(<ExtensionCategoryPanel />);
    expect(html).toContain("请选择项目");
  });

  it("空状态渲染上传按钮和两段空提示", () => {
    runtimeState.projection = emptyProjection([]);
    artifactsState.list = [];
    const html = renderToStaticMarkup(<ExtensionCategoryPanel />);
    expect(html).toContain("上传归档");
    expect(html).toContain("已安装 (0)");
    expect(html).toContain("归档库 (0)");
    expect(html).toContain("当前项目还未安装扩展");
    expect(html).toContain("还没有上传过归档");
  });

  it("仅有已安装 + marketplace 来源时不显示「下载归档」按钮", () => {
    runtimeState.projection = emptyProjection([marketplaceInstallation()]);
    const html = renderToStaticMarkup(<ExtensionCategoryPanel />);
    expect(html).toContain("Marketplace Only");
    expect(html).toContain("Marketplace");
    expect(html).not.toContain("下载归档");
    expect(html).toContain("卸载");
  });

  it("已安装 + 本地归档来源时渲染「下载归档」按钮", () => {
    runtimeState.projection = emptyProjection([localArchiveInstallation()]);
    const html = renderToStaticMarkup(<ExtensionCategoryPanel />);
    expect(html).toContain("Local Hello");
    expect(html).toContain("本地归档");
    expect(html).toContain("下载归档");
    expect(html).toContain("卸载");
  });

  it("artifact row 渲染 package name + 「从归档安装」按钮", () => {
    const html = renderToStaticMarkup(
      <ExtensionArtifactRow
        artifact={sampleArtifact()}
        busy={false}
        onInstall={() => undefined}
        onDownload={() => undefined}
      />,
    );
    expect(html).toContain("local-hello");
    expect(html).toContain("@agentdash/local-hello@0.1.0");
    expect(html).toContain("从归档安装");
  });

  it("runtime error 时渲染 error 文案", () => {
    runtimeState.status = "error";
    runtimeState.error = "boom";
    const html = renderToStaticMarkup(<ExtensionCategoryPanel />);
    expect(html).toContain("boom");
  });
});
