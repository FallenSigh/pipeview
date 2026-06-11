# xserial

**跨平台串口 / TCP / UDP 数据观测工具** — 可配置分帧、协议解码、多会话管理，支持文本、十六进制和实时波形绘图。

基于 Rust workspace，GUI 使用 [egui](https://github.com/emilk/egui) + [egui_plot](https://github.com/emilk/egui/tree/master/crates/egui_plot)，支持 Lua 脚本扩展分帧与解码逻辑。

---

## 架构

```
┌─────────────────────────────────────────────────────┐
│  xserial-gui (egui)          xserial-tui (ratatui)  │
│  面板: sidebar, console,        占位实现             │
│        hex_view, plot_view,                         │
│        config                                       │
├─────────────────────────────────────────────────────┤
│  xserial-client                                     │
│  SessionManager · Session · Config · History        │
│  Lua Runtime (mlua/LuaJIT)                          │
│  ┌──────────┐  ┌──────────┐  ┌──────────────────┐   │
│  │ LuaFramer│  │LuaDecoder│  │ Lua Session API  │   │
│  └──────────┘  └──────────┘  └──────────────────┘   │
├─────────────────────────────────────────────────────┤
│  xserial-core                                       │
│  Transport ──▶ Frame ──▶ Protocol ──▶ Pipeline      │
│  (Serial,    (Line,    (Text, Hex,  (MultiPipeline) │
│   TCP, UDP)   Fixed,    Plot,                       │
│               Length,   MixedTextPlot)               │
│               Cobs,                                  │
│               MixedTextPlot)                         │
└─────────────────────────────────────────────────────┘
```

| Crate | 类型 | 职责 |
|-------|------|------|
| `xserial-core` | library | 传输层、分帧器、协议解码器、Pipeline |
| `xserial-client` | library | 会话管理、配置持久化、事件分发、历史缓冲、Lua 运行时 |
| `xserial-gui` | binary | egui 桌面应用（主要入口） |
| `xserial-tui` | binary | ratatui 终端应用（占位，未达功能对等） |

依赖方向: `core ← client ← {gui, tui}`

---

## 构建

**前置条件**:

- **Rust ≥ 1.85**（workspace 使用 `edition = "2024"`）
- **Linux**: `libudev-dev`（`serialport` 依赖）
- **所有平台**: C 编译器（`mlua` 使用 `vendored` 特性从源码编译 LuaJIT）

```bash
# 编译检查
cargo check --workspace

# 运行测试（~266 个）
cargo test --workspace

# 格式化 + Clippy
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

---

## 运行

### GUI（主要入口）

```bash
cargo run -p xserial-gui
```

### TUI（占位实现）

```bash
cargo run -p xserial-tui
```

---

## 功能

### 传输层

| 类型 | 配置项 |
|------|--------|
| **Serial** | 端口名、波特率、数据位 (5/6/7/8)、校验位 (None/Odd/Even)、停止位 (1/2)、流控 (None/Software/Hardware)、DTR、RTS |
| **TCP** | 目标地址 `host:port` |
| **UDP** | 绑定地址 `host:port`，可选远端地址 |

DTR/RTS 控制仅在 Serial 连接时可用。

### 分帧器

| 分帧器 | 说明 | 配置 |
|--------|------|------|
| **Line** | 按换行符 `\n` 分割，可选去除 `\r` | `strip_cr`, `max_line_len` |
| **Fixed** | 固定字节数为一帧 | `frame_len` |
| **Length** | 长度前缀协议帧 | `len_bytes` (1/2/4/8), `endian`, `length_includes_self`, `max_payload` |
| **Cobs** | [COBS](https://en.wikipedia.org/wiki/Consistent_Overhead_Byte_Stuffing) 编码，`0x00` 分隔 | `max_frame` |
| **MixedTextPlot** | 单连接混合文本行 + COBS plot 帧 | `strip_cr`, `max_line_len`, `max_plot_frame` |
| **Lua** | 用户自定义 Lua 脚本 | `script_path` |

### 协议解码器

| 解码器 | 输出 | 配置 |
|--------|------|------|
| **Text** | UTF-8 / Latin1 文本 | `encoding` |
| **Hex** | 十六进制字符串 | `uppercase`, `separator`, `bytes_per_group`, `endian` |
| **Plot** | 数值波形数据 | `sample_type` (i8–f64), `endian`, `channels`, `format` (Interleaved / Block / XY) |
| **MixedTextPlot** | 混合流（文本 + 波形） | `encoding` |
| **Lua** | 用户自定义解码 | `script_path` |

#### Plot 采样格式

| 格式 | 说明 |
|------|------|
| **Interleaved** | 多通道交叉排列: `[ch0_s0, ch1_s0, ch0_s1, ch1_s1, …]` |
| **Block** | 按通道分块: `[ch0_s0, ch0_s1, …, ch1_s0, ch1_s1, …]` |
| **XY** | 2 通道，交替 x/y: `[x0, y0, x1, y1, …]` |

支持的采样类型: `i8`, `u8`, `i16`, `u16`, `i32`, `u32`, `i64`, `u64`, `f32`, `f64`

### Pipeline

每个 Session 可配置多个 Pipeline。同一个字节流被送入所有 Pipeline，每个 Pipeline 独立分帧、解码，互不干扰。例如：

- Pipeline "text": LineFramer → TextDecoder → 文本视图
- Pipeline "hex": FixedFramer(16) → HexDecoder → 十六进制视图
- Pipeline "plot": FixedFramer(256) → PlotDecoder → 波形视图

当数据到达时，只有成功解码的 Pipeline 产生输出。

### 会话管理

- **多会话**: 同时运行多个独立 session
- **连接控制**: Connect / Disconnect / Reconnect
- **自动重连**: 断线后每秒自动重试
- **动态重配置**: 运行时修改分帧器/解码器配置
- **数据发送**: 通过 Console 面板发送原始字节到连接
- **历史缓冲**: 环形缓冲区（默认 10,000 条），可配置上限
- **状态持久化**: Session 配置自动保存到 `gui-state.json`

### Lua 脚本扩展

内置 LuaJIT 运行时，支持：

- **自定义分帧器**: 实现 `feed(bytes)`, `flush()`, `reset()`, `pending_len()` 四个函数
- **自定义解码器**: 实现 `decode(frame)` 函数，返回 `{kind="text"|"hex"|"binary"|"plot", data=…}`
- **会话 API**: `xserial.open(config)` 创建 session，支持 `on_data(callback)` 事件回调
- **辅助函数**: `xserial.list_ports()`, `xserial.sleep(ms)`, `xserial.poll(limit)`, `xserial.log(msg)`

示例 Lua Line Framer:

```lua
local buffer = ""
return {
    feed = function(bytes)
        buffer = buffer .. bytes
        local frames = {}
        while true do
            local i = buffer:find("\n", 1, true)
            if not i then break end
            frames[#frames + 1] = buffer:sub(1, i - 1)
            buffer = buffer:sub(i + 1)
        end
        return frames
    end,
    flush = function()
        if #buffer == 0 then return nil end
        local frame = buffer; buffer = ""; return frame
    end,
    reset = function() buffer = "" end,
    pending_len = function() return #buffer end,
}
```

---

## GUI

### 面板

| 面板 | 功能 |
|------|------|
| **Sidebar** | Session 列表、创建/删除/编辑 session、连接切换 |
| **Config** | 传输参数、Pipeline 配置（分帧器 + 解码器类型及参数） |
| **Console** | 数据发送输入框，发送按钮 |
| **Text View** | 解码后的文本流，支持时间戳和方向标记 |
| **Hex View** | 十六进制字节展示，可配置分组和分隔符 |
| **Plot View** | 实时波形图，支持内嵌和浮动窗口两种模式 |

### 快捷键

| 快捷键 | 操作 |
|--------|------|
| `Ctrl+Tab` | 下一个 Tab |
| `Ctrl+Shift+Tab` | 上一个 Tab |
| `Ctrl+T` | 切换到 Text 视图 |
| `Ctrl+H` | 切换到 Hex 视图 |
| `Ctrl+P` | 切换到 Plot 视图 |
| `Ctrl+N` | 新建 Session |
| `Ctrl+E` | 编辑当前 Session |
| `Ctrl+W` | 删除当前 Session |
| `Ctrl+F5` | 切换连接 |
| `Ctrl+L` | 清空输出 |
| `Ctrl+,` | UI 设置 |
| `Esc` | 关闭浮层 |

### 状态持久化

GUI 状态（session 配置、活动 tab、显示选项）自动保存到：

| 平台 | 路径 |
|------|------|
| Linux | `$XDG_CONFIG_HOME/xserial/gui-state.json` 或 `~/.config/xserial/gui-state.json` |
| macOS | `~/Library/Application Support/xserial/gui-state.json` |
| Windows | `%APPDATA%\xserial\gui-state.json` |

---

## 开发工具

### Plot 测试服务器

```bash
# 启动 TCP 波形数据源
python tools/test_plot.py --wire-format mixed --host 127.0.0.1 --port 8091

# XY 格式，2 通道
python tools/test_plot.py --wire-format mixed --format xy --channels 2

# 固定长度 raw plot 帧
python tools/test_plot.py --wire-format raw --channels 2 --framelen 256
```

GUI 中配置: Transport `TCP 127.0.0.1:8091`, Framer `MixedTextPlot`, Decoder `MixedTextPlot`

### 性能分析

```bash
# 开启性能日志
RUST_LOG=xserial_gui::perf=info XSERIAL_GUI_PROFILE=1 cargo run -p xserial-gui

# 自定义统计间隔（默认 1000ms）
XSERIAL_GUI_PROFILE_INTERVAL_MS=500 cargo run -p xserial-gui
```

日志输出: frame 耗时、事件 drain 耗时、text/hex/plot 渲染耗时、plot 点数统计

### 日志

```bash
RUST_LOG=info cargo run -p xserial-gui
RUST_LOG=xserial_gui=debug cargo run -p xserial-gui
```

使用 `tracing-subscriber` + `RUST_LOG` 环境变量控制日志级别。

### 测试

```bash
# 全部测试
cargo test --workspace

# 特定 crate
cargo test -p xserial-core
cargo test -p xserial-client

# 集成测试
cargo test -p xserial-core --test pipeline
cargo test -p xserial-client --test session_lifecycle
cargo test -p xserial-client --test lua_tests

# 带输出
cargo test -p xserial-core -- --nocapture
```

所有测试自包含（TCP/UDP 绑定 `127.0.0.1:0`，串口测试使用虚拟端口名），无外部依赖。

---

## 目录结构

```
crates/
  xserial-core/          # 传输、分帧、协议解码、Pipeline
    src/
      transport/         # Serial, TCP, UDP
      frame/             # Line, Fixed, Length, Cobs, MixedTextPlot
      protocol/          # Text, Hex, Plot, MixedTextPlot
      pipeline.rs        # MultiPipeline
  xserial-client/        # 会话管理、Lua 运行时
    src/
      session.rs         # Session 事件循环
      manager.rs         # SessionManager
      config.rs          # SessionConfig, FramerConfig, DecoderConfig
      event.rs           # DecodedEntry
      history.rs         # RingBuffer
      cmd.rs             # SessionCmd
      lua/               # Lua 运行时、自定义 codec、会话 API
  xserial-gui/           # egui 桌面应用
    src/
      main.rs
      app.rs             # XserialApp 主控制器
      panels/            # sidebar, config, console, hex_view, plot_view
      shortcuts.rs       # 键盘快捷键
      ui_fonts.rs        # 字体加载
      perf.rs            # 性能分析
      app_state.rs       # 状态持久化
      buffers.rs         # UI 缓冲管理
  xserial-tui/           # ratatui 终端应用 (占位)
tests/                   # Lua 测试 fixture
  lua_line_framer.lua
  lua_text_decoder.lua
tools/                   # 开发辅助工具
  test_plot.py           # TCP 波形测试服务器
  test_plot_serial.c     # C 串口 plot 测试客户端
  test_text_serial.c     # C 串口文本测试客户端
  xs_mixed_plot.h        # MixedTextPlot 线格式 C 参考头文件
```

---

## MixedTextPlot 协议

单连接上混合传输文本行和波形数据的线格式。

**文本帧**: 普通文本行，以 `\n` 分隔。

**Plot 帧**:
```
0x1E 'P' | COBS(plot_packet) | 0x00
```

**Plot Packet** (13 字节头 + payload):
```
'X' 'P'          (magic)
version:u8       (固定 1)
format:u8        (0=Interleaved, 1=Block, 2=XY)
sample_type:u8   (F32=8)
endian:u8        (0=Little)
channels:u8
samples_per_channel:u16le
payload_len:u32le
payload:bytes    (channels × samples_per_channel × sample_byte_size)
```

参考实现: `tools/xs_mixed_plot.h`（C header）

---

## 技术细节

- **异步运行时**: tokio (multi-thread)
- **串口**: `tokio-serial` + `serialport`
- **分帧器特征**: `feed()`, `flush()`, `reset()`, `pending_len()` — 有状态字节流分帧
- **解码器特征**: `name()`, `decode(&[u8]) -> Option<DecodedData>` — 无状态帧解码
- **Session 事件循环**: `tokio::select!` 多路复用命令、读取、重连定时器
- **事件分发**: `tokio::sync::broadcast` — 多订阅者
- **Lua**: `mlua` 0.11, LuaJIT (vendored 编译), 支持 async/serde/send
- **无 feature flags**: 零条件编译，无 build script
- **无 CI**: 本地自检 `cargo test --workspace` + `cargo clippy`
