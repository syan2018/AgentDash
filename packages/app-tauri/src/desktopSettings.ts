import { invoke } from '@tauri-apps/api/core'
import type {
  DesktopApiSnapshot,
  DesktopAutostartStatus,
  DesktopUpdateInstallResult,
  DesktopUpdatePolicySnapshot,
  DesktopRuntimeSettings,
} from '@agentdash/core/local-runtime'
import type {
  CodexOAuthStatusResponse,
  StartCodexOAuthResponse,
} from '../../app-web/src/generated/llm-provider-contracts'
import { ensureTauriHost, isTauriHost } from './tauriHost'

export type DesktopAppSettings = DesktopRuntimeSettings

export interface DesktopAppBridge {
  loadSettings(): Promise<DesktopAppSettings>
  saveSettings(settings: DesktopAppSettings): Promise<DesktopAppSettings>
  getAutostartStatus(): Promise<DesktopAutostartStatus>
  setAutostartEnabled(enabled: boolean): Promise<DesktopAutostartStatus>
  getDesktopApiSnapshot(): Promise<DesktopApiSnapshot | null>
  getUpdatePolicySnapshot(): Promise<DesktopUpdatePolicySnapshot>
  refreshUpdatePolicy(): Promise<DesktopUpdatePolicySnapshot>
  installUpdate(): Promise<DesktopUpdateInstallResult>
  startCodexOAuth(request: DesktopCodexOAuthStartRequest): Promise<StartCodexOAuthResponse>
  cancelCodexOAuth(flowId: string): Promise<CodexOAuthStatusResponse>
  quit(): Promise<void>
}

export interface DesktopCodexOAuthStartRequest {
  api_origin: string
  access_token: string
  provider_id: string
  target: 'global_provider' | 'user_byok'
}

export function createTauriDesktopAppBridge(): DesktopAppBridge {
  return {
    loadSettings: desktopSettingsLoad,
    saveSettings: desktopSettingsSave,
    getAutostartStatus: desktopAutostartIsEnabled,
    setAutostartEnabled: desktopAutostartSetEnabled,
    getDesktopApiSnapshot: desktopApiSnapshot,
    getUpdatePolicySnapshot: desktopUpdatePolicySnapshot,
    refreshUpdatePolicy: desktopUpdatePolicyRefresh,
    installUpdate: desktopUpdateInstall,
    startCodexOAuth: codexOAuthStart,
    cancelCodexOAuth: codexOAuthCancel,
    quit: desktopQuitRequest,
  }
}

export async function desktopSettingsLoad(): Promise<DesktopAppSettings> {
  ensureTauriHost()
  return invoke('desktop_settings_load')
}

export async function desktopSettingsSave(settings: DesktopAppSettings): Promise<DesktopAppSettings> {
  ensureTauriHost()
  return invoke('desktop_settings_save', { settings })
}

export async function desktopAutostartIsEnabled(): Promise<DesktopAutostartStatus> {
  ensureTauriHost()
  return invoke('desktop_autostart_is_enabled')
}

export async function desktopAutostartSetEnabled(enabled: boolean): Promise<DesktopAutostartStatus> {
  ensureTauriHost()
  return invoke('desktop_autostart_set_enabled', { enabled })
}

export async function desktopQuitRequest(): Promise<void> {
  ensureTauriHost()
  return invoke('desktop_quit_request')
}

export async function desktopApiSnapshot(): Promise<DesktopApiSnapshot | null> {
  if (!isTauriHost()) return null
  return invoke('desktop_api_snapshot')
}

export async function desktopUpdatePolicySnapshot(): Promise<DesktopUpdatePolicySnapshot> {
  ensureTauriHost()
  return invoke('desktop_update_policy_snapshot')
}

export async function desktopUpdatePolicyRefresh(): Promise<DesktopUpdatePolicySnapshot> {
  ensureTauriHost()
  return invoke('desktop_update_policy_refresh')
}

export async function desktopUpdateInstall(): Promise<DesktopUpdateInstallResult> {
  ensureTauriHost()
  return invoke('desktop_update_install')
}

export async function codexOAuthStart(
  request: DesktopCodexOAuthStartRequest,
): Promise<StartCodexOAuthResponse> {
  ensureTauriHost()
  return invoke('codex_oauth_start', { request })
}

export async function codexOAuthCancel(flowId: string): Promise<CodexOAuthStatusResponse> {
  ensureTauriHost()
  return invoke('codex_oauth_cancel', { flowId })
}

export async function desktopSettingsLoadOrDefault(): Promise<DesktopAppSettings> {
  if (!isTauriHost()) {
    return {
      launch_at_login: false,
      start_minimized_to_tray: false,
      auto_connect_local_runtime: true,
    }
  }
  return desktopSettingsLoad()
}
