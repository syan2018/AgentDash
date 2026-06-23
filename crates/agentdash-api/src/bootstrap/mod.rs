pub mod auth;
pub mod background_workers;
pub mod frame_launch_envelope_provider;
pub mod relay;
pub mod repositories;
pub mod runtime_gateway;
pub mod session;
pub mod vfs;

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    #[test]
    fn bootstrap_modules_do_not_depend_on_routes() {
        let bootstrap_dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("bootstrap");
        let forbidden = ["crate::routes", "super::routes", "routes::"];

        for entry in fs::read_dir(&bootstrap_dir).expect("bootstrap dir should be readable") {
            let entry = entry.expect("bootstrap entry should be readable");
            let path = entry.path();
            if path.file_name().and_then(|name| name.to_str()) == Some("mod.rs") {
                continue;
            }
            if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
                continue;
            }
            let source = fs::read_to_string(&path).expect("bootstrap source should be readable");
            for pattern in forbidden {
                assert!(
                    !source.contains(pattern),
                    "{} must not depend on route modules via `{pattern}`",
                    path.display()
                );
            }
        }
    }
}
