import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiGet: vi.fn(),
  apiPost: vi.fn(),
  authenticatedFetch: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    get: mocks.apiGet,
    post: mocks.apiPost,
  },
  authenticatedFetch: mocks.authenticatedFetch,
}));

vi.mock("../api/origin", () => ({
  buildApiPath: (path: string) => `/api${path}`,
}));

import {
  downloadExtensionArtifact,
  installExtensionArtifact,
  listExtensionArtifacts,
  parseContentDispositionFilename,
  uploadExtensionArtifact,
} from "./extensionPackage";

function sampleArtifactWire(): Record<string, unknown> {
  return {
    id: "artifact-1",
    project_id: "project-1",
    extension_id: "local-hello",
    package_name: "@agentdash/local-hello",
    package_version: "0.1.0",
    asset_version: "2026.05.27",
    source_version: "0.1.0",
    storage_ref: "extension-packages/project-1/digest.agentdash-extension.tgz",
    archive_digest: "sha256:abc",
    manifest_digest: "sha256:def",
    manifest: { name: "local-hello", version: "0.1.0" },
    byte_size: 12345,
    created_at: "2026-05-27T00:00:00Z",
    updated_at: "2026-05-27T00:01:00Z",
  };
}

describe("extensionPackage list mapper", () => {
  beforeEach(() => {
    mocks.apiGet.mockReset();
    mocks.apiPost.mockReset();
    mocks.authenticatedFetch.mockReset();
  });

  it("maps full artifact shape including bigint byte_size", async () => {
    mocks.apiGet.mockResolvedValueOnce([sampleArtifactWire()]);
    const list = await listExtensionArtifacts("project-1");
    expect(mocks.apiGet).toHaveBeenCalledWith(
      "/projects/project-1/extension-artifacts",
    );
    expect(list).toHaveLength(1);
    const item = list[0];
    expect(item.id).toBe("artifact-1");
    expect(item.byte_size).toBe(12345n);
    expect(item.manifest).toEqual({ name: "local-hello", version: "0.1.0" });
    expect(item.archive_digest).toBe("sha256:abc");
  });

  it("accepts byte_size as numeric string", async () => {
    const wire = sampleArtifactWire();
    wire.byte_size = "9007199254740993"; // > Number.MAX_SAFE_INTEGER
    mocks.apiGet.mockResolvedValueOnce([wire]);
    const [item] = await listExtensionArtifacts("project-1");
    expect(item.byte_size).toBe(9007199254740993n);
  });

  it("rejects non-array list response", async () => {
    mocks.apiGet.mockResolvedValueOnce({});
    await expect(listExtensionArtifacts("project-1")).rejects.toThrow(/不是数组/);
  });
});

describe("extensionPackage install", () => {
  beforeEach(() => {
    mocks.apiPost.mockReset();
  });

  it("posts install request with body and maps response", async () => {
    mocks.apiPost.mockResolvedValueOnce({
      installation_id: "installation-1",
      extension_key: "local-hello",
      extension_id: "local-hello",
      package_artifact_id: "artifact-1",
      archive_digest: "sha256:abc",
    });
    const result = await installExtensionArtifact("project-1", "artifact-1", {
      extension_key: null,
      display_name: "Local Hello",
      overwrite: true,
    });
    expect(mocks.apiPost).toHaveBeenCalledWith(
      "/projects/project-1/extension-artifacts/artifact-1/install",
      {
        extension_key: null,
        display_name: "Local Hello",
        overwrite: true,
      },
    );
    expect(result.installation_id).toBe("installation-1");
    expect(result.extension_key).toBe("local-hello");
  });
});

describe("extensionPackage upload", () => {
  beforeEach(() => {
    mocks.authenticatedFetch.mockReset();
  });

  it("submits multipart form-data with archive_digest + archive", async () => {
    mocks.authenticatedFetch.mockImplementation(async (url, init) => {
      expect(url).toBe("/api/projects/project-1/extension-artifacts");
      expect(init?.method).toBe("POST");
      const body = init?.body;
      expect(body).toBeInstanceOf(FormData);
      const form = body as FormData;
      expect(form.get("archive_digest")).toBe("sha256:deadbeef");
      const archive = form.get("archive");
      expect(archive).toBeInstanceOf(File);
      expect((archive as File).name).toBe("local-hello.agentdash-extension.tgz");
      return new Response(JSON.stringify(sampleArtifactWire()), {
        status: 200,
        headers: { "Content-Type": "application/json" },
      });
    });

    const file = new File([new Uint8Array([1, 2, 3])], "local-hello.agentdash-extension.tgz", {
      type: "application/gzip",
    });
    const result = await uploadExtensionArtifact("project-1", file, "sha256:deadbeef");
    expect(result.id).toBe("artifact-1");
  });

  it("translates non-ok response into ApiError-style Error with status", async () => {
    mocks.authenticatedFetch.mockResolvedValueOnce(
      new Response(JSON.stringify({ error: "digest mismatch" }), {
        status: 400,
        headers: { "Content-Type": "application/json" },
      }),
    );
    const file = new File([new Uint8Array([1])], "x.tgz");
    await expect(
      uploadExtensionArtifact("project-1", file, "sha256:bad"),
    ).rejects.toMatchObject({
      message: "digest mismatch",
      status: 400,
    });
  });

  it("falls back to HTTP status text when error body is invalid", async () => {
    mocks.authenticatedFetch.mockResolvedValueOnce(
      new Response("oops", {
        status: 500,
        statusText: "Internal Server Error",
      }),
    );
    const file = new File([new Uint8Array([1])], "x.tgz");
    await expect(
      uploadExtensionArtifact("project-1", file, "sha256:bad"),
    ).rejects.toMatchObject({
      message: "Internal Server Error",
      status: 500,
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

  it("returns empty filename when Content-Disposition is missing", async () => {
    mocks.authenticatedFetch.mockResolvedValueOnce(
      new Response(new Uint8Array([1]), { status: 200 }),
    );
    const result = await downloadExtensionArtifact("project-1", "artifact-1");
    expect(result.filename).toBe("");
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
      parseContentDispositionFilename(
        "attachment; filename*=UTF-8''local%2Dhello.tgz",
      ),
    ).toBe("local-hello.tgz");
  });

  it("returns empty string when header is null", () => {
    expect(parseContentDispositionFilename(null)).toBe("");
  });

  it("returns empty string when header has no filename", () => {
    expect(parseContentDispositionFilename("attachment")).toBe("");
  });
});

afterEach(() => {
  vi.restoreAllMocks();
});
