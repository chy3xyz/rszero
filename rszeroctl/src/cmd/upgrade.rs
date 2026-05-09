use anyhow::Result;

pub fn execute() -> Result<()> {
    println!("rszeroctl upgrade");
    println!("Checking for updates...");
    println!("Current version: 0.1.0");
    println!("To upgrade: cargo install rszeroctl --force");
    Ok(())
}
