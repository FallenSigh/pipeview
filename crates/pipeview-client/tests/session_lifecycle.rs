use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use pipeview_client::SessionManager;
use pipeview_client::config::{DecoderConfig, FramerConfig, PipelineConfig, SessionConfig};
use pipeview_client::session::{Session, SessionEvent};
use pipeview_core::protocol::DecodedData;
use pipeview_core::transport::TransportConfig;

fn tcp_config(addr: String) -> SessionConfig {
    SessionConfig {
        transport: TransportConfig::Tcp { addr },
        pipelines: vec![PipelineConfig {
            name: "text".into(),
            framer: FramerConfig::Line {
                strip_cr: true,
                max_line_len: 1024 * 1024,
            },
            decoder: DecoderConfig::Text {
                encoding: pipeview_core::protocol::text::TextEncoding::Utf8,
            },
        }],
        history_limit: 100,
        auto_reconnect: false,
    }
}

// ── Session spawn + basic send/read ──────────────────────────────

#[tokio::test]
async fn session_echo_send_and_read() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 256];
        let n = stream.read(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"ping\n");
        stream.write_all(b"pong\n").await.unwrap();
    });

    let handle = Session::spawn(1, tcp_config(addr));
    handle.send(b"ping\n".to_vec()).await.unwrap();

    let entry = handle.read(5000).await.expect("should receive response");
    match entry.data {
        DecodedData::Text(s) => assert_eq!(s, "pong"),
        other => panic!("expected Text, got {:?}", other),
    }

    handle.close().await.unwrap();
    server.await.unwrap();
}

#[tokio::test]
async fn session_multiple_reads() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"a\nb\nc\n").await.unwrap();
    });

    let handle = Session::spawn(2, tcp_config(addr));

    // Subscribe once and collect all text events
    let mut rx = handle.subscribe();
    let mut texts = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let SessionEvent::Data(_, entry) = rx.recv().await.unwrap()
                && let DecodedData::Text(s) = entry.data
            {
                texts.push(s);
                if texts.len() == 3 {
                    break;
                }
            }
        }
    })
    .await
    .unwrap();

    assert_eq!(texts, vec!["a", "b", "c"]);
    handle.close().await.unwrap();
    server.await.unwrap();
}

// ── Session with multiple pipelines ──────────────────────────────

#[tokio::test]
async fn session_multi_pipeline_text_and_hex() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"hello\n").await.unwrap();
    });

    let mut config = tcp_config(addr);
    config.pipelines.push(PipelineConfig {
        name: "hex".into(),
        framer: FramerConfig::Line {
            strip_cr: true,
            max_line_len: 1024 * 1024,
        },
        decoder: DecoderConfig::Hex {
            uppercase: false,
            separator: " ".into(),
            bytes_per_group: 1,
            endian: pipeview_core::protocol::Endian::Big,
        },
    });

    let handle = Session::spawn(3, config);

    // Subscribe and collect both pipeline results
    let mut rx = handle.subscribe();
    let mut results = Vec::new();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            if let SessionEvent::Data(_, entry) = rx.recv().await.unwrap() {
                results.push((entry.pipeline_name.clone(), entry.data.clone()));
                if results.len() == 2 {
                    break;
                }
            }
        }
    })
    .await
    .unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, "text");
    assert_eq!(results[1].0, "hex");
    assert!(matches!(results[1].1, DecodedData::Hex(_)));

    handle.close().await.unwrap();
    server.await.unwrap();
}

// ── SessionClose + read timeout ──────────────────────────────────

#[tokio::test]
async fn session_read_timeout_returns_none() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        // Accept but send nothing — client should time out
        tokio::time::sleep(Duration::from_secs(10)).await;
    });

    let handle = Session::spawn(4, tcp_config(addr));

    // Short timeout — no data sent by server
    let result = handle.read(200).await;
    assert!(result.is_none(), "read should time out and return None");

    handle.close().await.unwrap();
    server.abort();
}

