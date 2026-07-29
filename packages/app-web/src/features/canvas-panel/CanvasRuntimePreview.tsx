import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import type { JsonValue } from "../../generated/common-contracts";
import type { InteractionOperationRefDto } from "../../generated/interaction-contracts";
import { executeCanvasAction, invokeCanvasOperation } from "../../services/canvas";
import { submitCanvasAgentInput } from "../../services/canvas";
import { persistCanvasRendererObservation } from "../../services/canvas";
import type { AgentInputContent } from "../../generated/agent-runtime-contracts";
import type { AgentRunRuntimeTarget } from "../../services/agentRunRuntime";
import { readSurfaceFileBlob } from "../../services/vfs";
import type { CanvasRuntimeSnapshot } from "../../types";
import {
  buildPreviewDocument,
  createRuntimeAssetUrlCache,
  resolveRuntimeAssetUrl,
  revokeAllRuntimeAssetUrls,
  revokeRuntimeAssetUrl,
  type BuiltPreviewDocument,
  type RuntimeAssetUrlCache,
} from "./CanvasRuntimePreview.runtime";

const CONTRACT = "agentdash.canvas-host.v1";

export interface CanvasRuntimePreviewProps {
  snapshot: CanvasRuntimeSnapshot | null;
  agentRunTarget?: AgentRunRuntimeTarget | null;
}

type PreviewStatus = "idle" | "building" | "ready" | "error";

interface PreviewGeneration {
  frameId: string;
  generation: number;
  assetCache: RuntimeAssetUrlCache;
  port: MessagePort | null;
}

interface CanvasHostEnvelope {
  contract: typeof CONTRACT;
  kind: "connected" | "request" | "notification";
  frame_id: string;
  generation: number;
  request_id?: string;
  method?: string;
  payload?: unknown;
}

