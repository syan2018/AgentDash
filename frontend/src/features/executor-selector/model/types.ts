/**
 * 执行器发现与选择相关的类型定义
 */

/** 连接器类型 */
export type ConnectorType = "local_executor" | "remote_acp_backend";

/** 连接器能力声明 */
export interface ConnectorCapabilities {
  supports_cancel: boolean;
  supports_discovery: boolean;
  supports_variants: boolean;
  supports_model_override: boolean;
  supports_permission_policy: boolean;
}

/** 连接器信息 */
export interface ConnectorInfo {
  id: string;
  connector_type: ConnectorType;
  capabilities: ConnectorCapabilities;
}

/** 后端返回的执行器信息 */
export interface ExecutorInfo {
  id: string;
  name: string;
  variants: string[];
  available: boolean;
  /** 该执行器可用的远程后端 ID 列表（为空则仅本机） */
  backend_ids?: string[];
}

/** 后端 /api/agents/discovery 响应 */
export interface DiscoveryResponse {
  connector: ConnectorInfo;
  executors: ExecutorInfo[];
}

/** 执行器发现 Hook 状态 */
export interface UseExecutorDiscoveryResult {
  connector: ConnectorInfo | null;
  executors: ExecutorInfo[];
  isLoading: boolean;
  error: Error | null;
  refetch: () => void;
}

// ===================== discovered-options（vibe-kanban 对齐） =====================

export type PermissionPolicy = "AUTO" | "SUPERVISED" | "PLAN";

export interface ModelProvider {
  id: string;
  name: string;
}

export interface ModelInfo {
  id: string;
  name: string;
  provider_id?: string | null;
  /** 是否支持 extended thinking */
  reasoning: boolean;
  /** 上下文窗口大小（tokens） */
  context_window: number;
  /** 是否被当前 provider 设置为屏蔽 */
  blocked?: boolean;
}

export interface AgentInfo {
  id: string;
  label: string;
  description?: string | null;
  is_default: boolean;
}

export interface ModelSelectorConfig {
  providers: ModelProvider[];
  models: ModelInfo[];
  default_model?: string | null;
  agents: AgentInfo[];
  permissions: PermissionPolicy[];
}

export interface ExecutorDiscoveredOptions {
  model_selector: ModelSelectorConfig;
  slash_commands: Array<{ name: string; description?: string | null }>;
  loading_models: boolean;
  loading_agents: boolean;
  loading_slash_commands: boolean;
  error: string | null;
}

export interface ExecutorDiscoveryStreamState {
  options: ExecutorDiscoveredOptions | null;
  commands: Array<{ name: string; description?: string | null }>;
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
  permissionPolicy?: string;
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
  permissionPolicy: string;
  recentEntries: RecentExecutorEntry[];
  setExecutor: (executor: string) => void;
  setProviderId: (providerId: string) => void;
  setModelId: (modelId: string) => void;
  setThinkingLevel: (thinkingLevel: string) => void;
  setPermissionPolicy: (policy: string) => void;
  recordUsage: () => void;
  reset: () => void;
  /**
   * 用来自外部（agent 默认 / session context 真值）的配置原子性 hydrate 当前状态。
   * 不会触发 setExecutor 的副作用清洗。仅当传入的字段非空（trim 后）时覆盖对应字段。
   */
  hydrate: (source: ExecutorConfigSource | null | undefined) => void;
}
