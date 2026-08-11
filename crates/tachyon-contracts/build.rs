//! Embeds the repository product version into release-facing Rust crates.

use std::fs;
use std::io;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = Path::new("../../VERSION");
    println!("cargo:rerun-if-changed={}", path.display());

    let source = fs::read_to_string(path)?;
    let version = source.trim();
    if version.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "VERSION must not be empty").into());
    }
    if !version
        .bytes()
        .all(|value| value.is_ascii_alphanumeric() || matches!(value, b'.' | b'-' | b'+'))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "VERSION contains an unsupported character",
        )
        .into());
    }
    println!("cargo:rustc-env=TAC_PRODUCT_VERSION={version}");
    Ok(())
}
