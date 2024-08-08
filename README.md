# About

Small tool for debugging [Valhalla](https://github.com/valhalla/valhalla) routes. The main reason to have some backend in this tool is to workaround inability to send requests to Valhalla directly from the web page due to [CORS](https://en.wikipedia.org/wiki/Cross-origin_resource_sharing) that [Valhalla doesn't support](https://github.com/valhalla/valhalla/issues/4328).

This tool expects that Valhalla is available at `http://localhost:8002/` for simplicity, so either run Valhalla locally or tunnel port via ssh to where it run.

## Build & Run

```sh
cargo run --release
```

Note: `MAPBOX_ACCESS_TOKEN` env variable is used for Mapbox GL JS, which can be set by following:

```sh
MAPBOX_ACCESS_TOKEN="my access token" cargo run --release
```
