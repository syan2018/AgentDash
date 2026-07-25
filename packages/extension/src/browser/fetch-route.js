// @ts-check

/**
 * @typedef {"panel_only" | "agent_and_panel"} FetchRouteScope
 * @typedef {{ kind: "http_proxy", base_url: string }} HttpProxyFetchRouteTarget
 * @typedef {{ kind: "custom_protocol", protocol_key: string, method: string }} CustomProtocolFetchRouteTarget
 * @typedef {{ kind: "backend_service", service_key: string }} BackendServiceFetchRouteTarget
 * @typedef {HttpProxyFetchRouteTarget | CustomProtocolFetchRouteTarget | BackendServiceFetchRouteTarget} FetchRouteTarget
 * @typedef {{ route: string, target: FetchRouteTarget, scope: FetchRouteScope }} FetchRouteBinding
 * @typedef {{ route: FetchRouteBinding, url: string, path: string }} FetchRouteMatch
 * @typedef {{ route: FetchRouteBinding, url: string, method: string, headers: Record<string, string>, body?: string }} RoutedFetchRequest
 * @typedef {{ status: number, headers?: Record<string, string>, body?: string }} RoutedFetchResponse
 * @typedef {{ invokeFetchRoute(request: RoutedFetchRequest): Promise<RoutedFetchResponse> }} FetchRouteBridge
 */

/** @type {Record<string, "http_proxy" | "custom_protocol" | "backend_service">} */
const ROUTE_TARGET_PREFIXES = {
  httpProxy: "http_proxy",
  http_proxy: "http_proxy",
  customProtocol: "custom_protocol",
  custom_protocol: "custom_protocol",
  backendService: "backend_service",
  backend_service: "backend_service",
};

/**
 * Parses a CLI binding such as `/api/**=httpProxy:https://api.example.com`.
 *
 * The target kind is intentionally required. A bare `/api/**=api` is ambiguous
 * because localhost and backend service ownership are explicit authoring facts.
 *
 * @param {string} value
 * @returns {FetchRouteBinding}
 */
export function parseFetchRouteBinding(value) {
  const separator = value.indexOf("=");
  if (separator <= 0 || separator === value.length - 1) {
    throw new Error("fetch route 必须使用 <route>=<target> 格式");
  }
  const route = value.slice(0, separator).trim();
  const targetSpec = value.slice(separator + 1).trim();
  const targetSeparator = targetSpec.indexOf(":");
  if (targetSeparator <= 0 || targetSeparator === targetSpec.length - 1) {
    throw new Error(
      "fetch route target 必须显式声明为 httpProxy:<baseUrl>、customProtocol:<channel>#<method> 或 backendService:<serviceKey>",
    );
  }
  const rawKind = targetSpec.slice(0, targetSeparator);
  const normalizedKind = ROUTE_TARGET_PREFIXES[rawKind];
  if (!normalizedKind) {
    throw new Error(
      `fetch route target kind 非法: ${rawKind}; 支持 httpProxy、customProtocol、backendService`,
    );
  }
  const rawTarget = targetSpec.slice(targetSeparator + 1).trim();
  return normalizeFetchRouteBinding({
    route,
    target: parseFetchRouteTarget(normalizedKind, rawTarget),
    scope: "panel_only",
  });
}

/**
 * @param {unknown} value
 * @returns {FetchRouteBinding}
 */
export function normalizeFetchRouteBinding(value) {
  const record = asRecord(value);
  if (!record) throw new Error("fetch route binding 必须是对象");
  const route = stringField(record, "route");
  if (!route) throw new Error("fetch route.route 不能为空");
  validateRoutePattern(route);
  const target = normalizeFetchRouteTarget(record.target);
  const rawScope = record.scope;
  const scope = rawScope === undefined ? "panel_only" : rawScope;
  if (scope !== "panel_only" && scope !== "agent_and_panel") {
    throw new Error("fetch route.scope 必须是 panel_only 或 agent_and_panel");
  }
  return { route, target, scope };
}

/**
 * @param {string | URL} url
 * @param {FetchRouteBinding[]} routes
 * @param {{ baseUrl?: string }} [options]
 * @returns {FetchRouteMatch | null}
 */
export function matchFetchRoute(url, routes, options = {}) {
  const parsed = parseUrl(url, options.baseUrl);
  for (const route of routes) {
    if (matchesRoutePattern(parsed, route.route)) {
      return {
        route,
        url: parsed.href,
        path: parsed.pathname + parsed.search,
      };
    }
  }
  return null;
}

/**
 * @param {string | URL} url
 * @param {FetchRouteBinding[]} routes
 * @param {{ baseUrl?: string }} [options]
 * @returns {boolean}
 */
export function hasFetchRoute(url, routes, options = {}) {
  return matchFetchRoute(url, routes, options) != null;
}

/**
 * Creates an explicit routed fetch function. It does not install itself on
 * `window.fetch`; callers choose where this compatibility layer is used.
 *
 * @param {FetchRouteBinding[]} routes
 * @param {FetchRouteBridge} bridge
 * @param {{ baseUrl?: string, fallbackFetch?: typeof fetch }} [options]
 * @returns {typeof fetch}
 */
