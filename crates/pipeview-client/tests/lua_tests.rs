use std::sync::Once;
use std::time::Duration;
use std::{fs, path::PathBuf};

use mlua::Lua;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

static INIT: Once = Once::new();

fn init_tracing() {
    INIT.call_once(|| {
        tracing_subscriber::fmt()
            .with_env_filter("pipeview=trace")
            .try_init()
            .ok();
    });
}

fn write_temp_lua(name: &str, script: &str) -> PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("pipeview_{name}_{nonce}.lua"));
    fs::write(&path, script).unwrap();
    path
}

fn text_pipeline_lua() -> &'static str {
    r#"
        {
        name = "text",
        framer = { Line = {} },
        decoder = { Text = {} }
        }
    "#
}

fn hex_pipeline_lua() -> &'static str {
    r#"
        {
        name = "hex",
        framer = { Line = {} },
        decoder = { Hex = { uppercase = true } }
        }
    "#
}

// ── pipeview.sleep / pipeview.log ──────────────────────────────────

#[tokio::test]
async fn lua_sleep_and_log() {
    let lua = Lua::new();
    pipeview_client::lua::register(&lua).unwrap();

    lua.load(
        r#"
        pipeview.log("test start")
        pipeview.sleep(50)
        pipeview.log("test end")
        "#,
    )
    .exec_async()
    .await
    .unwrap();
}

// ── pipeview.list_ports ───────────────────────────────────────────

#[tokio::test]
async fn lua_list_ports() {
    let lua = Lua::new();
    pipeview_client::lua::register(&lua).unwrap();

    let result: Vec<String> = lua
        .load(r#"return pipeview.list_ports()"#)
        .eval_async()
        .await
        .unwrap();

    // Should return a table (possibly empty if no serial ports)
    // Just verify it doesn't crash
    let _ = result;
}

// ── pipeview.open + session:send / session:read / session:close ────

#[tokio::test]
async fn lua_session_open_send_read_close() {
    init_tracing();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 256];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello\n");
        stream.write_all(b"world\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    });

    let lua = Lua::new();
    pipeview_client::lua::register(&lua).unwrap();

    let script = format!(
        r#"
        local sess = pipeview.open({{
            transport = {{ Tcp = {{ addr = "{}" }} }},
            pipelines = {{{}}}
        }})

        sess:send("hello\n")

        local r = sess:read(5000)
        assert(r ~= nil, "expected data")
        assert(r.pipeline == "text", "expected text pipeline")
        assert(r.data == "world", "expected 'world', got " .. tostring(r.data))

        sess:close()
        "#,
        addr,
        text_pipeline_lua()
    );

    lua.load(&script).exec_async().await.unwrap();
    server.await.unwrap();
}

// ── session:read with timeout ────────────────────────────────────

#[tokio::test]
async fn lua_session_read_timeout() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let _server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    });

    let lua = Lua::new();
    pipeview_client::lua::register(&lua).unwrap();

    let script = format!(
        r#"
        local sess = pipeview.open({{
            transport = {{ Tcp = {{ addr = "{}" }} }},
            pipelines = {{{}}}
        }})

        local r = sess:read(100)
        assert(r == nil, "read should time out")

        sess:close()
        "#,
        addr,
        text_pipeline_lua()
    );

    lua.load(&script).exec_async().await.unwrap();
}

// ── pipeview.open with hex decoder ────────────────────────────────

#[tokio::test]
async fn lua_session_hex_decoder() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"ABC\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    });

    let lua = Lua::new();
    pipeview_client::lua::register(&lua).unwrap();

    let script = format!(
        r#"
        local sess = pipeview.open({{
            transport = {{ Tcp = {{ addr = "{}" }} }},
            pipelines = {{{}}}
        }})

        local r = sess:read(5000)
        assert(r ~= nil)
        assert(r.pipeline == "hex")
        assert(r.kind == "hex")
        assert(r.data == "41 42 43", "expected hex of ABC, got " .. tostring(r.data))

        sess:close()
        "#,
        addr,
        hex_pipeline_lua()
    );

    lua.load(&script).exec_async().await.unwrap();
    server.await.unwrap();
}

// ── pipeview.open with Lua framer + Lua decoder ───────────────────

#[tokio::test]
async fn lua_session_custom_lua_pipeline() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let framer_path = write_temp_lua(
        "custom_framer",
        r#"
        local buffer = ""
        return {
            feed = function(bytes)
                buffer = buffer .. bytes
                local frames = {}
                while true do
                    local i = buffer:find("|", 1, true)
                    if not i then break end
                    frames[#frames + 1] = buffer:sub(1, i - 1)
                    buffer = buffer:sub(i + 1)
                end
                return frames
            end,
            flush = function()
                if #buffer == 0 then return nil end
                local frame = buffer
                buffer = ""
                return frame
            end,
            reset = function()
                buffer = ""
            end,
            pending_len = function()
                return #buffer
            end,
        }
        "#,
    );
    let decoder_path = write_temp_lua(
        "custom_decoder",
        r#"
        return {
            decode = function(frame)
                return { kind = "text", data = string.upper(frame) }
            end,
        }
        "#,
    );

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"alpha|").await.unwrap();
        tokio::time::sleep(Duration::from_millis(50)).await;
        stream.write_all(b"beta|").await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    });

    let lua = Lua::new();
    pipeview_client::lua::register(&lua).unwrap();

    let script = format!(
        r#"
        local sess = pipeview.open({{
            transport = {{ Tcp = {{ addr = "{}" }} }},
            pipelines = {{
                {{
                    name = "custom",
                    framer = {{ Lua = {{ script_path = [[{}]] }} }},
                    decoder = {{ Lua = {{ script_path = [[{}]] }} }}
                }}
            }}
        }})

        local r1 = sess:read(5000)
        assert(r1 ~= nil, "expected first custom frame")
        assert(r1.pipeline == "custom", "expected custom pipeline, got " .. tostring(r1.pipeline))
        assert(r1.kind == "text", "expected text kind, got " .. tostring(r1.kind))
        assert(r1.data == "ALPHA", "expected ALPHA, got " .. tostring(r1.data))

        local r2 = sess:read(5000)
        assert(r2 ~= nil, "expected second custom frame")
        assert(r2.data == "BETA", "expected BETA, got " .. tostring(r2.data))

        sess:close()
        "#,
        addr,
        framer_path.display(),
        decoder_path.display()
    );

    lua.load(&script).exec_async().await.unwrap();
    server.await.unwrap();
    fs::remove_file(framer_path).unwrap();
    fs::remove_file(decoder_path).unwrap();
}

