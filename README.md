# About

C++-to-Rust bindings for [Valhalla](https://github.com/valhalla/valhalla) Routing Engine, powered by [cxx](http://cxx.rs).

`valhalla-rs` provides drop-in infrastructure (`cargo add valhalla` and you're ready) for utility projects that need to access Valhalla's road graph data, expose additional Valhalla functionality, or benefit from calling Valhalla's routing engine in-process.

[valhalla-debug](https://github.com/kinkard/valhalla-debug) demonstrates this use case.

Features:

- [x] **Tile access**: Read Valhalla tiles and access road graph edges (`DirectedEdge`, `EdgeInfo`) and nodes (`NodeInfo`) - see [tiles_tests](tests/tiles_test.rs) for examples
- [x] **Live traffic**: Write live traffic information directly to memory-mapped traffic.tar - see [tiles_tests](tests/tiles_test.rs) for examples
- [x] **Actor API**: Route building and routing operations similar to [Valhalla's Python bindings](https://github.com/valhalla/valhalla/blob/master/src/bindings/python/examples/actor_examples.ipynb) - see [actor_tests](tests/actor_test.rs) for examples

TODOs:

- [ ] **Logging**: Redirect Valhalla logging to Rust's `tracing` crate or provide an interface for redirecting it to a custom logger.
- [ ] **Reading individual tile files**: Support reading info from Valhalla tiles from `tile_dir` (individual file per tile). Currently only `tile_extract` (single tiles.tar file) is supported.
- [ ] **Historical traffic**: All minor functionality for out-of-the-box historical traffic support. Currently minor stuff should be done manually, such as converting `GraphId` to the tile file name or writing historical speeds (free flow, congested, 5m bins) to the csv files.

Design choices:

- `valhalla::GraphReader` is intended to be as simple as possible and hold no mutable inner state, leaving the caching and other optimizations to the caller. This allows for easy reuse of the same `GraphReader` instance across multiple threads.
- `valhalla::Actor` accepts only `proto::Options` and not `proto::Api` or Valhalla JSON request to have small strongly-typed API. Still, there is a convenience method to convert JSON into `proto::Options` called `valhalla::Actor::parse_json_request()`.
- `valhalla::EdgeInfo::shape` direction is aligned with the edge direction. For comparison, in C++ Valhalla user should revert the shape based on `DirectedEdge::forward` flag (so both forward and reverse edges can use the same `EdgeInfo`). Because of how C++-to-Rust bindings work, additional allocation is required any way, so it was simpler to just always return the shape in the correct direction.

## Usage

Run `cargo add valhalla` or add this to your Cargo.toml:

```toml
[dependencies]
valhalla = "0.6"
```

## Dependencies

Since Valhalla heavily relies on system libraries, you need to install the following dependencies to build this project:

```sh
sudo apt-get update && sudo apt-get install -y --no-install-recommends clang pkg-config build-essential cmake libboost-dev liblz4-dev libprotobuf-dev protobuf-compiler zlib1g-dev
```

See the [Dockerfile](Dockerfile) for a complete reference setup, or the [Valhalla documentation](https://valhalla.github.io/valhalla/building/#platform-specific-builds) for other platforms.

## License

This project provides Rust bindings for the Valhalla routing engine and distributes (via [crates.io](https://crates.io/crates/valhalla)) the Valhalla source code. The entire project is licensed under the [MIT License](LICENSE).

The original Valhalla license is available at [valhalla/COPYING](https://github.com/valhalla/valhalla/blob/master/COPYING).
