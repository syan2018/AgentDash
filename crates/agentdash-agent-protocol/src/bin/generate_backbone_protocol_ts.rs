use std::process::Command;

fn main() {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args([
            "run",
            "-p",
            "agentdash-contracts",
            "--bin",
            "generate_contracts_ts",
        ])
        .args(std::env::args().skip(1))
        .status()
        .expect("run canonical TypeScript contract generator");

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}
