# An isolated environment for tests and sanity checks on CI

FROM rust:slim-trixie AS builder

# Rust tools
RUN rustup component add rustfmt clippy

# System dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    # LLVM toolchain for proper LTO support between Rust and C/C++
    clang \
    llvm \
    lld \
    # Valhalla build dependencies
    build-essential \
    cmake \
    libboost-dev \
    liblz4-dev \
    libprotobuf-dev \
    protobuf-compiler \
    zlib1g-dev

# https://doc.rust-lang.org/beta/rustc/linker-plugin-lto.html
ENV CC=clang CXX=clang++ AR=llvm-ar RANLIB=llvm-ranlib
# TODO: Latest Rust requires clang-21, which is not available in apt for trixie.
# Install it for `-Clinker-plugin-lto -Clinker=clang`
ENV RUSTFLAGS="-Clink-arg=-fuse-ld=lld"

WORKDIR /usr/src/app

COPY . .

# Check formatting before building to avoid unnecessary rebuilds
RUN cargo fmt --all -- --check

RUN cargo clippy -- -Dwarnings

RUN cargo test

RUN cargo build --release

# Multi-stage build example:
# ```
# FROM debian:trixie-slim AS runner
# WORKDIR /usr
# # Runtime dependency for valhalla
# RUN apt-get update && apt-get install -y --no-install-recommends libprotobuf-lite32
# # Running integration tests to ensure that all runtime deps are installed correctly
# COPY --from=builder /usr/src/app/target/release/my-app /usr/local/bin/my-app
# ENTRYPOINT [ "my-app" ]
# ```
