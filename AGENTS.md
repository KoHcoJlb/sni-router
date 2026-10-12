# Repository guidance

## Verification and builds

- Run `cargo test --locked`; the only existing test is a Traefik rule-parser smoke test. Focus it with `cargo test --locked router::traefik::tests::test_host_parser -- --exact`. It checks parsing succeeds, not the returned hosts; it needs no running Traefik.
- Use `cargo +nightly fmt --all -- --check` to check formatting (omit `--check` to apply). `rustfmt.toml` uses nightly-only import grouping options.
- `nix build .#default` builds the Crane package. `flake.nix` also exports `nixosModules.default`, but defines no development shell or flake checks.

## Runtime configuration

- Start with `SNI_ROUTER_CONFIG=/path/to/config.toml cargo run --locked`. Without that variable, config is read from `sni-router.toml` in the working directory; that file is gitignored and is not a shipped example.
- `src/config/mod.rs` and `src/config/router.rs` define the schema: required `routers` (can be `[]`) and `[default_upstream]` with `addr`, `http_port`, `https_port`; optional `[listener]` defaults to ports 80/443. Listeners bind all IPv4 interfaces; use high listener ports for unprivileged local runs.
- Traefik is the only router variant: use `[[routers]]` followed by `[routers.traefik]` with `addr`, `username`, `password`. Addresses must be IP literals. Only the credential fields use shell-style environment/home expansion; the whole TOML file is not interpolated.
- The NixOS module generates TOML from `services.sni-router.config` and sets `SNI_ROUTER_CONFIG`. `services.sni-router.environmentFile` supplies systemd environment variables, including those referenced by credentials.
- Logging is fixed by `src/main.rs` to `sni_router=info`; setting `RUST_LOG` does not override it.

## Routing and transport gotchas

- `src/main.rs` runs route refresh alongside HTTP/TLS listeners. Both listeners share `src/listener/tcp.rs`: inspect buffered initial bytes, connect upstream, replay those bytes, then relay. TLS is passed through, not terminated.
- Every upstream connection, including the fallback, receives a binary PROXY protocol v2 header before client bytes. Test backends must accept it; a plain HTTP/TLS server alone is insufficient.
- Discovered routes always connect to the router's IP on ports 80/443. Configured `default_upstream` ports apply only to fallback traffic. Host/SNI lookups are exact string matches; HTTP Host ports and hostname case are not normalized.
- Traefik discovery requests `http://traefik.local/api/http/routers` with basic auth, resolving that hostname to the configured IP. Rules contribute literal ``Host(`name`)`` matches; this is not a general Traefik rule evaluator. `traefik.local` itself is excluded.
- `src/router/mod.rs` rebuilds and replaces the route map, then sleeps 10 seconds. First router wins duplicate hosts; a fetch failure drops that router's routes rather than preserving its last successful result.
- Relay code differs by platform: Linux uses `realm_io::bidi_zero_copy`; other targets use Tokio's buffered copy. Non-Linux checks do not exercise the Linux path.
