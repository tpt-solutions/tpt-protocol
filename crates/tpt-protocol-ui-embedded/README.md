# tpt-protocol-ui-embedded

Native embedded UI for the **TPT Protocol** daemon, built with
[Slint](https://slint.dev). Renders a live, high-contrast dashboard of TUP data
points streamed over the daemon's local WebSocket (bound to `127.0.0.1` only).

```sh
cargo run -p tpt-protocol-ui-embedded -- --url ws://127.0.0.1:9080
```

The daemon must be running in simulated or real mode:

```sh
cargo run -p tpt-protocol-daemon -- --simulate modbus
```

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE) at
your option.
