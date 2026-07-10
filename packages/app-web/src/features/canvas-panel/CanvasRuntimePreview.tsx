import { useCallback, useEffect, useId, useRef, useState } from "react";
import type { UserInput } from "../../generated/backbone-protocol";
import type { JsonValue } from "../../generated/common-contracts";
import {
  invokeCanvasRuntimeAction,
  submitCanvasAgentInput,
  uploadCanvasInteractionSnapshot,
  uploadCanvasRenderObservation,
  type AgentRunCanvasBridgeIdentity,
  type CanvasRuntimeDiagnosticEntry,
  type SubmitCanvasAgentInput,
} from "../../services/canvas";
import { readSurfaceFileBlob } from "../../services/vfs";
import type { CanvasRuntimeSnapshot, RuntimeActionDescriptor } from "../../types";
import {
  buildPreviewDocument,
  createRuntimeAssetUrlCache,
  resolveRuntimeAssetUrl,
  revokeAllRuntimeAssetUrls as revokeAllRuntimeAssetUrlsInCache,
  revokeRuntimeAssetUrl as revokeRuntimeAssetUrlInCache,
  type BuiltPreviewDocument,
  type RuntimeAssetUrlCache,
} from "./CanvasRuntimePreview.runtime";
import { buildPreviewFailureObservation } from "./CanvasRuntimePreview.observation";

export interface CanvasRuntimePreviewProps {
  snapshot: CanvasRuntimeSnapshot | null;
  agentRunBridge?: AgentRunCanvasBridgeIdentity | null;
  showBridgeUnavailable?: boolean;
  onAgentRunWorkspaceRefresh?: (() => Promise<unknown>) | null;
  extensionProtocolBridge?: (request: CanvasExtensionProtocolRequest) => Promise<unknown>;
}

type PreviewStatus = "idle" | "building" | "ready" | "error";

interface PreviewEnvelope {
  kind: "canvas-preview-ready" | "canvas-preview-error";
  frame_id: string;
  generation: number;
  message?: string;
}

interface RuntimeInvokeEnvelope {
  kind: "canvas-runtime-invoke";
  frame_id: string;
  generation: number;
  request_id: string;
  action_key: string;
  input?: unknown;
}

interface RuntimeResultEnvelope {
  kind: "canvas-runtime-result";
  frame_id: string;
  generation: number;
  request_id: string;
  ok: boolean;
  result?: unknown;
  error?: string;
}

interface AssetUrlRequestEnvelope {
  kind: "canvas-asset-url-request";
  frame_id: string;
  generation: number;
  request_id: string;
  uri: string;
}

interface AssetUrlResultEnvelope {
  kind: "canvas-asset-url-result";
  frame_id: string;
  generation: number;
  request_id: string;
  ok: boolean;
  url?: string;
  error?: string;
}

interface AssetRevokeEnvelope {
  kind: "canvas-asset-revoke";
  frame_id: string;
  generation: number;
  url: string;
}

export interface CanvasExtensionProtocolRequest {
  protocol_key: string;
  method: string;
  input: unknown;
  dependency_alias?: string | null;
}

interface ExtensionProtocolInvokeEnvelope extends CanvasExtensionProtocolRequest {
  kind: "canvas-extension-channel-invoke";
  frame_id: string;
  generation: number;
  request_id: string;
}

interface ExtensionProtocolResultEnvelope {
  kind: "canvas-extension-channel-result";
  frame_id: string;
  generation: number;
  request_id: string;
  ok: boolean;
  result?: unknown;
  error?: string;
}

interface RenderObservationEnvelope {
  kind: "canvas-render-observation";
  frame_id: string;
  generation: number;
  status: "building" | "ready" | "error";
  message?: string;
  viewport: {
    width: number;
    height: number;
    device_pixel_ratio: number;
  };
  document: {
    root_empty: boolean;
    body_text_preview: string;
    element_count: number;
    focused_element?: string;
  };
  diagnostics: CanvasRuntimeDiagnosticEntry[];
}

