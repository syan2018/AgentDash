import { describe, expect, it } from "vitest";
import {
  buildRunnerSetupCommand,
  defaultRunnerWorkspaceRoot,
} from "./setupCommand";

describe("buildRunnerSetupCommand", () => {
  it("assembles the generic-binary setup command with explicit origin and fixed service flags", () => {
    const command = buildRunnerSetupCommand({
      origin: "https://cloud.example.com",
      token: "rrt_plain_abc123",
      runnerName: "build-server-01",
      workspaceRoot: "/srv/agentdash/workspaces",
    });

    expect(command).toBe(
      "agentdash-local setup --server https://cloud.example.com --token rrt_plain_abc123 " +
        "--name build-server-01 --workspace-root /srv/agentdash/workspaces --install-service --start",
    );
  });

  it("strips trailing slashes from the origin so the URL never doubles up", () => {
    const command = buildRunnerSetupCommand({
      origin: "https://cloud.example.com///",
      token: "rrt_x",
      runnerName: "r1",
      workspaceRoot: "/data",
    });

    expect(command).toContain("--server https://cloud.example.com ");
    expect(command).not.toContain("example.com/ ");
  });

  it("falls back to the default workspace root when blank", () => {
    const command = buildRunnerSetupCommand({
      origin: "https://cloud.example.com",
      token: "rrt_x",
      runnerName: "r1",
      workspaceRoot: "   ",
    });

    expect(command).toContain(`--workspace-root ${defaultRunnerWorkspaceRoot()}`);
  });

  it("shell-quotes runner names and paths that contain spaces", () => {
    const command = buildRunnerSetupCommand({
      origin: "https://cloud.example.com",
      token: "rrt_x",
      runnerName: "My Server",
      workspaceRoot: "C:/Program Files/agentdash",
    });

    expect(command).toContain("--name 'My Server'");
    expect(command).toContain("--workspace-root 'C:/Program Files/agentdash'");
  });

  it("always includes --install-service and --start", () => {
    const command = buildRunnerSetupCommand({
      origin: "https://cloud.example.com",
      token: "rrt_x",
      runnerName: "r1",
      workspaceRoot: "/data",
    });

    expect(command.endsWith("--install-service --start")).toBe(true);
  });
});
