fn main() {
    println!("cargo:rerun-if-env-changed=AGENTDASH_DESKTOP_DEFAULT_API_MODE");
    println!("cargo:rerun-if-env-changed=AGENTDASH_DESKTOP_DEFAULT_API_ORIGIN");
    println!("cargo:rerun-if-env-changed=AGENTDASH_DESKTOP_DEFAULT_API_SIDECAR");
    tauri_build::build();
}
