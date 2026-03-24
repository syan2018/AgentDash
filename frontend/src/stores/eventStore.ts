import { create } from 'zustand';
import type { StreamEvent } from '../types';
import { connectEventStream } from '../api/eventStream';
import { registerStreamConnection } from '../api/streamRegistry';
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
  eventSource: EventSource | null;
  unregisterStream: (() => void) | null;

  connect: (projectId: string) => void;
  disconnect: () => void;
}

export const useEventStore = create<EventState>((set, get) => ({
  activeProjectId: null,
  lastEventId: 0,
  connected: false,
  connectionState: 'disconnected',
  eventSource: null,
  unregisterStream: null,

  connect: (projectId) => {
    const current = get();
    if (current.eventSource && current.activeProjectId === projectId) return;
    if (current.eventSource) {
      current.eventSource.close();
      current.unregisterStream?.();
    }
    set({
      activeProjectId: projectId,
      lastEventId: 0,
      connectionState: 'connecting',
      connected: false,
      eventSource: null,
      unregisterStream: null,
    });

    const source = connectEventStream(
      projectId,
      (event: StreamEvent) => {
        if (get().eventSource !== source) return;
        switch (event.type) {
          case 'Connected':
            set({
              lastEventId: event.data.last_event_id,
              connected: true,
              connectionState: 'connected',
            });
            break;
          case 'StateChanged':
            set({ lastEventId: event.data.id, connected: true, connectionState: 'connected' });
            useStoryStore.getState().handleStateChange(event.data);
            break;
          case 'Heartbeat':
            break;
        }
      },
      () => {
        if (get().eventSource !== source) return;
        set({ connected: true, connectionState: 'connected' });
      },
      () => {
        if (get().eventSource !== source) return;
        set((state) => ({
          connected: false,
          connectionState: state.lastEventId > 0 ? 'reconnecting' : 'connecting',
        }));
      },
    );

    const unregister = registerStreamConnection({
      close: () => source.close(),
    });

    set({ eventSource: source, unregisterStream: unregister });
  },

  disconnect: () => {
    const { eventSource, unregisterStream } = get();
    if (eventSource) {
      eventSource.close();
    }
    unregisterStream?.();
    set({
      activeProjectId: null,
      lastEventId: 0,
      eventSource: null,
      unregisterStream: null,
      connected: false,
      connectionState: 'disconnected',
    });
  },
}));

// Vite Fast Refresh 下模块可能会被替换而不触发页面完全刷新。
// 这里确保旧的 SSE 连接会在 HMR dispose 时被关闭，避免累积连接导致 proxy ECONNRESET 噪音。
if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    useEventStore.getState().disconnect();
  });
}
