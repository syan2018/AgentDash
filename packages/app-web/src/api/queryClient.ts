import { QueryClient } from "@tanstack/react-query";

/**
 * 全局 QueryClient — 承载所有 server-state（列表/详情拉取、失效与缓存）。
 *
 * 默认 staleTime 5s：短窗口内重复挂载不重复打后端，但保持数据足够新鲜；
 * 各 query 可按需覆盖。重试关掉，错误直接抛给调用方处理（与既有 store 行为一致）。
 */
export const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5_000,
      retry: false,
      refetchOnWindowFocus: false,
    },
  },
});
