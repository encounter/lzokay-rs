use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=wrapper.hpp");
    println!("cargo:rerun-if-changed=lzokay/lzokay.cpp");
    println!("cargo:rerun-if-changed=lzokay/lzokay.hpp");
    cc::Build::new()
        .cpp(true)
        .file("lzokay/lzokay.cpp")
        .flag_if_supported("-std=c++14") // GCC/Clang
        .flag_if_supported("/std:c++14") // MSVC
        .compile("lzokay");
    #[allow(unused_mut)]
    let mut bindings = bindgen::Builder::default()
        .header("wrapper.hpp")
        .clang_arg("-Ilzokay")
        .allowlist_function("lzokay::.*")
        .size_t_is_usize(true)
        .ctypes_prefix("types")
        .derive_debug(false)
        .clang_arg("-std=c++14")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks));
    #[cfg(not(feature = "std"))]
    {
        bindings = bindings.layout_tests(false);
    }
    if matches!(env::var("CARGO_CFG_TARGET_OS"), Result::Ok(v) if v == "android") {
        if let Result::Ok(cc) = env::var("TARGET_CXX") {
            let mut sysroot = PathBuf::from(cc).with_file_name("../sysroot");
            sysroot = sysroot.canonicalize().unwrap_or_else(|err| {
                panic!("Failed to locate {}: {}", sysroot.to_string_lossy(), err)
            });
            bindings = bindings.clang_arg(format!("--sysroot={}", sysroot.to_string_lossy()));
        }
    }
    let result = bindings.generate().expect("Unable to generate bindings");
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    result.write_to_file(out_path.join("bindings.rs")).expect("Couldn't write bindings!");
}
