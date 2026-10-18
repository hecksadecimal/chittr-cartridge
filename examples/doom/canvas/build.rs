use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=WASI_SDK_PATH");
    if env::var_os("CARGO_FEATURE_ENGINE").is_none() {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let engine = root.join("../engine");
    println!("cargo:rerun-if-changed={}", engine.display());

    // The WAD is not a build input: the viewer supplies it at runtime through
    // the card's <lc-file> picker, so no game data is embedded in the wasm.
    let mut c = cc::Build::new();
    c.include(engine.join("vendor"))
        .include(&engine)
        .flag("-include")
        .flag(engine.join("portable.h").to_str().unwrap())
        .flag("-std=gnu99")
        .flag("-fno-strict-aliasing")
        .flag("-fwrapv")
        .flag("-Wno-implicit-function-declaration")
        .warnings(false)
        .opt_level(2);
    if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "wasm32" {
        let sdk =
            PathBuf::from(env::var_os("WASI_SDK_PATH").expect(
                "WASI_SDK_PATH must point to an unpacked WASI SDK for the wasm engine build",
            ));
        let sysroot = sdk.join("share/wasi-sysroot");
        c.compiler(sdk.join("bin/clang"))
            .archiver(sdk.join("bin/llvm-ar"))
            .target("wasm32-wasip1")
            .flag(format!("--sysroot={}", sysroot.display()));
        println!(
            "cargo:rustc-link-search=native={}",
            sysroot.join("lib/wasm32-wasip1").display()
        );
        // Use libc's formatting/memory code, not its WASI syscall trampoline object.
        // Work on a copy: the installed SDK is never modified.
        let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
        let libc = out.join("libdoom_libc.a");
        fs::copy(sysroot.join("lib/wasm32-wasip1/libc.a"), &libc).unwrap();
        let removed = std::process::Command::new(sdk.join("bin/llvm-ar"))
            .args(["d"])
            .arg(&libc)
            .arg("__wasilibc_real.c.obj")
            .status()
            .expect("strip WASI syscall trampolines from local libc copy");
        assert!(removed.success(), "Cannot prepare freestanding static libc");
        // Share a single allocator with Rust; no WASI allocator or native host I/O.
        c.file(root.join("wasm_libc.c"));
        println!("cargo:rerun-if-changed=wasm_libc.c");
        println!("cargo:rustc-link-arg=--max-memory=67108864");
    } else {
        println!("cargo:rustc-link-lib=m");
    }
    let sources = fs::read_to_string(engine.join("sources.txt")).expect("engine source manifest");
    for source in sources
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
    {
        c.file(engine.join(source));
    }
    c.compile("doom_engine");
    if env::var("CARGO_CFG_TARGET_ARCH").unwrap() == "wasm32" {
        println!("cargo:rustc-link-lib=static=doom_libc");
        println!("cargo:rustc-link-lib=static=m");
    }
}
