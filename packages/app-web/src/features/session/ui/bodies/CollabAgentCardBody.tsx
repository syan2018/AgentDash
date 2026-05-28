/**
 * 协作 Agent 工具调用 body
 */

import type { ThreadItem } from "../../../../generated/backbone-protocol";

type CollabItem = Extract<ThreadItem, { type: "collabAgentToolCall" }>;

export function CollabAgentCardBody({ item }: { item: CollabItem }) {
  return (
    <div className="space-y-2 text-xs">
      <div className="flex gap-4">
        <div>
          <p className="text-muted-foreground/60 font-medium">工具</p>
          <p className="font-mono text-foreground">{item.tool}</p>
        </div>
        <div>
          <p className="text-muted-foreground/60 font-medium">状态</p>
          <p className="text-foreground">{item.status}</p>
        </div>
      </div>
      {item.prompt && (
        <div>
          <p className="mb-1 text-muted-foreground/60 font-medium">提示</p>
          <p className="whitespace-pre-wrap text-foreground/80">{item.prompt}</p>
        </div>
      )}
      {item.model && (
        <div>
          <p className="text-muted-foreground/60 font-medium">模型</p>
          <p className="font-mono text-foreground/80">{item.model}</p>
        </div>
      )}
      {item.receiverThreadIds.length > 0 && (
        <div>
          <p className="mb-1 text-muted-foreground/60 font-medium">目标线程</p>
          <div className="flex flex-wrap gap-1">
            {item.receiverThreadIds.map((tid) => (
              <span key={tid} className="rounded bg-secondary px-1.5 py-0.5 font-mono text-[10px]">
                {tid.slice(0, 8)}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
