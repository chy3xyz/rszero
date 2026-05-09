use anyhow::Result;

pub fn execute() -> Result<()> {
    println!("rszeroctl environment info:");
    println!("  Rust: {}", env!("CARGO_PKG_VERSION"));
    println!("  Platform: {}", std::env::consts::OS);
    println!("  Arch: {}", std::env::consts::ARCH);
    Ok(())
}