export function createRoutedFetch(routes, bridge, options = {}) {
  const normalizedRoutes = routes.map((route) => normalizeFetchRouteBinding(route));
  return async (input, init) => {
    const requestUrl = fetchInputUrl(input);
    const match = matchFetchRoute(requestUrl, normalizedRoutes, { baseUrl: options.baseUrl });
    if (!match) {
      if (options.fallbackFetch) return options.fallbackFetch(input, init);
      throw new Error(`fetch route 未声明: ${requestUrl}`);
    }
    const method = init?.method ?? fetchInputMethod(input) ?? "GET";
    const body = await fetchInputBody(input, init);
    const output = await bridge.invokeFetchRoute({
      route: match.route,
      url: match.url,
      method,
      headers: headersToRecord(init?.headers ?? fetchInputHeaders(input)),
      body,
    });
    return new Response(responseBodyForStatus(output.status, output.body), {
      status: output.status,
      headers: output.headers,
    });
  };
}

/**
 * @param {string | URL} url
 * @param {{ baseUrl?: string }} [options]
 * @returns {boolean}
 */
export function isLocalhostUrl(url, options = {}) {
  const parsed = parseUrl(url, options.baseUrl);
  return parsed.hostname === "localhost"
    || parsed.hostname === "127.0.0.1"
    || parsed.hostname === "::1";
}

/**
 * @param {FetchRouteTarget} target
 * @returns {string}
 */
export function describeFetchRouteTarget(target) {
  if (target.kind === "http_proxy") return `httpProxy:${target.base_url}`;
  if (target.kind === "custom_protocol") return `customProtocol:${target.protocol_key}#${target.method}`;
  return `backendService:${target.service_key}`;
}

/**
 * @param {"http_proxy" | "custom_protocol" | "backend_service"} kind
 * @param {string} value
 * @returns {FetchRouteTarget}
 */
function parseFetchRouteTarget(kind, value) {
  if (kind === "http_proxy") {
    return { kind, base_url: normalizeHttpBaseUrl(value) };
  }
  if (kind === "custom_protocol") {
    const methodSeparator = value.indexOf("#");
    if (methodSeparator <= 0 || methodSeparator === value.length - 1) {
      throw new Error("customProtocol fetch route target 必须使用 <protocol_key>#<method>");
    }
    const protocolKey = value.slice(0, methodSeparator).trim();
    const method = value.slice(methodSeparator + 1).trim();
    validateNamespacedKey(protocolKey, "customProtocol protocol_key");
    validateMethodName(method, "customProtocol method");
    return { kind, protocol_key: protocolKey, method };
  }
  validateServiceKey(value, "backendService service_key");
  return { kind, service_key: value };
}

/**
 * @param {unknown} value
 * @returns {FetchRouteTarget}
 */
function normalizeFetchRouteTarget(value) {
  const record = asRecord(value);
  if (!record) throw new Error("fetch route.target 必须是对象");
  if (record.kind === "http_proxy") {
    const baseUrl = stringField(record, "base_url");
    if (!baseUrl) throw new Error("http_proxy target.base_url 不能为空");
    return { kind: "http_proxy", base_url: normalizeHttpBaseUrl(baseUrl) };
  }
  if (record.kind === "custom_protocol") {
    const protocolKey = stringField(record, "protocol_key");
    const method = stringField(record, "method");
    if (!protocolKey || !method) {
      throw new Error("custom_protocol target.protocol_key 和 target.method 不能为空");
    }
    validateNamespacedKey(protocolKey, "custom_protocol target.protocol_key");
    validateMethodName(method, "custom_protocol target.method");
    return { kind: "custom_protocol", protocol_key: protocolKey, method };
  }
  if (record.kind === "backend_service") {
    const serviceKey = stringField(record, "service_key");
    if (!serviceKey) throw new Error("backend_service target.service_key 不能为空");
    validateServiceKey(serviceKey, "backend_service target.service_key");
    return { kind: "backend_service", service_key: serviceKey };
  }
  throw new Error("fetch route.target.kind 必须是 http_proxy、custom_protocol 或 backend_service");
}

/**
 * @param {URL} parsed
 * @param {string} pattern
 * @returns {boolean}
 */
function matchesRoutePattern(parsed, pattern) {
  if (/^https?:\/\//i.test(pattern)) {
    const target = parsed.origin + parsed.pathname;
    return matchesPathPattern(target, stripTrailingSearch(pattern));
  }
  return matchesPathPattern(parsed.pathname, stripTrailingSearch(pattern));
}

/**
 * @param {string} candidate
 * @param {string} pattern
 * @returns {boolean}
 */
function matchesPathPattern(candidate, pattern) {
  if (pattern.endsWith("/**")) {
    const prefix = pattern.slice(0, -3);
    return candidate === prefix || candidate.startsWith(`${prefix}/`);
  }
  if (pattern.endsWith("*")) {
    return candidate.startsWith(pattern.slice(0, -1));
  }
  return candidate === pattern;
}

