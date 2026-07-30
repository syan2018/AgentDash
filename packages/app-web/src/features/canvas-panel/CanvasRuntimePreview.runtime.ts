import ts from "typescript";
import type { CanvasRuntimeFile, CanvasRuntimeSnapshot } from "../../types";

export interface BuiltPreviewDocument {
  srcDoc: string;
  dispose: () => void;
}

export interface ParsedVfsAssetUri {
  mountId: string;
  path: string;
}

export interface RuntimeAssetUrlCache {
  urls: Set<string>;
  uriCache: Map<string, string>;
  pending: Map<string, Promise<string>>;
}

export interface ReadRuntimeAssetBlobInput {
  surfaceRef: string;
  mountId: string;
  path: string;
}

const DEFAULT_IMPORTS: Record<string, string> = {
  react: "https://esm.sh/react@18?dev",
  "react/jsx-runtime": "https://esm.sh/react@18/jsx-runtime?dev",
  "react/jsx-dev-runtime": "https://esm.sh/react@18/jsx-dev-runtime?dev",
  "react-dom": "https://esm.sh/react-dom@18?dev",
  "react-dom/client": "https://esm.sh/react-dom@18/client?dev",
};

const MODULE_EXTENSIONS = [".ts", ".tsx", ".js", ".jsx", ".json", ".css"];

export function createRuntimeAssetUrlCache(): RuntimeAssetUrlCache {
  return {
    urls: new Set<string>(),
    uriCache: new Map<string, string>(),
    pending: new Map<string, Promise<string>>(),
  };
}

export function buildCanvasRuntimeSnapshotFingerprint(snapshot: CanvasRuntimeSnapshot): string {
  return stableJsonStringify({
    canvas_id: snapshot.canvas_id,
    canvas_mount_id: snapshot.canvas_mount_id,
    vfs_mount_id: snapshot.vfs_mount_id,
    resource_surface_ref: snapshot.resource_surface_ref ?? null,
    entry: snapshot.entry,
    files: [...snapshot.files]
      .sort((a, b) => a.path.localeCompare(b.path))
      .map((file) => ({
        path: file.path,
        content: file.content,
        file_type: file.file_type,
      })),
    bindings: [...snapshot.bindings]
      .sort((a, b) => {
        const alias = a.alias.localeCompare(b.alias);
        if (alias !== 0) return alias;
        const source = a.source_uri.localeCompare(b.source_uri);
        if (source !== 0) return source;
        return a.data_path.localeCompare(b.data_path);
      })
      .map((binding) => ({
        alias: binding.alias,
        source_uri: binding.source_uri,
        data_path: binding.data_path,
        content_type: binding.content_type,
        resolved: binding.resolved,
      })),
    import_map: {
      imports: sortRecord(snapshot.import_map.imports),
    },
    libraries: [...snapshot.libraries].sort(),
    operations: snapshot.operations,
    features: snapshot.features,
  });
}

export function areCanvasRuntimeSnapshotsEquivalent(
  prev: CanvasRuntimeSnapshot | null,
  next: CanvasRuntimeSnapshot | null,
): boolean {
  if (prev === next) return true;
  if (!prev || !next) return false;
  return buildCanvasRuntimeSnapshotFingerprint(prev) === buildCanvasRuntimeSnapshotFingerprint(next);
}

export async function resolveRuntimeAssetUrl(options: {
  surfaceRef: string;
  uri: string;
  cache: RuntimeAssetUrlCache;
  readBlob: (input: ReadRuntimeAssetBlobInput) => Promise<Blob>;
  createObjectUrl?: (blob: Blob) => string;
}): Promise<string> {
  const parsed = parseVfsAssetUri(options.uri);
  if (typeof parsed === "string") {
    throw new Error(parsed);
  }

  const cacheKey = `${options.surfaceRef}\n${parsed.mountId}\n${parsed.path}`;
  const cachedUrl = options.cache.uriCache.get(cacheKey);
  if (cachedUrl) {
    return cachedUrl;
  }

  let pending = options.cache.pending.get(cacheKey);
  if (!pending) {
    pending = options.readBlob({
      surfaceRef: options.surfaceRef,
      mountId: parsed.mountId,
      path: parsed.path,
    }).then((blob) => {
      if (!isImageBlob(blob)) {
        throw new Error(`资源不是图片 MIME: ${blob.type || "unknown"}`);
      }

      const createObjectUrl = options.createObjectUrl ?? URL.createObjectURL;
      const url = createObjectUrl(blob);
      options.cache.urls.add(url);
      options.cache.uriCache.set(cacheKey, url);
      return url;
    }).finally(() => {
      options.cache.pending.delete(cacheKey);
    });
    options.cache.pending.set(cacheKey, pending);
  }

  return pending;
}

