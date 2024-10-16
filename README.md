# About

Valhalla Debug is a tool for debugging Valhalla routes and investigating routing/data issues.
The tool is straightforward and not particularly user-friendly, but it does allow you to:

- **Build a route and experiment with different parameters:** Left-click on the map to add a waypoint, or manually enter waypoints in the box at the top left, then press “Do route.” You can drag pins to adjust them, but the only way to remove a waypoint is to edit the box directly. To reset everything, clear the box and press “Do route” again.
- **Visualize route expansion:** After pressing “Do expansion,” you can use the slider to view the expansion process and identify problem spots in the road graph.
Investigate road graph edges: Right-click on the map to bring up popups with information on the road graph edges that Valhalla is using. Multiple popups can be opened at once.
- **View current traffic data:** Click the “Show traffic” button to see the current traffic that Valhalla uses. For performance reasons, traffic data is limited in amount; zooming out too far will reduce traffic details and may eventually hide traffic entirely. Keep in mind there are multiple levels of edges (highways, arterials, and local roads), so zooming in further before pressing “Show traffic” will display more details.

![image](https://github.com/user-attachments/assets/c0040c58-3a1b-4da7-930b-0a90e951d476)

```
$ ./valhalla-debug --help
Usage: valhalla-debug [OPTIONS]

Options:
      --port <PORT>
          Port to listen [default: 3000]
      --concurrency <CONCURRENCY>
          Max threads to use [default: 4]
      --route-prefix <ROUTE_PREFIX>
          A common prefix for all routes, useful for resolving route conflicts in multi-service environments [default: ]
      --valhalla-url <VALHALLA_URL>
          Valhalla base url to send requests to [default: http://localhost:8002]
      --valhalla-config-path <VALHALLA_CONFIG_PATH>
          Path to valhalla json config file. Required for an access to valhalla graph information
  -h, --help
          Print help
```

## Build & Run

```sh
cargo run --release
```

Note: `MAPBOX_ACCESS_TOKEN` env variable is used for Mapbox GL JS, which can be set by following:

```sh
MAPBOX_ACCESS_TOKEN="my access token" cargo run --release
```

## License

All code in this project is dual-licensed under either:

- [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0) ([`LICENSE-APACHE`](LICENSE-APACHE))
- [MIT license](https://opensource.org/licenses/MIT) ([`LICENSE-MIT`](LICENSE-MIT))

at your option.
