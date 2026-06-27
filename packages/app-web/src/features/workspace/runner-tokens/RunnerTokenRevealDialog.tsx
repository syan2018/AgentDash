import { useMemo, useState } from "react";
import {
  buildRunnerSetupCommand,
  defaultRunnerWorkspaceRoot,
  resolveCloudOrigin,
} from "./setupCommand";

export interface RunnerTokenRevealDialogProps {
  /** 一次性明文 token（仅创建/轮换返回时存在）。 */
  plaintextToken: string;
  /** token 名称，用作默认 runner 名称。 */
  tokenName: string;
  /** "create" | "rotate"，用于标题文案。 */
  mode: "create" | "rotate";
  onClose: () => void;
}

export function RunnerTokenRevealDialog({
  plaintextToken,
  tokenName,
  mode,
  onClose,
}: RunnerTokenRevealDialogProps) {
  const [runnerName, setRunnerName] = useState(tokenName);
  const [workspaceRoot, setWorkspaceRoot] = useState(defaultRunnerWorkspaceRoot());
  const [copiedToken, setCopiedToken] = useState(false);
  const [copiedCommand, setCopiedCommand] = useState(false);

  const origin = useMemo(() => resolveCloudOrigin(), []);
  const setupCommand = useMemo(
    () =>
      buildRunnerSetupCommand({
        origin,
        token: plaintextToken,
        runnerName,
        workspaceRoot,
      }),
    [origin, plaintextToken, runnerName, workspaceRoot],
  );

  const handleCopyToken = async () => {
    await navigator.clipboard.writeText(plaintextToken);
    setCopiedToken(true);
    setTimeout(() => setCopiedToken(false), 2000);
  };

  const handleCopyCommand = async () => {
    await navigator.clipboard.writeText(setupCommand);
    setCopiedCommand(true);
    setTimeout(() => setCopiedCommand(false), 2000);
  };

  return (
    <>
      <div className="fixed inset-0 z-[90] bg-foreground/24 backdrop-blur-[2px]" onClick={onClose} />
      <div className="fixed inset-0 z-[91] flex items-center justify-center p-4">
        <div className="w-full max-w-xl rounded-[12px] border border-warning/30 bg-background shadow-2xl">
          <div className="border-b border-warning/20 bg-warning/5 px-5 py-4">
            <span className="inline-block rounded-[6px] border border-warning/30 bg-warning/10 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wider text-warning">
              接入令牌
            </span>
            <h4 className="mt-1 text-base font-semibold text-foreground">
              {tokenName} — {mode === "rotate" ? "已轮换，新令牌仅此一次可见" : "令牌仅此一次可见"}
            </h4>
          </div>
          <div className="space-y-4 p-5">
            <div>
              <p className="text-xs font-medium text-muted-foreground">接入令牌（明文）</p>
              <div className="mt-1 flex items-center gap-2">
                <code
                  data-testid="runner-token-plaintext"
                  className="flex-1 rounded-[8px] border border-border bg-secondary/50 px-3 py-2 font-mono text-xs text-foreground break-all"
                >
                  {plaintextToken}
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

            <div className="grid gap-3 md:grid-cols-2">
              <div>
                <label className="agentdash-form-label">Runner 名称</label>
                <input
                  value={runnerName}
                  onChange={(event) => setRunnerName(event.target.value)}
                  className="agentdash-form-input"
                  placeholder="这台服务器的名称"
                />
              </div>
              <div>
                <label className="agentdash-form-label">Workspace 根目录</label>
                <input
                  value={workspaceRoot}
                  onChange={(event) => setWorkspaceRoot(event.target.value)}
                  className="agentdash-form-input"
                  placeholder={defaultRunnerWorkspaceRoot()}
                />
              </div>
            </div>

            <div>
              <p className="text-xs font-medium text-muted-foreground">
                在目标服务器上执行（云端地址 {origin || "未知，请检查 API 配置"}）
              </p>
              <div className="relative mt-1">
                <pre
                  data-testid="runner-setup-command"
                  className="rounded-[8px] border border-border bg-secondary/50 px-3 py-2 pr-16 font-mono text-[11px] text-foreground whitespace-pre-wrap break-all"
                >
                  {setupCommand}
                </pre>
                <button
                  type="button"
                  onClick={() => void handleCopyCommand()}
                  className="absolute top-2 right-2 rounded-[6px] border border-border bg-background px-2 py-0.5 text-[10px] text-muted-foreground transition-colors hover:bg-secondary"
                >
                  {copiedCommand ? "已复制" : "复制命令"}
                </button>
              </div>
            </div>

            <p className="rounded-[8px] border border-warning/20 bg-warning/5 px-3 py-2 text-xs text-warning">
              此令牌不会再次展示，请立即复制 setup 命令或令牌并安全保管。
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
