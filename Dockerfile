# An isolated environment for tests and sanity checks on CI

# Valhalla relies on protobuf dynamic library that should match in both build and runtime environments.
# It would probably be easy just to use `libprotobuf-dev` in both places, but `libprotobuf-lite32` is much smaller.
ARG protobuf_version=3.21.12-3

FROM rust:slim-bookworm AS builder
ARG protobuf_version

# Rust tools
RUN rustup component add rustfmt clippy

# System dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
    pkg-config \
    # For some reason GCC fails to compile valhalla so we use clang instead
    clang \
    # Valhalla build dependencies
    build-essential \
    cmake \
    libboost-dev \
    liblz4-dev \
    libprotobuf-dev=$protobuf_version \
    protobuf-compiler \
    zlib1g-dev

ENV CC=clang CXX=clang++

WORKDIR /usr/src/app

COPY . .

# Check formatting before building to avoid unnecessary rebuilds
RUN cargo fmt --all -- --check

RUN cargo clippy -- -Dwarnings

RUN cargo test

RUN cargo build --release

# Multi-stage build example:
# ```
# FROM debian:bookworm-slim AS runner
# ARG protobuf_version
# WORKDIR /usr
# # Runtime dependency for valhalla
# RUN apt-get update && apt-get install -y --no-install-recommends libprotobuf-lite32=$protobuf_version
# # Running integration tests to ensure that all runtime deps are installed correctly
# COPY --from=builder /usr/src/app/target/release/my-app /usr/local/bin/my-app
# ENTRYPOINT [ "my-app" ]
# ```
