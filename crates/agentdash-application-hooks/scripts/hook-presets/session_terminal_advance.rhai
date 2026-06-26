// session_terminal_advance — 当 session 进入终态时自动推进 lifecycle step

let policy = ctx.workflow.transition_policy;
if policy == "session_terminal_matches" {
    return #{
        completion: #{
            mode: "session_terminal_matches",
            satisfied: false,
            reason: "当前 step 需要等待 session 真正进入终态，再由 runtime 推进"
        },
        diagnostics: [
            make_diagnostic(
                "before_stop_session_ended",
                "当前 workflow step 会在 session 进入终态后自然推进"
            )
        ]
    };
}

if ctx.signals.auto_completion {
    let checks = ctx.contract.checks;
    if checks != () {
        for check in checks {
            if check.kind == "session_terminal_in" {
                return #{
                    completion: #{
                        mode: "session_terminal_matches",
                        satisfied: false,
                        reason: "当前 step 需要等待 session 真正进入终态，再由 runtime 推进"
                    },
                    diagnostics: [
                        make_diagnostic(
                            "before_stop_session_ended",
                            "当前 workflow step 会在 session 进入终态后自然推进"
                        )
                    ]
                };
            }
        }
    }
}

#{}
