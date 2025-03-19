use miniserde::{Deserialize, json};
use std::path::Path;

fn main() {
    let build_type = if matches!(std::env::var("PROFILE"), Ok(profile) if profile == "debug") {
        "Debug"
    } else {
        "Release"
    };
    let cores = std::thread::available_parallelism().unwrap().get();

    // Build & link required Valhalla libraries
    let dst = cmake::Config::new("./valhalla/")
        .define("CMAKE_BUILD_TYPE", build_type)
        .define("CMAKE_EXPORT_COMPILE_COMMANDS", "ON") // Required to extract include paths
        // Enable link-time optimization only in Release configuration to have reasonable compile times in Debug
        .define(
            "CMAKE_INTERPROCEDURAL_OPTIMIZATION",
            if build_type == "Release" { "ON" } else { "OFF" },
        )
        // Disable everything we don't need to reduce number of system dependencies and speed up compilation
        .define("ENABLE_TOOLS", "OFF")
        .define("ENABLE_DATA_TOOLS", "OFF")
        .define("ENABLE_SERVICES", "OFF")
        .define("ENABLE_HTTP", "OFF")
        .define("ENABLE_PYTHON_BINDINGS", "OFF")
        .define("ENABLE_TESTS", "OFF")
        .define("ENABLE_GDAL", "OFF")
        .build_arg(format!("-j{cores}"))
        .build_target("valhalla")
        .build();

    let valhalla_includes = extract_includes(&dst.join("build/compile_commands.json"), "config.cc");

    // Manually link valhalla because `cmake` crate doesn't fetch the mystery about who depends on what from cmake
    let dst = dst.display().to_string();
    println!("cargo:rustc-link-search={dst}/build/src/");
    println!("cargo:rustc-link-lib=valhalla");

    // bindings
    cxx_build::bridge("src/lib.rs")
        .file("src/libvalhalla.cpp")
        .std("c++17")
        .includes(valhalla_includes)
        .compile("libvalhalla-cxxbridge");
    println!("cargo:rerun-if-changed=src/lib.rs");
    println!("cargo:rerun-if-changed=src/libvalhalla.hpp");
    println!("cargo:rerun-if-changed=src/libvalhalla.cpp");
}

/// https://clang.llvm.org/docs/JSONCompilationDatabase.html
#[derive(Deserialize)]
struct CompileCommand {
    command: String,
    file: String,
}

fn extract_includes(compile_commands: &Path, cpp_source: &str) -> Vec<String> {
    assert!(compile_commands.exists(), "compile_commands.json not found");

    let content =
        std::fs::read_to_string(compile_commands).expect("Failed to read compile_commands.json");
    let commands: Vec<CompileCommand> =
        json::from_str(&content).expect("Failed to parse compile_commands.json");

    let command = commands
        .into_iter()
        .find(|cmd| cmd.file.ends_with(cpp_source))
        .expect("Failed to find reference cpp source file");

    // Parse -I/path/to/include and -isystem /path/to/include
    let args: Vec<&str> = command.command.split_whitespace().collect();
    let mut includes = Vec::new();

    for i in 0..args.len() {
        if args[i].starts_with("-I") {
            // Handle -I/path/to/include
            includes.push(args[i][2..].to_string());
        } else if args[i] == "-isystem" && i + 1 < args.len() {
            // Handle -isystem /path/to/include
            includes.push(args[i + 1].to_string());
        }
    }
    includes
}
