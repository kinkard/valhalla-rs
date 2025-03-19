fn main() {
    let build_type = if matches!(std::env::var("PROFILE"), Ok(profile) if profile == "debug") {
        "Debug"
    } else {
        "Release"
    };
    let cores = std::thread::available_parallelism().unwrap().get();

    // Build & link required Valhalla libraries
    let dst = cmake::Config::new("./")
        .define("CMAKE_BUILD_TYPE", build_type)
        .build_arg(format!("-j{cores}"))
        .build();
    let dst = dst.display();

    // Link wrapper library
    println!("cargo:rustc-link-search={dst}/build/");
    println!("cargo:rustc-link-lib=libvalhalla-sys");

    // Manually link valhalla because `cmake` crate doesn't fetch the mystery about who depends on what from cmake
    println!("cargo:rustc-link-search={dst}/build/valhalla/src/");
    println!("cargo:rustc-link-lib=valhalla");

    // bindings
    cxx_build::bridge("src/lib.rs")
        .std("c++17")
        .compile("libvalhalla-cxxbridge");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/libvalhalla.hpp");
    println!("cargo:rerun-if-changed=src/libvalhalla.cpp");
    println!("cargo:rerun-if-changed=src/CMakeLists.txt");
}
