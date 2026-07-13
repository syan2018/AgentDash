import { createContext, useContext, useState, type ReactNode } from "react";
import type { CommandAvailability, InteractionResponse, RuntimeInteractionKind } from "../../../generated/agent-runtime-contracts";
import { interactionResponseFromText } from "../../agent-run-workspace/model/interactionResponse";

interface RuntimeInteractionActions {
  availability?: CommandAvailability;
  resolve?: (interactionId: string, response: InteractionResponse) => Promise<void>;
}

const Context = createContext<RuntimeInteractionActions>({});

export function SessionRuntimeInteractionProvider({ children, availability, resolve }: RuntimeInteractionActions & { children: ReactNode }) {
  return <Context.Provider value={{ availability, resolve }}>{children}</Context.Provider>;
}

export function SessionRuntimeInteractionCard({ interactionId, kind }: { interactionId: string; kind: RuntimeInteractionKind }) {
  const actions = useContext(Context);
  const [input, setInput] = useState("");
  const [submitted, setSubmitted] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const available = actions.availability?.status === "available" && actions.resolve != null;
  const approval = kind === "command_approval" || kind === "file_change_approval" || kind === "permission_approval";
  const submit = async (response: InteractionResponse) => {
    if (!actions.resolve) return;
    try { await actions.resolve(interactionId, response); setSubmitted(true); setError(null); }
    catch (cause) { setError(cause instanceof Error ? cause.message : "interaction response 提交失败"); }
  };
  const submitText = () => {
    const parsed = interactionResponseFromText(kind, input);
    if (!parsed.ok) { setError(parsed.error); return; }
    void submit(parsed.response);
  };
  return (
    <div className="rounded-[8px] border border-warning/25 bg-warning/5 px-3 py-2 text-xs">
      <div className="font-medium text-warning">等待交互 · {kind}</div>
      <div className="mt-1 break-all font-mono text-[10px] text-muted-foreground">{interactionId}</div>
      {approval ? (
        <div className="mt-2 flex gap-2">
          <button type="button" disabled={!available || submitted} onClick={() => void submit({ kind: "approved" })} className="rounded-[6px] border border-success/30 px-2 py-1 text-success disabled:opacity-50">批准</button>
          <button type="button" disabled={!available || submitted} onClick={() => void submit({ kind: "denied", reason: null })} className="rounded-[6px] border border-warning/30 px-2 py-1 text-warning disabled:opacity-50">拒绝</button>
        </div>
      ) : (
        <div className="mt-2 space-y-2">
          <textarea value={input} onChange={(event) => setInput(event.target.value)} disabled={!available || submitted} className="min-h-16 w-full rounded-[6px] border border-input bg-background p-2" />
          <button type="button" disabled={!available || submitted} onClick={submitText} className="rounded-[6px] border border-border px-2 py-1 disabled:opacity-50">提交响应</button>
        </div>
      )}
      {!available && <div className="mt-2 text-warning">当前 Runtime 未声明 interaction response 可用。</div>}
      {submitted && <div className="mt-2 text-success">响应已提交</div>}
      {error && <div className="mt-2 text-destructive">{error}</div>}
    </div>
  );
}