export function revokeRuntimeAssetUrl(
  cache: RuntimeAssetUrlCache,
  url: string,
  revokeObjectUrl: (url: string) => void = URL.revokeObjectURL,
): void {
  if (!cache.urls.delete(url)) {
    return;
  }

  revokeObjectUrl(url);
  for (const [key, cachedUrl] of cache.uriCache) {
    if (cachedUrl === url) {
      cache.uriCache.delete(key);
    }
  }
}

export function revokeAllRuntimeAssetUrls(
  cache: RuntimeAssetUrlCache,
  revokeObjectUrl: (url: string) => void = URL.revokeObjectURL,
): void {
  for (const url of cache.urls) {
    revokeObjectUrl(url);
  }
  cache.urls.clear();
  cache.uriCache.clear();
  cache.pending.clear();
}

export function buildPreviewDocument(
  snapshot: CanvasRuntimeSnapshot,
  frameId: string,
  generation = 1,
): BuiltPreviewDocument {
  const fileMap = new Map(snapshot.files.map((file) => [normalizePath(file.path), file]));
  const moduleSources: Record<string, string> = {};
  const externalImports = {
    ...DEFAULT_IMPORTS,
    ...snapshot.import_map.imports,
  };

  const cssContent = snapshot.files
    .filter((file) => isCssFile(file.path))
    .map((file) => file.content)
    .join("\n\n");

  const getModuleRef = (requestPath: string): string => {
    const normalizedPath = resolveExistingModulePath(fileMap, requestPath);
    return canvasModuleRef(normalizedPath);
  };

  for (const [normalizedPath, file] of fileMap) {
    moduleSources[canvasModuleRef(normalizedPath)] = buildModuleCode(
      file,
      normalizedPath,
      fileMap,
      getModuleRef,
    );
  }

  const entryRef = getModuleRef(snapshot.entry);
  const safeCss = sanitizeCssForStyleTag(cssContent);
  const bootScript = buildCanvasHostBootScript({
    frameId,
    generation,
    entryRef,
    externalImports,
    moduleSources,
  });

  return {
    srcDoc: `<!DOCTYPE html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline' blob: https://esm.sh; style-src 'unsafe-inline'; img-src blob: data:; connect-src https://esm.sh; font-src data:;" />
    <title>Canvas Preview</title>
    <style>
      :root {
        color-scheme: light;
      }

      html, body {
        margin: 0;
        min-height: 100%;
        background: #ffffff;
        color: #0f172a;
        font-family: "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif;
      }

      body {
        min-height: 100vh;
      }

      #root {
        min-height: 100vh;
      }

${safeCss}
    </style>
  </head>
  <body>
    <div id="root"></div>
    <script>
${bootScript}
    </script>
  </body>
</html>`,
    dispose: () => {},
  };
}

