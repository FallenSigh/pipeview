# pipeview

**跨平台串口 / TCP / UDP 数据观测工具** — 可配置分帧、协议解码、多会话管理、实时波形绘图，单二进制零依赖。

基于 Rust + [egui](https://github.com/emilk/egui) 构建，支持 Lua 脚本扩展。

[English](README_en.md)

---

## 目录

- [功能特性](#功能特性)
- [快速开始](#快速开始)
- [完整示例：无人机遥测解析](#完整示例无人机遥测解析)
- [Lua 脚本开发指南](#lua-脚本开发指南)
- [架构](#架构)
- [配置与持久化](#配置与持久化)
- [开发工具](#开发工具)
- [技术栈](#技术栈)
- [License](#license)

---

## 功能特性

### 传输层

| 类型 | 说明 | 配置项 |
|------|------|--------|
| **Serial** | 串口通信 | 端口名、波特率 (300–12M)、数据位 (5/6/7/8)、校验位 (None/Odd/Even)、停止位 (1/2)、流控 (None/Software/Hardware)、DTR/RTS 控制 |
| **TCP** | TCP 客户端 | 目标地址 `host:port` |
| **UDP** | UDP 通信 | 绑定地址 `host:port`，可选远端地址 |

### 分帧器（Framer）

将原始字节流切分为独立帧。每个 Session 可配置**多条独立管线**，同一字节流并行送入多个 framer→decoder 链路。

| 分帧器 | 说明 | 配置参数 |
|--------|------|----------|
| **Line** | 按 `\n` 分割文本行 | `strip_cr`（去除 `\r`）、`max_line_len`（最大行长度） |
| **Fixed** | 固定字节数为一帧 | `frame_len`（帧长度） |
| **Length** | 长度前缀协议 | `len_bytes` (1/2/4/8)、`endian` (大小端)、`length_includes_self`、`max_payload` |
| **COBS** | [Consistent Overhead Byte Stuffing](https://en.wikipedia.org/wiki/Consistent_Overhead_Byte_Stuffing) 编码 | `max_frame`（最大帧长） |
| **MixedTextPlot** | 单连接混合文本行 + COBS 编码的 plot 帧 | `strip_cr`、`max_line_len`、`max_plot_frame` |
| **Lua** | 用户自定义分帧脚本 | `script_path`（Lua 脚本路径） |

### 解码器（Decoder）

将帧数据解析为可展示的内容。

| 解码器 | 输出类型 | 配置参数 |
|--------|----------|----------|
| **Text** | 文本 | `encoding`（UTF-8 / Latin1 / ASCII） |
| **Hex** | 十六进制 | `uppercase`、`separator`、`bytes_per_group`、`endian` |
| **Plot** | 波形数据 | `sample_type` (i8–f64)、`endian`、`channels` (1–64)、`format` (Interleaved / Block / XY) |
| **MixedTextPlot** | 混合文本 + 波形 | `encoding` |
| **Lua** | 自定义 | `script_path`（Lua 脚本路径） |

**Plot 采样格式：**

| 格式 | 说明 | 字节排列 |
|------|------|----------|
| **Interleaved** | 多通道交叉排列 | `[ch0_s0, ch1_s0, ch2_s0, ch0_s1, ch1_s1, …]` |
| **Block** | 按通道分块排列 | `[ch0_s0, ch0_s1, …, ch1_s0, ch1_s1, …]` |
| **XY** | 2 通道交替 x/y | `[x0, y0, x1, y1, …]` |

支持的采样类型：`i8`、`u8`、`i16`、`u16`、`i32`、`u32`、`i64`、`u64`、`f32`、`f64`

### 视图

- **文本视图** — 带时间戳、方向标记 (`[IN]`/`[OUT]`)、管线标签的格式化文本流。支持搜索（`Ctrl+F`）、大小写匹配、匹配计数和高亮。
- **十六进制视图** — hex dump 与 ASCII 侧栏并排展示，可按分组和大小端解析多字节数值。
- **波形视图** — 基于 [egui_plot](https://github.com/emilk/egui/tree/master/crates/egui_plot) 的实时波形。支持自动缩放、框选缩放、坐标轴锁定、跟随最新数据、浮动窗口、通道图例。

### 会话管理

- **多会话并发** — 标签页切换，每个 Session 独立配置
- **连接控制** — Connect / Disconnect / Reconnect，支持断线自动重连
- **运行时重配置** — 修改分帧器/解码器参数无需断开连接
- **数据发送** — Text 模式（UTF-8，可选换行符）和 Hex 模式（十六进制字节）。支持 `None` / `LF` / `CR` / `CRLF` 四种行尾
- **环形缓冲** — 默认 10,000 条历史记录，可配置 100–1,000,000
- **日志到文件** — 每行数据实时写入，1KB 缓冲 + 后台线程，不阻塞 UI
- **搜索** — `Ctrl+F` 呼出搜索栏，大小写敏感切换，F3/Shift+F3 前后跳转

### Lua 脚本扩展

内置 LuaJIT 运行时：

- **自定义分帧器** — 实现 `feed(bytes)`、`flush()`、`reset()`、`pending_len()` 四个函数
- **自定义解码器** — 实现 `decode(frame)` 函数，返回 text/hex/plot/binary 四种类型
- **会话 API** — `pipeview.open()` 创建 Session、`session:on_data()` 事件回调、`session:send()` 发送数据
- **工具函数** — `pipeview.list_ports()`、`pipeview.sleep(ms)`、`pipeview.poll(limit)`、`pipeview.log(msg)`

### 快捷键

| 快捷键 | 操作 |
|--------|------|
| `Ctrl+N` | 新建会话 |
| `Ctrl+E` | 编辑当前会话 |
| `Ctrl+W` | 删除当前会话 |
| `Ctrl+F5` | 切换连接 |
| `Ctrl+T` / `Ctrl+H` / `Ctrl+P` | 切换到文本 / 十六进制 / 波形视图 |
| `Ctrl+Tab` / `Ctrl+Shift+Tab` | 下一个 / 上一个标签页 |
| `Ctrl+L` | 清空输出 |
| `Ctrl+F` | 搜索 |
| `F3` / `Shift+F3` | 上一个 / 下一个匹配 |
| `Ctrl+,` | UI 设置 |
| `Esc` | 关闭浮层 |

---

## 快速开始

### 前置条件

| 平台 | 依赖 |
|------|------|
| **Linux** | `libudev-dev`（`apt install libudev-dev`） |
| **macOS** | 无需额外依赖 |
| **Windows** | 无需额外依赖 |
| **所有平台** | Rust ≥ 1.85、C 编译器（GCC / Clang / MSVC） |

### 构建与运行

```bash
# 克隆项目
git clone https://github.com/your-org/pipeview.git
cd pipeview

# 编译运行
cargo run -p pipeview-gui

# 运行测试
cargo test --workspace           # ~308 个测试
cargo clippy --workspace --all-targets -- -D warnings
```

### 发布构建

```bash
cargo build -p pipeview-gui --release
# 二进制位于 target/release/pipeview-gui (Linux/macOS)
# 或 target/release/pipeview-gui.exe (Windows)
```

---

## 完整示例：无人机遥测解析

以 Betaflight/INAV 飞控遥测为例，数据格式为：

```
AHRS q:1.0000,0.0998,0.0499,0.0200|YPR:8.87,4.44,2.96|Gyro:14.78,8.87,5.92|RC:1559,1544,1500,1519|M:1612,1588,1603,1597|L:0 F:1 C:0
```

### 步骤 1：启动测试数据源

```bash
python tools/test_drone.py --rate 10 --port 8091
```

输出：

```
Listening on 127.0.0.1:8091, waiting for connections...
```

### 步骤 2：配置 Session

在 pipeview-gui 中创建 Session：

| 配置项 | 值 |
|--------|-----|
| Transport | `TCP 127.0.0.1:8091` |
| Pipeline 1 | Framer: `Line` → Decoder: `Lua` → 选择 `examples/drone_text.lua` |
| Pipeline 2 | Framer: `Line` → Decoder: `Lua` → 选择 `examples/drone_plot.lua` |

### 步骤 3：查看结果

- **文本视图** — 显示格式化的传感器数据
- **波形视图** — 显示 Gyro 三轴实时曲线 (gz/gy/gx)

`examples/drone_plot.lua` 的核心逻辑：

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

## Lua 脚本开发指南

### 分帧器 API

分帧器将原始字节流切分为帧，Lua 脚本必须返回包含以下函数的 table：

```lua
return {
    -- 输入新到达的字节，返回帧数组（Lua strings）
    feed = function(bytes)
        -- bytes: Lua string（原始字节）
        -- 返回: { frame1, frame2, ... } 或 nil
    end,

    -- 刷新缓冲区中残留的数据
    flush = function()
        -- 返回: 最后一帧（Lua string）或 nil
    end,

    -- 重置内部状态
    reset = function()
    end,

    -- 返回缓冲区中待处理的字节数
    pending_len = function()
        -- 返回: number
    end,
}
```

参考实现：`tests/lua_line_framer.lua`（按 `\n` 分割的行分帧器）

### 解码器 API

解码器将帧解析为结构化数据，Lua 脚本必须返回包含 `decode` 函数的 table：

```lua
return {
    decode = function(frame)
        -- frame: Lua string（来自分帧器的一帧）
        -- 返回 nil → 跳过此帧
        -- 返回 string → 自动视为 Text
        -- 返回 table → 必须包含 kind 字段
    end,
}
```

**返回值格式：**

| `kind` | 必需字段 | 可选字段 | 用途 |
|--------|----------|----------|------|
| `"text"` | `data: string` | — | 文本视图 |
| `"hex"` | `data: string` | — | 十六进制视图 |
| `"binary"` | `data: string` | — | 原始二进制 |
| `"plot"` | `channels: {{number,…},…}` | `sample_type`、`format` | 波形视图 |

**Plot 返回值示例：**

```lua
return {
    kind = "plot",
    channels = {
        { 1.0, 2.0, 3.0 },  -- channel 0
        { 4.0, 5.0, 6.0 },  -- channel 1
    },
    sample_type = "F64",     -- 默认 F64，可选 I8/U8/I16/U16/I32/U32/I64/U64/F32
    format = "Block",        -- 默认 Interleaved，可选 Block/XY
}
```

更多示例见 `examples/` 目录。

---

## 架构

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

| Crate | 类型 | 职责 |
|-------|------|------|
| `pipeview-core` | library | 传输层（Serial/TCP/UDP）、分帧器（Line/Fixed/Length/COBS/Mixed/Lua）、协议解码器（Text/Hex/Plot）、MultiPipeline |
| `pipeview-client` | library | Session 生命周期管理、SessionManager、事件广播（tokio broadcast）、RingBuffer 历史、Lua 运行时及会话 API |
| `pipeview-gui` | binary | egui 桌面应用，包含 sidebar/config/console/text/hex/plot 面板、键盘快捷键、字体管理、性能分析 |
| `pipeview-tui` | binary | ratatui 终端应用（功能未对齐 GUI，仍在开发中） |

**依赖方向：** `core ← client ← {gui, tui}`

**数据流：**

```
[Transport] → read bytes → [MultiPipeline]
    → Pipeline 1: Framer → Decoder → DecodedEntry → broadcast → GUI buffers
    → Pipeline 2: Framer → Decoder → DecodedEntry → broadcast → GUI buffers
    → ...
```

每条管线独立分帧、解码，互不干扰。只有成功解码的管线产生输出。

---

## 配置与持久化

GUI 状态自动保存，路径遵循各平台规范：

| 平台 | 路径 |
|------|------|
| Linux | `$XDG_CONFIG_HOME/pipeview/gui-state.json` 或 `~/.config/pipeview/gui-state.json` |
| macOS | `~/Library/Application Support/pipeview/gui-state.json` |
| Windows | `%APPDATA%\pipeview\gui-state.json` |

持久化的内容包括：Session 配置（传输参数、管线设置）、日志开关及路径、活动标签页、显示选项。

日志文件默认保存在配置目录的 `logs/` 子目录下，文件命名格式为 `session_{id}_{timestamp}.log`。

---

## 开发工具

### 测试数据生成器

```bash
# Plot 波形测试数据
python tools/test_plot.py --wire-format mixed --host 127.0.0.1 --port 8091
python tools/test_plot.py --wire-format mixed --format xy --channels 2
python tools/test_plot.py --wire-format raw --channels 2 --framelen 256

# 无人机遥测测试数据
python tools/test_drone.py --rate 10 --port 8092
```

### 性能分析

```bash
RUST_LOG=pipeview_gui::perf=info XSERIAL_GUI_PROFILE=1 cargo run -p pipeview-gui
XSERIAL_GUI_PROFILE_INTERVAL_MS=500 cargo run -p pipeview-gui
```

输出每帧的耗时、事件 drain 耗时、text/hex/plot 渲染耗时、plot 点数统计。

### 日志

```bash
RUST_LOG=info cargo run -p pipeview-gui          # 应用日志
RUST_LOG=pipeview_gui=debug cargo run -p pipeview-gui  # 详细日志
```

使用 `tracing-subscriber` + `RUST_LOG` 环境变量控制。

### 项目结构

```
crates/
  pipeview-core/        # 传输、分帧、协议
  pipeview-client/      # 会话管理、Lua 运行时
  pipeview-gui/         # egui 桌面应用
  pipeview-tui/         # ratatui 终端应用
examples/              # Lua 脚本示例
  drone_plot.lua       # 飞控遥测波形解码器
  drone_text.lua       # 飞控遥测文本解码器
tests/                 # Lua 测试 fixture
tools/                 # 开发辅助工具
  test_plot.py         # 波形测试数据生成器
  test_drone.py        # 飞控测试数据生成器
  test_plot_serial.c   # C 串口波形测试客户端
  test_text_serial.c   # C 串口文本测试客户端
  xs_mixed_plot.h      # MixedTextPlot 协议参考头文件
```

---

## 技术栈

- **运行时**：tokio (multi-thread)
- **串口**：`tokio-serial` + `serialport`
- **GUI**：egui + egui_plot
- **Lua**：mlua 0.11, LuaJIT (vendored 编译), async/serde/send
- **事件分发**：`tokio::sync::broadcast`（多订阅者）
- **零 feature flags**、零 build script、零条件编译

## License

MIT
