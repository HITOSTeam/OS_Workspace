use std::{env, path::PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_default();
    let linker_script = match arch.as_str() {
        "riscv64" => "src/linker.ld",
        "loongarch64" => "src/linker_loongarch.ld",
        _ => return,
    };
    let linker_script = manifest_dir.join(linker_script);

    println!("cargo:rerun-if-changed={}", linker_script.display());
    println!("cargo:rustc-link-arg=-T{}", linker_script.display());
}