export function CanvasRuntimePreview({
  snapshot,
  agentRunTarget = null,
}: CanvasRuntimePreviewProps) {
  const iframeRef = useRef<HTMLIFrameElement | null>(null);
  const frameIdBase = `canvas-preview-${useId()}`;
  const generationSeqRef = useRef(0);
  const activeGenerationRef = useRef<PreviewGeneration | null>(null);
  const activeDocumentRef = useRef<BuiltPreviewDocument | null>(null);
  const snapshotRef = useRef<CanvasRuntimeSnapshot | null>(snapshot);
  const observationQueueRef = useRef<Promise<unknown>>(Promise.resolve());
  const [activeDocument, setActiveDocument] = useState<BuiltPreviewDocument | null>(null);
  const [status, setStatus] = useState<PreviewStatus>("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [buildError, setBuildError] = useState<string | null>(null);

  useEffect(() => {
    snapshotRef.current = snapshot;
  }, [snapshot]);

  useEffect(() => {
    let generation: PreviewGeneration | null = null;
    let built: BuiltPreviewDocument | null = null;
    const timer = window.setTimeout(() => {
      const previous = activeGenerationRef.current;
      activeGenerationRef.current = null;
      previous?.port?.close();
      if (previous) {
        revokeAllRuntimeAssetUrls(previous.assetCache);
      }
      activeDocumentRef.current?.dispose();
      activeDocumentRef.current = null;

      if (!snapshot) {
        setActiveDocument(null);
        setStatus("idle");
        setMessage(null);
        setBuildError(null);
        return;
      }

      const generationNumber = ++generationSeqRef.current;
      generation = {
        frameId: `${frameIdBase}-${generationNumber}`,
        generation: generationNumber,
        assetCache: createRuntimeAssetUrlCache(),
        port: null,
      };
      try {
        built = buildPreviewDocument(snapshot, generation.frameId, generation.generation);
        activeGenerationRef.current = generation;
        activeDocumentRef.current = built;
        setActiveDocument(built);
        setStatus("building");
        setMessage("正在连接 Canvas host");
        setBuildError(null);
      } catch (error) {
        setActiveDocument(null);
        setStatus("error");
        setMessage("Canvas 预览构建失败");
        setBuildError(error instanceof Error ? error.message : String(error));
      }
    }, 0);

    return () => {
      window.clearTimeout(timer);
      const active = activeGenerationRef.current;
      if (active === generation) {
        activeGenerationRef.current = null;
      }
      generation?.port?.close();
      if (generation) {
        revokeAllRuntimeAssetUrls(generation.assetCache);
      }
      if (activeDocumentRef.current === built) {
        built?.dispose();
        activeDocumentRef.current = null;
      }
    };
  }, [frameIdBase, snapshot]);

  const operationIndex = useMemo(() => {
    const index = new Map<string, CanvasRuntimeSnapshot["operations"][number]>();
    for (const operation of snapshot?.operations ?? []) {
      index.set(operationRefKey(operation.operation_ref), operation);
    }
    return index;
  }, [snapshot]);

  const handleRequest = useCallback(async (
    generation: PreviewGeneration,
    envelope: CanvasHostEnvelope,
  ): Promise<unknown> => {
    const current = snapshotRef.current;
    if (!current) {
      throw new Error("Canvas runtime snapshot 不可用");
    }
    const payload = asRecord(envelope.payload);
    switch (envelope.method) {
      case "operations.list":
        return current.operations;
      case "operations.describe": {
        const operationRef = parseOperationRef(payload.operation_ref);
        const descriptor = operationIndex.get(operationRefKey(operationRef));
        if (!descriptor) {
          throw new Error("Operation 不在当前 Canvas actor surface");
        }
        return descriptor;
      }
      case "operations.invoke": {
        const operationRef = parseOperationRef(payload.operation_ref);
        if (!operationIndex.has(operationRefKey(operationRef))) {
          throw new Error("Operation 不在当前 Canvas actor surface");
        }
        return invokeCanvasOperation({
          projectId: current.project_id,
          definitionId: current.canvas_id,
          instanceId: current.interaction_instance_id,
          operationRef,
          value: (payload.input ?? {}) as JsonValue,
          idempotencyKey:
            typeof payload.idempotency_key === "string"
              ? payload.idempotency_key
              : undefined,
        });
      }
      case "actions.invoke": {
        if (!current.interaction_instance_id) {
          throw new Error("当前 preview 未绑定 Interaction instance");
        }
        const result = await executeCanvasAction({
          instanceId: current.interaction_instance_id,
          actionKey: requireString(payload.action_key, "action key"),
          payload: (payload.payload ?? {}) as JsonValue,
          expectedStateRevision: parseStateRevision(
            payload.expected_state_revision,
            current.interaction_state_revision,
          ),
          agentRunTarget: agentRunTarget
            ? {
              runId: agentRunTarget.runId,
              agentId: agentRunTarget.agentId,
            }
            : null,
        });
        if (result.instance) {
          snapshotRef.current = {
            ...current,
            interaction_state: result.instance.state,
            interaction_state_revision: Number(result.instance.state_revision),
          };
        }
        return result;
      }
      case "assets.url": {
        const uri = requireString(payload.uri, "asset uri");
        if (!current.resource_surface_ref) {
          throw new Error("当前 Canvas 没有可用 resource surface");
        }
        return resolveRuntimeAssetUrl({
          surfaceRef: current.resource_surface_ref,
          uri,
          cache: generation.assetCache,
          readBlob: readSurfaceFileBlob,
        });
      }
      case "assets.revoke": {
        revokeRuntimeAssetUrl(
          generation.assetCache,
          requireString(payload.url, "asset url"),
        );
        return { revoked: true };
      }
      case "interaction.get_state":
        if (!current.interaction_instance_id) {
          throw new Error("当前 preview 未绑定 Interaction instance");
        }
        return {
          instance_id: current.interaction_instance_id,
          state: current.interaction_state ?? {},
          state_revision: current.interaction_state_revision ?? 0,
        };
      case "interaction.dispatch":
      case "interaction.emit": {
        if (!current.interaction_instance_id) {
          throw new Error("当前 preview 未绑定 Interaction instance");
        }
        const operationRef = parseOperationRef(payload.operation_ref);
        if (!operationIndex.has(operationRefKey(operationRef))) {
          throw new Error("Interaction Operation 不在当前 actor surface");
        }
        const expectedRevision = parseStateRevision(
          payload.expected_revision,
          current.interaction_state_revision,
        );
        const result = await invokeCanvasOperation({
          projectId: current.project_id,
          definitionId: current.canvas_id,
          instanceId: current.interaction_instance_id,
          operationRef,
          value: {
            instance_id: current.interaction_instance_id,
            command_id: crypto.randomUUID(),
            payload: (payload.payload ?? {}) as JsonValue,
            expected_state_revision: expectedRevision,
          },
          idempotencyKey: crypto.randomUUID(),
        });
        const record = asRecord(result);
        if ("state" in record && "state_revision" in record) {
          snapshotRef.current = {
            ...current,
            interaction_state: record.state as JsonValue,
            interaction_state_revision: parseStateRevision(record.state_revision),
          };
        }
        return result;
      }
      case "agent.submit":
        if (!current.interaction_instance_id || !agentRunTarget) {
          throw new Error("当前 preview 未绑定 AgentRun mailbox");
        }
        return submitCanvasAgentInput({
          instanceId: current.interaction_instance_id,
          runId: agentRunTarget.runId,
          agentId: agentRunTarget.agentId,
          clientCommandId: canvasSubmitCommandId(payload),
          content: canvasSubmitContent(payload),
          includeInteractionState: canvasSubmitFlag(
            payload,
            "include_interaction_state",
          ),
          includeRenderObservation: canvasSubmitFlag(
            payload,
            "include_render_observation",
          ),
        });
      default:
        throw new Error(`Canvas host method 不受支持: ${envelope.method ?? "unknown"}`);
    }
  }, [agentRunTarget, operationIndex]);

  const connectHost = useCallback(() => {
    const generation = activeGenerationRef.current;
    const target = iframeRef.current?.contentWindow;
    if (!generation || !target) {
      return;
    }
    generation.port?.close();
    const channel = new MessageChannel();
    generation.port = channel.port1;
    channel.port1.onmessage = (event: MessageEvent<unknown>) => {
      if (activeGenerationRef.current !== generation) {
        return;
      }
      const envelope = parseEnvelope(event.data);
      if (!envelope || !isCurrentEnvelope(generation, envelope)) {
        return;
      }
      if (envelope.kind === "connected") {
        setStatus("building");
        setMessage("Canvas host 已连接，正在启动 runtime");
        return;
      }
      if (envelope.kind === "notification") {
        if (envelope.method === "runtime.ready") {
          setStatus("ready");
          setMessage("Canvas runtime 已就绪");
        } else if (envelope.method === "runtime.error") {
          const payload = asRecord(envelope.payload);
          setStatus("error");
          setMessage(
            typeof payload.message === "string"
              ? payload.message
              : "Canvas runtime 启动失败",
          );
        } else if (envelope.method === "diagnostics.report") {
          const observation = asRecord(asRecord(envelope.payload).observation);
          if (observation.status === "error") {
            setStatus("error");
          }
          const instanceId = snapshotRef.current?.interaction_instance_id;
          if (instanceId) {
            observationQueueRef.current = observationQueueRef.current
              .catch(() => undefined)
              .then(() => persistCanvasRendererObservation({
                instanceId,
                frameId: generation.frameId,
                generation: generation.generation,
                observation: observation as JsonValue,
              }));
          }
        }
        return;
      }
      if (!envelope.request_id) {
        return;
      }
      void handleRequest(generation, envelope).then(
        (result) => respond(generation, envelope.request_id!, true, result),
        (error) => respond(
          generation,
          envelope.request_id!,
          false,
          undefined,
          error instanceof Error ? error.message : String(error),
        ),
      );
    };
    channel.port1.start();
    target.postMessage(
      {
        contract: CONTRACT,
        kind: "connect",
        frame_id: generation.frameId,
        generation: generation.generation,
      },
      "*",
      [channel.port2],
    );
  }, [handleRequest]);

  if (buildError) {
    return (
      <div className="m-4 space-y-2 rounded-[8px] border border-destructive/30 bg-destructive/10 p-4">
        <h4 className="text-sm font-semibold text-destructive">运行时预览构建失败</h4>
        <pre className="overflow-auto whitespace-pre-wrap text-xs text-destructive">
          {buildError}
        </pre>
      </div>
    );
  }

  if (!snapshot || !activeDocument) {
    return (
      <div className="flex flex-1 items-center justify-center px-4 text-sm text-muted-foreground">
        正在构建 Canvas 预览...
      </div>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="flex shrink-0 items-center justify-between border-b border-border/50 bg-secondary/10 px-3 py-1">
        <span className="text-[11px] text-muted-foreground">
          {message ?? "等待 Canvas runtime"}
        </span>
        <span className={statusClassName(status)}>{statusLabel(status)}</span>
      </div>
      {!snapshot.interaction_instance_id && (
        <div className="shrink-0 border-b border-border bg-secondary/30 px-3 py-1.5 text-[11px] text-muted-foreground">
          定义预览可调用当前用户 Operations；Interaction 与 Agent submit 需要实例 attachment。
        </div>
      )}
      <div className="min-h-0 flex-1">
        <iframe
          ref={iframeRef}
          title={`canvas-preview-${snapshot.canvas_id}`}
          sandbox="allow-scripts"
          referrerPolicy="no-referrer"
          srcDoc={activeDocument.srcDoc}
          onLoad={connectHost}
          className="h-full w-full border-0 bg-white"
        />
      </div>
    </div>
  );
}

function respond(
  generation: PreviewGeneration,
  requestId: string,
  ok: boolean,
  result?: unknown,
  error?: string,
): void {
  generation.port?.postMessage({
    contract: CONTRACT,
    kind: "response",
    frame_id: generation.frameId,
    generation: generation.generation,
    request_id: requestId,
    ok,
    result,
    error,
  });
}

function parseEnvelope(value: unknown): CanvasHostEnvelope | null {
  const record = asRecord(value);
  if (
    record.contract !== CONTRACT
    || !["connected", "request", "notification"].includes(String(record.kind))
    || typeof record.frame_id !== "string"
    || typeof record.generation !== "number"
  ) {
    return null;
  }
  return record as unknown as CanvasHostEnvelope;
}

function isCurrentEnvelope(
  generation: PreviewGeneration,
  envelope: CanvasHostEnvelope,
): boolean {
  return envelope.frame_id === generation.frameId
    && envelope.generation === generation.generation
    && activeEnvelopeGeneration(generation);
}

function activeEnvelopeGeneration(generation: PreviewGeneration): boolean {
  return generation.port !== null;
}

function parseOperationRef(value: unknown): InteractionOperationRefDto {
  const record = asRecord(value);
  if (
    typeof record.namespace !== "string"
    || typeof record.provider_key !== "string"
    || typeof record.operation_key !== "string"
    || typeof record.contract_version !== "number"
  ) {
    throw new Error("OperationRef 不完整");
  }
  return {
    namespace: record.namespace,
    provider_key: record.provider_key,
    operation_key: record.operation_key,
    contract_version: record.contract_version,
  };
}

function operationRefKey(ref: InteractionOperationRefDto): string {
  return `${ref.namespace}\n${ref.provider_key}\n${ref.operation_key}\n${ref.contract_version}`;
}

function asRecord(value: unknown): Record<string, unknown> {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
}

function requireString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.trim() === "") {
    throw new Error(`${label} 必须是非空字符串`);
  }
  return value;
}

