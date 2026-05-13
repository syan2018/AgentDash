import { useCallback, useSyncExternalStore } from "react";

const STORAGE_KEY = "agentdash:debug-prefs";

export interface DebugPrefs {
  hookVerbose: boolean;
}

const DEFAULT_PREFS: DebugPrefs = { hookVerbose: false };

let cachedPrefs: DebugPrefs | null = null;
const listeners = new Set<() => void>();

function notify() {
  for (const fn of listeners) fn();
}

function readPrefs(): DebugPrefs {
  if (cachedPrefs) return cachedPrefs;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (raw) {
      cachedPrefs = { ...DEFAULT_PREFS, ...JSON.parse(raw) };
    } else {
      cachedPrefs = DEFAULT_PREFS;
    }
  } catch {
    cachedPrefs = DEFAULT_PREFS;
  }
  return cachedPrefs!;
}

function writePrefs(prefs: DebugPrefs) {
  cachedPrefs = prefs;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(prefs));
  } catch {
    // silent
  }
  notify();
}

function subscribe(callback: () => void) {
  listeners.add(callback);
  return () => { listeners.delete(callback); };
}

function getSnapshot(): DebugPrefs {
  return readPrefs();
}

export function useDebugPrefs() {
  const prefs = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  const setHookVerbose = useCallback((value: boolean) => {
    writePrefs({ ...readPrefs(), hookVerbose: value });
  }, []);

  return { prefs, setHookVerbose };
}

export function getDebugPrefs(): DebugPrefs {
  return readPrefs();
}
