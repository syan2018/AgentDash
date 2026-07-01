import { useCallback, useEffect, useRef, useState } from "react";
import { cn } from "@agentdash/ui";
import { hasDesktopExternalBrowserOpener, openDesktopExternalBrowser } from "../../../desktop/externalBrowser";
import { btnPrimaryCls } from "./primitives";

type OAuthLoginStatus = "idle" | "starting" | "waiting" | "completed" | "failed";

export interface OAuthLoginStartResponse {
  flow_id: string;
  auth_url: string;
  expires_at: string;
}

export interface OAuthLoginStatusResponse {
  flow_id: string;
  status: "pending" | "completed" | "failed";
  message?: string | null;
}

interface OAuthLoginWizardProps {
  start: () => Promise<OAuthLoginStartResponse>;
  getStatus: (flowId: string) => Promise<OAuthLoginStatusResponse>;
  cancel: (flowId: string) => Promise<OAuthLoginStatusResponse>;
  onCompleted?: () => Promise<void> | void;
  idleLabel: string;
  startingLabel?: string;
  waitingLabel?: string;
  cancelLabel?: string;
  authLinkLabel?: string;
  openedMessage?: string;
  manualMessage?: string;
  completedMessage?: string;
  failedMessage?: string;
  disabled?: boolean;
  disabledMessage?: string;
  className?: string;
  buttonClassName?: string;
  surface?: "panel" | "inline";
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    window.setTimeout(resolve, ms);
  });
}

function loginButtonLabel(
  status: OAuthLoginStatus,
  idleLabel: string,
  startingLabel: string,
  waitingLabel: string,
): string {
  if (status === "starting") return startingLabel;
  if (status === "waiting") return waitingLabel;
  return idleLabel;
}

export function OAuthLoginWizard({
  start,
  getStatus,
  cancel,
  onCompleted,
  idleLabel,
  startingLabel = "启动中…",
  waitingLabel = "登录中…",
  cancelLabel = "取消",
  authLinkLabel = "打开授权页",
  openedMessage = "已在外部浏览器打开授权页，等待授权完成…",
  manualMessage = "请打开授权页并完成登录，完成后这里会自动更新状态。",
  completedMessage = "登录已完成",
  failedMessage = "登录失败",
  disabled = false,
  disabledMessage,
  className,
  buttonClassName = btnPrimaryCls,
  surface = "panel",
}: OAuthLoginWizardProps) {
  const [status, setStatus] = useState<OAuthLoginStatus>("idle");
  const [message, setMessage] = useState<string | null>(null);
  const [authUrl, setAuthUrl] = useState<string | null>(null);
  const flowIdRef = useRef<string | null>(null);
  const pollCancelledRef = useRef(false);

  useEffect(() => {
    return () => {
      pollCancelledRef.current = true;
      const flowId = flowIdRef.current;
      flowIdRef.current = null;
      if (flowId) {
        void cancel(flowId).catch(() => undefined);
      }
    };
  }, [cancel]);

  const pollFlow = useCallback(async (flowId: string) => {
    while (!pollCancelledRef.current) {
      await sleep(1200);
      if (pollCancelledRef.current) return;

      const nextStatus = await getStatus(flowId);
      if (nextStatus.status === "pending") continue;

      flowIdRef.current = null;
      if (nextStatus.status === "completed") {
        setStatus("completed");
        setMessage(nextStatus.message ?? completedMessage);
        await onCompleted?.();
        return;
      }

      setStatus("failed");
      setMessage(nextStatus.message ?? failedMessage);
      return;
    }
  }, [completedMessage, failedMessage, getStatus, onCompleted]);

  const handleStart = useCallback(async () => {
    pollCancelledRef.current = false;
    setStatus("starting");
    setMessage(null);
    setAuthUrl(null);
    try {
      const flow = await start();
      flowIdRef.current = flow.flow_id;
      setAuthUrl(flow.auth_url);
      setStatus("waiting");
      if (hasDesktopExternalBrowserOpener()) {
        const opened = await openDesktopExternalBrowser(flow.auth_url);
        setMessage(opened ? openedMessage : manualMessage);
      } else {
        setMessage(manualMessage);
      }
      void pollFlow(flow.flow_id).catch((error: unknown) => {
        flowIdRef.current = null;
        setStatus("failed");
        setMessage(error instanceof Error ? error.message : String(error));
      });
    } catch (error) {
      const flowId = flowIdRef.current;
      flowIdRef.current = null;
      if (flowId) {
        await cancel(flowId).catch(() => undefined);
      }
      setStatus("failed");
      setMessage(error instanceof Error ? error.message : String(error));
    }
  }, [cancel, manualMessage, openedMessage, pollFlow, start]);

  const handleCancel = useCallback(async () => {
    pollCancelledRef.current = true;
    const flowId = flowIdRef.current;
    flowIdRef.current = null;
    if (flowId) {
      await cancel(flowId).catch(() => undefined);
    }
    setStatus("idle");
    setMessage(null);
    setAuthUrl(null);
  }, [cancel]);

  const buttonLabel = loginButtonLabel(status, idleLabel, startingLabel, waitingLabel);
  const rootClassName = surface === "panel"
    ? "rounded-[8px] border border-border bg-muted/20 p-3"
    : "space-y-2";
  const visibleMessage = message ?? (disabled ? disabledMessage ?? null : null);

  return (
    <div className={cn(rootClassName, className)}>
      <div className="flex flex-wrap items-center gap-2">
        <button
          type="button"
          className={buttonClassName}
          disabled={disabled || status === "starting" || status === "waiting"}
          onClick={() => void handleStart()}
        >
          {buttonLabel}
        </button>
        {status === "waiting" && (
          <button
            type="button"
            className="rounded-[8px] border border-border px-3 py-2 text-sm text-muted-foreground transition-colors hover:bg-secondary hover:text-foreground"
            onClick={() => void handleCancel()}
          >
            {cancelLabel}
          </button>
        )}
        {authUrl && status === "waiting" && (
          <a
            className="text-xs text-primary hover:underline"
            href={authUrl}
            target="_blank"
            rel="noreferrer"
          >
            {authLinkLabel}
          </a>
        )}
      </div>
      {visibleMessage && (
        <p className={cn("mt-2 text-xs", status === "failed" ? "text-destructive" : "text-muted-foreground")}>
          {visibleMessage}
        </p>
      )}
    </div>
  );
}
