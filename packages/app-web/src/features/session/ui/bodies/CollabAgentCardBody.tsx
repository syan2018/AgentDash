/**
 * 协作 Agent 工具调用 body
 */

import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { useDebugPrefs } from "../../../../hooks/use-debug-prefs";
import { CB } from "./cardBodyTokens";

type CollabItem = Extract<ThreadItem, { type: "collabAgentToolCall" }>;

export function CollabAgentCardBody({ item }: { item: CollabItem }) {
  const { prefs } = useDebugPrefs();
  const rawProtocol = {
    sender_thread_id: item.senderThreadId,
    receiver_thread_ids: item.receiverThreadIds,
  };

  return (
    <div className={`${CB.sectionGap} text-xs`}>
      <div className="flex gap-4">
        <div>
          <p className={CB.sectionTitle}>工具</p>
          <p className="font-mono text-foreground/80">{item.tool}</p>
        </div>
        <div>
          <p className={CB.sectionTitle}>状态</p>
          <p className="text-foreground/80">{item.status}</p>
        </div>
      </div>
      {item.prompt && (
        <div>
          <p className={`mb-0.5 ${CB.sectionTitle}`}>提示</p>
          <p className="whitespace-pre-wrap text-foreground/80">{item.prompt}</p>
        </div>
      )}
      {item.model && (
        <div>
          <p className={CB.sectionTitle}>模型</p>
          <p className="font-mono text-foreground/80">{item.model}</p>
        </div>
      )}
      {prefs.hookVerbose && item.receiverThreadIds.length > 0 && (
        <div>
          <p className={`mb-1 ${CB.sectionTitle}`}>Raw protocol</p>
          <pre className={`${CB.codeBlock} max-h-40 overflow-auto whitespace-pre-wrap break-words`}>
            {JSON.stringify(rawProtocol, null, 2)}
          </pre>
        </div>
      )}
    </div>
  );
}
