export type JsonPrimitive = string | number | boolean | null;
export type JsonValue = JsonPrimitive | JsonValue[] | { [key: string]: JsonValue };
export type JsonObject = { [key: string]: JsonValue };

export interface ExtensionPanelContext extends JsonObject {
  project_id: string;
  extension_id: string;
  extension_key: string;
  panel_type_id: string;
  uri: string;
}

export interface ExtensionBridgeOptions {
  target?: Window;
  origin?: string;
  timeout_ms?: number;
}

export interface ExtensionEventEnvelope<TPayload extends JsonValue = JsonValue> {
  type: string;
  payload: TPayload;
}

export interface ExtensionBridge {
  invokeAction<TInput extends JsonValue, TOutput extends JsonValue>(
    actionKey: string,
    input: TInput,
  ): Promise<TOutput>;
  invokeProtocol<TInput extends JsonValue, TOutput extends JsonValue>(
    protocolKey: string,
    method: string,
    input: TInput,
    options?: { dependency_alias?: string },
  ): Promise<TOutput>;
  openWorkspaceTab(typeId: string, uri: string): Promise<void>;
  vfs: {
    read(path: string): Promise<string>;
    write(path: string, content: string): Promise<void>;
  };
  events: {
    emit<TPayload extends JsonValue>(event: ExtensionEventEnvelope<TPayload>): void;
    subscribe<TPayload extends JsonValue>(
      type: string,
      handler: (payload: TPayload) => void,
    ): () => void;
  };
  metadata: {
    getContext(): Promise<ExtensionPanelContext>;
  };
}

type PendingRequest = {
  resolve(value: JsonValue): void;
  reject(error: Error): void;
  timeout_id: number;
};

type EventHandler = (payload: JsonValue) => void;

const DEFAULT_TIMEOUT_MS = 30_000;

export function createExtensionBridge(options: ExtensionBridgeOptions = {}): ExtensionBridge {
  const target = options.target ?? window.parent;
  const origin = options.origin ?? "*";
  const timeoutMs = options.timeout_ms ?? DEFAULT_TIMEOUT_MS;
  const pending = new Map<string, PendingRequest>();
  const handlers = new Map<string, Set<EventHandler>>();

  window.addEventListener("message", (event) => {
    const message = asRecord(event.data);
    if (!message || message.channel !== "agentdash.extension") return;
    if (message.kind === "response") {
      const requestId = typeof message.request_id === "string" ? message.request_id : "";
      const request = pending.get(requestId);
      if (!request) return;
      pending.delete(requestId);
      window.clearTimeout(request.timeout_id);
      if (typeof message.error === "string" && message.error.trim() !== "") {
        request.reject(new Error(message.error));
        return;
      }
      request.resolve(toJsonValue(message.result));
      return;
    }
    if (message.kind === "event") {
      const type = typeof message.type === "string" ? message.type : "";
      const subscribers = handlers.get(type);
      if (!subscribers) return;
      dispatchEvent(type, toJsonValue(message.payload));
    }
  });

  function dispatchEvent(type: string, payload: JsonValue) {
    const subscribers = handlers.get(type);
    if (!subscribers) return;
    for (const handler of subscribers) {
      handler(payload);
    }
  }

  function request<TOutput extends JsonValue>(method: string, params: JsonObject): Promise<TOutput> {
    const requestId = crypto.randomUUID();
    return new Promise<TOutput>((resolve, reject) => {
      const timeoutId = window.setTimeout(() => {
        pending.delete(requestId);
        reject(new Error(`AgentDash extension bridge request timed out: ${method}`));
      }, timeoutMs);
      pending.set(requestId, {
        resolve(value) {
          resolve(value as TOutput);
        },
        reject,
        timeout_id: timeoutId,
      });
      target.postMessage(
        {
          channel: "agentdash.extension",
          kind: "request",
          request_id: requestId,
          method,
          params,
        },
        origin,
      );
    });
  }

  return {
    invokeAction(actionKey, input) {
      return request("runtime.invoke_action", { action_key: actionKey, input });
    },
    invokeProtocol(protocolKey, method, input, options = {}) {
      return request("extension.invoke_protocol", {
        protocol_key: protocolKey,
        method,
        input,
        dependency_alias: options.dependency_alias ?? null,
      });
    },
    async openWorkspaceTab(typeId, uri) {
      await request("workspace.open_tab", { type_id: typeId, uri });
    },
    vfs: {
      read(path) {
        return request<string>("vfs.read", { path });
      },
      async write(path, content) {
        await request("vfs.write", { path, content });
      },
    },
    events: {
      emit(event) {
        dispatchEvent(event.type, toJsonValue(event.payload));
      },
      subscribe(type, handler) {
        const subscribers = handlers.get(type) ?? new Set<EventHandler>();
        subscribers.add(handler as EventHandler);
        handlers.set(type, subscribers);
        return () => {
          subscribers.delete(handler as EventHandler);
          if (subscribers.size === 0) {
            handlers.delete(type);
          }
        };
      },
    },
    metadata: {
      getContext() {
        return request<ExtensionPanelContext>("metadata.get_context", {});
      },
    },
  };
}

function asRecord(raw: unknown): Record<string, unknown> | null {
  return raw != null && typeof raw === "object" && !Array.isArray(raw)
    ? raw as Record<string, unknown>
    : null;
}

function toJsonValue(raw: unknown): JsonValue {
  if (raw === null || typeof raw === "string" || typeof raw === "boolean") return raw;
  if (typeof raw === "number") return Number.isFinite(raw) ? raw : null;
  if (Array.isArray(raw)) return raw.map(toJsonValue);
  const record = asRecord(raw);
  if (!record) return null;
  const result: JsonObject = {};
  for (const [key, value] of Object.entries(record)) {
    result[key] = toJsonValue(value);
  }
  return result;
}

export * from "./fetch-route.js";