function buildCanvasHostBootScript(input: {
  frameId: string;
  generation: number;
  entryRef: string;
  externalImports: Record<string, string>;
  moduleSources: Record<string, string>;
}): string {
  return `
    const CONTRACT = "agentdash.canvas-host.v1";
    const frameId = ${JSON.stringify(input.frameId)};
    const generation = ${JSON.stringify(input.generation)};
    const entryRef = ${JSON.stringify(input.entryRef)};
    const externalImports = ${serializeForInlineScript(input.externalImports)};
    const moduleSources = ${serializeForInlineScript(input.moduleSources)};
    const moduleUrls = new Map(
      Object.entries(moduleSources).map(([moduleRef, source]) => [
        moduleRef,
        URL.createObjectURL(new Blob([source], { type: "text/javascript" })),
      ]),
    );
    const importMapElement = document.createElement("script");
    importMapElement.type = "importmap";
    importMapElement.textContent = JSON.stringify({
      imports: {
        ...externalImports,
        ...Object.fromEntries(moduleUrls),
      },
    });
    document.head.appendChild(importMapElement);
    const entryUrl = moduleUrls.get(entryRef);
    if (!entryUrl) {
      throw new Error("Canvas 入口模块不存在: " + entryRef);
    }
    const pending = new Map();
    const MAX_OUTSTANDING_REQUESTS = 32;
    const MAX_REQUEST_BYTES = 262144;
    let requestSeq = 0;
    let hostPort = null;

    const rejectPending = (reason) => {
      for (const item of pending.values()) {
        clearTimeout(item.timeout);
        item.reject(new Error(reason));
      }
      pending.clear();
    };

    const request = (method, payload = {}) => {
      if (!hostPort) {
        return Promise.reject(new Error("Canvas host 尚未连接"));
      }
      if (pending.size >= MAX_OUTSTANDING_REQUESTS) {
        return Promise.reject(new Error("Canvas host 并发请求过多"));
      }
      let requestBytes = 0;
      try {
        requestBytes = new TextEncoder().encode(JSON.stringify(payload)).byteLength;
      } catch {
        return Promise.reject(new Error("Canvas host 请求必须可序列化"));
      }
      if (requestBytes > MAX_REQUEST_BYTES) {
        return Promise.reject(new Error("Canvas host 请求超过 256 KiB"));
      }
      const requestId = "canvas-host-" + (++requestSeq);
      return new Promise((resolve, reject) => {
        const timeout = window.setTimeout(() => {
          pending.delete(requestId);
          reject(new Error("Canvas host 请求超时: " + method));
        }, 30000);
        pending.set(requestId, { resolve, reject, timeout });
        hostPort.postMessage({
          contract: CONTRACT,
          kind: "request",
          frame_id: frameId,
          generation,
          request_id: requestId,
          method,
          payload,
        });
      });
    };

    const notify = (method, payload = {}) => {
      if (!hostPort) return;
      hostPort.postMessage({
        contract: CONTRACT,
        kind: "notification",
        frame_id: frameId,
        generation,
        method,
        payload,
      });
    };

    const exactOperationRef = (value) => {
      if (!value || typeof value !== "object") {
        throw new Error("OperationRef 必须是对象");
      }
      const ref = {
        namespace: String(value.namespace || ""),
        provider_key: String(value.provider_key || ""),
        operation_key: String(value.operation_key || ""),
        contract_version: Number(value.contract_version),
      };
      if (!ref.namespace || !ref.provider_key || !ref.operation_key
          || !Number.isInteger(ref.contract_version) || ref.contract_version < 1) {
        throw new Error("OperationRef 不完整");
      }
      return ref;
    };

    window.agentdash = Object.freeze({
      operations: Object.freeze({
        list: () => request("operations.list"),
        describe: (operationRef) =>
          request("operations.describe", { operation_ref: exactOperationRef(operationRef) }),
        invoke: (operationRef, value = {}, options = {}) =>
          request("operations.invoke", {
            operation_ref: exactOperationRef(operationRef),
            input: value,
            idempotency_key: options && options.idempotencyKey
              ? String(options.idempotencyKey)
              : undefined,
          }),
      }),
      actions: Object.freeze({
        invokeRaw: (actionKey, payload = {}, options = {}) =>
          request("actions.invoke", {
            action_key: String(actionKey || ""),
            payload,
            expected_state_revision:
              options && Number.isInteger(options.expectedStateRevision)
                ? Number(options.expectedStateRevision)
                : undefined,
          }),
        invoke: async (actionKey, payload = {}, options = {}) => {
          const response = await window.agentdash.actions.invokeRaw(actionKey, payload, options);
          if (!response || typeof response !== "object" || !response.outcome) {
            throw new Error("Canvas action 返回值不完整");
          }
          return response.outcome.value;
        },
      }),
      assets: Object.freeze({
        url: (uri) => request("assets.url", { uri: String(uri || "") }),
        revoke: (url) => request("assets.revoke", { url: String(url || "") }),
      }),
      interaction: Object.freeze({
        getState: () => request("interaction.get_state"),
        dispatch: (operationRef, payload = {}, expectedRevision) =>
          request("interaction.dispatch", {
            operation_ref: exactOperationRef(operationRef),
            payload,
            expected_revision: expectedRevision,
          }),
        emit: (operationRef, payload = {}, expectedRevision) =>
          request("interaction.emit", {
            operation_ref: exactOperationRef(operationRef),
            payload,
            expected_revision: expectedRevision,
          }),
      }),
      agent: Object.freeze({
        submit: (value) => request("agent.submit", { input: value }),
      }),
      diagnostics: Object.freeze({
        report: (observation) => notify("diagnostics.report", { observation }),
      }),
    });

    const reportRuntime = (status, message) => {
      const bodyText = document.body && document.body.innerText
        ? document.body.innerText.replace(/\\s+/g, " ").trim().slice(0, 1000)
        : "";
      notify("diagnostics.report", {
        observation: {
          status,
          message: message || undefined,
          viewport: {
            width: window.innerWidth || 0,
            height: window.innerHeight || 0,
            device_pixel_ratio: window.devicePixelRatio || 1,
          },
          document: {
            root_empty: !document.getElementById("root")?.hasChildNodes(),
            body_text_preview: bodyText,
            element_count: document.body ? document.body.querySelectorAll("*").length : 0,
          },
        },
      });
    };

    const boot = async () => {
      reportRuntime("building");
      try {
        const entryModule = await import(entryUrl);
        const root = document.getElementById("root");
        const maybeComponent = entryModule && entryModule.default;
        if (typeof maybeComponent === "function" && root && !root.hasChildNodes()) {
          const [{ createElement }, { createRoot }] = await Promise.all([
            import("react"),
            import("react-dom/client"),
          ]);
          createRoot(root).render(createElement(maybeComponent));
        }
        reportRuntime("ready");
        notify("runtime.ready");
      } catch (error) {
        const message = error instanceof Error
          ? error.stack || error.message
          : String(error || "unknown");
        reportRuntime("error", message);
        notify("runtime.error", { message });
      }
    };

    const connect = (event) => {
      const data = event.data;
      const port = event.ports && event.ports[0];
      if (!data || data.contract !== CONTRACT || data.kind !== "connect"
          || data.frame_id !== frameId || data.generation !== generation || !port) {
        return;
      }
      window.removeEventListener("message", connect);
      hostPort = port;
      hostPort.onmessage = (portEvent) => {
        const message = portEvent.data;
        if (!message || message.contract !== CONTRACT
            || message.kind !== "response"
            || message.frame_id !== frameId
            || message.generation !== generation) {
          return;
        }
        const item = pending.get(message.request_id);
        if (!item) return;
        pending.delete(message.request_id);
        clearTimeout(item.timeout);
        if (message.ok) item.resolve(message.result);
        else item.reject(new Error(message.error || "Canvas host 请求失败"));
      };
      hostPort.onmessageerror = () => rejectPending("Canvas host 消息无法解码");
      hostPort.start();
      hostPort.postMessage({
        contract: CONTRACT,
        kind: "connected",
        frame_id: frameId,
        generation,
      });
      void boot();
    };
    window.addEventListener("message", connect);
    window.addEventListener("pagehide", () => {
      rejectPending("Canvas runtime 已卸载");
      hostPort?.close();
      hostPort = null;
      for (const url of moduleUrls.values()) {
        URL.revokeObjectURL(url);
      }
      moduleUrls.clear();
    }, { once: true });
  `;
}

