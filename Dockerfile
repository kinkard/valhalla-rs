FROM rust:slim-bookworm AS builder

# System dependencies
RUN apt-get update && apt-get install -y --no-install-recommends \
  # Required for tokio and reqwest via `openssl-sys`
  libssl-dev \
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

ENV CC=clang
ENV CXX=clang++

WORKDIR /usr/src/app

# First build a dummy target to cache dependencies in a separate Docker layer
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() { println!("Dummy image called!"); }' > src/main.rs
# And for every other target in the workspace
COPY libvalhalla/Cargo.toml ./libvalhalla/
RUN mkdir -p libvalhalla/src && touch libvalhalla/src/lib.rs
RUN cargo build --release

# libvalhalla compilation takes a lot, worth to move it into a separate cache layer
COPY libvalhalla ./libvalhalla
RUN touch -a -m ./libvalhalla/src/lib.rs
RUN cargo build --release

# Now build the real target
COPY src ./src
# Update modified attribute as otherwise cargo won't rebuild it
RUN touch -a -m ./src/main.rs
RUN cargo build --release

FROM debian:bookworm-slim AS runner
# Web page with map
WORKDIR /usr
COPY web ./web
# Runtime dependency for tokio and reqwest
RUN apt-get update && apt-get install -y --no-install-recommends libssl3
COPY --from=builder /usr/src/app/target/release/valhalla-debug /usr/local/bin/valhalla-debug
ENTRYPOINT ["valhalla-debug"]
