# About

C++-to-Rust bindings for [Valhalla](https://github.com/valhalla/valhalla) to access road graph tiles, powered by [cxx](http://cxx.rs).

## Dependencies

As Valhalla heavilly relies on system libraries, you need to install the following dependencies to build this project:

```sh
sudo apt-get update && sudo apt-get install -y --no-install-recommends clang pkg-config build-essential cmake libboost-dev liblz4-dev libprotobuf-dev protobuf-compiler zlib1g-dev
```

You can use the provided [Dockerfile](Dockerfile) as a reference for projects that want to use libvalhalla. It demonstrates the necessary dependencies and environment setup.

## License

All code in this project is dual-licensed under either:

- [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0) ([`LICENSE-APACHE`](LICENSE-APACHE))
- [MIT license](https://opensource.org/licenses/MIT) ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
