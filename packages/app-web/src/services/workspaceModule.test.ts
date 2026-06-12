import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  apiGetMock: vi.fn(),
  apiPostMock: vi.fn(),
}));

vi.mock("../api/client", () => ({
  api: {
    get: mocks.apiGetMock,
    post: mocks.apiPostMock,
  },
}));

import {
  fetchProjectWorkspaceModules,
  presentWorkspaceModule,
} from "./workspaceModule";

describe("workspace module service", () => {
  beforeEach(() => {
    mocks.apiGetMock.mockReset();
    mocks.apiGetMock.mockResolvedValue([]);
    mocks.apiPostMock.mockReset();
    mocks.apiPostMock.mockResolvedValue({
      module_id: "canvas:mount-a",
      view_key: "preview",
      renderer_kind: "canvas",
      presentation_uri: "canvas://mount-a",
      title: "Canvas A",
    });
  });

  it("fetches project workspace modules from the project projection endpoint", async () => {
    await fetchProjectWorkspaceModules("project/1");

    expect(mocks.apiGetMock).toHaveBeenCalledWith(
      "/projects/project%2F1/workspace-modules",
    );
  });

  it("posts user-open presentation requests through the project workspace module endpoint", async () => {
    await presentWorkspaceModule("project/1", {
      module_id: "canvas:mount-a",
      view_key: "preview",
      runtime_session_id: "session-1",
    });

    expect(mocks.apiPostMock).toHaveBeenCalledWith(
      "/projects/project%2F1/workspace-modules/present",
      {
        module_id: "canvas:mount-a",
        view_key: "preview",
        runtime_session_id: "session-1",
      },
    );
  });
});
