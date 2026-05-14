declare global {
  interface Window {
    __AGENTDASH_DESKTOP_OPEN_EXTERNAL__?: (url: string) => Promise<void>;
  }
}

export function hasDesktopExternalBrowserOpener(): boolean {
  return typeof window !== "undefined" && window.__AGENTDASH_DESKTOP_OPEN_EXTERNAL__ !== undefined;
}

export async function openDesktopExternalBrowser(url: string): Promise<boolean> {
  const opener = typeof window !== "undefined" ? window.__AGENTDASH_DESKTOP_OPEN_EXTERNAL__ : undefined;
  if (!opener) return false;
  await opener(url);
  return true;
}

