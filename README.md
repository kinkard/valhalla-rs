# About

C++-to-Rust bindings for [Valhalla](https://github.com/valhalla/valhalla) Routing Engine, powered by [cxx](http://cxx.rs).

`valhalla-rs` provides drop-in infrastructure (`cargo add valhalla` and you're ready) for utility projects that need to access Valhalla's road graph data, expose additional Valhalla functionality, or benefit from calling Valhalla's routing engine in-process.

[valhalla-debug](https://github.com/kinkard/valhalla-debug) demonstrates this use case.

Features:

- [x] **Tile access**: Read Valhalla tiles and access road graph edges (`DirectedEdge`, `EdgeInfo`) and nodes (`NodeInfo`) - see [tiles_tests](tests/tiles_test.rs) for examples
- [x] **Actor API**: Route building and routing operations similar to [Valhalla's Python bindings](https://github.com/valhalla/valhalla/blob/master/src/bindings/python/examples/actor_examples.ipynb) - see [actor_tests](tests/actor_test.rs) for examples
- [ ] **Live traffic**: Write traffic information directly to memory-mapped traffic.tar

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
