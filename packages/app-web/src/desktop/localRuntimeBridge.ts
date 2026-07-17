import {
  DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
  DEFAULT_LOCAL_RUNTIME_SERVER_URL,
} from '@agentdash/core/local-runtime';
import type {
  DesktopApiSnapshot,
  DesktopAutostartStatus,
  DesktopRuntimeSettings,
  DesktopUpdateInstallResult,
  DesktopUpdatePolicySnapshot,
  LocalRuntimeClient,
  LocalRuntimeProfile,
  LocalRuntimeStatus,
} from '@agentdash/core/local-runtime';
import type { BrowseDirectoryResult } from '@agentdash/views/directory-browser';
import type {
  CodexOAuthStatusResponse,
  StartCodexOAuthResponse,
} from '../generated/llm-provider-contracts';
import { ensureDesktopDefaultsLoaded, resolveDefaultLocalRuntimeServerUrl } from './defaults';

declare global {
  interface Window {
    __AGENTDASH_DESKTOP_LOCAL_RUNTIME__?: LocalRuntimeClient;
    __AGENTDASH_DESKTOP_BROWSE_DIRECTORY__?: (path?: string) => Promise<BrowseDirectoryResult>;
    __AGENTDASH_DESKTOP_APP__?: DesktopAppBridge;
  }
}

export interface DesktopAppBridge {
  loadSettings(): Promise<DesktopRuntimeSettings>;
  saveSettings(settings: DesktopRuntimeSettings): Promise<DesktopRuntimeSettings>;
  getAutostartStatus(): Promise<DesktopAutostartStatus>;
  setAutostartEnabled(enabled: boolean): Promise<DesktopAutostartStatus>;
  getDesktopApiSnapshot(): Promise<DesktopApiSnapshot | null>;
  getUpdatePolicySnapshot(): Promise<DesktopUpdatePolicySnapshot>;
  refreshUpdatePolicy(): Promise<DesktopUpdatePolicySnapshot>;
  installUpdate(): Promise<DesktopUpdateInstallResult>;
  startCodexOAuth(request: DesktopCodexOAuthStartRequest): Promise<StartCodexOAuthResponse>;
  cancelCodexOAuth(flowId: string): Promise<CodexOAuthStatusResponse>;
  quit(): Promise<void>;
}

export interface DesktopCodexOAuthStartRequest {
  api_origin: string;
  access_token?: string;
  provider_id: string;
  target: 'global_provider' | 'user_byok';
}

const DESKTOP_RUNTIME_AUTO_CONNECT_MAX_ATTEMPTS = 8;
const DESKTOP_RUNTIME_AUTO_CONNECT_RETRY_MS = 2000;

let desktopRuntimeAutoConnectCompleted = false;
let desktopRuntimeAutoConnectInFlight: Promise<void> | null = null;
let desktopRuntimeAutoConnectRetryTimer: number | null = null;
let desktopRuntimeAutoConnectAttempts = 0;
let desktopRuntimeAutoConnectLastToken = '';
let desktopRuntimeAutoConnectLastCurrentUserAvailable = false;

interface DesktopRuntimeAuthState {
  currentUserAvailable: boolean;
}

type DesktopRuntimeAutoConnectOutcome = 'complete' | 'pending' | 'inactive';

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

export function ensureDesktopLocalRuntimeStarted(
  accessToken: string,
  authState?: DesktopRuntimeAuthState,
): Promise<void> {
  const token = accessToken.trim();
  const currentUserAvailable = authState?.currentUserAvailable ?? token.length > 0;
  if (!currentUserAvailable) {
    desktopRuntimeAutoConnectCompleted = false;
    desktopRuntimeAutoConnectAttempts = 0;
    desktopRuntimeAutoConnectLastToken = '';
    desktopRuntimeAutoConnectLastCurrentUserAvailable = false;
    clearDesktopRuntimeAutoConnectRetry();
    return Promise.resolve();
  }

  if (
    token !== desktopRuntimeAutoConnectLastToken
    || currentUserAvailable !== desktopRuntimeAutoConnectLastCurrentUserAvailable
  ) {
    desktopRuntimeAutoConnectCompleted = false;
    desktopRuntimeAutoConnectAttempts = 0;
    clearDesktopRuntimeAutoConnectRetry();
  }
  desktopRuntimeAutoConnectLastToken = token;
  desktopRuntimeAutoConnectLastCurrentUserAvailable = currentUserAvailable;
  if (desktopRuntimeAutoConnectCompleted) return Promise.resolve();
  if (desktopRuntimeAutoConnectInFlight) return desktopRuntimeAutoConnectInFlight;

  desktopRuntimeAutoConnectInFlight = runDesktopLocalRuntimeAutoConnect(token)
    .then((outcome) => {
      if (outcome === 'complete') {
        desktopRuntimeAutoConnectCompleted = true;
        clearDesktopRuntimeAutoConnectRetry();
      } else if (outcome === 'pending') {
        scheduleDesktopRuntimeAutoConnectRetry();
      } else {
        clearDesktopRuntimeAutoConnectRetry();
      }
    })
    .catch((error: unknown) => {
      scheduleDesktopRuntimeAutoConnectRetry();
      throw error;
    })
    .finally(() => {
      desktopRuntimeAutoConnectInFlight = null;
    });

  return desktopRuntimeAutoConnectInFlight;
}