interface InteractionSnapshotEnvelope {
  kind: "canvas-interaction-snapshot";
  frame_id: string;
  generation: number;
  state: { [key: string]: JsonValue };
  recent_events: Array<{
    kind: string;
    payload: JsonValue;
    occurred_at: string;
  }>;
}

interface AgentSubmitEnvelope extends SubmitCanvasAgentInput {
  kind: "canvas-agent-submit";
  frame_id: string;
  generation: number;
  request_id: string;
}

interface AgentSubmitResultEnvelope {
  kind: "canvas-agent-submit-result";
  frame_id: string;
  generation: number;
  request_id: string;
  ok: boolean;
  result?: unknown;
  error?: string;
}

interface PreviewGeneration {
  frameId: string;
  generation: number;
  assetCache: RuntimeAssetUrlCache;
}

/**
 * Blob URL revoke 的安全延迟（ms）。
 * iframe srcDoc 更新后浏览器异步解析新文档并 fetch blob URL，
 * 需要给它足够的时间完成所有模块加载后再 revoke。
 */
const BLOB_REVOKE_DELAY_MS = 8_000;

export function CanvasRuntimePreview({
  snapshot,
  agentRunBridge = null,
  showBridgeUnavailable = false,
  onAgentRunWorkspaceRefresh = null,
  extensionProtocolBridge,
}: CanvasRuntimePreviewProps) {
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const agentRunBridgeRef = useRef<AgentRunCanvasBridgeIdentity | null>(agentRunBridge);
  // 使用 React useId 生成渲染期稳定的 frame id，避免 Math.random 在 render 中被纯度规则拒绝。
  const frameIdBase = `canvas-preview-${useId()}`;
  const generationSeqRef = useRef(0);
  const activeGenerationRef = useRef<PreviewGeneration | null>(null);
  const latestObservationIdRef = useRef<string | null>(null);
  const latestInteractionSnapshotIdRef = useRef<string | null>(null);
  const [runtimeStatus, setRuntimeStatus] = useState<PreviewStatus>("idle");
  const [runtimeMessage, setRuntimeMessage] = useState<string | null>(null);

  const [activeSrcDoc, setActiveSrcDoc] = useState<string | null>(null);
  const [buildError, setBuildError] = useState<string | null>(null);

  useEffect(() => {
    agentRunBridgeRef.current = agentRunBridge;
  }, [agentRunBridge]);

  // snapshot 变化时重建预览文档。构建失败/成功都要把结果写回 UI 状态，属于合法的 derived state。
  useEffect(() => {
    if (!snapshot) {
      const capturedGeneration = activeGenerationRef.current;
      activeGenerationRef.current = null;
      latestObservationIdRef.current = null;
      latestInteractionSnapshotIdRef.current = null;
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

    const generationNumber = ++generationSeqRef.current;
    const generation: PreviewGeneration = {
      frameId: `${frameIdBase}-${generationNumber}`,
      generation: generationNumber,
      assetCache: createRuntimeAssetUrlCache(),
    };
    let built: BuiltPreviewDocument | null = null;
    try {
      built = buildPreviewDocument(snapshot, generation.frameId, generation.generation);
      activeGenerationRef.current = generation;
      latestObservationIdRef.current = null;
      latestInteractionSnapshotIdRef.current = null;
      setActiveSrcDoc(built.srcDoc);
      setBuildError(null);
      setRuntimeStatus("building");
      setRuntimeMessage("正在装载 Canvas 运行时...");
    } catch (error) {
      const message = error instanceof Error ? error.message : "Canvas 预览构建失败";
      activeGenerationRef.current = null;
      revokeAllRuntimeAssetUrlsInCache(generation.assetCache);
      setActiveSrcDoc(null);
      setBuildError(message);
      setRuntimeStatus("error");
      setRuntimeMessage(message);
      const currentBridge = agentRunBridgeRef.current;
      if (currentBridge) {
        void uploadCanvasRenderObservation(
          currentBridge,
          buildPreviewFailureObservation(
            generation.frameId,
            generation.generation,
            message,
            iframeRef.current,
          ),
        ).catch(() => {});
      }
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

  const sendExtensionProtocolResult = useCallback((payload: ExtensionProtocolResultEnvelope) => {
    iframeRef.current?.contentWindow?.postMessage(payload, "*");
  }, []);

  const sendAgentSubmitResult = useCallback((payload: AgentSubmitResultEnvelope, target?: MessageEventSource | null) => {
    const destination = target && "postMessage" in target ? target : iframeRef.current?.contentWindow;
    destination?.postMessage(payload, { targetOrigin: "*" });
  }, []);

  const handleRuntimeInvoke = useCallback(async (payload: RuntimeInvokeEnvelope) => {
    const generation = activeGenerationRef.current;
    if (!generation || payload.frame_id !== generation.frameId || payload.generation !== generation.generation) {
      return;
    }

    if (!agentRunBridge) {
      sendRuntimeResult({
        kind: "canvas-runtime-result",
        frame_id: generation.frameId,
        generation: generation.generation,
        request_id: payload.request_id,
        ok: false,
        error: "Canvas runtime bridge 需要绑定 AgentRun Canvas reference 后才能调用",
      });
      return;
    }

    if (!snapshot) {
      sendRuntimeResult({
        kind: "canvas-runtime-result",
        frame_id: generation.frameId,
        generation: generation.generation,
        request_id: payload.request_id,
        ok: false,
        error: "Canvas runtime snapshot 不可用",
      });
      return;
    }

    const visibleActions = runtimeActionsForSnapshot(snapshot);
    const actionVisible = visibleActions.some((action) => action.action_key === payload.action_key);
    if (!snapshot.runtime_bridge.enabled || !actionVisible) {
      sendRuntimeResult({
        kind: "canvas-runtime-result",
        frame_id: generation.frameId,
        generation: generation.generation,
        request_id: payload.request_id,
        ok: false,
        error: `Canvas runtime action 不可见: ${payload.action_key}`,
      });
      return;
    }

    try {
      const result = await invokeCanvasRuntimeAction(agentRunBridge, {
        action_key: payload.action_key,
        input: toJsonValue(payload.input ?? {}),
      });
      if (activeGenerationRef.current !== generation) {
        return;
      }
      sendRuntimeResult({
        kind: "canvas-runtime-result",
        frame_id: generation.frameId,
        generation: generation.generation,
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
        generation: generation.generation,
        request_id: payload.request_id,
        ok: false,
        error: error instanceof Error ? error.message : "Canvas runtime action 调用失败",
      });
    }
  }, [agentRunBridge, sendRuntimeResult, snapshot]);

  const handleAssetUrlRequest = useCallback(async (payload: AssetUrlRequestEnvelope) => {
    const generation = activeGenerationRef.current;
    if (!generation || payload.frame_id !== generation.frameId || payload.generation !== generation.generation) {
      return;
    }

    const surfaceRef = snapshot?.resource_surface_ref?.trim();
    if (!surfaceRef) {
      sendAssetUrlResult({
        kind: "canvas-asset-url-result",
        frame_id: generation.frameId,
        generation: generation.generation,
        request_id: payload.request_id,
        ok: false,
        error: "Canvas 图片资源需要绑定 runtime resource surface",
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
        generation: generation.generation,
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
        generation: generation.generation,
        request_id: payload.request_id,
        ok: false,
        error: error instanceof Error ? error.message : "图片资源读取失败",
      });
    }
  }, [sendAssetUrlResult, snapshot]);

  const handleExtensionProtocolInvoke = useCallback(async (payload: ExtensionProtocolInvokeEnvelope) => {
    const generation = activeGenerationRef.current;
    if (!generation || payload.frame_id !== generation.frameId || payload.generation !== generation.generation) {
      return;
    }

    if (!extensionProtocolBridge) {
      sendExtensionProtocolResult({
        kind: "canvas-extension-channel-result",
        frame_id: generation.frameId,
        generation: generation.generation,
        request_id: payload.request_id,
        ok: false,
        error: "当前 Canvas runtime 未绑定 Extension protocol bridge",
      });
      return;
    }

    try {
      const result = await extensionProtocolBridge({
        protocol_key: payload.protocol_key,
        method: payload.method,
        input: payload.input ?? {},
        dependency_alias: payload.dependency_alias ?? null,
      });
      if (activeGenerationRef.current !== generation) {
        return;
      }
      sendExtensionProtocolResult({
        kind: "canvas-extension-channel-result",
        frame_id: generation.frameId,
        generation: generation.generation,
        request_id: payload.request_id,
        ok: true,
        result,
      });
    } catch (error) {
      if (activeGenerationRef.current !== generation) {
        return;
      }
      sendExtensionProtocolResult({
        kind: "canvas-extension-channel-result",
        frame_id: generation.frameId,
        generation: generation.generation,
        request_id: payload.request_id,
        ok: false,
        error: error instanceof Error ? error.message : "Canvas extension protocol 调用失败",
      });
    }
  }, [extensionProtocolBridge, sendExtensionProtocolResult]);

  const handleRenderObservation = useCallback(async (payload: RenderObservationEnvelope) => {
    const generation = activeGenerationRef.current;
    if (!generation || payload.frame_id !== generation.frameId || payload.generation !== generation.generation) {
      return;
    }
    if (!agentRunBridge) {
      return;
    }

    try {
      const observation = await uploadCanvasRenderObservation(agentRunBridge, {
        frame_id: payload.frame_id,
        generation: payload.generation,
        status: payload.status,
        message: payload.message,
        viewport: payload.viewport,
        document: payload.document,
        diagnostics: payload.diagnostics,
      });
      if (activeGenerationRef.current === generation) {
        latestObservationIdRef.current = observation.observation_id;
      }
    } catch {
      // Observation upload is diagnostic-only; keep preview rendering responsive.
    }
  }, [agentRunBridge]);

  const handleInteractionSnapshot = useCallback(async (payload: InteractionSnapshotEnvelope) => {
    const generation = activeGenerationRef.current;
    if (!generation || payload.frame_id !== generation.frameId || payload.generation !== generation.generation) {
      return;
    }
    if (!agentRunBridge) {
      return;
    }

    try {
      const snapshot = await uploadCanvasInteractionSnapshot(agentRunBridge, {
        frame_id: payload.frame_id,
        state: payload.state,
        recent_events: payload.recent_events,
      });
      if (activeGenerationRef.current === generation) {
        latestInteractionSnapshotIdRef.current = snapshot.snapshot_id;
      }
    } catch {
      // Interaction sync failure is surfaced by submit if it matters for user intent.
    }
  }, [agentRunBridge]);

  const handleAgentSubmit = useCallback(async (
    payload: AgentSubmitEnvelope,
    source: MessageEventSource | null,
  ) => {
    const generation = activeGenerationRef.current;
    if (!generation || payload.frame_id !== generation.frameId || payload.generation !== generation.generation) {
      sendAgentSubmitResult({
        kind: "canvas-agent-submit-result",
        frame_id: payload.frame_id,
        generation: payload.generation,
        request_id: payload.request_id,
        ok: false,
        error: "Canvas Agent bridge generation 已过期，请刷新后重试。",
      }, source);
      return;
    }

    if (!agentRunBridge) {
      sendAgentSubmitResult({
        kind: "canvas-agent-submit-result",
        frame_id: generation.frameId,
        generation: generation.generation,
        request_id: payload.request_id,
        ok: false,
        error: "当前 Canvas 没有 live AgentRun bridge，无法提交给 Agent。",
      }, source);
      return;
    }

    try {
      const input = normalizeAgentSubmitInput(payload);
      if (input.include_interaction_state) {
        input.interaction_snapshot_id = latestInteractionSnapshotIdRef.current ?? undefined;
      }
      if (input.include_render_observation) {
        input.render_observation_id = latestObservationIdRef.current ?? undefined;
      }
      const result = await submitCanvasAgentInput(agentRunBridge, input);
      if (activeGenerationRef.current !== generation) {
        sendAgentSubmitResult({
          kind: "canvas-agent-submit-result",
          frame_id: payload.frame_id,
          generation: payload.generation,
          request_id: payload.request_id,
          ok: false,
          error: "Canvas Agent bridge generation 已过期，请刷新后重试。",
        }, source);
        return;
      }
      sendAgentSubmitResult({
        kind: "canvas-agent-submit-result",
        frame_id: generation.frameId,
        generation: generation.generation,
        request_id: payload.request_id,
        ok: true,
        result,
      }, source);
      await onAgentRunWorkspaceRefresh?.();
    } catch (error) {
      sendAgentSubmitResult({
        kind: "canvas-agent-submit-result",
        frame_id: generation.frameId,
        generation: generation.generation,
        request_id: payload.request_id,
        ok: false,
        error: error instanceof Error ? error.message : "Canvas 请求提交给 Agent 失败",
      }, source);
    }
  }, [agentRunBridge, onAgentRunWorkspaceRefresh, sendAgentSubmitResult]);

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

    if (isExtensionProtocolInvokeEnvelope(payload) && payload.frame_id === generation?.frameId) {
      void handleExtensionProtocolInvoke(payload);
      return;
    }

    if (
      isAssetRevokeEnvelope(payload)
      && payload.frame_id === generation?.frameId
      && payload.generation === generation.generation
    ) {
      revokeRuntimeAssetUrlInCache(generation.assetCache, payload.url);
      return;
    }

    if (isRenderObservationEnvelope(payload) && payload.frame_id === generation?.frameId) {
      void handleRenderObservation(payload);
      return;
    }

    if (isInteractionSnapshotEnvelope(payload) && payload.frame_id === generation?.frameId) {
      void handleInteractionSnapshot(payload);
      return;
    }

    if (isAgentSubmitEnvelope(payload)) {
      void handleAgentSubmit(payload, event.source);
      return;
    }

    if (
      !isPreviewEnvelope(payload)
      || payload.frame_id !== generation?.frameId
      || payload.generation !== generation.generation
    ) {
      return;
    }

    if (payload.kind === "canvas-preview-ready") {
      setRuntimeStatus("ready");
      setRuntimeMessage("Canvas 预览已启动");
    } else {
      setRuntimeStatus("error");
      setRuntimeMessage(payload.message ?? "Canvas 运行时报错");
    }
  }, [
    handleAgentSubmit,
    handleAssetUrlRequest,
    handleExtensionProtocolInvoke,
    handleInteractionSnapshot,
    handleRenderObservation,
    handleRuntimeInvoke,
  ]);

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
      {showBridgeUnavailable && !agentRunBridge && (
        <div className="shrink-0 border-b border-border bg-secondary/30 px-3 py-1.5 text-[11px] text-muted-foreground">
          Canvas AgentRun bridge 不可用，interaction 与 submit 已禁用；普通预览仍可渲染。
        </div>
      )}

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
    && typeof record.generation === "number"
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
    && typeof record.generation === "number"
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
    && typeof record.generation === "number"
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
    && typeof record.generation === "number"
    && typeof record.url === "string"
  );
}

function isExtensionProtocolInvokeEnvelope(value: unknown): value is ExtensionProtocolInvokeEnvelope {
  if (value == null || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return (
    record.kind === "canvas-extension-channel-invoke"
    && typeof record.frame_id === "string"
    && typeof record.generation === "number"
    && typeof record.request_id === "string"
    && typeof record.protocol_key === "string"
    && typeof record.method === "string"
  );
}

function isRenderObservationEnvelope(value: unknown): value is RenderObservationEnvelope {
  if (!isRecord(value)) return false;
  return (
    value.kind === "canvas-render-observation"
    && typeof value.frame_id === "string"
    && typeof value.generation === "number"
    && (value.status === "building" || value.status === "ready" || value.status === "error")
    && (value.message == null || typeof value.message === "string")
    && isViewportObservation(value.viewport)
    && isDocumentObservation(value.document)
    && Array.isArray(value.diagnostics)
    && value.diagnostics.every(isDiagnosticEntry)
  );
}

function isInteractionSnapshotEnvelope(value: unknown): value is InteractionSnapshotEnvelope {
  if (!isRecord(value)) return false;
  return (
    value.kind === "canvas-interaction-snapshot"
    && typeof value.frame_id === "string"
    && typeof value.generation === "number"
    && isJsonObject(value.state)
    && Array.isArray(value.recent_events)
    && value.recent_events.every(isInteractionEvent)
  );
}

function isAgentSubmitEnvelope(value: unknown): value is AgentSubmitEnvelope {
  if (!isRecord(value)) return false;
  return (
    value.kind === "canvas-agent-submit"
    && typeof value.frame_id === "string"
    && typeof value.generation === "number"
    && typeof value.request_id === "string"
    && (value.text == null || typeof value.text === "string")
    && (value.input == null || isUserInputArray(value.input))
    && (value.include_interaction_state == null || typeof value.include_interaction_state === "boolean")
    && (value.include_render_observation == null || typeof value.include_render_observation === "boolean")
    && (
      value.delivery_intent == null
      || value.delivery_intent === "queue"
      || value.delivery_intent === "steer"
    )
    && (value.client_command_id == null || typeof value.client_command_id === "string")
  );
}

function isViewportObservation(value: unknown): value is RenderObservationEnvelope["viewport"] {
  return (
    isRecord(value)
    && typeof value.width === "number"
    && typeof value.height === "number"
    && typeof value.device_pixel_ratio === "number"
  );
}

function isDocumentObservation(value: unknown): value is RenderObservationEnvelope["document"] {
  return (
    isRecord(value)
    && typeof value.root_empty === "boolean"
    && typeof value.body_text_preview === "string"
    && typeof value.element_count === "number"
    && (value.focused_element == null || typeof value.focused_element === "string")
  );
}

function isDiagnosticEntry(value: unknown): value is CanvasRuntimeDiagnosticEntry {
  return (
    isRecord(value)
    && (value.level === "info" || value.level === "warn" || value.level === "error")
    && (value.source === "runtime" || value.source === "console" || value.source === "bridge")
    && typeof value.message === "string"
  );
}

function isInteractionEvent(value: unknown): value is InteractionSnapshotEnvelope["recent_events"][number] {
  return (
    isRecord(value)
    && typeof value.kind === "string"
    && isJsonValue(value.payload)
    && typeof value.occurred_at === "string"
  );
}

function isUserInputArray(value: unknown): value is UserInput[] {
  return Array.isArray(value) && value.every(isUserInput);
}

function isUserInput(value: unknown): value is UserInput {
  if (!isRecord(value) || typeof value.type !== "string") return false;
  switch (value.type) {
    case "text":
      return typeof value.text === "string" && Array.isArray(value.text_elements);
    case "image":
      return typeof value.url === "string";
    case "localImage":
      return typeof value.path === "string";
    case "skill":
    case "mention":
      return typeof value.name === "string" && typeof value.path === "string";
    default:
      return false;
  }
}

function normalizeAgentSubmitInput(payload: AgentSubmitEnvelope): SubmitCanvasAgentInput {
  return {
    text: payload.text,
    input: payload.input,
    include_interaction_state: payload.include_interaction_state,
    include_render_observation: payload.include_render_observation,
    delivery_intent: payload.delivery_intent,
    client_command_id: payload.client_command_id,
  };
}

function runtimeActionsForSnapshot(snapshot: CanvasRuntimeSnapshot): RuntimeActionDescriptor[] {
  return snapshot.runtime_bridge.surface?.actions ?? [];
}

function toJsonValue(value: unknown): JsonValue {
  if (value === null || typeof value === "string" || typeof value === "boolean") return value;
  if (typeof value === "number") return Number.isFinite(value) ? value : null;
  if (Array.isArray(value)) return value.map(toJsonValue);
  if (!isRecord(value)) return null;
  const result: { [key: string]: JsonValue } = {};
  for (const [key, item] of Object.entries(value)) {
    result[key] = toJsonValue(item);
  }
  return result;
}

function isJsonValue(value: unknown): value is JsonValue {
  if (value === null) return true;
  if (typeof value === "string" || typeof value === "boolean") return true;
  if (typeof value === "number") return Number.isFinite(value);
  if (Array.isArray(value)) return value.every(isJsonValue);
  return isJsonObject(value);
}

function isJsonObject(value: unknown): value is { [key: string]: JsonValue } {
  if (!isRecord(value)) return false;
  return Object.values(value).every(isJsonValue);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
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
