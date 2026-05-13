import { api } from "../api/client";
import type { AuthGroup, AuthMode, CurrentUser } from "../types";
import { isAuthMode } from "../types";

function normalizeAuthMode(value: unknown): AuthMode {
  if (!isAuthMode(value)) {
    throw new Error(`未知的 auth_mode: ${String(value ?? "")}`);
  }
  return value;
}

function mapAuthGroup(raw: Record<string, unknown>): AuthGroup {
  return {
    group_id: String(raw.group_id ?? ""),
    display_name: raw.display_name != null ? String(raw.display_name) : null,
  };
}

export function mapCurrentUser(raw: Record<string, unknown>): CurrentUser {
  return {
    auth_mode: normalizeAuthMode(raw.auth_mode),
    user_id: String(raw.user_id ?? ""),
    subject: String(raw.subject ?? raw.user_id ?? ""),
    display_name: raw.display_name != null ? String(raw.display_name) : null,
    email: raw.email != null ? String(raw.email) : null,
    groups: Array.isArray(raw.groups)
      ? raw.groups
          .filter((item): item is Record<string, unknown> => Boolean(item) && typeof item === "object")
          .map(mapAuthGroup)
      : [],
    is_admin: Boolean(raw.is_admin),
    provider: raw.provider != null ? String(raw.provider) : null,
    extra: raw.extra ?? null,
  };
}

export async function fetchCurrentUser(): Promise<CurrentUser> {
  const raw = await api.get<Record<string, unknown>>("/me");
  return mapCurrentUser(raw);
}
