use super::*;
use std::path::Path;

mod apply;
mod execute;
mod plan;
mod preview;
mod verification;

fn parse_cli_json(stdout: &[u8]) -> serde_json::Value {
    serde_json::from_slice(stdout).expect("cli output should be valid json")
}

fn write_fixture(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture parent dir");
    }

    fs::write(path, contents).expect("write fixture");
}
