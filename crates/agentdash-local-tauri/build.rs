fn main() {
    println!("cargo:rerun-if-env-changed=AGENTDASH_DESKTOP_DEFAULT_API_MODE");
    println!("cargo:rerun-if-env-changed=AGENTDASH_DESKTOP_DEFAULT_API_ORIGIN");
    println!("cargo:rerun-if-env-changed=AGENTDASH_DESKTOP_DEFAULT_API_SIDECAR");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed=icons/icon.ico");
    tauri_build::build();
}
