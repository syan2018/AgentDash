import { useState } from "react";
import { VfsBrowser } from "../../vfs";

export function KnowledgeSection({
  enabled,
  onToggle,
  projectId,
  agentId,
  linkId,
}: {
  enabled: boolean;
  onToggle: (next: boolean) => void;
  projectId?: string;
  agentId?: string;
  linkId?: string;
}) {
  const [isOpen, setIsOpen] = useState(false);

  return (
    <div className="mt-4 rounded-[12px] border border-border/70 bg-secondary/15">
      {/* ── 开关行 ── */}
      <div className="flex items-center gap-3 px-4 py-3">
        <label className="flex cursor-pointer items-center gap-2.5">
          <span className="relative inline-flex h-[18px] w-[32px] shrink-0">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => onToggle(e.target.checked)}
              className="peer sr-only"
            />
            {/* eslint-disable-next-line no-restricted-syntax -- 开关轨道为药丸形态 */}
            <span className="absolute inset-0 rounded-full bg-border transition-colors duration-160 peer-checked:bg-primary" />
            {/* eslint-disable-next-line no-restricted-syntax -- 开关旋钮为圆形 */}
            <span className="absolute left-[3px] top-[3px] h-3 w-3 rounded-full bg-background shadow-sm transition-transform duration-160 peer-checked:translate-x-[14px]" />
          </span>
          <span className="text-xs font-medium text-foreground">启用知识库</span>
        </label>
        <span className="text-[10px] text-muted-foreground">
          {enabled ? "Agent 会在 session 间积累知识" : "Agent 无状态运行（默认）"}
        </span>
        {enabled && projectId && agentId && (
          <button
            type="button"
            onClick={() => setIsOpen((v) => !v)}
            className="ml-auto text-[10px] text-muted-foreground hover:text-foreground"
          >
            {isOpen ? "收起" : "浏览知识库"}
          </button>
        )}
      </div>

      {/* ── VFS 浏览器 ── */}
      {enabled && isOpen && projectId && agentId && linkId && (
        <div className="border-t border-border px-2 py-3">
          <VfsBrowser
            source={{
              source_type: "project_agent_knowledge",
              project_id: projectId,
              agent_id: agentId,
              link_id: linkId,
            }}
            visibleMountIds={["agent-knowledge"]}
            initialMountId="agent-knowledge"
          />
        </div>
      )}
    </div>
  );
}
