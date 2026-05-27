import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  authenticatedFetch: vi.fn(),
}));

vi.mock("../api/client", () => ({
  authenticatedFetch: mocks.authenticatedFetch,
}));

vi.mock("../api/origin", () => ({
  buildApiPath: (path: string) => `/api${path}`,
}));

import {
  downloadExtensionArtifact,
  importExtensionPackage,
  parseContentDispositionFilename,
} from "./extensionPackage";

function sampleArtifactWire(): Record<string, unknown> {
  return {
    id: "artifact-1",
    owner_kind: "project",
    owner_id: "project-1",
    extension_id: "local-hello",
    package_name: "@agentdash/local-hello",
    package_version: "0.1.0",
    asset_version: "2026.05.27",
    source_version: "0.1.0",
    storage_ref: "extension-packages/project/project-1/digest.agentdash-extension.tgz",
    archive_digest: "sha256:abc",
    manifest_digest: "sha256:def",
    manifest: { name: "local-hello", version: "0.1.0" },
    byte_size: 12345,
    created_at: "2026-05-27T00:00:00Z",
    updated_at: "2026-05-27T00:01:00Z",
  };
}

describe("extensionPackage import", () => {
  beforeEach(() => {
    mocks.authenticatedFetch.mockReset();
  });

  it("submits one-step import/install form-data and maps owner-aware artifact", async () => {
    mocks.authenticatedFetch.mockImplementation(async (url, init) => {
      expect(url).toBe("/api/projects/project-1/extensions/import-package");
      expect(init?.method).toBe("POST");
      const body = init?.body;
      expect(body).toBeInstanceOf(FormData);
      const form = body as FormData;
      expect(form.get("archive_digest")).toBe("sha256:deadbeef");
      expect(form.get("extension_key")).toBe("local-hello");
      expect(form.get("display_name")).toBe("Local Hello");
      expect(form.get("overwrite")).toBe("true");
      expect(form.get("archive")).toBeInstanceOf(File);
      return new Response(
        JSON.stringify({
          artifact: sampleArtifactWire(),
          installation: {
            installation_id: "installation-1",
            extension_key: "local-hello",
            extension_id: "local-hello",
            package_artifact_id: "artifact-1",
            archive_digest: "sha256:abc",
          },
        }),
        {
          status: 200,
          headers: { "Content-Type": "application/json" },
        },
      );
    });

    const file = new File([new Uint8Array([1])], "local-hello.tgz");
    const result = await importExtensionPackage("project-1", file, "sha256:deadbeef", {
      extension_key: "local-hello",
      display_name: "Local Hello",
      overwrite: true,
    });

    expect(result.artifact.owner_kind).toBe("project");
    expect(result.artifact.owner_id).toBe("project-1");
    expect(result.artifact.byte_size).toBe(12345n);
    expect(result.installation.installation_id).toBe("installation-1");
  });

  it("translates import error response", async () => {
    mocks.authenticatedFetch.mockResolvedValueOnce(
      new Response(JSON.stringify({ error: "digest mismatch" }), {
        status: 400,
        headers: { "Content-Type": "application/json" },
      }),
    );
    const file = new File([new Uint8Array([1])], "x.tgz");

    await expect(
      importExtensionPackage("project-1", file, "sha256:bad", {
        extension_key: null,
        display_name: null,
        overwrite: true,
      }),
    ).rejects.toMatchObject({
      message: "digest mismatch",
      status: 400,
    });
  });
});

describe("extensionPackage download", () => {
  beforeEach(() => {
    mocks.authenticatedFetch.mockReset();
  });

  it("returns blob and parses Content-Disposition filename", async () => {
    const payload = new Uint8Array([1, 2, 3]);
    mocks.authenticatedFetch.mockResolvedValueOnce(
      new Response(payload, {
        status: 200,
        headers: {
          "Content-Type": "application/gzip",
          "Content-Disposition":
            "attachment; filename=\"local-hello-0.1.0.agentdash-extension.tgz\"",
        },
      }),
    );

    const result = await downloadExtensionArtifact("project-1", "artifact-1");

    expect(mocks.authenticatedFetch).toHaveBeenCalledWith(
      "/api/projects/project-1/extension-artifacts/artifact-1/archive",
      { method: "GET" },
    );
    expect(result.blob).toBeInstanceOf(Blob);
    expect(result.filename).toBe("local-hello-0.1.0.agentdash-extension.tgz");
  });

  it("translates download error response", async () => {
    mocks.authenticatedFetch.mockResolvedValueOnce(
      new Response(JSON.stringify({ error: "not found" }), {
        status: 404,
        headers: { "Content-Type": "application/json" },
      }),
    );

    await expect(
      downloadExtensionArtifact("project-1", "missing"),
    ).rejects.toMatchObject({
      message: "not found",
      status: 404,
    });
  });
});

describe("parseContentDispositionFilename", () => {
  it("parses quoted filename", () => {
    expect(
      parseContentDispositionFilename(
        'attachment; filename="local-hello-0.1.0.agentdash-extension.tgz"',
      ),
    ).toBe("local-hello-0.1.0.agentdash-extension.tgz");
  });

  it("parses RFC 5987 filename* with UTF-8 encoding", () => {
    expect(
      parseContentDispositionFilename("attachment; filename*=UTF-8''local%2Dhello.tgz"),
    ).toBe("local-hello.tgz");
  });

  it("returns empty string when header has no filename", () => {
    expect(parseContentDispositionFilename("attachment")).toBe("");
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});
