import { invoke } from '@tauri-apps/api/core'
import type {
  DesktopApiSnapshot,
  DesktopAutostartStatus,
  DesktopRuntimeSettings,
} from '@agentdash/core/local-runtime'
import { ensureTauriHost, isTauriHost } from './tauriHost'

export type DesktopAppSettings = DesktopRuntimeSettings

export interface DesktopAppBridge {
  loadSettings(): Promise<DesktopAppSettings>
  saveSettings(settings: DesktopAppSettings): Promise<DesktopAppSettings>
  getAutostartStatus(): Promise<DesktopAutostartStatus>
  setAutostartEnabled(enabled: boolean): Promise<DesktopAutostartStatus>
  getDesktopApiSnapshot(): Promise<DesktopApiSnapshot | null>
  quit(): Promise<void>
}

export function createTauriDesktopAppBridge(): DesktopAppBridge {
  return {
    loadSettings: desktopSettingsLoad,
    saveSettings: desktopSettingsSave,
    getAutostartStatus: desktopAutostartIsEnabled,
    setAutostartEnabled: desktopAutostartSetEnabled,
    getDesktopApiSnapshot: desktopApiSnapshot,
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
