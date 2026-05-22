import { useCallback, useEffect, useId, useRef, useState } from "react";
import { invokeCanvasRuntimeAction } from "../../services/canvas";
import { readSurfaceFileBlob } from "../../services/vfs";
import type { CanvasRuntimeSnapshot } from "../../types";
import {
  buildPreviewDocument,
  createRuntimeAssetUrlCache,
  resolveRuntimeAssetUrl,
  revokeAllRuntimeAssetUrls as revokeAllRuntimeAssetUrlsInCache,
  revokeRuntimeAssetUrl as revokeRuntimeAssetUrlInCache,
  type BuiltPreviewDocument,
  type RuntimeAssetUrlCache,
} from "./CanvasRuntimePreview.runtime";

export interface CanvasRuntimePreviewProps {
  snapshot: CanvasRuntimeSnapshot | null;
}

type PreviewStatus = "idle" | "building" | "ready" | "error";

interface PreviewEnvelope {
  kind: "canvas-preview-ready" | "canvas-preview-error";
  frame_id: string;
  message?: string;
}

interface RuntimeInvokeEnvelope {
  kind: "canvas-runtime-invoke";
  frame_id: string;
  request_id: string;
  action_key: string;
  input?: unknown;
}

interface RuntimeResultEnvelope {
  kind: "canvas-runtime-result";
  frame_id: string;
  request_id: string;
  ok: boolean;
  result?: unknown;
  error?: string;
}

interface AssetUrlRequestEnvelope {
  kind: "canvas-asset-url-request";
  frame_id: string;
  request_id: string;
  uri: string;
}

interface AssetUrlResultEnvelope {
  kind: "canvas-asset-url-result";
  frame_id: string;
  request_id: string;
  ok: boolean;
  url?: string;
  error?: string;
}

interface AssetRevokeEnvelope {
  kind: "canvas-asset-revoke";
  frame_id: string;
  url: string;
}

interface PreviewGeneration {
  frameId: string;
  assetCache: RuntimeAssetUrlCache;
}

/**
 * Blob URL revoke 的安全延迟（ms）。
 * iframe srcDoc 更新后浏览器异步解析新文档并 fetch blob URL，
 * 需要给它足够的时间完成所有模块加载后再 revoke。
 */
const BLOB_REVOKE_DELAY_MS = 8_000;

