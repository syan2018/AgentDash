import type { JsonObject, JsonValue } from "@agentdash/extension-sdk";

export const LOCAL_HELLO_ACTION_KEY = "local-hello.profile";

export type LocalHelloProfile = JsonObject & {
  username: string;
  platform: string;
  arch: string;
  backend_id: string;
  project_id: string;
  session_id: string;
  workspace_roots: JsonValue[];
};

export function normalizeProfile(raw: JsonObject): LocalHelloProfile {
  return {
    username: readText(raw.username, "local-user"),
    platform: readText(raw.platform, "unknown"),
    arch: readText(raw.arch, "unknown"),
    backend_id: readText(raw.backend_id, "unknown"),
    project_id: readText(raw.project_id, "unknown"),
    session_id: readText(raw.session_id, "unknown"),
    workspace_roots: Array.isArray(raw.workspace_roots) ? raw.workspace_roots : [],
  };
}

export function readText(value: JsonValue | undefined, fallback: string): string {
  return typeof value === "string" && value.trim() !== "" ? value : fallback;
}
