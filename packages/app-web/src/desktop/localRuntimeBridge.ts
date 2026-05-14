import {
  DEFAULT_LOCAL_RUNTIME_BACKEND_NAME,
  DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
  DEFAULT_LOCAL_RUNTIME_SERVER_URL,
} from '@agentdash/core/local-runtime';
import type { LocalRuntimeClient, LocalRuntimeProfile } from '@agentdash/core/local-runtime';
import { API_ORIGIN } from '../api/origin';

declare global {
  interface Window {
    __AGENTDASH_DESKTOP_LOCAL_RUNTIME__?: LocalRuntimeClient;
  }
}

export function getDesktopLocalRuntimeClient(): LocalRuntimeClient | null {
  if (typeof window === 'undefined') return null;
  return window.__AGENTDASH_DESKTOP_LOCAL_RUNTIME__ ?? null;
}

export async function ensureDesktopLocalRuntimeStarted(accessToken: string): Promise<void> {
  const client = getDesktopLocalRuntimeClient();
  const token = accessToken.trim();
  if (!client || !token) return;

  const snapshot = await client.runtimeSnapshot().catch(() => null);
  if (snapshot?.state === 'starting' || snapshot?.state === 'running') return;

  const profile = await loadOrCreateAutoStartProfile(client, token);
  if (!profile.auto_start) return;

  await client.runtimeStart({
    ...profile,
    access_token: token,
    server_url: resolveDesktopServerUrl(profile.server_url),
  });
}

async function loadOrCreateAutoStartProfile(
  client: LocalRuntimeClient,
  accessToken: string,
): Promise<LocalRuntimeProfile> {
  const current = await client.profileLoad().catch(() => null);
  if (current) {
    const normalized = {
      ...current,
      access_token: accessToken,
      server_url: resolveDesktopServerUrl(current.server_url),
      profile_id: current.profile_id || DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
      device_id: current.device_id || createDeviceId(),
    };
    await client.profileSave(normalized);
    return normalized;
  }

  const created: LocalRuntimeProfile = {
    server_url: resolveDesktopServerUrl(''),
    access_token: accessToken,
    profile_id: DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
    device_id: createDeviceId(),
    name: DEFAULT_LOCAL_RUNTIME_BACKEND_NAME,
    accessible_roots: [],
    executor_enabled: true,
    auto_start: true,
    backend_id: null,
    relay_ws_url: null,
  };
  await client.profileSave(created);
  return created;
}

function resolveDesktopServerUrl(value: string): string {
  const explicit = value.trim().replace(/\/+$/, '');
  if (explicit) return explicit;
  return API_ORIGIN || DEFAULT_LOCAL_RUNTIME_SERVER_URL;
}

function createDeviceId(): string {
  if (typeof crypto !== 'undefined' && typeof crypto.randomUUID === 'function') {
    return crypto.randomUUID();
  }
  return `device-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
}
