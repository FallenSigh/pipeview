# Repository Guidelines

## Build Prerequisites

- **Linux**: `libudev-dev` (for `serialport`).
- **All platforms**: a C compiler (`cc`, `gcc`, `clang`, or MSVC) — `mlua` uses `features = ["vendored"]` which compiles LuaJIT from source at build time.
- **No pinned toolchain**: all crates use `edition = "2024"` (requires Rust ≥ 1.85). No `rust-toolchain.toml`; builds with whatever `rustc` is on `PATH`.

## Project Structure

`xserial` is a Rust workspace. The root `Cargo.toml` defines four crates in `crates/`, with dependencies flowing:

```
xserial-core  (no internal deps — transport, framing, protocol, pipeline)
     ↑
xserial-client  (depends on xserial-core — sessions, history, commands, Lua)
  ↑        ↑
xserial-gui   xserial-tui
(both depend on xserial-core AND xserial-client)
```

| Crate | Type | What goes here |
|-------|------|---------------|
| `xserial-core` | library (`src/lib.rs`) | `transport/`, `frame/`, `protocol/`, `pipeline.rs` — protocol and I/O. Integration tests: `tests/pipeline.rs`. |
| `xserial-client` | library (`src/lib.rs`) | `session.rs`, `manager.rs`, `config.rs`, `history.rs`, `lua/` — state management and Lua bindings. Integration tests: `tests/session_lifecycle.rs`, `tests/lua_tests.rs`. |
| `xserial-gui` | binary (`src/main.rs`) | egui/eframe app. Panels in `src/panels/`. Font helpers in `src/ui_fonts.rs`. Profiling in `src/perf.rs`. |
| `xserial-tui` | binary (`src/main.rs`) | ratatui/crossterm app. **Still a placeholder** — zero tests, not at feature parity with GUI. |

**Module boundaries**: transport/protocol → `xserial-core`. Session/Lua → `xserial-client`. UI state → `xserial-gui` or `xserial-tui`. Never put I/O logic in the UI crates, never put UI state in the client crate.

## Build, Test, and Development Commands

```bash
cargo check --workspace              # fast compile check (all crates)
cargo test --workspace               # full suite (~266 tests)
cargo fmt --all                      # standard Rust formatting
cargo clippy --workspace --all-targets -- -D warnings   # lint gate
```

**Run specific tests:**

```bash
cargo test -p xserial-core -- tcp_line_text_utf8
cargo test -p xserial-client -- session_echo_send_and_read
cargo test -p xserial-core -- --nocapture          # show test output

# Integration tests only
cargo test -p xserial-core --test pipeline
cargo test -p xserial-client --test session_lifecycle
cargo test -p xserial-client --test lua_tests
```

**Launch:**

```bash
cargo run -p xserial-gui
cargo run -p xserial-tui
```

## Environment & Configuration

### Tracing (all entrypoints)

```bash
RUST_LOG=info cargo run -p xserial-gui
RUST_LOG=xserial_gui::perf=debug cargo run -p xserial-gui
```

`tracing_subscriber::EnvFilter::from_default_env()` reads `RUST_LOG`. The workspace dep `tracing-subscriber` has `features = ["env-filter"]`.

### GUI Profiling

```bash
RUST_LOG=xserial_gui::perf=info XSERIAL_GUI_PROFILE=1 cargo run -p xserial-gui
XSERIAL_GUI_PROFILE_INTERVAL_MS=500 cargo run -p xserial-gui    # custom interval (default 1s)
```

Profiling logs go to target `xserial_gui::perf`.

### State & Config File Paths (runtime, not compile-time)

Uses `env::consts::OS` + env vars — no `cfg(target_os)` gating for path resolution:

| Platform | Root config path |
|----------|-----------------|
| Linux | `$XDG_CONFIG_HOME/xserial/` or `~/.config/xserial/` |
| macOS | `~/Library/Application Support/xserial/` |
| Windows | `%APPDATA%\xserial\` |

State files: `gui-state.json`, `tui-state.json`, `font-settings.json`.

### GUI Plot Testing

```bash
python tools/test_plot.py --wire-format mixed --host 127.0.0.1 --port 8091
python tools/test_plot.py --wire-format mixed --format xy --channels 2
python tools/test_plot.py --wire-format raw --channels 2 --framelen 256
```

Configure in GUI: Transport `TCP 127.0.0.1:8091`, Framer `MixedTextPlot`, Decoder `MixedTextPlot`.

The wire format spec lives in `tools/xs_mixed_plot.h` (C header, reference only — not compiled into Rust).

## Coding Style

Standard Rust conventions (`cargo fmt`). No crate-level lint attrs, no `[lints]` table in any `Cargo.toml`. Names: `snake_case` for modules/files/functions/tests, `PascalCase` for types, `SCREAMING_SNAKE_CASE` for constants.

## Testing Guidelines

- **Inline unit tests** (`#[cfg(test)] mod tests`) next to implementation — 24 source files with `#[test]` or `#[tokio::test]`.
- **Integration tests** in `crates/<crate>/tests/*.rs` — 3 files covering pipeline, session lifecycle, and Lua bindings.
- **~266 tests total** — all active (zero `#[ignore]`). Single `#[should_panic]` in `frame/fixed.rs`.
- **All tests are self-contained**: TCP/UDP bind to `127.0.0.1:0` (OS-assigned port). No hardcoded ports. No external services. Serial tests use dummy port names and never open real hardware.
- **Async tests** use `#[tokio::test]` (no `async-std`). **No benchmarks** exist.
- Run `cargo test --workspace` before opening PRs. Add targeted regression tests for transport, framing, parsing, and session-state fixes.

## Known Quirks & Constraints

- **No CI** — no `.github/workflows` or other CI config. Agents must self-verify with `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`.
- **No feature flags** — zero `[features]` in any `Cargo.toml`. No `#[cfg(feature)]` in any source.
- **No build scripts** — zero `build.rs` files. No codegen. No `include!`/`include_str!`/`include_bytes!`.
- **All `unsafe` is test-only** — constructing noop wakers for poll-based unit tests in `transport/`. No non-test `unsafe` anywhere.
- **`xserial-tui` is a stub** — no tests, minimal implementation. GUI is the primary target.
- **Platform font directories** use `#[cfg(target_os)]` in `ui_fonts.rs` (three blocks: Windows, macOS, Linux/BSD).
- **`tokio` features differ by build**: `xserial-client` uses `["sync", "time"]` for production, `["full"]` for dev-dependencies (tests need net/IO).

## Commit & PR Guidelines

Short, imperative subjects (`Persist GUI font settings`), occasional `feat:` prefix for major additions. PRs: state which crate(s) changed, list validation commands, include screenshots/recordings for GUI/TUI changes. Explicitly call out protocol, transport, or Lua API compatibility impacts.

## Reference

- `tools/test_plot.py --help` — TCP plot-frame generator options.
- `tools/xs_mixed_plot.h` — wire format spec for MixedTextPlot (COBS-framed binary plot + text on one stream).
