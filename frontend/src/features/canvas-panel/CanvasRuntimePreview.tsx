import { useCallback, useEffect, useRef, useState } from "react";
import ts from "typescript";
import type { CanvasRuntimeFile, CanvasRuntimeSnapshot } from "../../types";

export interface CanvasRuntimePreviewProps {
  snapshot: CanvasRuntimeSnapshot | null;
}

type PreviewStatus = "idle" | "building" | "ready" | "error";

interface PreviewEnvelope {
  kind: "canvas-preview-ready" | "canvas-preview-error";
  frame_id: string;
  message?: string;
}

interface BuiltPreviewDocument {
  srcDoc: string;
  dispose: () => void;
}

const DEFAULT_IMPORTS: Record<string, string> = {
  react: "https://esm.sh/react@18?dev",
  "react/jsx-runtime": "https://esm.sh/react@18/jsx-runtime?dev",
  "react/jsx-dev-runtime": "https://esm.sh/react@18/jsx-dev-runtime?dev",
  "react-dom": "https://esm.sh/react-dom@18?dev",
  "react-dom/client": "https://esm.sh/react-dom@18/client?dev",
};

const MODULE_EXTENSIONS = [".ts", ".tsx", ".js", ".jsx", ".json", ".css"];

/**
 * Blob URL revoke 的安全延迟（ms）。
 * iframe srcDoc 更新后浏览器异步解析新文档并 fetch blob URL，
 * 需要给它足够的时间完成所有模块加载后再 revoke。
 */
const BLOB_REVOKE_DELAY_MS = 8_000;

