/**
 * ACP 会话列表容器
 *
 * 显示 ACP 会话的所有条目，支持消息聚合和虚拟滚动
 */

import { useRef, useEffect } from "react";
import { useAcpSession } from "../model";
import type { UseAcpSessionOptions } from "../model";
import { AcpSessionEntry } from "./AcpSessionEntry";
import { isAggregatedGroup, isAggregatedThinkingGroup } from "../model/types";
import type { AcpDisplayItem } from "../model/types";

export interface AcpSessionListProps extends UseAcpSessionOptions {
  renderItem?: (item: AcpDisplayItem, index: number) => React.ReactNode;
  itemClassName?: string;
  className?: string;
  autoScroll?: boolean;
  emptyState?: React.ReactNode;
  loadingState?: React.ReactNode;
  errorState?: (error: Error) => React.ReactNode;
}

export function AcpSessionList(props: AcpSessionListProps) {
  const {
    sessionId,
    endpoint,
    enableAggregation = true,
    className = "",
    itemClassName = "",
    autoScroll = true,
    emptyState,
    loadingState,
    errorState,
  } = props;

  const {
    displayItems,
    isConnected,
    isLoading,
    error,
    reconnect,
  } = useAcpSession({
    sessionId,
    endpoint,
    enableAggregation,
  });

  const containerRef = useRef<HTMLDivElement>(null);
  const shouldScrollRef = useRef(true);

  useEffect(() => {
    if (!autoScroll || !containerRef.current || !shouldScrollRef.current) {
      return;
    }

    const container = containerRef.current;
    container.scrollTop = container.scrollHeight;
  }, [displayItems, autoScroll]);

  const handleScroll = () => {
    if (!containerRef.current) return;

    const container = containerRef.current;
    const isAtBottom =
      container.scrollHeight - container.scrollTop - container.clientHeight < 50;
    shouldScrollRef.current = isAtBottom;
  };

  const renderConnectionStatus = () => {
    if (!isConnected) {
      return (
        <div className="sticky top-0 z-10 bg-warning/10 px-4 py-2 text-center text-xs text-warning">
          连接已断开，正在尝试重新连接...
        </div>
      );
    }
    return null;
  };

  if (isLoading && displayItems.length === 0) {
    return (
      <div className={`flex h-full items-center justify-center ${className}`}>
        {loadingState ?? (
          <div className="text-center">
            <div className="mx-auto h-8 w-8 animate-spin rounded-full border-2 border-primary border-t-transparent" />
            <p className="mt-2 text-sm text-muted-foreground">加载中...</p>
          </div>
        )}
      </div>
    );
  }

  if (error && displayItems.length === 0) {
    return (
      <div className={`flex h-full items-center justify-center ${className}`}>
        {errorState?.(error) ?? (
          <div className="text-center">
            <p className="text-sm text-destructive">连接失败: {error.message}</p>
            <button
              type="button"
              onClick={reconnect}
              className="mt-2 rounded-md bg-primary px-3 py-1 text-sm text-primary-foreground hover:bg-primary/90"
            >
              重新连接
            </button>
          </div>
        )}
      </div>
    );
  }

  if (displayItems.length === 0) {
    return (
      <div className={`flex h-full items-center justify-center ${className}`}>
        {emptyState ?? (
          <p className="text-sm text-muted-foreground">暂无消息</p>
        )}
      </div>
    );
  }

  return (
    <div
      ref={containerRef}
      onScroll={handleScroll}
      className={`h-full overflow-y-auto ${className}`}
    >
      {renderConnectionStatus()}
      <div className="space-y-1 p-4">
        {displayItems.map((item) => (
          <div key={getItemKey(item)} className={itemClassName}>
            <AcpSessionEntry item={item} />
          </div>
        ))}
      </div>
    </div>
  );
}

function getItemKey(item: AcpDisplayItem): string {
  if (isAggregatedGroup(item)) {
    return item.groupKey;
  }
  if (isAggregatedThinkingGroup(item)) {
    return item.groupKey;
  }
  return item.id;
}

export default AcpSessionList;
