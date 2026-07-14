# Examples

Runnable examples for `valhalla-rs`. Each example is set up as its own crate so its dependencies are clear.

Run any of them **from this `examples/` directory** with `cargo run -p <name>`:

## traffic_debug

A CLI for inspecting and manipulating a live `traffic.tar`, memory-mapped through a `GraphReader`.
Read it if you want to see the `LiveTraffic` read+write API in real use - decoding edge speeds and
per-segment detail (`speed()`, `segments()`), and writing records back (`from_uniform_speed`,
`with_congestion` / `with_incidents` / `with_spare`, `TrafficTile::write_edge_traffic`) - with no
hand-decoding of the raw `u64` bitfield anywhere.

Subcommands: `scan`, `inspect`, `set-speed`, `close`, `reset`, `clear` - see `--help` for details.

```sh
cargo run -p traffic_debug -- \
    --tile-extract path/to/tiles.tar \
    --traffic-extract path/to/traffic.tar \
    scan
```

## valhalla-service

Rust version of Valhalla's [`valhalla_service`](https://github.com/valhalla/valhalla/blob/master/src/valhalla_service.cc) that exposes the Actor API over HTTP.
Supports all [Valhalla endpoints](https://valhalla.github.io/valhalla/api/) (`/route`, `/matrix`, `/isochrone`, ...) and both JSON and protobuf request and response formats.

```sh
cargo run -p valhalla-service --release -- path/to/valhalla.json --port 3000 --concurrency 8
```

- `valhalla.json` - a standard Valhalla config (see `valhalla::ConfigBuilder` or `valhalla_build_config`), pointing at your tile extract.
- `--port` - listen port (default `3000`).
- `--concurrency` - number of worker threads / `Actor` instances (defaults to the number of available CPUs).

```sh
curl -s http://localhost:3000/route \
  -H 'content-type: application/json' \
  -d '{"locations":[{"lat":52.52,"lon":13.40},{"lat":52.50,"lon":13.45}],"costing":"auto"}'
```
