# Examples

Runnable examples for `valhalla-rs`. Run any of them with `cargo run --example <name>`.

## traffic_debug

A CLI for inspecting and manipulating a live `traffic.tar`, memory-mapped through a `GraphReader`.
Read it if you want to see the `LiveTraffic` read+write API in real use - decoding edge speeds and
per-segment detail (`speed()`, `segments()`), and writing records back (`from_uniform_speed`,
`with_congestion` / `with_incidents` / `with_spare`, `TrafficTile::write_edge_traffic`) - with no
hand-decoding of the raw `u64` bitfield anywhere.

Subcommands: `scan`, `inspect`, `set-speed`, `close`, `reset`, `clear` - see `--help` for details.

```sh
cargo run --example traffic_debug -- \
    --tile-extract path/to/tiles.tar \
    --traffic-extract path/to/traffic.tar \
    scan
```
