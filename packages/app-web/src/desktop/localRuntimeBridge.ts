import type { LocalRuntimeClient } from '@agentdash/core/local-runtime';

declare global {
  interface Window {
    __AGENTDASH_DESKTOP_LOCAL_RUNTIME__?: LocalRuntimeClient;
  }
}

export function getDesktopLocalRuntimeClient(): LocalRuntimeClient | null {
  if (typeof window === 'undefined') return null;
  return window.__AGENTDASH_DESKTOP_LOCAL_RUNTIME__ ?? null;
}
