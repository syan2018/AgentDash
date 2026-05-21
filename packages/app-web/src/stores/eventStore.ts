import { create } from 'zustand';
import type { StreamEvent } from '../types';
import {
  connectProjectEventStream,
  type ProjectEventStreamConnection,
} from '../api/eventStream';
import { registerStreamConnection } from '../api/streamRegistry';
import { useCoordinatorStore } from './coordinatorStore';
import { useStoryStore } from './storyStore';

export type EventConnectionState =
  | 'disconnected'
  | 'connecting'
  | 'connected'
  | 'reconnecting';

interface EventState {
  activeProjectId: string | null;
  lastEventId: number;
  connected: boolean;
  connectionState: EventConnectionState;
  streamConnection: ProjectEventStreamConnection | null;
  unregisterStream: (() => void) | null;

  connect: (projectId: string) => void;
  disconnect: () => void;
}

let backendRefreshTimer: ReturnType<typeof setTimeout> | null = null;

function scheduleBackendRefresh() {
  if (backendRefreshTimer) return;
  backendRefreshTimer = setTimeout(() => {
    backendRefreshTimer = null;
    void useCoordinatorStore.getState().fetchBackends();
  }, 100);
}

export const useEventStore = create<EventState>((set, get) => ({
  activeProjectId: null,
  lastEventId: 0,
  connected: false,
  connectionState: 'disconnected',
  streamConnection: null,
  unregisterStream: null,

  connect: (projectId) => {
    const current = get();
    if (current.streamConnection && current.activeProjectId === projectId) return;
    if (current.streamConnection) {
      current.streamConnection.close();
      current.unregisterStream?.();
    }
    set({
      activeProjectId: projectId,
      lastEventId: 0,
      connectionState: 'connecting',
      connected: false,
      streamConnection: null,
      unregisterStream: null,
    });

    let streamConnection: ProjectEventStreamConnection | null = null;
    streamConnection = connectProjectEventStream({
      projectId,
      onEvent: (event: StreamEvent) => {
        if (get().streamConnection !== streamConnection) return;
        switch (event.type) {
          case 'Connected':
            set({
              lastEventId: event.data.last_event_id,
              connected: true,
              connectionState: 'connected',
            });
            scheduleBackendRefresh();
            break;
          case 'StateChanged':
            set({ lastEventId: event.data.id, connected: true, connectionState: 'connected' });
            useStoryStore.getState().handleStateChange(event.data);
            break;
          case 'BackendRuntimeChanged':
            set({ connected: true, connectionState: 'connected' });
            scheduleBackendRefresh();
            break;
          case 'Heartbeat':
            break;
        }
      },
      onLifecycleChange: (lifecycle) => {
        if (get().streamConnection !== streamConnection) return;
        if (lifecycle === 'connected') {
          set({ connected: true, connectionState: 'connected' });
          return;
        }
        if (lifecycle === 'connecting' || lifecycle === 'reconnecting') {
          set({
            connected: false,
            connectionState: lifecycle,
          });
        }
      },
      onError: (error) => {
        console.warn('项目事件流连接异常:', error.message);
      },
    });

    const unregister = registerStreamConnection({
      close: () => streamConnection?.close(),
    });

    set({ streamConnection, unregisterStream: unregister });
  },

  disconnect: () => {
    const { streamConnection, unregisterStream } = get();
    if (streamConnection) {
      streamConnection.close();
    }
    unregisterStream?.();
    if (backendRefreshTimer) {
      clearTimeout(backendRefreshTimer);
      backendRefreshTimer = null;
    }
    set({
      activeProjectId: null,
      lastEventId: 0,
      streamConnection: null,
      unregisterStream: null,
      connected: false,
      connectionState: 'disconnected',
    });
  },
}));

// Vite Fast Refresh 下模块可能会被替换而不触发页面完全刷新。
// 这里确保旧的项目事件流连接会在 HMR dispose 时被关闭，避免累积连接。
if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    useEventStore.getState().disconnect();
  });
}
