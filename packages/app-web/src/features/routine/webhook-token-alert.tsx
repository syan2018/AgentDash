import { useState } from "react";

export function WebhookTokenAlert({
  token,
  endpointId,
  routineName,
  onClose,
}: {
  token: string;
  endpointId: string;
  routineName: string;
  onClose: () => void;
}) {
  const [copiedToken, setCopiedToken] = useState(false);
  const [copiedCurl, setCopiedCurl] = useState(false);
  const triggerPath = `/api/routine-triggers/${endpointId}/fire`;
  const fullUrl = `${window.location.origin}${triggerPath}`;

  const curlExample = `curl -X POST ${fullUrl} \\\n  -H "Authorization: Bearer ${token}" \\\n  -H "Content-Type: application/json" \\\n  -d '{"message": "hello"}'`;

  const handleCopyToken = async () => {
    await navigator.clipboard.writeText(token);
    setCopiedToken(true);
    setTimeout(() => setCopiedToken(false), 2000);
  };

  const handleCopyCurl = async () => {
    await navigator.clipboard.writeText(curlExample);
    setCopiedCurl(true);
    setTimeout(() => setCopiedCurl(false), 2000);
  };

  return (
    <>
      <div className="fixed inset-0 z-[90] bg-foreground/24 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-lg rounded-[12px] border border-warning/30 bg-background shadow-2xl">
          <div className="border-b border-warning/20 bg-warning/5 px-5 py-4">
            <span className="inline-block rounded-[6px] border border-warning/30 bg-warning/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-warning">
              Webhook Token
            </span>
            <h4 className="mt-1 text-base font-semibold text-foreground">
              {routineName} — Token 仅此一次可见
            </h4>
          </div>
          <div className="space-y-4 p-5">
            <div>
              <p className="text-xs font-medium text-muted-foreground">触发端点</p>
              <code className="mt-1 block rounded-[8px] border border-border bg-secondary/50 px-3 py-2 font-mono text-xs text-foreground break-all">
                POST {fullUrl}
              </code>
            </div>
            <div>
              <p className="text-xs font-medium text-muted-foreground">Bearer Token</p>
              <div className="mt-1 flex items-center gap-2">
                <code className="flex-1 rounded-[8px] border border-border bg-secondary/50 px-3 py-2 font-mono text-xs text-foreground break-all">
                  {token}
                </code>
                <button
                  type="button"
                  onClick={() => void handleCopyToken()}
                  className="agentdash-button-secondary shrink-0 text-xs"
                >
                  {copiedToken ? "已复制" : "复制"}
                </button>
              </div>
            </div>
            <div>
              <p className="text-xs font-medium text-muted-foreground">调用示例</p>
              <div className="relative mt-1">
                <pre className="rounded-[8px] border border-border bg-secondary/50 px-3 py-2 font-mono text-[11px] text-foreground whitespace-pre-wrap break-all">
                  {curlExample}
                </pre>
                <button
                  type="button"
                  onClick={() => void handleCopyCurl()}
                  className="absolute top-2 right-2 rounded-[6px] border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground transition-colors hover:bg-secondary"
                >
                  {copiedCurl ? "已复制" : "复制"}
                </button>
              </div>
            </div>
            <p className="rounded-[8px] border border-warning/20 bg-warning/5 px-3 py-2 text-xs text-warning">
              此 Token 不会再次展示，请立即复制并安全保管。
            </p>
          </div>
          <div className="flex items-center justify-end border-t border-border px-5 py-4">
            <button type="button" onClick={onClose} className="agentdash-button-primary">
              我已复制，关闭
            </button>
          </div>
        </div>
      </div>
    </>
  );
}
