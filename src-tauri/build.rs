use std::process::Command;
use std::env;
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=src/audio/macos/sck_audio.swift");
    
    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    
    if target_os == "macos" {
        let out_dir = env::var("OUT_DIR").unwrap();
        let swift_src = Path::new("src/audio/macos/sck_audio.swift");
        let swift_out = Path::new(&out_dir).join("sck_audio");
        
        // Ensure the source file exists
        if swift_src.exists() {
            let status = Command::new("swiftc")
                .arg("-O") // Optimize for speed
                .arg(swift_src)
                .arg("-o")
                .arg(&swift_out)
                .status()
                .expect("Failed to execute swiftc");
                
            if !status.success() {
                println!("cargo:warning=Failed to compile sck_audio.swift");
            }
        }
    }

    tauri_build::build()
}
