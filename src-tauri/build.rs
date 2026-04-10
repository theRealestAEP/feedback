use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=sidecar/ImageDictionSidecar.swift");

    #[cfg(target_os = "macos")]
    compile_sidecar();

    tauri_build::build()
}

#[cfg(target_os = "macos")]
fn compile_sidecar() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("missing manifest dir"));
    let source = manifest_dir
        .join("sidecar")
        .join("ImageDictionSidecar.swift");
    let binary_dir = manifest_dir.join("binaries");
    let target = env::var("TARGET").expect("missing TARGET");
    let binary = binary_dir.join(format!("imagediction-sidecar-{target}"));

    fs::create_dir_all(&binary_dir).expect("failed to create sidecar binaries directory");

    if let (Ok(source_meta), Ok(binary_meta)) = (fs::metadata(&source), fs::metadata(&binary)) {
        let source_modified = source_meta.modified().ok();
        let binary_modified = binary_meta.modified().ok();

        if let (Some(source_modified), Some(binary_modified)) = (source_modified, binary_modified) {
            if binary_modified >= source_modified {
                return;
            }
        }
    }

    let status = Command::new("xcrun")
        .arg("swiftc")
        .arg("-parse-as-library")
        .arg("-O")
        .arg("-o")
        .arg(&binary)
        .arg(&source)
        .status()
        .expect("failed to launch swiftc");

    if !status.success() {
        panic!("failed to compile ImageDiction sidecar");
    }
}
