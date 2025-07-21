# An isolated environment for tests and sanity checks on CI

FROM rust:slim-bookworm AS builder

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
    libprotobuf-dev \
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
# WORKDIR /usr
# # Runtime dependency for valhalla
# RUN apt-get update && apt-get install -y --no-install-recommends libprotobuf-lite32
# # Running integration tests to ensure that all runtime deps are installed correctly
# COPY --from=builder /usr/src/app/target/release/my-app /usr/local/bin/my-app
# ENTRYPOINT [ "my-app" ]
# ```
