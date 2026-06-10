# xserial

`xserial` 是一个基于 Rust workspace 的串口 / TCP / UDP 数据观测工具，支持可配置分帧、协议解码、会话管理，以及以文本、十六进制和绘图为核心的桌面 GUI。

## 工作区结构

仓库当前包含 4 个 crate：

- `xserial-core`
  负责传输层、分帧器、协议解码器和 pipeline 基础能力。
- `xserial-client`
  负责会话生命周期、配置、事件分发、历史缓冲和 Lua 相关接口。
- `xserial-gui`
  当前主要使用入口，基于 `egui` / `eframe`。
- `xserial-tui`
  终端 UI crate，目前还只是占位实现。

## 当前功能

- 在一个 GUI 中管理多个 session。
- 支持 `Serial`、`TCP`、`UDP` 三种传输方式。
- 支持以下 framer：
  `Line`、`Fixed`、`Length`、`Cobs`、`MixedTextPlot`。
- 支持以下 decoder：
  `Text`、`Hex`、`Plot`、`MixedTextPlot`。
- 每个 session 可在 `Text`、`Hex`、`Plot` 三种视图之间切换。
- 支持单连接 mixed text + plot 数据流。
- 支持 `Interleaved`、`Block`、`XY` 三种 plot 格式。
- 可在 sidebar 中直接编辑 session 配置。
- plot 视图支持嵌入主界面，也支持在 GUI 内部弹出为浮动窗口。

## 构建

```bash
cargo check --workspace
```

## 运行

启动 GUI：

```bash
cargo run -p xserial-gui
```

当前 `xserial-tui` 还不是完整实现：

```bash
cargo run -p xserial-tui
```

## 测试

运行整个 workspace 的测试：

```bash
cargo test --workspace
```

常用本地检查命令：

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
```

## GUI 快速开始

1. 启动 `xserial-gui`。
2. 在 sidebar 中创建一个 session。
3. 选择传输方式：`Serial`、`TCP` 或 `UDP`。
4. 添加一个或多个 pipeline，并选择匹配的 framer / decoder。
5. 连接 session。
6. 在 `Text`、`Hex`、`Plot` 视图之间切换。

补充说明：

- session 配置入口在 sidebar 的齿轮按钮。
- 连接控制是单个切换按钮，不再区分独立的 `Connect` / `Disconnect`。
- plot 视图支持内嵌和浮动窗口两种显示方式。

## Plot 测试服务器

仓库自带一个本地波形测试工具：

```bash
python tools/test_plot.py --help
```

示例：通过 TCP 发送 mixed text + plot 数据流：

```bash
python tools/test_plot.py --wire-format mixed --host 127.0.0.1 --port 8091
```

GUI 中对应配置为：

- Transport：`TCP 127.0.0.1:8091`
- Framer：`MixedTextPlot`
- Decoder：`MixedTextPlot`

示例：发送 `XY` plot 数据：

```bash
python tools/test_plot.py --wire-format mixed --format xy --channels 2
```

示例：发送固定长度 raw plot 帧：

```bash
python tools/test_plot.py --wire-format raw --channels 2 --framelen 256
```

GUI 中对应配置为：

- Framer：`Fixed`
- Decoder：`Plot`
- Sample type：`F32`
- Endian：`Little`
- `Channels` / `Format` 与脚本参数保持一致

## Profiling

GUI 内置了一套轻量级性能日志，默认关闭。

开启方式：

```bash
RUST_LOG=xserial_gui::perf=info XSERIAL_GUI_PROFILE=1 cargo run -p xserial-gui
```

修改统计输出间隔：

```bash
RUST_LOG=xserial_gui::perf=info XSERIAL_GUI_PROFILE=1 XSERIAL_GUI_PROFILE_INTERVAL_MS=500 cargo run -p xserial-gui
```

日志会输出：

- frame 耗时
- 事件 drain 耗时
- text / hex / plot 渲染耗时
- plot 点数量统计

## 仓库目录

```text
crates/
  xserial-core/
  xserial-client/
  xserial-gui/
  xserial-tui/
tools/
  test_plot.py
```

## 开发约定

- 传输层、协议和 pipeline 逻辑优先放在 `xserial-core`。
- 会话管理和状态编排优先放在 `xserial-client`。
- UI 相关状态放在 `xserial-gui` 或 `xserial-tui`。
- 集成测试位于 `crates/*/tests`。

## 当前状态

目前项目中最完整的是 GUI 路径。`xserial-tui` 已经在 workspace 中，但还没有达到和 GUI 同等的可用程度。
