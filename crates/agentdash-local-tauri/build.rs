fn main() {
    println!("cargo:rerun-if-env-changed=AGENTDASH_DESKTOP_DEFAULT_API_ORIGIN");
    tauri_build::build();
}