function parseStateRevision(value: unknown, fallback?: number): number {
  if (typeof value === "bigint" && value >= 0n && value <= BigInt(Number.MAX_SAFE_INTEGER)) {
    return Number(value);
  }
  if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) {
    return value;
  }
  if (typeof value === "string" && /^\d+$/.test(value)) {
    const parsed = Number(value);
    if (Number.isSafeInteger(parsed)) return parsed;
  }
  if (fallback !== undefined) return fallback;
  throw new Error("Interaction expected state revision 无效");
}

function canvasSubmitContent(payload: Record<string, unknown>): AgentInputContent[] {
  const value = asRecord(payload.input);
  if (Array.isArray(value.input) && value.input.length > 0) {
    return value.input as AgentInputContent[];
  }
  const text = typeof value.text === "string" ? value.text.trim() : "";
  if (!text) {
    throw new Error("agent.submit 需要 text 或 canonical input");
  }
  return [{ kind: "text", text }];
}

function canvasSubmitCommandId(payload: Record<string, unknown>): string {
  const value = asRecord(payload.input);
  const supplied = typeof value.client_command_id === "string"
    ? value.client_command_id.trim()
    : "";
  return supplied || `canvas-agent-${crypto.randomUUID()}`;
}

function canvasSubmitFlag(
  payload: Record<string, unknown>,
  key: "include_interaction_state" | "include_render_observation",
): boolean {
  return asRecord(payload.input)[key] === true;
}

function statusLabel(status: PreviewStatus): string {
  switch (status) {
    case "idle":
      return "空闲";
    case "building":
      return "启动中";
    case "ready":
      return "就绪";
    case "error":
      return "错误";
  }
}

function statusClassName(status: PreviewStatus): string {
  const color = status === "ready"
    ? "text-emerald-600"
    : status === "error"
      ? "text-destructive"
      : "text-muted-foreground";
  return `text-[11px] font-medium ${color}`;
}
