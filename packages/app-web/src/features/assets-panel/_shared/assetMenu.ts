import type { CardMenuItem } from "@agentdash/ui";

export interface PrimaryActionOption {
  label: string;
  onSelect: () => void;
}

export interface PublishActionOption {
  /** true = 已发布过同 key 资产，菜单显示"更新发布"；false = "发布到资源市场"。 */
  published: boolean;
  onSelect: () => void;
}

export interface DangerActionOption {
  label: string;
  /** busy=true 时菜单显示 busyLabel（默认"处理中…"），不阻止点击 —— 调用方自行阻止重复触发。 */
  busy?: boolean;
  busyLabel?: string;
  onSelect: () => void;
}

export interface BuildAssetMenuOptions {
  /** 第一项主操作（编辑 / 查看 / 详情）。 */
  primary: PrimaryActionOption;
  /** 发布到资源市场。null 或 undefined 时隐藏。 */
  publish?: PublishActionOption | null;
  /** primary（与可选 publish）之后、danger 之前的额外项（克隆 / 下载 / ...）。 */
  extras?: CardMenuItem[];
  /** 危险操作（删除 / 卸载）。null 或 undefined 时隐藏，且不渲染分隔线。 */
  danger?: DangerActionOption | null;
}

/**
 * 构造资产卡片菜单项数组。统一 publish 文案（"更新发布" / "发布到资源市场"）
 * 与 danger 项的 busy 文案，避免每个 CategoryPanel 各自拼接。
 *
 * 顺序：primary → publish? → extras... → 分隔线（仅当有 danger）→ danger?
 */
export function buildAssetMenuItems({
  primary,
  publish,
  extras = [],
  danger,
}: BuildAssetMenuOptions): CardMenuItem[] {
  const items: CardMenuItem[] = [
    { key: "primary", label: primary.label, onSelect: primary.onSelect },
  ];
  if (publish) {
    items.push({
      key: "publish",
      label: publish.published ? "更新发布" : "发布到资源市场",
      onSelect: publish.onSelect,
    });
  }
  for (const extra of extras) items.push(extra);
  if (danger) {
    items.push({ key: "---", label: "", onSelect: () => {} });
    const busyLabel = danger.busyLabel ?? "处理中…";
    items.push({
      key: "danger",
      label: danger.busy ? busyLabel : danger.label,
      danger: true,
      onSelect: danger.onSelect,
    });
  }
  return items;
}
