/** Story 状态 */
export type StoryStatus =
  | 'created'
  | 'context_ready'
  | 'decomposed'
  | 'executing'
  | 'completed'
  | 'failed';

/** Task 状态 */
export type TaskStatus =
  | 'pending'
  | 'assigned'
  | 'running'
  | 'awaiting_verification'
  | 'completed'
  | 'failed';

/** 后端类型 */
export type BackendType = 'local' | 'remote';

/** Story — 用户价值单元 */
export interface Story {
  id: string;
  backend_id: string;
  title: string;
  description: string;
  status: StoryStatus;
  context: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

/** Task — 执行容器 */
export interface Task {
  id: string;
  story_id: string;
  title: string;
  description: string;
  status: TaskStatus;
  agent_type: string | null;
  agent_pid: string | null;
  workspace_path: string | null;
  artifacts: unknown[];
  created_at: string;
  updated_at: string;
}

/** 后端连接配置 */
export interface BackendConfig {
  id: string;
  name: string;
  endpoint: string;
  auth_token: string | null;
  enabled: boolean;
  backend_type: BackendType;
}

/** 视图配置 */
export interface ViewConfig {
  id: string;
  name: string;
  backend_ids: string[];
  filters: Record<string, unknown>;
  sort_by: string | null;
}

/** 状态变更事件 */
export interface StateChange {
  id: number;
  entity_id: string;
  kind: string;
  payload: Record<string, unknown>;
  backend_id: string;
  created_at: string;
}

/** 流式事件 */
export type StreamEvent =
  | { type: 'Connected'; data: { last_event_id: number } }
  | { type: 'StateChanged'; data: StateChange }
  | { type: 'Heartbeat'; data: { timestamp: number } };
