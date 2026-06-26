declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown
  }
}

export function isTauriHost(): boolean {
  return typeof window !== 'undefined' && window.__TAURI_INTERNALS__ !== undefined
}

export function ensureTauriHost(): void {
  if (!isTauriHost()) {
    throw new Error('当前页面未运行在 Tauri 宿主中')
  }
}
