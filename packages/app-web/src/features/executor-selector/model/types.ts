/**
 * 执行器发现与选择相关的类型定义
 */

import type {
  ExecutionProfileAgentDto,
  ExecutionProfileDiscoveryResponse,
  ExecutionProfileDto,
  ExecutionProfileModelDto,
  ExecutionProfileOptionsDto,
  ExecutionProfileProviderDto,
  ExecutionProfileSlashCommandDto,
} from "../../../generated/project-agent-contracts";

/** 产品可配置 execution profile；不是 Runtime offer。 */
export type ExecutorInfo = ExecutionProfileDto;

/** 后端 /api/agents/discovery 响应 */
export type DiscoveryResponse = ExecutionProfileDiscoveryResponse;

/** 执行器发现 Hook 状态 */
export interface UseExecutorDiscoveryResult {
  executors: ExecutorInfo[];
  isLoading: boolean;
  error: Error | null;
  refetch: () => void;
}

// ===================== discovered-options =====================

export type ModelProvider = ExecutionProfileProviderDto;

export type ModelInfo = ExecutionProfileModelDto;

export type AgentInfo = ExecutionProfileAgentDto;

export interface ModelSelectorConfig {
  providers: ModelProvider[];
  models: ModelInfo[];
  default_model?: string | null;
  agents: AgentInfo[];
}

export type ExecutorDiscoveredOptions = ExecutionProfileOptionsDto;

export interface ExecutorDiscoveryStreamState {
  options: ExecutorDiscoveredOptions | null;
  commands: ExecutionProfileSlashCommandDto[];
  discovering: boolean;
  error: string | null;
}

export interface UseExecutorDiscoveredOptionsResult {
  options: ExecutorDiscoveredOptions | null;
  isConnected: boolean;
  isInitialized: boolean;
  error: Error | null;
  reconnect: () => void;
}

/** 用户选择的执行器配置（用于持久化到 localStorage，使用 camelCase） */
export interface PersistedExecutorConfig {
  executor: string;
  providerId?: string;
  modelId?: string;
  /** 推理级别，替代旧的 reasoningId 字段（v2 格式） */
  thinkingLevel?: string;
}

/** 最近使用记录 */
export interface RecentExecutorEntry {
  executor: string;
  providerId?: string;
  modelId?: string;
  timestamp: number;
}

/** 执行器配置来源（可选字段；空值或 undefined 会退回 localStorage / 硬编码默认） */
export type ExecutorConfigSource = Partial<PersistedExecutorConfig>;

/** 执行器配置 Hook 返回值 */
export interface UseExecutorConfigResult {
  executor: string;
  providerId: string;
  modelId: string;
  /** 推理级别，替代旧的 reasoningId 字段 */
  thinkingLevel: string;
  recentEntries: RecentExecutorEntry[];
  setExecutor: (executor: string) => void;
  setProviderId: (providerId: string) => void;
  setModelId: (modelId: string) => void;
  setThinkingLevel: (thinkingLevel: string) => void;
  recordUsage: () => void;
  reset: () => void;
  /**
   * 用来自外部（agent 默认 / session context 真值）的配置原子性 hydrate 当前状态。
   * 不会触发 setExecutor 的副作用清洗。仅当传入的字段非空（trim 后）时覆盖对应字段。
   */
  hydrate: (source: ExecutorConfigSource | null | undefined) => void;
}