export function CanvasRuntimePreview({ snapshot }: CanvasRuntimePreviewProps) {
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  // 使用 React useId 生成渲染期稳定的 frame id，避免 Math.random 在 render 中被纯度规则拒绝。
  const frameIdBase = `canvas-preview-${useId()}`;
  const generationSeqRef = useRef(0);
  const activeGenerationRef = useRef<PreviewGeneration | null>(null);
  const [runtimeStatus, setRuntimeStatus] = useState<PreviewStatus>("idle");
  const [runtimeMessage, setRuntimeMessage] = useState<string | null>(null);

  const [activeSrcDoc, setActiveSrcDoc] = useState<string | null>(null);
  const [buildError, setBuildError] = useState<string | null>(null);

  // snapshot 变化时重建预览文档。构建失败/成功都要把结果写回 UI 状态，属于合法的 derived state。
  useEffect(() => {
    if (!snapshot) {
      const capturedGeneration = activeGenerationRef.current;
      activeGenerationRef.current = null;
      // eslint-disable-next-line react-hooks/set-state-in-effect
      setActiveSrcDoc(null);
      setBuildError(null);
      setRuntimeStatus("idle");
      setRuntimeMessage(null);
      if (capturedGeneration) {
        revokeAllRuntimeAssetUrlsInCache(capturedGeneration.assetCache);
      }
      return;
    }

    const generation: PreviewGeneration = {
      frameId: `${frameIdBase}-${++generationSeqRef.current}`,
      assetCache: createRuntimeAssetUrlCache(),
    };
    let built: BuiltPreviewDocument | null = null;
    try {
      built = buildPreviewDocument(snapshot, generation.frameId);
      activeGenerationRef.current = generation;
      setActiveSrcDoc(built.srcDoc);
      setBuildError(null);
      setRuntimeStatus("building");
      setRuntimeMessage("正在装载 Canvas 运行时...");
    } catch (error) {
      activeGenerationRef.current = null;
      revokeAllRuntimeAssetUrlsInCache(generation.assetCache);
      setActiveSrcDoc(null);
      setBuildError(error instanceof Error ? error.message : "Canvas 预览构建失败");
      setRuntimeStatus("error");
      setRuntimeMessage(error instanceof Error ? error.message : "Canvas 预览构建失败");
    }

    const capturedBuilt = built;
    // 在 effect 执行时捕获 iframe 引用；组件挂载期间该 ref 指向同一节点，
    // cleanup 使用捕获值可避免触发 react-hooks/exhaustive-deps 对 ref 的警告。
    const capturedIframe = iframeRef.current;
    return () => {
      if (activeGenerationRef.current === generation) {
        activeGenerationRef.current = null;
      }
      if (!capturedBuilt) return;

      if (capturedIframe) {
        capturedIframe.srcdoc = "";
      }

      setTimeout(() => {
        capturedBuilt.dispose();
        revokeAllRuntimeAssetUrlsInCache(generation.assetCache);
      }, BLOB_REVOKE_DELAY_MS);
    };
  }, [snapshot, frameIdBase]);

  const sendRuntimeResult = useCallback((payload: RuntimeResultEnvelope) => {
    iframeRef.current?.contentWindow?.postMessage(payload, "*");
  }, []);

  const sendAssetUrlResult = useCallback((payload: AssetUrlResultEnvelope) => {
    iframeRef.current?.contentWindow?.postMessage(payload, "*");
  }, []);

  const handleRuntimeInvoke = useCallback(async (payload: RuntimeInvokeEnvelope) => {
    const generation = activeGenerationRef.current;
    if (!generation || payload.frame_id !== generation.frameId) {
      return;
    }

    if (!snapshot?.session_id) {
      sendRuntimeResult({
        kind: "canvas-runtime-result",
        frame_id: generation.frameId,
        request_id: payload.request_id,
        ok: false,
        error: "Canvas runtime bridge 需要绑定 Session 后才能调用",
      });
      return;
    }

    const visibleActions = snapshot.runtime_bridge.surface?.actions ?? [];
    const actionVisible = visibleActions.some((action) => action.action_key === payload.action_key);
    if (!snapshot.runtime_bridge.enabled || !actionVisible) {
      sendRuntimeResult({
        kind: "canvas-runtime-result",
        frame_id: generation.frameId,
        request_id: payload.request_id,
        ok: false,
        error: `Canvas runtime action 不可见: ${payload.action_key}`,
      });
      return;
    }

    try {
      const result = await invokeCanvasRuntimeAction(snapshot.canvas_id, {
        session_id: snapshot.session_id,
        action_key: payload.action_key,
        input: payload.input ?? {},
      });
      if (activeGenerationRef.current !== generation) {
        return;
      }
      sendRuntimeResult({
        kind: "canvas-runtime-result",
        frame_id: generation.frameId,
        request_id: payload.request_id,
        ok: true,
        result,
      });
    } catch (error) {
      if (activeGenerationRef.current !== generation) {
        return;
      }
      sendRuntimeResult({
        kind: "canvas-runtime-result",
        frame_id: generation.frameId,
        request_id: payload.request_id,
        ok: false,
        error: error instanceof Error ? error.message : "Canvas runtime action 调用失败",
      });
    }
  }, [sendRuntimeResult, snapshot]);

  const handleAssetUrlRequest = useCallback(async (payload: AssetUrlRequestEnvelope) => {
    const generation = activeGenerationRef.current;
    if (!generation || payload.frame_id !== generation.frameId) {
      return;
    }

    const surfaceRef = snapshot?.resource_surface_ref?.trim();
    if (!snapshot?.session_id || !surfaceRef) {
      sendAssetUrlResult({
        kind: "canvas-asset-url-result",
        frame_id: generation.frameId,
        request_id: payload.request_id,
        ok: false,
        error: "Canvas 图片资源需要绑定 Session",
      });
      return;
    }

    try {
      const url = await resolveRuntimeAssetUrl({
        surfaceRef,
        uri: payload.uri,
        cache: generation.assetCache,
        readBlob: readSurfaceFileBlob,
      });
      if (activeGenerationRef.current !== generation) {
        revokeRuntimeAssetUrlInCache(generation.assetCache, url);
        return;
      }
      sendAssetUrlResult({
        kind: "canvas-asset-url-result",
        frame_id: generation.frameId,
        request_id: payload.request_id,
        ok: true,
        url,
      });
    } catch (error) {
      if (activeGenerationRef.current !== generation) {
        return;
      }
      sendAssetUrlResult({
        kind: "canvas-asset-url-result",
        frame_id: generation.frameId,
        request_id: payload.request_id,
        ok: false,
        error: error instanceof Error ? error.message : "图片资源读取失败",
      });
    }
  }, [sendAssetUrlResult, snapshot]);

  const handleIframeMessage = useCallback((event: MessageEvent<unknown>) => {
    const iframe = iframeRef.current;
    if (!iframe || event.source !== iframe.contentWindow) {
      return;
    }
    const payload = event.data;
    const generation = activeGenerationRef.current;
    if (isRuntimeInvokeEnvelope(payload) && payload.frame_id === generation?.frameId) {
      void handleRuntimeInvoke(payload);
      return;
    }

    if (isAssetUrlRequestEnvelope(payload) && payload.frame_id === generation?.frameId) {
      void handleAssetUrlRequest(payload);
      return;
    }

    if (isAssetRevokeEnvelope(payload) && payload.frame_id === generation?.frameId) {
      revokeRuntimeAssetUrlInCache(generation.assetCache, payload.url);
      return;
    }

    if (!isPreviewEnvelope(payload) || payload.frame_id !== generation?.frameId) {
      return;
    }

    if (payload.kind === "canvas-preview-ready") {
      setRuntimeStatus("ready");
      setRuntimeMessage("Canvas 预览已启动");
    } else {
      setRuntimeStatus("error");
      setRuntimeMessage(payload.message ?? "Canvas 运行时报错");
    }
  }, [handleAssetUrlRequest, handleRuntimeInvoke]);

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
        <pre className="overflow-auto whitespace-pre-wrap rounded-[8px] border border-destructive/20 bg-background px-3 py-2 text-xs text-destructive">
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

function isRuntimeInvokeEnvelope(value: unknown): value is RuntimeInvokeEnvelope {
  if (value == null || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return (
    record.kind === "canvas-runtime-invoke"
    && typeof record.frame_id === "string"
    && typeof record.request_id === "string"
    && typeof record.action_key === "string"
  );
}

function isAssetUrlRequestEnvelope(value: unknown): value is AssetUrlRequestEnvelope {
  if (value == null || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return (
    record.kind === "canvas-asset-url-request"
    && typeof record.frame_id === "string"
    && typeof record.request_id === "string"
    && typeof record.uri === "string"
  );
}

function isAssetRevokeEnvelope(value: unknown): value is AssetRevokeEnvelope {
  if (value == null || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return (
    record.kind === "canvas-asset-revoke"
    && typeof record.frame_id === "string"
    && typeof record.url === "string"
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
