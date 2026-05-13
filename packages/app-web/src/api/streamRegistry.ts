interface ClosableConnection {
  close: () => void;
}

const streamConnections = new Set<ClosableConnection>();

export function registerStreamConnection(connection: ClosableConnection): () => void {
  streamConnections.add(connection);
  return () => {
    streamConnections.delete(connection);
  };
}

export function closeAllStreamConnections(): void {
  const snapshot = Array.from(streamConnections);
  streamConnections.clear();
  for (const connection of snapshot) {
    try {
      connection.close();
    } catch {
      // 关闭连接时不抛给 UI，避免 HMR 阶段打断刷新流程。
    }
  }
}

if (import.meta.hot) {
  import.meta.hot.dispose(() => {
    closeAllStreamConnections();
  });
}
