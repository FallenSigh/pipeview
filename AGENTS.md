# Repository Guidelines

## Project Structure & Module Organization
`xserial` is a Rust workspace. The root [Cargo.toml](/D:/Dev/xserial/Cargo.toml) defines four crates in `crates/`:
- `xserial-core`: transport, framing, protocol, and pipeline primitives in `src/`, with integration tests in `tests/pipeline.rs`.
- `xserial-client`: session management, history, commands, and Lua bindings, with integration tests under `tests/`.
- `xserial-tui`: terminal app entrypoint in `src/main.rs`.
- `xserial-gui`: egui/eframe app, with UI panels in `src/panels/` and font helpers in `src/ui_fonts.rs`.

Use `tools/test_plot.py` as a local waveform source for GUI plot testing. Treat `target/` as generated output.

## Build, Test, and Development Commands
- `cargo check --workspace`: fast compile check for all crates.
- `cargo test --workspace`: run the workspace test suite, including `crates/xserial-core/tests` and `crates/xserial-client/tests`.
- `cargo run -p xserial-gui`: launch the desktop GUI.
- `cargo run -p xserial-tui`: launch the terminal UI.
- `cargo fmt --all`: apply standard Rust formatting.
- `cargo clippy --workspace --all-targets -- -D warnings`: catch lint issues before review.
- `python tools/test_plot.py --help`: inspect options for the TCP plot-frame generator.

## Coding Style & Naming Conventions
Follow standard Rust formatting: 4-space indentation, trailing commas in multiline literals, and `snake_case` for modules, files, functions, and tests. Use `PascalCase` for structs/enums and `SCREAMING_SNAKE_CASE` for constants. Keep crate boundaries clean: protocol/transport code belongs in `xserial-core`; session and Lua integration belong in `xserial-client`; UI state stays in `xserial-gui` or `xserial-tui`.

## Testing Guidelines
Prefer focused unit tests next to implementation and integration tests in `crates/<crate>/tests/*.rs`. Name tests after behavior, for example `session_lifecycle` or `pipeline`. Run `cargo test --workspace` before opening a PR; add targeted regression tests for transport, framing, parsing, and session-state fixes.

## Commit & Pull Request Guidelines
Recent history uses short, imperative subjects such as `Persist GUI font settings` and `Improve session controls and GUI views`, with occasional `feat:` prefixes for major additions. Keep subjects concise and action-oriented. PRs should state which crate(s) changed, list validation commands, and include screenshots or short recordings for GUI/TUI changes. Call out protocol, transport, or Lua API compatibility impacts explicitly.
