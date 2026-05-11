/**
 * Workflow contract 受控 panel 组件集合。
 *
 * 每个 panel 负责一个语义区块（基础信息 / 注入 / Hook 规则 / 能力 / 端口），
 * 通过 props + onChange 与容器交互，不直接依赖 workflowStore。
 */

export { InjectionPanel } from "./InjectionPanel";
export type { InjectionPanelProps } from "./InjectionPanel";

export { HookRulesPanel } from "./HookRulesPanel";
export type { HookRulesPanelProps } from "./HookRulesPanel";

export { CapabilityPanel } from "./CapabilityPanel";
export type { CapabilityPanelProps } from "./CapabilityPanel";

export {
  PortsPanel,
  OutputPortItem,
  InputPortItem,
  PortViewCard,
  GATE_LABEL,
  CTX_LABEL,
} from "./PortsPanel";
export type { PortsPanelProps } from "./PortsPanel";