async function runDesktopLocalRuntimeAutoConnect(
  accessToken: string,
): Promise<DesktopRuntimeAutoConnectOutcome> {
  const client = getDesktopLocalRuntimeClient();
  const desktopApp = getDesktopAppBridge();
  const token = accessToken.trim();
  if (!client || !desktopApp) return 'inactive';
  desktopRuntimeAutoConnectAttempts += 1;

  const updatePolicy = await desktopApp.getUpdatePolicySnapshot().catch(() => null);
  if (updatePolicy?.force_update_required) return 'inactive';

  const settings = await desktopApp.loadSettings();
  if (!settings.auto_connect_local_runtime) return 'inactive';

  const snapshot = await client.runtimeSnapshot().catch(() => null);
  const snapshotOutcome = classifyDesktopRuntimeSnapshot(snapshot);
  if (snapshotOutcome !== 'startable') return snapshotOutcome;

  await ensureDesktopDefaultsLoaded();
  const profile = await loadOrCreateAutoConnectProfile(client);
  if (!profile.auto_start) return 'inactive';

  const started = await client.runtimeStart({
    ...profile,
    access_token: token,
    server_url: resolveDesktopServerUrl(),
  });
  const startedOutcome = classifyDesktopRuntimeSnapshot(started);
  if (startedOutcome === 'startable') {
    throw new Error(started.message ?? 'Desktop local runtime auto-connect failed');
  }
  return startedOutcome;
}

function classifyDesktopRuntimeSnapshot(
  snapshot: LocalRuntimeStatus | null,
): DesktopRuntimeAutoConnectOutcome | 'startable' {
  if (!snapshot) return 'startable';
  if (
    snapshot.state === 'running'
    && snapshot.backend_id.length > 0
    && snapshot.relay_connection?.state === 'registered'
    && snapshot.relay_connection.registered_backend_id === snapshot.backend_id
  ) {
    return 'complete';
  }
  switch (snapshot.state) {
    case 'claiming':
    case 'waiting_for_api':
    case 'starting':
    case 'running':
    case 'retrying':
    case 'stopping':
      return 'pending';
    case 'disabled':
    case 'waiting_for_auth':
      return 'inactive';
    case 'idle':
    case 'stopped':
    case 'error':
      return 'startable';
  }
}

function scheduleDesktopRuntimeAutoConnectRetry(): void {
  if (typeof window === 'undefined') return;
  if (!getDesktopLocalRuntimeClient() || !getDesktopAppBridge()) return;
  if (desktopRuntimeAutoConnectRetryTimer !== null) return;
  if (desktopRuntimeAutoConnectAttempts >= DESKTOP_RUNTIME_AUTO_CONNECT_MAX_ATTEMPTS) return;

  desktopRuntimeAutoConnectRetryTimer = window.setTimeout(() => {
    desktopRuntimeAutoConnectRetryTimer = null;
    ensureDesktopLocalRuntimeStarted(
      desktopRuntimeAutoConnectLastToken,
      { currentUserAvailable: desktopRuntimeAutoConnectLastCurrentUserAvailable },
    ).catch(() => undefined);
  }, DESKTOP_RUNTIME_AUTO_CONNECT_RETRY_MS);
}

function clearDesktopRuntimeAutoConnectRetry(): void {
  if (typeof window === 'undefined' || desktopRuntimeAutoConnectRetryTimer === null) return;
  window.clearTimeout(desktopRuntimeAutoConnectRetryTimer);
  desktopRuntimeAutoConnectRetryTimer = null;
}

async function loadOrCreateAutoConnectProfile(client: LocalRuntimeClient): Promise<LocalRuntimeProfile> {
  const current = await client.profileLoad().catch(() => null);
  if (current) {
    const normalized = {
      ...current,
      access_token: '',
      server_url: resolveDesktopServerUrl(),
      profile_id: current.profile_id || DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
      machine_id: current.machine_id || '',
      machine_label: current.machine_label ?? null,
      auto_start: current.auto_start,
    };
    return client.profileSave(normalized);
  }

  const created: LocalRuntimeProfile = {
    server_url: resolveDesktopServerUrl(),
    access_token: '',
    profile_id: DEFAULT_LOCAL_RUNTIME_PROFILE_ID,
    machine_id: '',
    machine_label: null,
    workspace_roots: [],
    executor_enabled: true,
    auto_start: true,
    backend_id: null,
    relay_ws_url: null,
  };
  return client.profileSave(created);
}

function resolveDesktopServerUrl(): string {
  return resolveDefaultLocalRuntimeServerUrl() || DEFAULT_LOCAL_RUNTIME_SERVER_URL;
}