// ── session:on_data callback ─────────────────────────────────────

#[tokio::test]
async fn lua_session_on_data_callback() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"line1\nline2\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    });

    let lua = Lua::new();
    pipeview_client::lua::register(&lua).unwrap();

    let script = format!(
        r#"
        local received = {{}}
        local sess = pipeview.open({{
            transport = {{ Tcp = {{ addr = "{}" }} }},
            pipelines = {{{}}}
        }})

        sess:on_data(function(name, data)
            table.insert(received, {{ name = name, data = data }})
        end)

        for _ = 1, 50 do
            if #received < 2 then
                pipeview.poll(1)
                pipeview.sleep(10)
            end
        end

        assert(#received == 2, "expected 2 callbacks, got " .. #received)
        assert(received[1].name == "text")
        assert(received[1].data == "line1")
        assert(received[2].name == "text")
        assert(received[2].data == "line2")

        sess:close()
        "#,
        addr,
        text_pipeline_lua()
    );

    lua.load(&script).exec_async().await.unwrap();
    server.await.unwrap();
}

// ── session:reconfigure from Lua ─────────────────────────────────

#[tokio::test]
async fn lua_session_reconfigure() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"test\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
    });

    let listener2 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr2 = listener2.local_addr().unwrap().to_string();

    let server2 = tokio::spawn(async move {
        let (mut stream, _) = listener2.accept().await.unwrap();
        stream.write_all(b"ABC\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
    });

    let lua = Lua::new();
    pipeview_client::lua::register(&lua).unwrap();

    let script = format!(
        r#"
        local sess = pipeview.open({{
            transport = {{ Tcp = {{ addr = "{}" }} }},
            pipelines = {{{}}}
        }})

        local r = sess:read(5000)
        assert(r.data == "test")

        -- Reconfigure to hex on different address
        sess:reconfigure({{
            transport = {{ Tcp = {{ addr = "{}" }} }},
            pipelines = {{{}}}
        }})

        local r2 = sess:read(5000)
        assert(r2.pipeline == "hex")
        assert(r2.data == "41 42 43")

        sess:close()
        "#,
        addr,
        text_pipeline_lua(),
        addr2,
        hex_pipeline_lua()
    );

    lua.load(&script).exec_async().await.unwrap();
    server.await.unwrap();
    server2.await.unwrap();
}

// ── session:next_event ───────────────────────────────────────────

#[tokio::test]
async fn lua_session_next_event_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"streamed\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    });

    let lua = Lua::new();
    pipeview_client::lua::register(&lua).unwrap();

    let script = format!(
        r#"
        local sess = pipeview.open({{
            transport = {{ Tcp = {{ addr = "{}" }} }},
            pipelines = {{{}}}
        }})

        local e1 = sess:next_event(5000)
        assert(e1 ~= nil, "expected connected event")
        assert(e1.type == "connected", "expected connected, got " .. tostring(e1.type))

        local e2 = sess:next_event(5000)
        assert(e2 ~= nil, "expected data event")
        assert(e2.type == "data", "expected data, got " .. tostring(e2.type))
        assert(e2.pipeline == "text")
        assert(e2.kind == "text")
        assert(e2.data == "streamed")

        sess:close()
        "#,
        addr,
        text_pipeline_lua()
    );

    lua.load(&script).exec_async().await.unwrap();
    server.await.unwrap();
}

// ── session:off + pipeview.poll ───────────────────────────────────

#[tokio::test]
async fn lua_session_off_stops_future_callbacks() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"first\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(150)).await;
        stream.write_all(b"second\n").await.unwrap();
        tokio::time::sleep(Duration::from_millis(200)).await;
    });

    let lua = Lua::new();
    pipeview_client::lua::register(&lua).unwrap();

    let script = format!(
        r#"
        local received = {{}}
        local sess = pipeview.open({{
            transport = {{ Tcp = {{ addr = "{}" }} }},
            pipelines = {{{}}}
        }})

        local token = sess:on_data(function(name, data)
            table.insert(received, name .. ":" .. data)
        end)

        for _ = 1, 50 do
            if #received == 0 then
                pipeview.poll(1)
                pipeview.sleep(10)
            end
        end

        assert(#received == 1, "expected first callback before off, got " .. #received)
        assert(received[1] == "text:first")
        assert(sess:off(token) == true)

        pipeview.sleep(250)
        pipeview.poll(10)

        assert(#received == 1, "callback should not fire after off")
        assert(sess:off(token) == false)

        sess:close()
        "#,
        addr,
        text_pipeline_lua()
    );

    lua.load(&script).exec_async().await.unwrap();
    server.await.unwrap();
}