export function CanvasRuntimePreview({ snapshot }: CanvasRuntimePreviewProps) {
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const frameIdRef = useRef(`canvas-preview-${Math.random().toString(36).slice(2)}`);
  const [runtimeStatus, setRuntimeStatus] = useState<PreviewStatus>("idle");
  const [runtimeMessage, setRuntimeMessage] = useState<string | null>(null);

  const [activeSrcDoc, setActiveSrcDoc] = useState<string | null>(null);
  const [buildError, setBuildError] = useState<string | null>(null);

  useEffect(() => {
    if (!snapshot) {
      setActiveSrcDoc(null);
      setBuildError(null);
      setRuntimeStatus("idle");
      setRuntimeMessage(null);
      return;
    }

    let built: BuiltPreviewDocument | null = null;
    try {
      built = buildPreviewDocument(snapshot, frameIdRef.current);
      setActiveSrcDoc(built.srcDoc);
      setBuildError(null);
      setRuntimeStatus("building");
      setRuntimeMessage("正在装载 Canvas 运行时...");
    } catch (error) {
      setActiveSrcDoc(null);
      setBuildError(error instanceof Error ? error.message : "Canvas 预览构建失败");
      setRuntimeStatus("error");
      setRuntimeMessage(error instanceof Error ? error.message : "Canvas 预览构建失败");
    }

    const capturedBuilt = built;
    return () => {
      if (!capturedBuilt) return;

      const iframe = iframeRef.current;
      if (iframe) {
        iframe.srcdoc = "";
      }

      setTimeout(() => capturedBuilt.dispose(), BLOB_REVOKE_DELAY_MS);
    };
  }, [snapshot]);

  const handleIframeMessage = useCallback((event: MessageEvent<unknown>) => {
    const iframe = iframeRef.current;
    if (!iframe || event.source !== iframe.contentWindow) {
      return;
    }
    const payload = event.data;
    if (!isPreviewEnvelope(payload) || payload.frame_id !== frameIdRef.current) {
      return;
    }

    if (payload.kind === "canvas-preview-ready") {
      setRuntimeStatus("ready");
      setRuntimeMessage("Canvas 预览已启动");
    } else {
      setRuntimeStatus("error");
      setRuntimeMessage(payload.message ?? "Canvas 运行时报错");
    }
  }, []);

  useEffect(() => {
    window.addEventListener("message", handleIframeMessage);
    return () => {
      window.removeEventListener("message", handleIframeMessage);
    };
  }, [handleIframeMessage]);

  if (!snapshot) {
    return (
      <div className="flex flex-1 items-center justify-center px-4 text-sm text-muted-foreground">
        当前还没有可运行的 Canvas 快照。
      </div>
    );
  }

  if (buildError) {
    return (
      <div className="flex flex-1 flex-col gap-3 p-4">
        <div>
          <p className="text-[11px] uppercase tracking-[0.12em] text-destructive/80">Preview</p>
          <h4 className="mt-1 text-sm font-semibold text-destructive">运行时预览构建失败</h4>
        </div>
        <pre className="overflow-auto whitespace-pre-wrap rounded-[10px] border border-destructive/20 bg-background px-3 py-2 text-xs text-destructive">
          {buildError}
        </pre>
      </div>
    );
  }

  if (!activeSrcDoc) {
    return (
      <div className="flex flex-1 items-center justify-center px-4 text-sm text-muted-foreground">
        正在构建 Canvas 预览...
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      {/* 状态条 */}
      <div className="flex shrink-0 items-center justify-between border-b border-border/50 bg-secondary/10 px-3 py-1">
        <span className="text-[11px] text-muted-foreground">
          {runtimeMessage ?? "等待 Canvas 运行时启动"}
        </span>
        <span className={statusClassName(runtimeStatus)}>
          {statusLabel(runtimeStatus)}
        </span>
      </div>

      {/* iframe 自适应填满剩余高度 */}
      <div className="min-h-0 flex-1">
        <iframe
          ref={iframeRef}
          title={`canvas-preview-${snapshot.canvas_id}`}
          sandbox="allow-scripts allow-same-origin"
          srcDoc={activeSrcDoc}
          className="h-full w-full border-0 bg-white"
        />
      </div>
    </div>
  );
}

function buildPreviewDocument(
  snapshot: CanvasRuntimeSnapshot,
  frameId: string,
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
    const send = (kind, message) => {
      window.parent.postMessage({ kind, frame_id: frameId, message }, "*");
    };
    const isRootEmpty = (root) => {
      if (!root) return false;
      if (root.childElementCount > 0) return false;
      return (root.textContent || "").trim().length === 0;
    };

    window.addEventListener("error", (event) => {
      send("canvas-preview-error", event.message || "Canvas 运行时发生未捕获异常");
    });

    window.addEventListener("unhandledrejection", (event) => {
      const reason = event.reason instanceof Error ? event.reason.message : String(event.reason ?? "unknown");
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
            return;
          }
        }
        send("canvas-preview-ready");
      })
      .catch((error) => {
        const message = error instanceof Error ? error.stack || error.message : String(error ?? "unknown");
        send("canvas-preview-error", explainDependencyFailure(message));
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
    if (!isLocalSpecifier(specifier)) {
      return specifier;
    }

    const resolvedPath = resolveImportPath(currentPath, specifier);
    const existingPath = resolveExistingModulePath(fileMap, resolvedPath);
    return getModuleUrl(existingPath);
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
  const normalizedRequest = normalizePath(requestPath);
  const candidates = [
    normalizedRequest,
    ...MODULE_EXTENSIONS.map((extension) => `${normalizedRequest}${extension}`),
    ...MODULE_EXTENSIONS.map((extension) => `${normalizedRequest}/index${extension}`),
  ];

  const matched = candidates.find((candidate) => fileMap.has(candidate));
  if (!matched) {
    throw new Error(`无法解析 Canvas 模块: ${requestPath}`);
  }

  return matched;
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

function isLocalSpecifier(specifier: string): boolean {
  return specifier.startsWith("./") || specifier.startsWith("../") || specifier.startsWith("/");
}

function isScriptFile(path: string): boolean {
  return [".ts", ".tsx", ".js", ".jsx", ".mjs"].some((extension) => path.endsWith(extension));
}

function isCssFile(path: string): boolean {
  return path.endsWith(".css");
}

/**
 * 在 <style> 标签内安全嵌入 CSS。
 * <style> 的内容模型是 raw text，HTML 实体不会被解码，
 * 所以不能用 escapeHtml（会把 > 变成 &gt; 破坏 CSS 子选择器等）。
 * 唯一需要防范的是 CSS 中出现 </style 导致提前闭合标签。
 */
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

function isPreviewEnvelope(value: unknown): value is PreviewEnvelope {
  if (value == null || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return (
    (record.kind === "canvas-preview-ready" || record.kind === "canvas-preview-error")
    && typeof record.frame_id === "string"
    && (record.message == null || typeof record.message === "string")
  );
}

function statusLabel(status: PreviewStatus): string {
  switch (status) {
    case "building":
      return "启动中";
    case "ready":
      return "运行中";
    case "error":
      return "异常";
    default:
      return "待命";
  }
}

function statusClassName(status: PreviewStatus): string {
  const baseClassName = "rounded-full border px-2 py-0.5 text-[10px] font-medium";
  switch (status) {
    case "building":
      return `${baseClassName} border-amber-200 bg-amber-50 text-amber-700`;
    case "ready":
      return `${baseClassName} border-emerald-200 bg-emerald-50 text-emerald-700`;
    case "error":
      return `${baseClassName} border-destructive/30 bg-destructive/10 text-destructive`;
    default:
      return `${baseClassName} border-border bg-secondary/20 text-muted-foreground`;
  }
}
