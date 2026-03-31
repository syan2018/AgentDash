import { buildApiPath } from './origin';

const TOKEN_KEY = 'agentdash_access_token';
const TOKEN_COOKIE = 'agentdash_access_token';
const TOKEN_COOKIE_MAX_AGE_SECONDS = 60 * 60 * 24 * 30; // 30 天

export function getStoredToken(): string | null {
  const local = localStorage.getItem(TOKEN_KEY);
  if (local) {
    return local;
  }
  return readCookieToken();
}

export function setStoredToken(token: string): void {
  localStorage.setItem(TOKEN_KEY, token);
  writeCookieToken(token);
}

export function clearStoredToken(): void {
  localStorage.removeItem(TOKEN_KEY);
  clearCookieToken();
}

async function request<T>(
  path: string,
  options: RequestInit = {}
): Promise<T> {
  const url = buildApiPath(path);
  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    ...options.headers as Record<string, string>,
  };

  const token = getStoredToken();
  if (token && !headers['Authorization']) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  const res = await fetch(url, { ...options, headers });

  if (!res.ok) {
    const body = await res.json().catch(() => ({ error: res.statusText }));
    const error = new Error(body.error || `HTTP ${res.status}`);
    (error as ApiHttpError).status = res.status;
    throw error;
  }

  if (res.status === 204) {
    return undefined as unknown as T;
  }
  return res.json();
}

export interface ApiHttpError extends Error {
  status?: number;
}

export const api = {
  get: <T>(path: string) => request<T>(path),
  post: <T>(path: string, data: unknown) =>
    request<T>(path, { method: 'POST', body: JSON.stringify(data) }),
  put: <T>(path: string, data: unknown) =>
    request<T>(path, { method: 'PUT', body: JSON.stringify(data) }),
  patch: <T>(path: string, data: unknown) =>
    request<T>(path, { method: 'PATCH', body: JSON.stringify(data) }),
  delete: <T>(path: string) => request<T>(path, { method: 'DELETE' }),
};

/**
 * 带认证的 fetch 包装 — 自动注入 Bearer token。
 *
 * 用于不经 `api.*` 而直接调用 `fetch()` 的场景（如 services 层、EventSource 等）。
 * 与原生 fetch 签名兼容，仅在 headers 中缺少 Authorization 时自动补充。
 */
export function authenticatedFetch(input: RequestInfo | URL, init?: RequestInit): Promise<Response> {
  const headers: Record<string, string> = {
    ...(init?.headers as Record<string, string>),
  };
  const token = getStoredToken();
  if (token && !headers['Authorization']) {
    headers['Authorization'] = `Bearer ${token}`;
  }
  return fetch(input, { ...init, headers });
}

function readCookieToken(): string | null {
  if (typeof document === 'undefined') {
    return null;
  }
  const encodedName = `${TOKEN_COOKIE}=`;
  const chunks = document.cookie.split(';');
  for (const chunk of chunks) {
    const item = chunk.trim();
    if (item.startsWith(encodedName)) {
      const raw = item.slice(encodedName.length);
      try {
        return decodeURIComponent(raw);
      } catch {
        return raw || null;
      }
    }
  }
  return null;
}

function writeCookieToken(token: string): void {
  if (typeof document === 'undefined') {
    return;
  }
  document.cookie = `${TOKEN_COOKIE}=${encodeURIComponent(token)}; Path=/; Max-Age=${TOKEN_COOKIE_MAX_AGE_SECONDS}; SameSite=Lax`;
}

function clearCookieToken(): void {
  if (typeof document === 'undefined') {
    return;
  }
  document.cookie = `${TOKEN_COOKIE}=; Path=/; Max-Age=0; SameSite=Lax`;
}
