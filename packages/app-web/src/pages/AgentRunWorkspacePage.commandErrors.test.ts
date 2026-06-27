import { describe, expect, it } from "vitest";

import {
  apiErrorCode,
  isStaleAgentRunCommandError,
  silentCommandRefreshError,
} from "./AgentRunWorkspacePage.commandErrors";

describe("AgentRunWorkspacePage command errors", () => {
  it("detects structured stale command errors", () => {
    const error = Object.assign(new Error("stale"), { errorCode: "stale_command" });

    expect(apiErrorCode(error)).toBe("stale_command");
    expect(isStaleAgentRunCommandError(error)).toBe(true);
  });

  it("marks command refresh errors as silent", () => {
    const error = silentCommandRefreshError();

    expect((error as { silentCommandRefresh?: unknown }).silentCommandRefresh).toBe(true);
  });
});