#[tokio::test]
async fn session_close_stops_events() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let _server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
    });

    let handle = Session::spawn(5, tcp_config(addr));
    handle.close().await.unwrap();

    // After close, read should return None immediately
    let result = handle.read(100).await;
    assert!(result.is_none());
}

// ── SessionManager ───────────────────────────────────────────────

#[tokio::test]
async fn manager_create_and_list() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let _server = tokio::spawn(async move {
        let (_stream, _) = listener.accept().await.unwrap();
        tokio::time::sleep(Duration::from_secs(10)).await;
    });

    let mgr = SessionManager::new();
    assert_eq!(mgr.count(), 0);
    assert!(mgr.list_ids().is_empty());

    let h1 = mgr.create(tcp_config(addr.clone()));
    assert_eq!(mgr.count(), 1);
    assert_eq!(mgr.list_ids(), vec![1]);

    let h2 = mgr.create(tcp_config(addr));
    assert_eq!(mgr.count(), 2);

    assert!(mgr.get(1).is_some());
    assert!(mgr.get(2).is_some());
    assert!(mgr.get(99).is_none());

    mgr.remove(1);
    assert_eq!(mgr.count(), 1);
    assert!(mgr.get(1).is_none());

    h1.close().await.unwrap();
    h2.close().await.unwrap();
}

#[tokio::test]
async fn manager_event_subscription() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"data\n").await.unwrap();
    });

    let mgr = SessionManager::new();
    let mut rx = mgr.subscribe();

    let handle = mgr.create(tcp_config(addr));

    // Wait for Connected event
    let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, SessionEvent::Connected(1)));

    // Wait for Data event
    let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(event, SessionEvent::Data(1, _)));

    handle.close().await.unwrap();
    server.await.unwrap();
}

// ── Reconfigure ──────────────────────────────────────────────────

#[tokio::test]
async fn session_reconfigure_changes_pipeline() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"old\n").await.unwrap();
    });

    let handle = Session::spawn(6, tcp_config(addr));
    let entry = handle.read(5000).await.unwrap();
    assert!(matches!(entry.data, DecodedData::Text(s) if s == "old"));
    server.await.unwrap();

    // Reconfigure to hex pipeline on a new address (don't close first)
    let listener2 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr2 = listener2.local_addr().unwrap().to_string();

    let server2 = tokio::spawn(async move {
        let (mut stream, _) = listener2.accept().await.unwrap();
        stream.write_all(b"ABC\n").await.unwrap();
    });

    let mut new_cfg = tcp_config(addr2);
    new_cfg.pipelines[0].name = "hex".into();
    new_cfg.pipelines[0].decoder = DecoderConfig::Hex {
        uppercase: true,
        separator: " ".into(),
        bytes_per_group: 1,
        endian: pipeview_core::protocol::Endian::Big,
    };

    handle.reconfigure(new_cfg).await.unwrap();
    let entry = handle.read(5000).await.unwrap();
    assert!(matches!(entry.data, DecodedData::Hex(s) if s == "41 42 43"));

    handle.close().await.unwrap();
    server2.await.unwrap();
}

// ── SessionHandle event subscription ─────────────────────────────

#[tokio::test]
async fn session_handle_subscribe_sees_events() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"event_test\n").await.unwrap();
    });

    let handle = Session::spawn(7, tcp_config(addr));
    let mut rx = handle.subscribe();

    // Should see Connected then Data
    let e1 = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    assert!(matches!(e1, SessionEvent::Connected(7)));

    let e2 = tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .unwrap()
        .unwrap();
    match e2 {
        SessionEvent::Data(7, entry) => {
            assert_eq!(entry.pipeline_name, "text");
        }
        other => panic!("expected Data, got {:?}", other),
    }

    handle.close().await.unwrap();
    server.await.unwrap();
}
