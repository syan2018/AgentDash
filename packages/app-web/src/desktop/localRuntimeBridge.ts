import {
  DEFAULT_LOCAL_RUNTIME_BACKEND_NAME,
  DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
  DEFAULT_LOCAL_RUNTIME_SERVER_URL,
} from '@agentdash/core/local-runtime';
import type {
  DesktopApiSnapshot,
  DesktopAutostartStatus,
  DesktopRuntimeSettings,
  LocalRuntimeClient,
  LocalRuntimeProfile,
} from '@agentdash/core/local-runtime';
import type { BrowseDirectoryResult } from '@agentdash/views/directory-browser';
import { API_ORIGIN } from '../api/origin';

declare global {
  interface Window {
    __AGENTDASH_DESKTOP_LOCAL_RUNTIME__?: LocalRuntimeClient;
    __AGENTDASH_DESKTOP_BROWSE_DIRECTORY__?: (path?: string) => Promise<BrowseDirectoryResult>;
    __AGENTDASH_DESKTOP_APP__?: DesktopAppBridge;
  }
}

interface DesktopAppBridge {
  loadSettings(): Promise<DesktopRuntimeSettings>;
  saveSettings(settings: DesktopRuntimeSettings): Promise<DesktopRuntimeSettings>;
  getAutostartStatus(): Promise<DesktopAutostartStatus>;
  setAutostartEnabled(enabled: boolean): Promise<DesktopAutostartStatus>;
  getDesktopApiSnapshot(): Promise<DesktopApiSnapshot | null>;
  quit(): Promise<void>;
}

let desktopRuntimeAutoConnectAttempted = false;

export function getDesktopLocalRuntimeClient(): LocalRuntimeClient | null {
  if (typeof window === 'undefined') return null;
  return window.__AGENTDASH_DESKTOP_LOCAL_RUNTIME__ ?? null;
}

export function getDesktopAppBridge(): DesktopAppBridge | null {
  if (typeof window === 'undefined') return null;
  return window.__AGENTDASH_DESKTOP_APP__ ?? null;
}

export function getDesktopBrowseDirectory(): ((path?: string) => Promise<BrowseDirectoryResult>) | undefined {
  if (typeof window === 'undefined') return undefined;
  return window.__AGENTDASH_DESKTOP_BROWSE_DIRECTORY__;
}

export async function ensureDesktopLocalRuntimeStarted(accessToken: string): Promise<void> {
  const client = getDesktopLocalRuntimeClient();
  const desktopApp = getDesktopAppBridge();
  const token = accessToken.trim();
  if (!client || !desktopApp) return;
  if (desktopRuntimeAutoConnectAttempted) return;
  desktopRuntimeAutoConnectAttempted = true;

  const settings = await desktopApp.loadSettings();
  if (!settings.auto_connect_local_runtime) return;

  const snapshot = await client.runtimeSnapshot().catch(() => null);
  if (snapshot?.state === 'starting' || snapshot?.state === 'running') return;

  const profile = await loadOrCreateAutoConnectProfile(client, token);

  await client.runtimeStart({
    ...profile,
    access_token: token,
    server_url: resolveDesktopServerUrl(profile.server_url),
  });
}

async function loadOrCreateAutoConnectProfile(
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
      machine_id: current.machine_id || '',
      machine_label: current.machine_label ?? null,
      auto_start: current.auto_start,
    };
    return client.profileSave(normalized);
  }

  const created: LocalRuntimeProfile = {
    server_url: resolveDesktopServerUrl(''),
    access_token: accessToken,
    profile_id: DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
    machine_id: '',
    machine_label: null,
    name: DEFAULT_LOCAL_RUNTIME_BACKEND_NAME,
    workspace_roots: [],
    executor_enabled: true,
    auto_start: false,
    backend_id: null,
    relay_ws_url: null,
  };
  return client.profileSave(created);
}

function resolveDesktopServerUrl(value: string): string {
  const explicit = value.trim().replace(/\/+$/, '');
  if (explicit) return explicit;
  return API_ORIGIN || DEFAULT_LOCAL_RUNTIME_SERVER_URL;
}
