use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=platform/macos/host.m");
    println!("cargo:rerun-if-changed=platform/macos/NativeCamera.m");
    println!("cargo:rerun-if-changed=platform/macos/NativeCamera.h");
    println!("cargo:rerun-if-env-changed=ANYCUBE_MACOS_CC");
    println!("cargo:rerun-if-env-changed=SDKROOT");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("macos")
        || env::var_os("CARGO_FEATURE_APP").is_none()
    {
        return;
    }

    let target = env::var("TARGET").unwrap();
    let arch = match target.as_str() {
        "aarch64-apple-darwin" => "arm64",
        "x86_64-apple-darwin" => "x86_64",
        _ => panic!("Unsupported macOS target {target}"),
    };
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("anycube-host");
    let status = Command::new(env::var_os("ANYCUBE_MACOS_CC").unwrap_or_else(|| "clang".into()))
        .args(["-arch", arch, "-mmacosx-version-min=12.0", "-fobjc-arc", "-Wall", "-Wextra"])
        .args(["-framework", "Cocoa", "-framework", "WebKit", "-framework", "AVFoundation",
            "-framework", "CoreMedia", "-framework", "CoreVideo", "-framework", "CoreImage",
            "-framework", "CoreGraphics", "platform/macos/host.m", "platform/macos/NativeCamera.m", "-o"])
        .arg(output)
        .status()
        .expect("Could not run clang with a macOS SDK");
    assert!(status.success(), "Could not build the anycube macOS host");
}
