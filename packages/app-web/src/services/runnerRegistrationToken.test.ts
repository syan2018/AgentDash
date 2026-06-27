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

import {
  createRunnerRegistrationToken,
  listRunnerRegistrationTokens,
  revokeRunnerRegistrationToken,
  rotateRunnerRegistrationToken,
} from "./runnerRegistrationToken";

function metadata(overrides: Record<string, unknown> = {}) {
  return {
    id: "tok-1",
    project_id: "proj-1",
    name: "build-server-01",
    token_prefix: "rrt_abc",
    status: "active",
    created_by_user_id: "user-1",
    expires_at: "2026-12-31T00:00:00.000Z",
    revoked_at: null,
    last_used_at: null,
    last_claimed_backend_id: null,
    default_capability_slot: "default",
    machine_policy: {},
    created_at: "2026-06-27T00:00:00.000Z",
    updated_at: "2026-06-27T00:00:00.000Z",
    ...overrides,
  };
}

describe("runner registration token service", () => {
  beforeEach(() => {
    mocks.get.mockReset();
    mocks.post.mockReset();
  });

  it("lists tokens and never receives a plaintext token in metadata", async () => {
    mocks.get.mockResolvedValueOnce([metadata()]);

    const tokens = await listRunnerRegistrationTokens("proj-1");

    expect(mocks.get).toHaveBeenCalledWith("/projects/proj-1/runner-registration-tokens");
    expect(tokens).toHaveLength(1);
    expect(tokens[0]).not.toHaveProperty("registration_token");
  });

  it("creates a token and surfaces the one-time plaintext only on create", async () => {
    mocks.post.mockResolvedValueOnce({
      token: metadata(),
      registration_token: "rrt_plain_one_time",
    });

    const result = await createRunnerRegistrationToken("proj-1", {
      name: "build-server-01",
      machine_policy: {},
    });

    expect(mocks.post).toHaveBeenCalledWith("/projects/proj-1/runner-registration-tokens", {
      name: "build-server-01",
      machine_policy: {},
    });
    expect(result.registration_token).toBe("rrt_plain_one_time");
    expect(result.token).not.toHaveProperty("registration_token");
  });

  it("rotates a token and returns a fresh one-time plaintext", async () => {
    mocks.post.mockResolvedValueOnce({
      token: metadata({ token_prefix: "rrt_new" }),
      registration_token: "rrt_plain_rotated",
    });

    const result = await rotateRunnerRegistrationToken("proj-1", "tok-1");

    expect(mocks.post).toHaveBeenCalledWith(
      "/projects/proj-1/runner-registration-tokens/tok-1/rotate",
      {},
    );
    expect(result.registration_token).toBe("rrt_plain_rotated");
  });

  it("revokes a token and returns metadata without any plaintext", async () => {
    mocks.post.mockResolvedValueOnce({ token: metadata({ status: "revoked" }) });

    const result = await revokeRunnerRegistrationToken("proj-1", "tok-1");

    expect(mocks.post).toHaveBeenCalledWith(
      "/projects/proj-1/runner-registration-tokens/tok-1/revoke",
      {},
    );
    expect(result.token.status).toBe("revoked");
    expect(result).not.toHaveProperty("registration_token");
  });
});
