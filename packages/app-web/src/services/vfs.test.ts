import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiPostMock: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    post: mocks.apiPostMock,
  },
  authenticatedFetch: vi.fn(),
}));

vi.mock("../api/origin", () => ({
  buildApiPath: (path: string) => path,
}));

import {
  applySurfacePatch,
  createSurfaceFile,
  deleteSurfaceFile,
  renameSurfaceFile,
  writeSurfaceFile,
} from "./vfs";

describe("vfs surface mutation payloads", () => {
  beforeEach(() => {
    mocks.apiPostMock.mockReset();
    mocks.apiPostMock.mockResolvedValue({});
  });

  it("uses only surface, mount, and relative path coordinates for text mutations", async () => {
    await createSurfaceFile({
      surfaceRef: "project:project-1",
      mountId: "context",
      path: "notes/today.md",
      content: "hello",
    });
    await writeSurfaceFile({
      surfaceRef: "project:project-1",
      mountId: "context",
      path: "notes/today.md",
      content: "updated",
    });
    await deleteSurfaceFile({
      surfaceRef: "project:project-1",
      mountId: "context",
      path: "notes/today.md",
    });
    await renameSurfaceFile({
      surfaceRef: "project:project-1",
      mountId: "context",
      fromPath: "notes/today.md",
      toPath: "notes/tomorrow.md",
    });
    await applySurfacePatch({
      surfaceRef: "project:project-1",
      mountId: "context",
      patch: "*** Begin Patch\n*** End Patch\n",
    });

    const payloads = mocks.apiPostMock.mock.calls.map(([, payload]) => payload);
    expect(payloads).toEqual([
      {
        surface_ref: "project:project-1",
        mount_id: "context",
        path: "notes/today.md",
        content: "hello",
      },
      {
        surface_ref: "project:project-1",
        mount_id: "context",
        path: "notes/today.md",
        content: "updated",
      },
      {
        surface_ref: "project:project-1",
        mount_id: "context",
        path: "notes/today.md",
      },
      {
        surface_ref: "project:project-1",
        mount_id: "context",
        from_path: "notes/today.md",
        to_path: "notes/tomorrow.md",
      },
      {
        surface_ref: "project:project-1",
        mount_id: "context",
        patch: "*** Begin Patch\n*** End Patch\n",
      },
    ]);

    for (const payload of payloads) {
      expect(payload).not.toHaveProperty("owner_kind");
      expect(payload).not.toHaveProperty("owner_id");
      expect(payload).not.toHaveProperty("container_id");
      expect(payload).not.toHaveProperty("context_container_id");
    }
  });
});