export function parseVfsAssetUri(uri: string): ParsedVfsAssetUri | string {
  const trimmed = uri.trim();
  const separatorIndex = trimmed.indexOf("://");
  if (separatorIndex <= 0) {
    return "无效的 VFS 图片 URI";
  }

  const mountId = trimmed.slice(0, separatorIndex).trim();
  const rawPath = trimmed.slice(separatorIndex + 3).trim();
  if (!isValidMountId(mountId) || !rawPath) {
    return "无效的 VFS 图片 URI";
  }
  if (isReservedBrowserScheme(mountId)) {
    return "无效的 VFS 图片 URI";
  }
  if (rawPath.includes("?") || rawPath.includes("#")) {
    return "VFS 图片 URI 不支持 query 或 fragment";
  }
  if (isAbsoluteLikePath(rawPath)) {
    return "VFS 图片路径必须是 mount 相对路径";
  }

  const parts = rawPath
    .replace(/\\/g, "/")
    .split("/")
    .filter((part) => part.length > 0 && part !== ".");
  if (parts.length === 0 || parts.some((part) => part === "..")) {
    return "VFS 图片路径不能包含 ..";
  }

  return {
    mountId,
    path: parts.join("/"),
  };
}

function buildModuleCode(
  file: CanvasRuntimeFile,
  normalizedPath: string,
  fileMap: Map<string, CanvasRuntimeFile>,
  getModuleUrl: (requestPath: string) => string,
): string {
  if (normalizedPath.endsWith(".json")) {
    return `export default ${file.content.trim() || "null"};`;
  }

  if (normalizedPath.endsWith(".css")) {
    return `export default ${JSON.stringify(normalizedPath)};`;
  }

  if (!isScriptFile(normalizedPath)) {
    return `export default ${JSON.stringify(file.content)};`;
  }

  const transpiled = ts.transpileModule(file.content, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      jsx: ts.JsxEmit.ReactJSX,
      jsxImportSource: "react",
      verbatimModuleSyntax: true,
      isolatedModules: true,
      allowJs: true,
    },
    fileName: normalizedPath,
    reportDiagnostics: true,
  });

  const diagnostics = transpiled.diagnostics ?? [];
  const seriousDiagnostics = diagnostics.filter(
    (item) => item.category === ts.DiagnosticCategory.Error,
  );
  if (seriousDiagnostics.length > 0) {
    throw new Error(formatDiagnostics(seriousDiagnostics));
  }

  return rewriteModuleSpecifiers(transpiled.outputText, normalizedPath, fileMap, getModuleUrl);
}