/**
 * @param {string} route
 * @returns {void}
 */
function validateRoutePattern(route) {
  if (route.includes("?")) {
    throw new Error("fetch route pattern 不应包含 query string");
  }
  if (/^https?:\/\//i.test(route)) {
    const parsed = new URL(route);
    if (!parsed.pathname.startsWith("/")) throw new Error("fetch route absolute pattern 缺少 path");
    return;
  }
  if (!route.startsWith("/")) {
    throw new Error("fetch route pattern 必须以 / 或 http(s):// 开头");
  }
}

/**
 * @param {string} value
 * @returns {string}
 */
function stripTrailingSearch(value) {
  const index = value.indexOf("?");
  return index < 0 ? value : value.slice(0, index);
}

/**
 * @param {string} value
 * @returns {string}
 */
function normalizeHttpBaseUrl(value) {
  const parsed = new URL(value);
  if (parsed.protocol !== "http:" && parsed.protocol !== "https:") {
    throw new Error("httpProxy baseUrl 必须是 http 或 https URL");
  }
  parsed.hash = "";
  parsed.search = "";
  return parsed.href.replace(/\/$/, "");
}

/**
 * @param {string | URL} url
 * @param {string} [baseUrl]
 * @returns {URL}
 */
function parseUrl(url, baseUrl) {
  const base = baseUrl ?? globalThis.location?.href ?? "http://agentdash.local/";
  return new URL(String(url), base);
}

/**
 * @param {RequestInfo | URL} input
 * @returns {string}
 */
function fetchInputUrl(input) {
  if (typeof input === "string" || input instanceof URL) return String(input);
  return input.url;
}

/**
 * @param {RequestInfo | URL} input
 * @returns {string | undefined}
 */
function fetchInputMethod(input) {
  if (typeof input === "string" || input instanceof URL) return undefined;
  return input.method;
}

/**
 * @param {RequestInfo | URL} input
 * @returns {HeadersInit | undefined}
 */
function fetchInputHeaders(input) {
  if (typeof input === "string" || input instanceof URL) return undefined;
  return input.headers;
}

/**
 * @param {RequestInfo | URL} input
 * @param {RequestInit | undefined} init
 * @returns {Promise<string | undefined>}
 */
async function fetchInputBody(input, init) {
  if (typeof init?.body === "string") return init.body;
  if (init?.body instanceof URLSearchParams) return init.body.toString();
  if (typeof Blob !== "undefined" && init?.body instanceof Blob) return init.body.text();
  if (init?.body != null) return undefined;
  if (
    typeof Request !== "undefined"
    && input instanceof Request
    && input.method !== "GET"
    && input.method !== "HEAD"
  ) {
    return input.clone().text();
  }
  return undefined;
}

/**
 * @param {HeadersInit | undefined} headers
 * @returns {Record<string, string>}
 */
function headersToRecord(headers) {
  /** @type {Record<string, string>} */
  const result = {};
  if (!headers) return result;
  if (headers instanceof Headers) {
    headers.forEach((value, key) => {
      result[key] = value;
    });
    return result;
  }
  if (Array.isArray(headers)) {
    for (const [key, value] of headers) result[key] = value;
    return result;
  }
  for (const [key, value] of Object.entries(headers)) {
    result[key] = value;
  }
  return result;
}

/**
 * @param {number} status
 * @param {string | undefined} body
 * @returns {string | null}
 */
function responseBodyForStatus(status, body) {
  if (status === 204 || status === 205 || status === 304) return null;
  return body ?? "";
}

/**
 * @param {string} value
 * @param {string} label
 * @returns {void}
 */
function validateNamespacedKey(value, label) {
  if (!value.includes(".") || !value.split(".").every((segment) => /^[a-z0-9_-]+$/.test(segment))) {
    throw new Error(`${label} 必须包含 provider namespace，并只使用小写字母、数字、下划线、短横线和点分段`);
  }
}

/**
 * @param {string} value
 * @param {string} label
 * @returns {void}
 */
function validateMethodName(value, label) {
  if (!/^[A-Za-z][A-Za-z0-9_]*$/.test(value)) {
    throw new Error(`${label} 必须是合法 method 名称`);
  }
}

/**
 * @param {string} value
 * @param {string} label
 * @returns {void}
 */
function validateServiceKey(value, label) {
  if (!value.split(".").every((segment) => /^[a-z0-9_-]+$/.test(segment))) {
    throw new Error(`${label} 必须由小写字母、数字、下划线、短横线和点分段组成`);
  }
}

/**
 * @param {unknown} value
 * @returns {Record<string, unknown> | null}
 */
function asRecord(value) {
  return value != null && typeof value === "object" && !Array.isArray(value)
    ? /** @type {Record<string, unknown>} */ (value)
    : null;
}

/**
 * @param {Record<string, unknown>} record
 * @param {string} field
 * @returns {string | null}
 */
function stringField(record, field) {
  const value = record[field];
  return typeof value === "string" && value.trim() !== "" ? value.trim() : null;
}
