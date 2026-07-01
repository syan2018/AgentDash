export type SettingsTab = "overview" | "context" | "workspace" | "management";

export interface SettingsTabItem {
  key: SettingsTab;
  label: string;
  description: string;
}

export const SETTINGS_TABS: SettingsTabItem[] = [
  { key: "overview", label: "概览", description: "项目身份、摘要与基础信息" },
  { key: "context", label: "VFS 资源", description: "项目级 VFS Mount、解析结果与 runtime preview" },
  { key: "workspace", label: "工作空间", description: "项目在哪台机器上运行，以及工作空间落在哪里" },
  { key: "management", label: "权限与模板", description: "共享授权、模板策略、复制与删除" },
];