function rewriteModuleSpecifiers(
  code: string,
  currentPath: string,
  fileMap: Map<string, CanvasRuntimeFile>,
  getModuleUrl: (requestPath: string) => string,
): string {
  const replaceSpecifier = (specifier: string) => {
    if (isLocalSpecifier(specifier)) {
      const resolvedPath = resolveImportPath(currentPath, specifier);
      const existingPath = resolveExistingModulePath(fileMap, resolvedPath);
      return getModuleUrl(existingPath);
    }

    const canvasFilePath = maybeResolveExistingModulePath(fileMap, specifier);
    return canvasFilePath ? getModuleUrl(canvasFilePath) : specifier;
  };

  return code
    .replace(/(\bfrom\s*["'])([^"']+)(["'])/g, (_, prefix: string, specifier: string, suffix: string) =>
      `${prefix}${replaceSpecifier(specifier)}${suffix}`)
    .replace(/(\bimport\s*["'])([^"']+)(["'])/g, (_, prefix: string, specifier: string, suffix: string) =>
      `${prefix}${replaceSpecifier(specifier)}${suffix}`)
    .replace(/(\bimport\(\s*["'])([^"']+)(["']\s*\))/g, (_, prefix: string, specifier: string, suffix: string) =>
      `${prefix}${replaceSpecifier(specifier)}${suffix}`);
}

function resolveExistingModulePath(
  fileMap: Map<string, CanvasRuntimeFile>,
  requestPath: string,
): string {
  const matched = maybeResolveExistingModulePath(fileMap, requestPath);
  if (!matched) {
    throw new Error(`无法解析 Canvas 模块: ${requestPath}`);
  }

  return matched;
}

