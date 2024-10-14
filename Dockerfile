FROM rust:slim-bookworm as builder

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
RUN mkdir -p libvalhalla/src && echo '' > libvalhalla/src/lib.rs
COPY libvalhalla/Cargo.toml ./libvalhalla/
RUN cargo build --release

# Now build the real target
COPY src ./src
COPY libvalhalla ./libvalhalla
# Update modified attribute as otherwise cargo won't rebuild it
RUN touch -a -m ./src/main.rs
RUN touch -a -m ./libvalhalla/src/lib.rs
RUN cargo build --release

FROM debian:bookworm-slim as runner
# Runtime dependency for tokio and reqwest
RUN apt-get update && apt-get install -y --no-install-recommends libssl3
COPY --from=builder /usr/src/app/target/release/valhalla-debug /usr/local/bin/valhalla-debug
ENTRYPOINT ["valhalla-debug"]
