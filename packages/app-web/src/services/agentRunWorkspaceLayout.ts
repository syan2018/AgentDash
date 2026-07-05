import { settingsApi } from "../api/settings";
import type { JsonValue } from "../generated/common-contracts";
import type { WorkspaceTabLayout } from "../features/workspace-runtime";

function isWorkspaceTabLayout(value: unknown): value is WorkspaceTabLayout {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return Array.isArray(record.tabs)
    && (record.active_tab_uri == null || typeof record.active_tab_uri === "string");
}

export async function saveWorkspaceTabLayout(
  workspaceKey: string,
  layout: WorkspaceTabLayout,
): Promise<void> {
  await settingsApi.update(
    { scope: "user" },
    [{ key: workspaceTabLayoutSettingKey(workspaceKey), value: workspaceTabLayoutToJson(layout) }],
  );
}

export async function loadWorkspaceTabLayout(
  workspaceKey: string,
): Promise<WorkspaceTabLayout | null> {
  const settings = await settingsApi.list({
    scope: "user",
    category: workspaceTabLayoutSettingKey(workspaceKey),
  });
  const setting = settings.find((entry) => entry.key === workspaceTabLayoutSettingKey(workspaceKey));
  return isWorkspaceTabLayout(setting?.value) ? setting.value : null;
}

function workspaceTabLayoutSettingKey(workspaceKey: string): string {
  return `ui.agentrun_workspace_tab_layout.${workspaceKey}`;
}

function workspaceTabLayoutToJson(layout: WorkspaceTabLayout): JsonValue {
  return {
    tabs: layout.tabs.map((tab) => ({
      type_id: tab.type_id,
      uri: tab.uri,
      title: tab.title,
      pinned: tab.pinned,
    })),
    active_tab_uri: layout.active_tab_uri,
  };
}