function maybeResolveExistingModulePath(
  fileMap: Map<string, CanvasRuntimeFile>,
  requestPath: string,
): string | null {
  const normalizedRequest = normalizePath(requestPath);
  const candidates = [
    normalizedRequest,
    ...MODULE_EXTENSIONS.map((extension) => `${normalizedRequest}${extension}`),
    ...MODULE_EXTENSIONS.map((extension) => `${normalizedRequest}/index${extension}`),
  ];

  const matched = candidates.find((candidate) => fileMap.has(candidate));
  return matched ?? null;
}

function resolveImportPath(currentPath: string, specifier: string): string {
  if (specifier.startsWith("/")) {
    return normalizePath(specifier);
  }

  const baseUrl = new URL(`canvas://preview/${normalizePath(currentPath)}`);
  return normalizePath(new URL(specifier, baseUrl).pathname);
}

function normalizePath(path: string): string {
  return path.replace(/\\/g, "/").replace(/^\/+/, "");
}

function canvasModuleRef(path: string): string {
  return `canvas-module:${normalizePath(path)}`;
}

function isValidMountId(value: string): boolean {
  return (
    value.length > 0
    && !value.includes("://")
    && !value.includes("/")
    && !value.includes("\\")
    && !/\s/.test(value)
  );
}

function isReservedBrowserScheme(value: string): boolean {
  return ["http", "https", "file", "data", "blob"].includes(value.toLowerCase());
}

function isAbsoluteLikePath(value: string): boolean {
  return (
    value.startsWith("/")
    || value.startsWith("\\")
    || value.startsWith("//")
    || value.startsWith("\\\\")
    || /^[A-Za-z]:[\\/]/.test(value)
  );
}

function isImageBlob(blob: Blob): boolean {
  return blob.type.startsWith("image/");
}

function sortRecord(record: Record<string, string | undefined>): Record<string, string> {
  const sorted: Record<string, string> = {};
  for (const key of Object.keys(record).sort()) {
    sorted[key] = record[key] ?? "";
  }
  return sorted;
}

function stableJsonStringify(value: unknown): string {
  if (value === null) return "null";
  if (typeof value === "string") return JSON.stringify(value);
  if (typeof value === "number") return Number.isFinite(value) ? JSON.stringify(value) : "null";
  if (typeof value === "boolean") return value ? "true" : "false";
  if (Array.isArray(value)) {
    return `[${value.map((item) => stableJsonStringify(item)).join(",")}]`;
  }
  if (!isRecord(value)) {
    return "null";
  }

  const pairs = Object.keys(value)
    .filter((key) => value[key] !== undefined)
    .sort()
    .map((key) => `${JSON.stringify(key)}:${stableJsonStringify(value[key])}`);
  return `{${pairs.join(",")}}`;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

function isLocalSpecifier(specifier: string): boolean {
  return specifier.startsWith("./") || specifier.startsWith("../") || specifier.startsWith("/");
}

function isScriptFile(path: string): boolean {
  return [".ts", ".tsx", ".js", ".jsx", ".mjs"].some((extension) => path.endsWith(extension));
}

function isCssFile(path: string): boolean {
  return path.endsWith(".css");
}

function sanitizeCssForStyleTag(css: string): string {
  return css.replace(/<\/(style)/gi, "<\\/$1");
}

function serializeForInlineScript(value: unknown): string {
  return JSON.stringify(value)
    .replace(/</g, "\\u003c")
    .replace(/\u2028/g, "\\u2028")
    .replace(/\u2029/g, "\\u2029");
}

function formatDiagnostics(diagnostics: readonly ts.Diagnostic[]): string {
  return diagnostics
    .map((item) => {
      const message = ts.flattenDiagnosticMessageText(item.messageText, "\n");
      const line = item.file && item.start != null
        ? item.file.getLineAndCharacterOfPosition(item.start).line + 1
        : null;
      return line ? `第 ${line} 行: ${message}` : message;
    })
    .join("\n");
}
