import { API_ORIGIN } from "../api/origin";

const DESKTOP_DEFAULTS_PATH = "/agentdash-desktop-defaults.json";
const DEFAULT_DESKTOP_API_ORIGIN = "http://127.0.0.1:17301";

interface DesktopDefaults {
  default_cloud_origin?: string;
}

type Listener = () => void;

let desktopDefaults: DesktopDefaults = {};
let loadPromise: Promise<DesktopDefaults> | null = null;
const listeners = new Set<Listener>();

export function getDesktopDefaults(): DesktopDefaults {
  return desktopDefaults;
}

export function getDefaultCloudOrigin(): string {
  return desktopDefaults.default_cloud_origin ?? "";
}

export function resolveDefaultLocalRuntimeServerUrl(): string {
  return API_ORIGIN || getDefaultCloudOrigin() || DEFAULT_DESKTOP_API_ORIGIN;
}

export function subscribeDesktopDefaults(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export async function ensureDesktopDefaultsLoaded(): Promise<DesktopDefaults> {
  if (loadPromise) return loadPromise;
  loadPromise = loadDesktopDefaults();
  return loadPromise;
}

async function loadDesktopDefaults(): Promise<DesktopDefaults> {
  if (typeof fetch !== "function") {
    return desktopDefaults;
  }

  try {
    const response = await fetch(DESKTOP_DEFAULTS_PATH, { cache: "no-store" });
    if (!response.ok) {
      return desktopDefaults;
    }
    const parsed: unknown = await response.json();
    const next = normalizeDesktopDefaults(parsed);
    updateDesktopDefaults(next);
  } catch {
    return desktopDefaults;
  }

  return desktopDefaults;
}

function updateDesktopDefaults(next: DesktopDefaults): void {
  const changed = next.default_cloud_origin !== desktopDefaults.default_cloud_origin;
  desktopDefaults = next;
  if (!changed) return;
  for (const listener of listeners) {
    listener();
  }
}

function normalizeDesktopDefaults(value: unknown): DesktopDefaults {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return {};
  }

  const result: DesktopDefaults = {};
  const record = value as Record<string, unknown>;
  const defaultCloudOrigin = normalizeOrigin(record.default_cloud_origin);
  if (defaultCloudOrigin) {
    result.default_cloud_origin = defaultCloudOrigin;
  }
  return result;
}

function normalizeOrigin(value: unknown): string {
  if (typeof value !== "string") return "";
  const trimmed = value.trim().replace(/\/+$/, "");
  if (!trimmed) return "";
  try {
    const parsed = new URL(trimmed);
    return parsed.origin;
  } catch {
    return "";
  }
}
