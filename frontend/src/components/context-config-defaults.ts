import type {
  ContextContainerCapability,
  MountDerivationPolicy,
  SessionComposition,
} from "../types";

export const CONTEXT_CAPABILITY_OPTIONS: Array<{
  value: ContextContainerCapability;
  label: string;
}> = [
  { value: "read", label: "读" },
  { value: "write", label: "写" },
  { value: "list", label: "列" },
  { value: "search", label: "搜" },
  { value: "exec", label: "执行" },
];

export function createDefaultMountPolicy(): MountDerivationPolicy {
  return {
    include_local_workspace: true,
    local_workspace_capabilities: [],
  };
}

export function createDefaultSessionComposition(): SessionComposition {
  return {
    persona_label: null,
    persona_prompt: null,
    workflow_steps: [],
    required_context_blocks: [],
  };
}
