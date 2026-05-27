import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  get: vi.fn(),
  post: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    get: mocks.get,
    post: mocks.post,
  },
}));

import { fetchLibraryAssets, publishLibraryAsset } from "./sharedLibrary";

function extensionTemplateWire() {
  return {
    id: "asset-1",
    asset_type: "extension_template",
    scope: "user",
    owner_id: "user-1",
    key: "local-hello",
    display_name: "Local Hello",
    version: "0.1.0",
    source: "user_authored",
    payload_digest: "sha256:manifest",
    deprecated: false,
    payload: {
      manifest_version: "1",
      extension_id: "local-hello",
    },
    extension_package_artifact: {
      id: "artifact-1",
      package_name: "@agentdash/local-hello",
      package_version: "0.1.0",
      asset_version: "0.1.0",
      source_version: "0.1.0",
      archive_digest: "sha256:archive",
      manifest_digest: "sha256:manifest",
      byte_size: 12345,
      created_at: "2026-05-27T00:00:00Z",
    },
    created_at: "2026-05-27T00:00:00Z",
    updated_at: "2026-05-27T00:00:00Z",
  };
}

describe("sharedLibrary service", () => {
  beforeEach(() => {
    mocks.get.mockReset();
    mocks.post.mockReset();
  });

  it("normalizes extension package artifact byte_size from JSON number to bigint", async () => {
    mocks.get.mockResolvedValueOnce([extensionTemplateWire()]);

    const result = await fetchLibraryAssets({
      asset_type: "extension_template",
      include_deprecated: true,
    });

    expect(mocks.get).toHaveBeenCalledWith(
      "/shared-library/assets?asset_type=extension_template&include_deprecated=true",
    );
    expect(result[0].extension_package_artifact?.byte_size).toBe(12345n);
  });

  it("normalizes published extension template response", async () => {
    mocks.post.mockResolvedValueOnce(extensionTemplateWire());

    const result = await publishLibraryAsset("project-1", {
      asset_kind: "extension_installation",
      project_asset_id: "installation-1",
      scope: "user",
      key: "local-hello",
      display_name: "Local Hello",
      version: "0.1.0",
      overwrite: false,
    });

    expect(mocks.post).toHaveBeenCalledWith("/projects/project-1/shared-library/publish", {
      asset_kind: "extension_installation",
      project_asset_id: "installation-1",
      scope: "user",
      key: "local-hello",
      display_name: "Local Hello",
      version: "0.1.0",
      overwrite: false,
    });
    expect(result.extension_package_artifact?.byte_size).toBe(12345n);
  });
});
