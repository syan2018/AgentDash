import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiGet: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    get: mocks.apiGet,
  },
}));

import { fetchProjectExtensions } from "./extensionManagement";

function sampleExtensionWire(): Record<string, unknown> {
  return {
    installation_id: "installation-1",
    extension_key: "local-hello",
    extension_id: "local-hello",
    display_name: "Local Hello",
    enabled: true,
    installed_source: {
      library_asset_id: "library-1",
      source_ref: "user:u1:extension_template:local-hello",
      source_version: "0.1.0",
      source_digest: "sha256:abc",
      installed_at: "2026-05-27T00:00:00Z",
    },
    source_status: "up_to_date",
    current_source_version: "0.1.0",
    current_source_digest: "sha256:abc",
    package_mode: "packaged",
    package_artifact: {
      artifact_id: "artifact-1",
      package_name: "@agentdash/local-hello",
      package_version: "0.1.0",
      asset_version: "0.1.0",
      source_version: "0.1.0",
      storage_ref: "extension-packages/project/project-1/digest.tgz",
      archive_digest: "sha256:def",
      manifest_digest: "sha256:abc",
    },
    capability_summary: {
      commands: 1,
      flags: 1,
      message_renderers: 0,
      runtime_actions: 1,
      protocol_channels: 0,
      workspace_tabs: 1,
      permissions: 2,
      bundles: 1,
    },
    manifest: {
      manifest_version: "2",
      extension_id: "local-hello",
      package: { name: "@agentdash/local-hello", version: "0.1.0" },
      asset_version: "0.1.0",
    },
    created_at: "2026-05-27T00:00:00Z",
    updated_at: "2026-05-27T00:01:00Z",
  };
}

describe("extensionManagement service", () => {
  beforeEach(() => {
    mocks.apiGet.mockReset();
  });

  it("maps Project Extension management list", async () => {
    mocks.apiGet.mockResolvedValueOnce({ extensions: [sampleExtensionWire()] });

    const result = await fetchProjectExtensions("project-1");

    expect(mocks.apiGet).toHaveBeenCalledWith("/projects/project-1/extensions");
    expect(result.extensions).toHaveLength(1);
    const item = result.extensions[0];
    expect(item.extension_key).toBe("local-hello");
    expect(item.package_mode).toBe("packaged");
    expect(item.package_artifact?.artifact_id).toBe("artifact-1");
    expect(item.capability_summary.runtime_actions).toBe(1);
    expect(item.source_status).toBe("up_to_date");
  });

  it("rejects invalid package mode", async () => {
    const wire = sampleExtensionWire();
    wire.package_mode = "archive";
    mocks.apiGet.mockResolvedValueOnce({ extensions: [wire] });

    await expect(fetchProjectExtensions("project-1")).rejects.toThrow(/package_mode/);
  });
});
