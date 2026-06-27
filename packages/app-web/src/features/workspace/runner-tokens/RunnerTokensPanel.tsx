import { useCallback, useEffect, useMemo, useState } from "react";
import { ConfirmDialog } from "@agentdash/ui";
import { formatDateTime } from "../../../lib/format";
import {
  createRunnerRegistrationToken,
  listRunnerRegistrationTokens,
  revokeRunnerRegistrationToken,
  rotateRunnerRegistrationToken,
  type RunnerRegistrationToken,
} from "../../../services/runnerRegistrationToken";
import { RunnerTokenRevealDialog } from "./RunnerTokenRevealDialog";

const TOKEN_STATUS_LABELS: Record<RunnerRegistrationToken["status"], string> = {
  active: "可用",
  expired: "已过期",
  revoked: "已撤销",
};

const TOKEN_STATUS_TONE: Record<RunnerRegistrationToken["status"], string> = {
  active: "border-success/30 bg-success/10 text-success",
  expired: "border-warning/30 bg-warning/10 text-warning",
  revoked: "border-border bg-secondary/40 text-muted-foreground",
};

interface RevealState {
  plaintextToken: string;
  tokenName: string;
  mode: "create" | "rotate";
}

export function RunnerTokensPanel({
  projectId,
  canEdit,
}: {
  projectId: string;
  canEdit: boolean;
}) {
  const [tokens, setTokens] = useState<RunnerRegistrationToken[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [newTokenName, setNewTokenName] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [reveal, setReveal] = useState<RevealState | null>(null);
  const [pendingRevokeToken, setPendingRevokeToken] = useState<RunnerRegistrationToken | null>(null);
  const [busyTokenId, setBusyTokenId] = useState<string | null>(null);

  const load = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      const next = await listRunnerRegistrationTokens(projectId);
      setTokens(next);
    } catch (loadError) {
      setError((loadError as Error).message);
    } finally {
      setIsLoading(false);
    }
  }, [projectId]);

  useEffect(() => {
    void load();
  }, [load]);

  const sortedTokens = useMemo(
    () =>
      [...tokens].sort(
        (a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
      ),
    [tokens],
  );

  const handleCreate = async () => {
    const name = newTokenName.trim();
    if (!name) {
      setError("请先填写接入令牌名称");
      return;
    }
    setIsCreating(true);
    setError(null);
    try {
      const result = await createRunnerRegistrationToken(projectId, {
        name,
        machine_policy: {},
      });
      setTokens((current) => [result.token, ...current.filter((item) => item.id !== result.token.id)]);
      setNewTokenName("");
      setReveal({
        plaintextToken: result.registration_token,
        tokenName: result.token.name,
        mode: "create",
      });
    } catch (createError) {
      setError((createError as Error).message);
    } finally {
      setIsCreating(false);
    }
  };

  const handleRotate = async (token: RunnerRegistrationToken) => {
    setBusyTokenId(token.id);
    setError(null);
    try {
      const result = await rotateRunnerRegistrationToken(projectId, token.id);
      setTokens((current) =>
        current.map((item) => (item.id === result.token.id ? result.token : item)),
      );
      setReveal({
        plaintextToken: result.registration_token,
        tokenName: result.token.name,
        mode: "rotate",
      });
    } catch (rotateError) {
      setError((rotateError as Error).message);
    } finally {
      setBusyTokenId(null);
    }
  };

  const handleRevoke = async (token: RunnerRegistrationToken) => {
    setBusyTokenId(token.id);
    setError(null);
    try {
      const result = await revokeRunnerRegistrationToken(projectId, token.id);
      setTokens((current) =>
        current.map((item) => (item.id === result.token.id ? result.token : item)),
      );
    } catch (revokeError) {
      setError((revokeError as Error).message);
    } finally {
      setBusyTokenId(null);
      setPendingRevokeToken(null);
    }
  };

  return (
    <div className="space-y-4">
      <p className="text-sm leading-6 text-muted-foreground">
        给一台没有界面、没有登录态的服务器签发接入令牌，复制 setup 命令即可把它作为「服务器 runner」接入本项目。
        令牌仅在创建/轮换时显示一次明文。
      </p>

      <div className="grid gap-3 md:grid-cols-[minmax(0,1fr)_auto]">
        <input
          value={newTokenName}
          onChange={(event) => setNewTokenName(event.target.value)}
          disabled={!canEdit || isCreating}
          placeholder="令牌名称，例如 build-server-01"
          className="agentdash-form-input disabled:cursor-not-allowed disabled:opacity-60"
        />
        <button
          type="button"
          onClick={() => void handleCreate()}
          disabled={!canEdit || isCreating || !newTokenName.trim()}
          className="agentdash-button-primary disabled:cursor-not-allowed disabled:opacity-60"
        >
          {isCreating ? "创建中…" : "创建接入令牌"}
        </button>
      </div>

      {isLoading && <p className="text-xs text-muted-foreground">正在加载接入令牌…</p>}

      {!isLoading && sortedTokens.length === 0 && (
        <p className="rounded-[12px] border border-dashed border-border px-4 py-4 text-sm text-muted-foreground">
          还没有接入令牌。创建一个即可获得 setup 命令，把新服务器接入本项目。
        </p>
      )}

      <div className="space-y-2">
        {sortedTokens.map((token) => {
          const busy = busyTokenId === token.id;
          const isActive = token.status === "active";
          return (
            <div key={token.id} className="rounded-[12px] border border-border bg-background px-4 py-3">
              <div className="flex flex-wrap items-start justify-between gap-3">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <p className="truncate text-sm font-medium text-foreground">{token.name}</p>
                    <span
                      className={`rounded-[8px] border px-2 py-0.5 text-[10px] ${TOKEN_STATUS_TONE[token.status]}`}
                    >
                      {TOKEN_STATUS_LABELS[token.status]}
                    </span>
                    <span className="rounded-[8px] border border-border bg-secondary/40 px-2 py-0.5 font-mono text-[10px] text-muted-foreground">
                      {token.token_prefix}…
                    </span>
                  </div>
                  <p className="mt-1 text-xs text-muted-foreground">
                    过期：{formatDateTime(token.expires_at)} · 最近使用：
                    {token.last_used_at ? formatDateTime(token.last_used_at) : "从未"}
                  </p>
                </div>
                <div className="flex flex-wrap gap-2">
                  <button
                    type="button"
                    onClick={() => void handleRotate(token)}
                    disabled={!canEdit || busy || !isActive}
                    className="rounded-[8px] border border-border bg-background px-3 py-1.5 text-xs text-foreground hover:bg-secondary disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    轮换
                  </button>
                  <button
                    type="button"
                    onClick={() => setPendingRevokeToken(token)}
                    disabled={!canEdit || busy || !isActive}
                    className="rounded-[8px] border border-destructive/25 bg-destructive/5 px-3 py-1.5 text-xs text-destructive hover:bg-destructive/10 disabled:cursor-not-allowed disabled:opacity-50"
                  >
                    撤销
                  </button>
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {error && (
        <div className="rounded-[12px] border border-destructive/40 bg-destructive/10 px-4 py-3 text-sm text-destructive">
          {error}
        </div>
      )}

      {reveal && (
        <RunnerTokenRevealDialog
          plaintextToken={reveal.plaintextToken}
          tokenName={reveal.tokenName}
          mode={reveal.mode}
          onClose={() => setReveal(null)}
        />
      )}

      <ConfirmDialog
        open={pendingRevokeToken !== null}
        title="撤销接入令牌"
        description={
          pendingRevokeToken
            ? `撤销后「${pendingRevokeToken.name}」将无法再用于接入新的 runner，已上线的 runner 不受影响。`
            : ""
        }
        confirmLabel="确认撤销"
        tone="danger"
        onClose={() => setPendingRevokeToken(null)}
        onConfirm={() => {
          if (pendingRevokeToken) {
            void handleRevoke(pendingRevokeToken);
          }
        }}
      />
    </div>
  );
}
