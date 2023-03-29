use std::env::vars;

use anyhow::Result;

pub fn show_env() -> Result<()> {
    vars()
        .filter(|var| var.0.contains("DOCTOR"))
        .for_each(|var| println!("{}={}", var.0, var.1));

    Ok(())
}
