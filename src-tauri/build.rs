fn main() {
    #[cfg(target_os = "macos")]
    {
        use std::{env, path::PathBuf, process::Command};

        let object = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is missing"))
            .join("virtual_camera.o");
        let status = Command::new("/usr/bin/clang")
            .args([
                "-fobjc-arc",
                "-fblocks",
                "-mmacosx-version-min=13.0",
                "-c",
                "native/virtual_camera.m",
                "-o",
            ])
            .arg(&object)
            .status()
            .expect("failed to run clang for virtual camera bridge");
        assert!(status.success(), "virtual camera Objective-C bridge failed");
        println!("cargo:rustc-link-arg={}", object.display());
        println!("cargo:rustc-link-lib=framework=Foundation");
        println!("cargo:rustc-link-lib=framework=AppKit");
        println!("cargo:rustc-link-lib=framework=SystemExtensions");
        println!("cargo:rustc-link-lib=objc");
        println!("cargo:rerun-if-changed=native/virtual_camera.m");
    }
    tauri_build::build()
}
