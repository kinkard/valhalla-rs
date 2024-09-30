FROM rust:alpine as builder

# Dependencies for some crates
RUN apk add --no-cache alpine-sdk openssl-dev openssl-libs-static

WORKDIR /usr/src/app

# First build a dummy target to cache dependencies in a separate Docker layer
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main() { println!("Dummy image called!"); }' > src/main.rs
RUN cargo build --release

# Now build the real target
COPY src ./src
# Update modified attribute as otherwise cargo won't rebuild it
RUN touch -a -m ./src/main.rs
RUN cargo build --release

FROM alpine as runtime
COPY --from=builder /usr/src/app/target/release/valhalla-debug /usr/local/bin/valhalla-debug
CMD ["valhalla-debug"]
