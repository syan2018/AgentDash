import { describe, expect, it } from "vitest";

import type { ResolvedVfsSurface } from "../../../generated/vfs-contracts";
import {
  emptySessionRuntimeState,
  selectActiveSessionRuntimeState,
  type SessionRuntimeProjectionState,
} from "./useSessionRuntimeState";

const runtimeSurface: ResolvedVfsSurface = {
  surface_ref: "session-runtime:session-1",
  source: { source_type: "session_runtime", session_id: "session-1" },
  mounts: [
    {
      id: "main",
      display_name: "main",
      provider: "relay_fs",
      backend_id: "backend-1",
      capabilities: ["read", "write", "list", "search", "exec"],
      default_write: true,
      purpose: "workspace",
      backend_online: true,
      edit_capabilities: {
        create: true,
        delete: true,
        rename: true,
      },
    },
  ],
  default_mount_id: "main",
};

function readyState(): SessionRuntimeProjectionState {
  return {
    ...emptySessionRuntimeState(),
    session_id: "session-1",
    source_key: "session:session-1",
    status: "ready",
    runtime_surface: runtimeSurface,
  };
}

describe("selectActiveSessionRuntimeState", () => {
  it("保留匹配 session/source key 的 runtime surface", () => {
    const selected = selectActiveSessionRuntimeState(
      readyState(),
      "session-1",
      "session:session-1",
    );

    expect(selected.runtime_surface?.default_mount_id).toBe("main");
    expect(selected.runtime_surface?.mounts[0]?.id).toBe("main");
  });

  it("session/source key 不匹配时不泄漏上一份 runtime surface", () => {
    const selected = selectActiveSessionRuntimeState(
      readyState(),
      "session-2",
      "session:session-2",
    );

    expect(selected.runtime_surface).toBeNull();
    expect(selected.status).toBe("idle");
  });
});
