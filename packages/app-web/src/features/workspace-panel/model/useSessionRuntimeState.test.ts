import { describe, expect, it } from "vitest";

import type { SessionContextPayload } from "../../../services/session";
import {
  selectActiveSessionRuntimeState,
  type SessionRuntimeProjectionState,
} from "./useSessionRuntimeState";

const context: SessionContextPayload = {
  workspace_id: "workspace-1",
  agent_binding: null,
  vfs: null,
  runtime_surface: null,
  context_snapshot: null,
  session_capabilities: null,
};

const readyState: SessionRuntimeProjectionState = {
  session_id: "session-current",
  source_key: "project:project-1:binding-1",
  status: "ready",
  context,
  hook_runtime: null,
  error: null,
};

describe("Session runtime state selection", () => {
  it("session/source key 不匹配时不暴露旧 projection", () => {
    const selected = selectActiveSessionRuntimeState(
      readyState,
      "session-next",
      "project:project-2:binding-2",
    );

    expect(selected.status).toBe("idle");
    expect(selected.context).toBeNull();
    expect(selected.session_id).toBeNull();
    expect(selected.source_key).toBeNull();
  });

  it("session/source key 匹配时返回当前 runtime projection", () => {
    const selected = selectActiveSessionRuntimeState(
      readyState,
      "session-current",
      "project:project-1:binding-1",
    );

    expect(selected.status).toBe("ready");
    expect(selected.context?.workspace_id).toBe("workspace-1");
  });
});
