const rawOrigin = (import.meta.env.VITE_API_ORIGIN ?? "").trim();

// 统一去掉末尾 `/`，避免拼接出现 `//api/...`
export const API_ORIGIN = rawOrigin.replace(/\/+$/, "");

export function resolveApiUrl(path: string): string {
  if (/^https?:\/\//i.test(path)) {
    return path;
  }
  const normalized = path.startsWith("/") ? path : `/${path}`;
  if (!API_ORIGIN) {
    return normalized;
  }
  return `${API_ORIGIN}${normalized}`;
}

export function buildApiPath(path: string): string {
  const normalized = path.startsWith("/") ? path : `/${path}`;
  return resolveApiUrl(`/api${normalized}`);
}
