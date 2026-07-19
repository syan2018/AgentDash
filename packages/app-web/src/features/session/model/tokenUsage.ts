export type ContextUsageSource =
  | "provider"
  | "providerPlusEstimate"
  | "localEstimate";

export interface TokenUsageBreakdownInfo {
  inputTokens: number;
  outputTokens: number;
  totalTokens: number;
  cacheReadTokens: number;
  cacheCreationTokens: number;
  reasoningTokens: number;
}

export interface TokenUsageInfo {
  currentContextTokens: number;
  providerContextTokens: number;
  pendingEstimateTokens: number;
  cumulativeTotalTokens: number;
  modelContextWindow?: number;
  effectiveContextWindow?: number;
  reserveTokens: number;
  usageSource: ContextUsageSource;
  last: TokenUsageBreakdownInfo;
  total: TokenUsageBreakdownInfo;
}
