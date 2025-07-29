# About

C++-to-Rust bindings for [Valhalla](https://github.com/valhalla/valhalla) to access road graph tiles, powered by [cxx](http://cxx.rs).

The main motivation behind `valhalla-rs` is to provide easy-to-start infrastructure (`cargo add valhalla` and you're done) for small utility projects that need to access Valhalla's road graph data, expose additional Valhalla functionality, or where calling Valhalla's routing engine in-process is beneficial.

[valhalla-debug](https://github.com/kinkard/valhalla-debug) is a good example of such a project.

Features:

- [x] Reading Valhalla tiles and accessing information from road graph edges (`DirectedEdge`, `EdgeInfo`) and nodes (`NodeInfo`) - see [tiles_tests](tests/tiles_test.rs) for usage examples. _Not all getters are accessible from Rust, but can be easily added_.
- [x] Actor API (similar to [what is accessible from Python](https://github.com/valhalla/valhalla/blob/master/src/bindings/python/examples/actor_examples.ipynb)) for Valhalla's routing engine, allowing route building and other routing operations from Rust - see [actor_tests](tests/actor_test.rs) for usage examples.
- [ ] Writing live traffic information directly to memory-mapped traffic.tar

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

The [Dockerfile](Dockerfile) with tests and sanity checks can be used as a reference for necessary dependencies and environment setup.

For more details (and dependencies for other platforms), check the [Valhalla documentation](https://valhalla.github.io/valhalla/building/#platform-specific-builds).

## License

This project contains Rust bindings for the Valhalla routing engine. The entire project is licensed under the MIT License.

- **valhalla-rs bindings**: Copyright (c) 2025 kinkard
- **Valhalla source code**: Copyright (c) 2018 Valhalla contributors, Copyright (c) 2015-2017 Mapillary AB, Mapzen

Both components are licensed under the [MIT License](LICENSE).

For the original Valhalla license, see [valhalla/COPYING](valhalla/COPYING).
