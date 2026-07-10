import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  clampComponentSize,
  componentConnectMessage,
  componentHostMessage,
  ComponentEventRateGate,
  parseComponentMessage,
  validateComponentPayload,
  type ExtensionUiComponentDescriptor,
} from "../model/componentProtocol";

const COMPONENT_CSP = "default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'";

export interface ExtensionInteractionComponentProps {
  descriptor: ExtensionUiComponentDescriptor;
  artifactSrc: string;
  componentInstanceId: string;
  props: unknown;
  stateProjection: unknown;
  theme: "light" | "dark";
  locale: string;
  onEvent(eventType: string, payload: unknown): Promise<unknown> | unknown;
  onDiagnostic?(level: "debug" | "info" | "warn" | "error", message: string): void;
}

export function ExtensionInteractionComponent({
  descriptor,
  artifactSrc,
  componentInstanceId,
  props,
  stateProjection,
  theme,
  locale,
  onEvent,
  onDiagnostic,
}: ExtensionInteractionComponentProps) {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const portRef = useRef<MessagePort | null>(null);
  const eventGateRef = useRef(new ComponentEventRateGate());
  const [ready, setReady] = useState(false);
  const [size, setSize] = useState(() => ({
    width: descriptor.sizing.min_width,
    height: descriptor.sizing.min_height,
  }));
  const contractError = useMemo(() => {
    if (descriptor.contract_version !== 1 || descriptor.sandbox_profile !== "isolated_v1") {
      return "Extension component contract 不受支持";
    }
    if (!validateComponentPayload(descriptor.props_schema, props)) {
      return "Extension component props 不符合 descriptor schema";
    }
    if (!validateComponentPayload(descriptor.state_projection_schema, stateProjection)) {
      return "Extension component state projection 不符合 descriptor schema";
    }
    return null;
  }, [descriptor, props, stateProjection]);

  const connect = useCallback(() => {
    const frameWindow = iframeRef.current?.contentWindow;
    if (!frameWindow || contractError) return;
    portRef.current?.close();
    const channel = new MessageChannel();
    portRef.current = channel.port1;
    eventGateRef.current = new ComponentEventRateGate();
    setReady(false);
    channel.port1.onmessage = (event: MessageEvent<unknown>) => {
      const message = parseComponentMessage(event.data);
      if (!message) return;
      if (message.kind === "ready") {
        setReady(true);
        return;
      }
      if (message.kind === "resize") {
        setSize(clampComponentSize(descriptor, message.width, message.height));
        return;
      }
      if (message.kind === "diagnostic") {
        onDiagnostic?.(message.level, message.message);
        return;
      }
      if (!eventGateRef.current.admit(performance.now())) {
        channel.port1.postMessage(componentHostMessage("binding_result", {
          request_id: message.request_id,
          error: "component event rate limit exceeded",
        }));
        return;
      }
      const schema = descriptor.events_schema[message.event_type];
      if (!schema || !validateComponentPayload(schema, message.payload)) {
        channel.port1.postMessage(componentHostMessage("binding_result", {
          request_id: message.request_id,
          error: "component event schema rejected",
        }));
        return;
      }
      void Promise.resolve(onEvent(message.event_type, message.payload))
        .then((result) => channel.port1.postMessage(componentHostMessage("binding_result", {
          request_id: message.request_id,
          result,
        })))
        .catch((error: unknown) => channel.port1.postMessage(componentHostMessage("binding_result", {
          request_id: message.request_id,
          error: error instanceof Error ? error.message : "component binding failed",
        })));
    };
    channel.port1.start();
    frameWindow.postMessage(componentConnectMessage(), "*", [channel.port2]);
    channel.port1.postMessage(componentHostMessage("initialize", {
      component_instance_id: componentInstanceId,
      component_key: descriptor.component_key,
      contract_version: descriptor.contract_version,
      props,
      state_projection: stateProjection,
      theme,
      locale,
    }));
  }, [componentInstanceId, contractError, descriptor, locale, onDiagnostic, onEvent, props, stateProjection, theme]);

  useEffect(() => () => {
    portRef.current?.postMessage(componentHostMessage("dispose", {}));
    portRef.current?.close();
    portRef.current = null;
  }, []);

  useEffect(() => {
    if (!ready) return;
    portRef.current?.postMessage(componentHostMessage("projection", {
      props,
      state_projection: stateProjection,
      theme,
      locale,
    }));
  }, [locale, props, ready, stateProjection, theme]);

  if (contractError) {
    return <ComponentUnavailable detail={contractError} />;
  }

  return (
    <iframe
      ref={iframeRef}
      title={descriptor.component_key}
      src={artifactSrc}
      sandbox="allow-scripts"
      referrerPolicy="no-referrer"
      onLoad={connect}
      style={{ width: size.width, height: size.height }}
      className="max-h-full max-w-full border-0 bg-transparent"
      data-component-key={descriptor.component_key}
      data-component-ready={ready ? "true" : "false"}
      {...({ csp: COMPONENT_CSP } as Record<string, string>)}
    />
  );
}

function ComponentUnavailable({ detail }: { detail: string }) {
  return (
    <div className="rounded-[8px] border border-border bg-secondary/25 px-3 py-2 text-xs text-muted-foreground">
      {detail}
    </div>
  );
}
