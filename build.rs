use std::env;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    // Compile the ARM assembly startup file
    let asm_file = "asm/startup.s";
    println!("cargo:rerun-if-changed={}", asm_file);
    println!("cargo:rerun-if-changed=linker.ld");

    // Use arm-none-eabi-gcc to assemble the startup file
    let cc = env::var("CC_armv5te_none_eabi")
        .unwrap_or_else(|_| "arm-none-eabi-gcc".to_string());

    let obj_path = out_dir.join("startup.o");
    let mut cmd = std::process::Command::new(&cc);
    cmd.arg("-mcpu=arm926ej-s")
        .arg("-marm")
        .arg("-c")
        .arg(asm_file)
        .arg("-o")
        .arg(&obj_path)
        .status()
        .expect("failed to assemble startup.s");

    // Pass object file directly to linker
    println!("cargo:rustc-link-arg-bins={}", obj_path.display());
}
