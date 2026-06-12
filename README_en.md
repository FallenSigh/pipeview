# pipeview

**Cross-platform serial / TCP / UDP data inspection tool** with configurable framing, protocol decoding, multi-session management, and real-time waveform plotting — all in a single binary.

Built with Rust + [egui](https://github.com/emilk/egui), scriptable with Lua.

[中文文档](README.md)

---

## Table of Contents

- [Features](#features)
- [Quick Start](#quick-start)
- [Walkthrough: Drone Telemetry](#walkthrough-drone-telemetry)
- [Lua Scripting Guide](#lua-scripting-guide)
- [Architecture](#architecture)
- [Configuration & Persistence](#configuration--persistence)
- [Development Tools](#development-tools)
- [Tech Stack](#tech-stack)
- [License](#license)

---

## Features

### Transport

| Type | Description | Options |
|------|-------------|---------|
| **Serial** | Serial port communication | Port name, baud rate (300–12M), data bits (5/6/7/8), parity (None/Odd/Even), stop bits (1/2), flow control (None/Software/Hardware), DTR/RTS control |
| **TCP** | TCP client | Target address `host:port` |
| **UDP** | UDP communication | Bind address `host:port`, optional remote address |

### Framers

Framers split raw byte streams into discrete frames. Each session supports **multiple independent pipelines** — the same byte stream feeds multiple framer→decoder chains in parallel.

| Framer | Description | Parameters |
|--------|-------------|------------|
| **Line** | Split by `\n` | `strip_cr`, `max_line_len` |
| **Fixed** | Fixed-length frames | `frame_len` |
| **Length** | Length-prefixed protocol | `len_bytes` (1/2/4/8), `endian`, `length_includes_self`, `max_payload` |
| **COBS** | [Consistent Overhead Byte Stuffing](https://en.wikipedia.org/wiki/Consistent_Overhead_Byte_Stuffing) | `max_frame` |
| **MixedTextPlot** | Hybrid text lines + COBS-encoded plot frames on one stream | `strip_cr`, `max_line_len`, `max_plot_frame` |
| **Lua** | User-defined framer script | `script_path` |

### Decoders

Decoders parse framed data into displayable content.

| Decoder | Output | Parameters |
|---------|--------|------------|
| **Text** | Text | `encoding` (UTF-8 / Latin1 / ASCII) |
| **Hex** | Hexadecimal | `uppercase`, `separator`, `bytes_per_group`, `endian` |
| **Plot** | Waveform | `sample_type` (i8–f64), `endian`, `channels` (1–64), `format` (Interleaved / Block / XY) |
| **MixedTextPlot** | Hybrid text + waveform | `encoding` |
| **Lua** | Custom | `script_path` |

**Plot sample formats:**

| Format | Description | Byte layout |
|--------|-------------|-------------|
| **Interleaved** | Channels interleaved | `[ch0_s0, ch1_s0, ch2_s0, ch0_s1, ch1_s1, …]` |
| **Block** | Channel-contiguous blocks | `[ch0_s0, ch0_s1, …, ch1_s0, ch1_s1, …]` |
| **XY** | 2-channel x/y pairs | `[x0, y0, x1, y1, …]` |

Supported sample types: `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `f32`, `f64`

### Views

- **Text View** — formatted text with timestamps, direction markers (`[IN]`/`[OUT]`), and pipeline labels. Built-in search (`Ctrl+F`) with case-sensitive toggle, match counter, and highlighting.
- **Hex View** — hex dump alongside ASCII sidebar. Configurable byte grouping and endianness.
- **Plot View** — real-time waveforms via [egui_plot](https://github.com/emilk/egui/tree/master/crates/egui_plot). Auto-fit, box zoom, axis locking, follow-latest, detached window, channel legend.

### Sessions

- **Multi-session** — tabbed interface, each session independently configured
- **Connection control** — Connect / Disconnect / Reconnect with auto-reconnect on drop
- **Live reconfiguration** — change framer/decoder settings without restarting
- **Data sending** — Text mode (UTF-8, configurable line endings) and Hex mode (raw bytes). Four line ending options: None / LF / CR / CRLF
- **Ring buffer** — default 10,000 entries, configurable 100–1,000,000
- **Log to file** — background writer thread with 1 KB buffer, non-blocking
- **Search** — `Ctrl+F` search bar, case-sensitive toggle, F3/Shift+F3 navigation

### Lua Scripting

Built-in LuaJIT runtime:

- **Custom framers** — implement `feed(bytes)`, `flush()`, `reset()`, `pending_len()`
- **Custom decoders** — implement `decode(frame)`, return text/hex/plot/binary
- **Session API** — `pipeview.open()` to create sessions, `session:on_data()` for event callbacks, `session:send()` to transmit data
- **Utilities** — `pipeview.list_ports()`, `pipeview.sleep(ms)`, `pipeview.poll(limit)`, `pipeview.log(msg)`

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Ctrl+N` | New session |
| `Ctrl+E` | Edit current session |
| `Ctrl+W` | Delete current session |
| `Ctrl+F5` | Toggle connection |
| `Ctrl+T` / `Ctrl+H` / `Ctrl+P` | Switch to Text / Hex / Plot view |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | Next / Previous tab |
| `Ctrl+L` | Clear output |
| `Ctrl+F` | Search |
| `F3` / `Shift+F3` | Next / Previous match |
| `Ctrl+,` | UI settings |
| `Esc` | Close overlay |

---

## Quick Start

### Prerequisites

| Platform | Dependencies |
|----------|-------------|
| **Linux** | `libudev-dev` (`apt install libudev-dev`) |
| **macOS** | None |
| **Windows** | None |
| **All** | Rust ≥ 1.85, C compiler (GCC / Clang / MSVC) |

### Build & Run

```bash
git clone https://github.com/your-org/pipeview.git
cd pipeview

cargo run -p pipeview-gui

# Run tests
cargo test --workspace           # ~308 tests
cargo clippy --workspace --all-targets -- -D warnings
```

### Release Build

```bash
cargo build -p pipeview-gui --release
# Binary: target/release/pipeview-gui (Linux/macOS)
#         target/release/pipeview-gui.exe (Windows)
```

---

## Walkthrough: Drone Telemetry

Parse Betaflight/INAV flight controller telemetry in the format:

```
AHRS q:1.0000,0.0998,0.0499,0.0200|YPR:8.87,4.44,2.96|Gyro:14.78,8.87,5.92|RC:1559,1544,1500,1519|M:1612,1588,1603,1597|L:0 F:1 C:0
```

### Step 1: Start the data source

```bash
python tools/test_drone.py --rate 10 --port 8091
```

### Step 2: Configure the session

In pipeview-gui, create a session with two pipelines:

| Setting | Value |
|---------|-------|
| Transport | `TCP 127.0.0.1:8091` |
| Pipeline 1 | Framer: `Line` → Decoder: `Lua` → `examples/drone_text.lua` |
| Pipeline 2 | Framer: `Line` → Decoder: `Lua` → `examples/drone_plot.lua` |

### Step 3: View the results

- **Text View** — formatted sensor readouts
- **Plot View** — real-time gyroscope curves (gz/gy/gx)

The decoder in `examples/drone_plot.lua`:

```lua
return {
    decode = function(frame)
        local gz, gy, gx = frame:match("Gyro:([%d%.%-]+),([%d%.%-]+),([%d%.%-]+)")
        return {
            kind = "plot",
            channels = { { tonumber(gz) }, { tonumber(gy) }, { tonumber(gx) } },
            sample_type = "F64",
            format = "Block",
        }
    end,
}
```

---

## Lua Scripting Guide

### Framer API

A Lua framer splits raw bytes into frames. The script must return a table with these functions:

```lua
return {
    feed = function(bytes)
        -- bytes: Lua string (raw bytes)
        -- Returns: { frame1, frame2, ... } or nil
    end,

    flush = function()
        -- Returns: last buffered frame (string) or nil
    end,

    reset = function()
    end,

    pending_len = function()
        -- Returns: number of bytes pending in buffer
    end,
}
```

Reference: `tests/lua_line_framer.lua` (line-based framer splitting on `\n`).

### Decoder API

A Lua decoder parses a frame into structured data. The script must return a table with a `decode` function:

```lua
return {
    decode = function(frame)
        -- frame: Lua string (one frame from the framer)
        -- Returns nil → skip this frame
        -- Returns string → treated as Text
        -- Returns table → must contain "kind" field
    end,
}
```

**Return value formats:**

| `kind` | Required | Optional | Purpose |
|--------|----------|----------|---------|
| `"text"` | `data: string` | — | Text view |
| `"hex"` | `data: string` | — | Hex view |
| `"binary"` | `data: string` | — | Raw bytes |
| `"plot"` | `channels: {{number,…},…}` | `sample_type`, `format` | Plot view |

**Plot return value example:**

```lua
return {
    kind = "plot",
    channels = {
        { 1.0, 2.0, 3.0 },  -- channel 0
        { 4.0, 5.0, 6.0 },  -- channel 1
    },
    sample_type = "F64",    -- default F64; also I8/U8/I16/U32/U64/F32
    format = "Block",       -- default Interleaved; also Block/XY
}
```

More examples in `examples/`.

---

## Architecture

```
┌──────────────────────────────────────────────────┐
│  pipeview-gui (egui)       pipeview-tui (ratatui) │
├──────────────────────────────────────────────────┤
│  pipeview-client                                  │
│  SessionManager · Session · Config · History     │
│  Lua Runtime (mlua / LuaJIT)                     │
├──────────────────────────────────────────────────┤
│  pipeview-core                                    │
│  Transport ──▶ Frame ──▶ Protocol ──▶ Pipeline  │
└──────────────────────────────────────────────────┘
```

| Crate | Type | Purpose |
|-------|------|---------|
| `pipeview-core` | library | Transport (Serial/TCP/UDP), framers (Line/Fixed/Length/COBS/Mixed/Lua), decoders (Text/Hex/Plot), MultiPipeline |
| `pipeview-client` | library | Session lifecycle, SessionManager, event broadcast (tokio broadcast), RingBuffer history, Lua runtime & session API |
| `pipeview-gui` | binary | egui desktop app: sidebar, config, console, text/hex/plot panels, keyboard shortcuts, font management, profiling |
| `pipeview-tui` | binary | ratatui terminal app (feature-incomplete, under development) |

**Dependency flow:** `core ← client ← {gui, tui}`

**Data flow:**

```
[Transport] → read bytes → [MultiPipeline]
    → Pipeline 1: Framer → Decoder → DecodedEntry → broadcast → GUI buffers
    → Pipeline 2: Framer → Decoder → DecodedEntry → broadcast → GUI buffers
    → ...
```

Each pipeline frames and decodes independently. Only successful decodes produce output.

---

## Configuration & Persistence

GUI state is saved automatically to platform-standard locations:

| Platform | Path |
|----------|------|
| Linux | `$XDG_CONFIG_HOME/pipeview/gui-state.json` or `~/.config/pipeview/gui-state.json` |
| macOS | `~/Library/Application Support/pipeview/gui-state.json` |
| Windows | `%APPDATA%\pipeview\gui-state.json` |

Persisted data includes: session configurations (transport, pipelines), log settings, active tab, and display options.

Log files are written to `logs/` under the config directory, named `session_{id}_{timestamp}.log`.

---

## Development Tools

### Test Data Generators

```bash
# Plot waveform test data
python tools/test_plot.py --wire-format mixed --host 127.0.0.1 --port 8091
python tools/test_plot.py --wire-format mixed --format xy --channels 2
python tools/test_plot.py --wire-format raw --channels 2 --framelen 256

# Drone telemetry test data
python tools/test_drone.py --rate 10 --port 8092
```

### Profiling

```bash
RUST_LOG=pipeview_gui::perf=info XSERIAL_GUI_PROFILE=1 cargo run -p pipeview-gui
XSERIAL_GUI_PROFILE_INTERVAL_MS=500 cargo run -p pipeview-gui
```

### Tracing

```bash
RUST_LOG=info cargo run -p pipeview-gui
RUST_LOG=pipeview_gui=debug cargo run -p pipeview-gui
```

### Project Structure

```
crates/
  pipeview-core/        # Transport, framing, protocol
  pipeview-client/      # Session management, Lua runtime
  pipeview-gui/         # egui desktop application
  pipeview-tui/         # ratatui terminal application
examples/              # Lua script examples
  drone_plot.lua       # Drone telemetry plot decoder
  drone_text.lua       # Drone telemetry text decoder
tests/                 # Lua test fixtures
tools/                 # Development utilities
  test_plot.py         # Waveform test data generator
  test_drone.py        # Drone test data generator
  test_plot_serial.c   # C serial plot test client
  test_text_serial.c   # C serial text test client
  xs_mixed_plot.h      # MixedTextPlot protocol reference
```

---

## Tech Stack

- **Runtime**: tokio (multi-threaded)
- **Serial**: `tokio-serial` + `serialport`
- **GUI**: egui + egui_plot
- **Lua**: mlua 0.11, LuaJIT (vendored), async/serde/send
- **Events**: `tokio::sync::broadcast` (multi-subscriber)
- **Zero feature flags**, zero build scripts, zero conditional compilation

## License

MIT
