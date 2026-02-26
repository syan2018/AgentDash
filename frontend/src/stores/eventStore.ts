import { create } from 'zustand';
import type { StreamEvent } from '../types';
import { connectEventStream } from '../api/eventStream';

interface EventState {
  lastEventId: number;
  connected: boolean;
  eventSource: EventSource | null;

  connect: () => void;
  disconnect: () => void;
}

export const useEventStore = create<EventState>((set, get) => ({
  lastEventId: 0,
  connected: false,
  eventSource: null,

  connect: () => {
    if (get().eventSource) return;

    const source = connectEventStream(
      (event: StreamEvent) => {
        switch (event.type) {
          case 'Connected':
            set({ lastEventId: event.data.last_event_id, connected: true });
            break;
          case 'StateChanged':
            set({ lastEventId: event.data.id });
            break;
          case 'Heartbeat':
            break;
        }
      },
      () => {
        set({ connected: false });
      },
    );

    set({ eventSource: source });
  },

  disconnect: () => {
    const { eventSource } = get();
    if (eventSource) {
      eventSource.close();
      set({ eventSource: null, connected: false });
    }
  },
}));

// Vite Fast Refresh 下模块可能会被替换而不触发页面完全刷新。
// 这里确保旧的 SSE 连接会在 HMR dispose 时被关闭，避免累积连接导致 proxy ECONNRESET 噪音。
if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    useEventStore.getState().disconnect();
  });
}
