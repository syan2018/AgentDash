/**
 * 协作 Agent 工具调用 body
 */

import type { ThreadItem } from "../../../../generated/backbone-protocol";
import { CB } from "./cardBodyTokens";

type CollabItem = Extract<ThreadItem, { type: "collabAgentToolCall" }>;

export function CollabAgentCardBody({ item }: { item: CollabItem }) {
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
      {item.receiverThreadIds.length > 0 && (
        <div>
          <p className={`mb-0.5 ${CB.sectionTitle}`}>目标线程</p>
          <div className="flex flex-wrap gap-1">
            {item.receiverThreadIds.map((tid) => (
              <span key={tid} className={`${CB.kindBadge} font-mono`}>
                {tid.slice(0, 8)}
              </span>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
