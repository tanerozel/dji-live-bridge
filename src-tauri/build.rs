fn main() {
    // The development sidecar is named after the build target, so ask cargo for
    // the triple instead of guessing it from cfg at runtime.
    println!(
        "cargo:rustc-env=BUILD_TARGET_TRIPLE={}",
        std::env::var("TARGET").expect("TARGET is missing")
    );
    // AI Vision links ONNX Runtime statically. pyke's Windows build always
    // contains the DirectML provider, so it imports DirectML, DXCore, D3D12
    // and DXGI. The app only runs the CPU provider; load those DLLs lazily, as
    // Microsoft's own onnxruntime.dll does, so a PC without them (or with an
    // older DirectML) still starts.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        // Tauri's resource manifest is only linked into application binaries.
        // Unit-test executables also import rfd's TaskDialogIndirect, which
        // exists only in Common Controls v6. Let the linker embed the same
        // dependency in every executable so tests can start too.
        println!("cargo:rustc-link-arg=/MANIFEST:EMBED");
        println!(
            "cargo:rustc-link-arg=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'"
        );
        for dll in ["DirectML.dll", "dxcore.dll", "d3d12.dll", "dxgi.dll"] {
            println!("cargo:rustc-link-arg=/DELAYLOAD:{dll}");
        }
        println!("cargo:rustc-link-lib=delayimp");
    }
    // The Objective-C camera bridge is macOS-only; other platforms link nothing.
    #[cfg(target_os = "macos")]
    {
        use std::{env, path::PathBuf, process::Command};

        let object = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is missing"))
            .join("virtual_camera.o");
        // Compile for the Rust target, not the build machine: an Intel build on
        // an Apple Silicon Mac would otherwise link an arm64 object and fail.
        let arch = match env::var("CARGO_CFG_TARGET_ARCH").as_deref() {
            Ok("x86_64") => "x86_64",
            _ => "arm64",
        };
        let status = Command::new("/usr/bin/clang")
            .args([
                "-arch",
                arch,
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
    // The linker above supplies the Windows manifest, including for tests.
    // Keep Tauri's other resources without adding a duplicate manifest.
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .windows_attributes(tauri_build::WindowsAttributes::new_without_app_manifest()),
    )
    .expect("Tauri build failed")
}
