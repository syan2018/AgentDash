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
    runtime_bridge: snapshot.runtime_bridge,
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
  const objectUrls = new Set<string>();
  const moduleUrlCache = new Map<string, string>();

  const importMap = {
    imports: {
      ...DEFAULT_IMPORTS,
      ...snapshot.import_map.imports,
    },
  };

  const cssContent = snapshot.files
    .filter((file) => isCssFile(file.path))
    .map((file) => file.content)
    .join("\n\n");

  const dispose = () => {
    for (const url of objectUrls) {
      URL.revokeObjectURL(url);
    }
  };

  const createObjectUrl = (content: string, mimeType: string) => {
    const url = URL.createObjectURL(new Blob([content], { type: mimeType }));
    objectUrls.add(url);
    return url;
  };

  const getModuleUrl = (requestPath: string): string => {
    const normalizedPath = resolveExistingModulePath(fileMap, requestPath);
    const cached = moduleUrlCache.get(normalizedPath);
    if (cached) {
      return cached;
    }

    const file = fileMap.get(normalizedPath);
    if (!file) {
      throw new Error(`Canvas 预览缺少文件: ${normalizedPath}`);
    }

    const moduleCode = buildModuleCode(file, normalizedPath, fileMap, getModuleUrl);
    const url = createObjectUrl(moduleCode, "text/javascript");
    moduleUrlCache.set(normalizedPath, url);
    return url;
  };

  const entryUrl = getModuleUrl(snapshot.entry);
  const escapedImportMap = JSON.stringify(importMap, null, 2);
  const safeCss = sanitizeCssForStyleTag(cssContent);
  const bootScript = `
    const frameId = ${JSON.stringify(frameId)};
    const frameGeneration = ${JSON.stringify(generation)};
    const send = (kind, message) => {
      window.parent.postMessage({ kind, frame_id: frameId, generation: frameGeneration, message }, "*");
    };
    const runtimeInvokeTimeoutMs = 60000;
    const assetUrlTimeoutMs = 60000;
    const extensionProtocolTimeoutMs = 60000;
    const agentSubmitTimeoutMs = 60000;
    const pendingRuntimeInvocations = new Map();
    const pendingAssetUrls = new Map();
    const pendingExtensionProtocols = new Map();
    const pendingAgentSubmits = new Map();
    let runtimeInvokeSeq = 0;
    let assetUrlSeq = 0;
    let extensionProtocolSeq = 0;
    let agentSubmitSeq = 0;
    const diagnostics = [];
    const interactionState = {};
    const recentInteractionEvents = [];
    const maxDiagnostics = 40;
    const maxRecentEvents = 20;
    const toJsonSafe = (value, seen = new WeakSet()) => {
      if (value === null || typeof value === "string" || typeof value === "boolean") return value;
      if (typeof value === "number") return Number.isFinite(value) ? value : null;
      if (typeof value !== "object") return String(value);
      if (seen.has(value)) return "[Circular]";
      seen.add(value);
      if (Array.isArray(value)) {
        const result = value.map((item) => toJsonSafe(item, seen));
        seen.delete(value);
        return result;
      }
      const result = {};
      for (const [key, item] of Object.entries(value)) {
        result[key] = toJsonSafe(item, seen);
      }
      seen.delete(value);
      return result;
    };
    const pushDiagnostic = (level, source, message) => {
      diagnostics.push({
        level,
        source,
        message: String(message || "").slice(0, 1200),
      });
      if (diagnostics.length > maxDiagnostics) {
        diagnostics.splice(0, diagnostics.length - maxDiagnostics);
      }
    };
    const describeElement = (element) => {
      if (!element || !element.tagName) return undefined;
      const parts = [String(element.tagName).toLowerCase()];
      if (element.id) parts.push("#" + element.id);
      if (element.getAttribute) {
        const label = element.getAttribute("aria-label") || element.getAttribute("name") || element.getAttribute("role");
        if (label) parts.push("[" + String(label).slice(0, 80) + "]");
      }
      return parts.join("");
    };
    const captureObservation = (status, message) => {
      const root = document.getElementById("root");
      const bodyText = (document.body && document.body.innerText ? document.body.innerText : "").replace(/\\s+/g, " ").trim();
      window.parent.postMessage({
        kind: "canvas-render-observation",
        frame_id: frameId,
        generation: frameGeneration,
        status,
        message: typeof message === "string" && message.length > 0 ? message : undefined,
        viewport: {
          width: window.innerWidth || 0,
          height: window.innerHeight || 0,
          device_pixel_ratio: window.devicePixelRatio || 1,
        },
        document: {
          root_empty: isRootEmpty(root),
          body_text_preview: bodyText.slice(0, 1000),
          element_count: document.body ? document.body.querySelectorAll("*").length : 0,
          focused_element: describeElement(document.activeElement),
        },
        diagnostics: diagnostics.slice(),
      }, "*");
    };
    const publishInteractionSnapshot = () => {
      window.parent.postMessage({
        kind: "canvas-interaction-snapshot",
        frame_id: frameId,
        generation: frameGeneration,
        state: toJsonSafe(interactionState),
        recent_events: recentInteractionEvents.slice(),
      }, "*");
    };
    const pushInteractionEvent = (kind, payload) => {
      recentInteractionEvents.push({
        kind,
        payload: toJsonSafe(payload),
        occurred_at: new Date().toISOString(),
      });
      if (recentInteractionEvents.length > maxRecentEvents) {
        recentInteractionEvents.splice(0, recentInteractionEvents.length - maxRecentEvents);
      }
    };
    const originalConsole = {
      log: console.log.bind(console),
      info: console.info.bind(console),
      warn: console.warn.bind(console),
      error: console.error.bind(console),
    };
    console.log = (...args) => {
      pushDiagnostic("info", "console", args.map((item) => String(item)).join(" "));
      originalConsole.log(...args);
    };
    console.info = (...args) => {
      pushDiagnostic("info", "console", args.map((item) => String(item)).join(" "));
      originalConsole.info(...args);
    };
    console.warn = (...args) => {
      pushDiagnostic("warn", "console", args.map((item) => String(item)).join(" "));
      originalConsole.warn(...args);
    };
    console.error = (...args) => {
      pushDiagnostic("error", "console", args.map((item) => String(item)).join(" "));
      originalConsole.error(...args);
    };
    window.agentdash = Object.freeze({
      invoke(actionKey, input = {}) {
        if (typeof actionKey !== "string" || actionKey.trim().length === 0) {
          return Promise.reject(new Error("agentdash.invoke 需要非空 actionKey"));
        }

        const requestId = "canvas-rt-" + (++runtimeInvokeSeq);
        return new Promise((resolve, reject) => {
          const timeout = window.setTimeout(() => {
            pendingRuntimeInvocations.delete(requestId);
            reject(new Error("Canvas runtime action 调用超时"));
          }, runtimeInvokeTimeoutMs);
          pendingRuntimeInvocations.set(requestId, { resolve, reject, timeout });
          window.parent.postMessage({
            kind: "canvas-runtime-invoke",
            frame_id: frameId,
            generation: frameGeneration,
            request_id: requestId,
            action_key: actionKey,
            input,
          }, "*");
        });
      },
      assets: Object.freeze({
        url(uri) {
          if (typeof uri !== "string" || uri.trim().length === 0) {
            return Promise.reject(new Error("agentdash.assets.url 需要非空 VFS URI"));
          }

          const requestId = "canvas-asset-" + (++assetUrlSeq);
          return new Promise((resolve, reject) => {
            const timeout = window.setTimeout(() => {
              pendingAssetUrls.delete(requestId);
              reject(new Error("Canvas 图片资源读取超时"));
            }, assetUrlTimeoutMs);
            pendingAssetUrls.set(requestId, { resolve, reject, timeout });
            window.parent.postMessage({
              kind: "canvas-asset-url-request",
              frame_id: frameId,
              generation: frameGeneration,
              request_id: requestId,
              uri,
            }, "*");
          });
        },
        revoke(url) {
          if (typeof url !== "string" || url.trim().length === 0) {
            return;
          }
          window.parent.postMessage({
            kind: "canvas-asset-revoke",
            frame_id: frameId,
            generation: frameGeneration,
            url,
          }, "*");
        },
      }),
      interaction: Object.freeze({
        setState(key, value, options = {}) {
          if (typeof key !== "string" || key.trim().length === 0) {
            return Promise.reject(new Error("agentdash.interaction.setState 需要非空 key"));
          }
          interactionState[key] = toJsonSafe(value);
          pushInteractionEvent("state_set", {
            key,
            value,
            options,
          });
          publishInteractionSnapshot();
          return Promise.resolve({ ok: true });
        },
        clearState(key) {
          if (typeof key !== "string" || key.trim().length === 0) {
            return Promise.reject(new Error("agentdash.interaction.clearState 需要非空 key"));
          }
          delete interactionState[key];
          pushInteractionEvent("state_cleared", { key });
          publishInteractionSnapshot();
          return Promise.resolve({ ok: true });
        },
        emit(event) {
          const normalized = event && typeof event === "object" ? event : { kind: String(event || "event") };
          const kind = typeof normalized.kind === "string" && normalized.kind.trim().length > 0
            ? normalized.kind
            : "event";
          pushInteractionEvent(kind, normalized.payload ?? normalized);
          publishInteractionSnapshot();
          return Promise.resolve({ ok: true });
        },
        getState() {
          return toJsonSafe(interactionState);
        },
      }),
      agent: Object.freeze({
        submit(request = {}) {
          const normalized = request && typeof request === "object" ? request : {};
          const requestId = normalized.client_command_id && typeof normalized.client_command_id === "string"
            ? normalized.client_command_id
            : "canvas-agent-submit-" + (++agentSubmitSeq);
          return new Promise((resolve, reject) => {
            const timeout = window.setTimeout(() => {
              pendingAgentSubmits.delete(requestId);
              reject(new Error("Canvas Agent submit 超时"));
            }, agentSubmitTimeoutMs);
            pendingAgentSubmits.set(requestId, { resolve, reject, timeout });
            window.parent.postMessage({
              kind: "canvas-agent-submit",
              frame_id: frameId,
              generation: frameGeneration,
              request_id: requestId,
              text: typeof normalized.text === "string" ? normalized.text : undefined,
              input: Array.isArray(normalized.input) ? normalized.input : undefined,
              include_interaction_state: normalized.include_interaction_state === true,
              include_render_observation: normalized.include_render_observation === true,
              delivery_intent: normalized.delivery_intent === "steer" ? "steer" : normalized.delivery_intent === "queue" ? "queue" : undefined,
              client_command_id: typeof normalized.client_command_id === "string" ? normalized.client_command_id : requestId,
            }, "*");
          });
        },
      }),
      extensions: Object.freeze({
        invoke(protocolKey, method, input = {}, options = {}) {
          if (typeof protocolKey !== "string" || protocolKey.trim().length === 0) {
            return Promise.reject(new Error("agentdash.extensions.invoke 需要非空 protocolKey"));
          }
          if (typeof method !== "string" || method.trim().length === 0) {
            return Promise.reject(new Error("agentdash.extensions.invoke 需要非空 method"));
          }

          const requestId = "canvas-ext-protocol-" + (++extensionProtocolSeq);
          return new Promise((resolve, reject) => {
            const timeout = window.setTimeout(() => {
              pendingExtensionProtocols.delete(requestId);
              reject(new Error("Canvas extension protocol 调用超时"));
            }, extensionProtocolTimeoutMs);
            pendingExtensionProtocols.set(requestId, { resolve, reject, timeout });
            window.parent.postMessage({
              kind: "canvas-extension-channel-invoke",
              frame_id: frameId,
              generation: frameGeneration,
              request_id: requestId,
              protocol_key: protocolKey,
              method,
              input,
              dependency_alias: options && typeof options.dependency_alias === "string"
                ? options.dependency_alias
                : null,
            }, "*");
          });
        },
      }),
    });
    window.addEventListener("message", (event) => {
      const payload = event.data;
      if (
        !payload
        || (
          payload.kind !== "canvas-runtime-result"
          && payload.kind !== "canvas-asset-url-result"
          && payload.kind !== "canvas-extension-channel-result"
          && payload.kind !== "canvas-agent-submit-result"
        )
        || payload.frame_id !== frameId
        || payload.generation !== frameGeneration
        || typeof payload.request_id !== "string"
      ) {
        return;
      }

      if (payload.kind === "canvas-runtime-result") {
        const pending = pendingRuntimeInvocations.get(payload.request_id);
        if (!pending) {
          return;
        }
        pendingRuntimeInvocations.delete(payload.request_id);
        window.clearTimeout(pending.timeout);

        if (payload.ok) {
          pending.resolve(payload.result);
        } else {
          pending.reject(new Error(payload.error || "Canvas runtime action 调用失败"));
        }
        return;
      }

      if (payload.kind === "canvas-asset-url-result") {
        const pending = pendingAssetUrls.get(payload.request_id);
        if (!pending) {
          return;
        }
        pendingAssetUrls.delete(payload.request_id);
        window.clearTimeout(pending.timeout);

        if (payload.ok && typeof payload.url === "string") {
          pending.resolve(payload.url);
        } else {
          pending.reject(new Error(payload.error || "Canvas 图片资源读取失败"));
        }
        return;
      }

      if (payload.kind === "canvas-extension-channel-result") {
        const pending = pendingExtensionProtocols.get(payload.request_id);
        if (!pending) {
          return;
        }
        pendingExtensionProtocols.delete(payload.request_id);
        window.clearTimeout(pending.timeout);

        if (payload.ok) {
          pending.resolve(payload.result);
        } else {
          pending.reject(new Error(payload.error || "Canvas extension protocol 调用失败"));
        }
        return;
      }

      if (payload.kind === "canvas-agent-submit-result") {
        const pending = pendingAgentSubmits.get(payload.request_id);
        if (!pending) {
          return;
        }
        pendingAgentSubmits.delete(payload.request_id);
        window.clearTimeout(pending.timeout);

        if (payload.ok) {
          pending.resolve(payload.result);
        } else {
          pending.reject(new Error(payload.error || "Canvas 请求提交给 Agent 失败"));
        }
      }
    });
    const isRootEmpty = (root) => {
      if (!root) return false;
      if (root.childElementCount > 0) return false;
      return (root.textContent || "").trim().length === 0;
    };

    window.addEventListener("error", (event) => {
      const message = event.message || "Canvas 运行时发生未捕获异常";
      pushDiagnostic("error", "runtime", message);
      captureObservation("error", message);
      send("canvas-preview-error", message);
    });

    window.addEventListener("unhandledrejection", (event) => {
      const reason = event.reason instanceof Error ? event.reason.message : String(event.reason ?? "unknown");
      pushDiagnostic("error", "runtime", reason);
      captureObservation("error", reason);
      send("canvas-preview-error", reason);
    });

    const explainDependencyFailure = (message) => {
      const lower = String(message || "").toLowerCase();
      if (
        lower.includes("failed to fetch dynamically imported module")
        || lower.includes("error resolving module specifier")
        || lower.includes("module script")
      ) {
        return [
          "Canvas 运行时依赖加载失败，可能是 react/react-dom 或 importmap CDN 不可达。",
          "请检查网络/代理能否访问 esm.sh，或改为项目内本地依赖映射。",
          String(message || ""),
        ].join("\\n");
      }
      return message;
    };

    captureObservation("building");
    import(${JSON.stringify(entryUrl)})
      .then(async (entryModule) => {
        const root = document.getElementById("root");
        const maybeComponent = entryModule && entryModule.default;
        if (typeof maybeComponent === "function" && isRootEmpty(root)) {
          try {
            const [{ createElement }, { createRoot }] = await Promise.all([
              import("react"),
              import("react-dom/client"),
            ]);
            createRoot(root).render(createElement(maybeComponent));
          } catch (renderError) {
            const message = renderError instanceof Error
              ? renderError.stack || renderError.message
              : String(renderError ?? "unknown");
            send(
              "canvas-preview-error",
              "检测到默认导出 React 组件，但运行时无法完成 React 挂载。请检查 react/react-dom 依赖可用性。\\n" + message,
            );
            pushDiagnostic("error", "runtime", message);
            captureObservation("error", message);
            return;
          }
        }
        captureObservation("ready");
        send("canvas-preview-ready");
      })
      .catch((error) => {
        const message = error instanceof Error ? error.stack || error.message : String(error ?? "unknown");
        const explained = explainDependencyFailure(message);
        pushDiagnostic("error", "runtime", explained);
        captureObservation("error", explained);
        send("canvas-preview-error", explained);
      });
  `;

  return {
    srcDoc: `<!DOCTYPE html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Canvas Preview</title>
    <script type="importmap">${escapedImportMap}</script>
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
    <script type="module">
${bootScript}
    </script>
  </body>
</html>`,
    dispose,
  };
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
