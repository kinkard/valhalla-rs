# About

C++-to-Rust bindings for [Valhalla](https://github.com/valhalla/valhalla) to access road graph tiles, powered by [cxx](http://cxx.rs).

## Dependencies

As Valhalla heavilly relies on system libraries, you need to install the following dependencies to build this project:

```sh
sudo apt-get update && sudo apt-get install -y --no-install-recommends clang pkg-config build-essential cmake libboost-dev liblz4-dev libprotobuf-dev protobuf-compiler zlib1g-dev
```

You can use the provided [Dockerfile](Dockerfile) as a reference for projects that want to use `valhalla-rs`. It demonstrates the necessary dependencies and environment setup.

For more details check the [Valhalla documentation](https://valhalla.github.io/valhalla/building/#platform-specific-builds).

## License

This project contains Rust bindings for the Valhalla routing engine. The entire project is licensed under the MIT License.

- **valhalla-rs bindings**: Copyright (c) 2025 kinkard
- **Valhalla source code**: Copyright (c) 2018 Valhalla contributors, Copyright (c) 2015-2017 Mapillary AB, Mapzen

Both components are licensed under the [MIT License](LICENSE).

For the original Valhalla license, see [valhalla/COPYING](valhalla/COPYING).
