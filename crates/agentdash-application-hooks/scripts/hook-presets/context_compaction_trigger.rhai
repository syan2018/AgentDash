// context_compaction_trigger — 上下文压缩触发评估
// trigger: BeforeCompact
//
// 读取 ctx.token_stats 评估是否需要压缩。
// 当 input tokens 超过 (context_window - reserve_tokens) 时触发压缩。
//
// 可通过 params 覆盖默认参数：
//   reserve_tokens: 预留 token 数（默认 16384）
//   keep_last_n: 保留最近 N 条消息（默认 20）

let stats = ctx.token_stats;

// 无统计数据时跳过
if stats == () || stats.last_input_tokens == 0 || stats.context_window == 0 {
    return #{};
}

let reserve = if ctx.params != () && ctx.params.reserve_tokens != () {
    ctx.params.reserve_tokens
} else {
    16384
};

let keep_n = if ctx.params != () && ctx.params.keep_last_n != () {
    ctx.params.keep_last_n
} else {
    20
};

if stats.last_input_tokens > stats.context_window - reserve {
    #{
        compaction: #{
            reserve_tokens: reserve,
            keep_last_n: keep_n,
        }
    }
} else {
    #{}
}
