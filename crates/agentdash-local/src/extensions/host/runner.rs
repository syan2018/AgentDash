pub(super) const EXTENSION_HOST_RUNNER_ENTRY: &str = "agentdash-extension-host-runner.mjs";

pub(super) const EXTENSION_HOST_RUNNER_FILES: &[(&str, &str)] = &[
    (
        EXTENSION_HOST_RUNNER_ENTRY,
        include_str!("runner/agentdash-extension-host-runner.mjs"),
    ),
    ("context.mjs", include_str!("runner/context.mjs")),
    (
        "host-api-client.mjs",
        include_str!("runner/host-api-client.mjs"),
    ),
    ("loader.mjs", include_str!("runner/loader.mjs")),
    ("protocol.mjs", include_str!("runner/protocol.mjs")),
];
