import type { Story } from "../../types";

/** 判断 Story 是否有值得展示的上下文信息（供 SessionPage 判断是否渲染面板） */
export function hasStoryContextInfo(story: Story): boolean {
  const ctx = story.context;
  return (
    ctx.context_containers.length > 0
    || ctx.session_composition != null
    || ctx.mount_policy_override != null
    || ctx.disabled_container_ids.length > 0
  );
}
