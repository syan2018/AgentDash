import { useMemo } from "react";

import { useProjectWorkspaceModules } from "../../workspace-module";
import type { WorkspaceModuleKind } from "../../../generated/workspace-module-contracts";
import type { CapabilityChip } from "./capability-picker";
import { CapabilityPicker } from "./capability-picker";

const KIND_LABEL: Record<WorkspaceModuleKind, string> = {
  extension: "extension",
  canvas: "canvas",
  builtin: "builtin",
};

/**
 * ProjectAgent 可见 Workspace Module 白名单选择器。
 *
 * 复用 Child 3 的 `useProjectWorkspaceModules` 与生成类型，勾选写回 `module_id`
 * 数组（形如 `ext:{key}` / `canvas:{mount_id}`）。空选 = 全部可见。
 */
export function WorkspaceModuleVisibilityPicker({
  projectId,
  selectedRefs,
  onChange,
}: {
  projectId?: string;
  selectedRefs: string[];
  onChange: (refs: string[]) => void;
}) {
  const state = useProjectWorkspaceModules(projectId ?? null);
  const isLoading = state.status === "loading" || state.status === "refreshing";

  const summaries = useMemo(
    () =>
      state.modules
        .map((descriptor) => descriptor.summary)
        .slice()
        .sort((a, b) => a.title.localeCompare(b.title, "zh-CN")),
    [state.modules],
  );

  const toggleRef = (moduleId: string) => {
    if (selectedRefs.includes(moduleId)) {
      onChange(selectedRefs.filter((item) => item !== moduleId));
      return;
    }
    onChange([...selectedRefs, moduleId]);
  };

  const isAllowlistMode = selectedRefs.length > 0;

  return (
    <CapabilityPicker
      hint={
        isAllowlistMode
          ? "白名单模式：仅勾选的 Workspace Module 对该 Agent 可见。清空后回到默认（全部可见）。"
          : "默认模式：所有 Workspace Module 均可见。点击下方任一卡片即切换为白名单模式。"
      }
      isLoading={isLoading}
      error={state.status === "error" ? (state.error ?? "加载 Workspace Module 失败") : null}
      items={summaries}
      selectedKeys={selectedRefs}
      itemKey={(m) => m.module_id}
      itemToCardProps={(m) => {
        const chips: CapabilityChip[] = [{ label: KIND_LABEL[m.kind] }];
        if (m.status.kind === "unavailable") {
          chips.push({ label: "unavailable", variant: "warning" });
        }
        return {
          reactKey: m.module_id,
          title: m.title,
          subtitle: m.module_id,
          description: m.description?.trim() || undefined,
          chips,
        };
      }}
      onToggle={toggleRef}
      loadingText="正在加载 Workspace Module…"
      emptyAllText="当前项目还没有 Workspace Module"
      enabledEmptyText="尚未在白名单中加入 Module；当前为默认模式（全部可见）。"
      availableEmptyText="所有 Module 都已加入白名单。"
    />
  );
}
