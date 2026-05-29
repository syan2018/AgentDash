use std::{fs, path::Path};

fn main() {
    let migrations_dir = Path::new("../agentdash-infrastructure/migrations");
    println!("cargo:rerun-if-changed={}", migrations_dir.display());

    let migration_entries = fs::read_dir(migrations_dir)
        .ok()
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .collect::<Vec<_>>();
    for entry in &migration_entries {
        println!("cargo:rerun-if-changed={}", entry.path().display());
    }

    let schema_version = migration_entries
        .iter()
        .filter_map(|entry| {
            entry
                .file_name()
                .to_str()
                .and_then(|name| name.split('_').next())
                .and_then(|prefix| prefix.parse::<i64>().ok())
        })
        .max()
        .unwrap_or_default();

    println!("cargo:rustc-env=AGENTDASH_SCHEMA_VERSION={schema_version}");
}
