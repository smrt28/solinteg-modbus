# Repository Guidelines

## Project Structure & Module Organization

This repository is a Rust binary crate for reading Solinteg inverter data over Modbus TCP and optionally pushing readings to InfluxDB. The main application lives in `src/main.rs`, including configuration parsing, Modbus reads, output formatting, InfluxDB writes, and inline unit tests. `Cargo.toml` defines package metadata and dependencies. `example-config.toml` shows the expected runtime configuration shape. `notes.md` contains manual Modbus probing notes and register references. Build output belongs in `target/` and should not be committed.

## Build, Test, and Development Commands

- `cargo build`: compile the debug binary.
- `cargo run -- -c example-config.toml -1`: run a single inverter read with an explicit config file.
- `cargo run -- -c example-config.toml -j -1`: run one read and print JSON.
- `cargo test`: run all unit tests in `src/main.rs`.
- `cargo fmt`: apply standard Rust formatting.
- `cargo clippy --all-targets --all-features`: run lint checks when Clippy is installed.

The default config path is `$HOME/.config/solimon`; use `-c <path>` for local testing.

## Coding Style & Naming Conventions

Use Rust 2021 idioms and keep code formatted with `rustfmt`. Prefer four-space indentation, `snake_case` for functions and fields, `PascalCase` for structs, and descriptive names tied to inverter or InfluxDB concepts. Return `anyhow::Result` from fallible functions and add context with `.context()` or `.with_context()` for I/O, parsing, network, and protocol errors. Keep register conversion helpers small and testable.

## Testing Guidelines

Tests currently live in the `#[cfg(test)]` module inside `src/main.rs`. Name tests after the behavior under test, such as `config_path_from_args_uses_explicit_c_value`. Add tests for parsing, formatting, URL construction, line protocol, and numeric register conversions. Avoid tests that require a live inverter or InfluxDB instance unless they are clearly marked and isolated from `cargo test`.

## Commit & Pull Request Guidelines

Recent commits use short, imperative summaries such as `json output, default config in ~/.config/solimon` and `Home load added`. Keep commit subjects concise and focused on the behavior changed. Pull requests should include a brief description, commands run (`cargo test`, `cargo fmt`, Clippy if used), configuration impacts, and any hardware or service assumptions. Include sample output when changing CLI output or InfluxDB line protocol.

## Security & Configuration Tips

Do not commit real InfluxDB tokens, passwords, private keys, or site-specific secrets. Keep local credentials in an untracked config file and use `example-config.toml` only for sanitized examples. Treat inverter IPs, bucket IDs, and organization IDs as environment-specific values.
