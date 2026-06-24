import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  delete: vi.fn(),
  get: vi.fn(),
  post: vi.fn(),
  put: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    delete: mocks.delete,
    get: mocks.get,
    post: mocks.post,
    put: mocks.put,
  },
}));

import {
  copyCanvasToPersonal,
  fetchProjectCanvases,
  publishCanvasToProject,
  unpublishCanvas,
} from "./canvas";

describe("canvas service", () => {
  beforeEach(() => {
    mocks.delete.mockReset();
    mocks.get.mockReset();
    mocks.post.mockReset();
    mocks.put.mockReset();
  });

  it("fetches project canvases without a scope query when scope is omitted", async () => {
    mocks.get.mockResolvedValueOnce([]);

    await fetchProjectCanvases("project 1");

    expect(mocks.get).toHaveBeenCalledWith("/projects/project%201/canvases");
  });

  it("serializes project canvas list scope", async () => {
    mocks.get.mockResolvedValue([]);

    await fetchProjectCanvases("project-1", "mine");
    await fetchProjectCanvases("project-1", "shared");
    await fetchProjectCanvases("project-1", "all");

    expect(mocks.get).toHaveBeenNthCalledWith(1, "/projects/project-1/canvases?scope=mine");
    expect(mocks.get).toHaveBeenNthCalledWith(2, "/projects/project-1/canvases?scope=shared");
    expect(mocks.get).toHaveBeenNthCalledWith(3, "/projects/project-1/canvases?scope=all");
  });

  it("publishes a personal canvas to the project shared scope", async () => {
    const response = { canvas_id: "shared-1" };
    mocks.post.mockResolvedValueOnce(response);

    const result = await publishCanvasToProject("canvas/source", {
      title: "Shared dashboard",
      description: "Stable team view",
    });

    expect(mocks.post).toHaveBeenCalledWith("/canvases/canvas%2Fsource/publish-to-project", {
      title: "Shared dashboard",
      description: "Stable team view",
    });
    expect(result).toBe(response);
  });

  it("copies a shared canvas to a personal canvas", async () => {
    const response = { canvas_id: "personal-copy-1" };
    mocks.post.mockResolvedValueOnce(response);

    const result = await copyCanvasToPersonal("shared-1", {
      canvas_mount_id: "cvs-personal-copy",
    });

    expect(mocks.post).toHaveBeenCalledWith("/canvases/shared-1/copy-to-personal", {
      canvas_mount_id: "cvs-personal-copy",
    });
    expect(result).toBe(response);
  });

  it("unpublishes a project shared canvas", async () => {
    const response = {
      unpublished_canvas_id: "shared-1",
      source_canvas_id: "personal-1",
    };
    mocks.post.mockResolvedValueOnce(response);

    const result = await unpublishCanvas("shared-1");

    expect(mocks.post).toHaveBeenCalledWith("/canvases/shared-1/unpublish", {});
    expect(result).toBe(response);
  });
});
