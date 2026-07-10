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
import type { ProjectExtensionManagementListResponse } from "../generated/extension-management-contracts";

function sampleExtensionResponse(): ProjectExtensionManagementListResponse {
  return {
    extensions: [
      {
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
          protocols: 0,
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
      },
    ],
  };
}

describe("extensionManagement service", () => {
  beforeEach(() => {
    mocks.apiGet.mockReset();
  });

  it("returns generated Project Extension management DTO", async () => {
    const response = sampleExtensionResponse();
    mocks.apiGet.mockResolvedValueOnce(response);

    const result = await fetchProjectExtensions("project-1");

    expect(mocks.apiGet).toHaveBeenCalledWith("/projects/project-1/extensions");
    expect(result).toBe(response);
  });
});
