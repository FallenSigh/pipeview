use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, UdpSocket};
use tokio::time::timeout;

use xserial_core::frame::{
    Endian, Framer,
    cobs::cobs_encode as raw_cobs_encode,
    cobs::CobsFramer,
    fixed::FixedLengthFramer,
    length::{LengthConfig, LengthPrefixedFramer},
    line::{LineConfig, LineFramer},
    mixed::{MixedTextPlotConfig as MixedFramerConfig, MixedTextPlotFramer},
};
use xserial_core::protocol::{
    DecodedData, ProtocolDecoder,
    hex::{HexConfig, HexDecoder},
    mixed::{MIXED_PLOT_ESCAPE, MIXED_PLOT_MARKER, MixedTextPlotConfig, MixedTextPlotDecoder},
    plot::{PlotConfig, PlotDecoder, PlotFormat, SampleType},
    text::{TextDecoder, TextEncoding},
};
use xserial_core::transport::serial::{
    SerialDataBits, SerialFlowControl, SerialParity, SerialStopBits,
};
use xserial_core::transport::{Connection, TransportConfig, TransportType};

const TEST_TIMEOUT: Duration = Duration::from_secs(5);

async fn read_to_framer(conn: &mut Connection, framer: &mut dyn Framer) -> Vec<Vec<u8>> {
    let mut buf = [0u8; 4096];
    let mut all_frames = Vec::new();

    loop {
        match timeout(TEST_TIMEOUT, AsyncReadExt::read(conn, &mut buf)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => {
                all_frames.extend(framer.feed(&buf[..n]));
            }
            Ok(Err(_)) => break,
            Err(_) => break,
        }
    }

    if let Some(rest) = framer.flush() {
        all_frames.push(rest);
    }
    all_frames
}

// ══════════════════════════════════════════════════════════════════════
// TCP + LineFramer + TextDecoder
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tcp_line_text_utf8() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"hello\nworld\n").await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = LineFramer::new(LineConfig::default());
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();

    let decoder = TextDecoder::new(TextEncoding::Utf8);
    let results: Vec<String> = frames
        .iter()
        .filter_map(|f| decoder.decode(f))
        .filter_map(|d| match d {
            DecodedData::Text(s) => Some(s),
            _ => None,
        })
        .collect();

    assert_eq!(results, vec!["hello", "world"]);
    server.await.unwrap();
}

#[tokio::test]
async fn tcp_line_text_crlf_stripping() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"line1\r\nline2\r\n").await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = LineFramer::new(LineConfig::default());
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();

    let decoder = TextDecoder::new(TextEncoding::Utf8);
    let results: Vec<String> = frames
        .iter()
        .filter_map(|f| decoder.decode(f))
        .filter_map(|d| match d {
            DecodedData::Text(s) => Some(s),
            _ => None,
        })
        .collect();

    assert_eq!(results, vec!["line1", "line2"]);
    server.await.unwrap();
}

#[tokio::test]
async fn tcp_line_text_chinese_utf8() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all("你好\n世界\n".as_bytes()).await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = LineFramer::new(LineConfig::default());
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();

    let decoder = TextDecoder::new(TextEncoding::Utf8);
    let results: Vec<String> = frames
        .iter()
        .filter_map(|f| decoder.decode(f))
        .filter_map(|d| match d {
            DecodedData::Text(s) => Some(s),
            _ => None,
        })
        .collect();

    assert_eq!(results, vec!["你好", "世界"]);
    server.await.unwrap();
}

// ══════════════════════════════════════════════════════════════════════
// TCP + LengthPrefixedFramer + HexDecoder
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tcp_length_hex() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Frame 1: 3 bytes "foo"
        stream.write_all(&3u16.to_be_bytes()).await.unwrap();
        stream.write_all(b"foo").await.unwrap();
        // Frame 2: 3 bytes "bar"
        stream.write_all(&3u16.to_be_bytes()).await.unwrap();
        stream.write_all(b"bar").await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = LengthPrefixedFramer::new(LengthConfig::default());
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();

    let decoder = HexDecoder::new(HexConfig::default());
    let results: Vec<String> = frames
        .iter()
        .filter_map(|f| decoder.decode(f))
        .filter_map(|d| match d {
            DecodedData::Hex(s) => Some(s),
            _ => None,
        })
        .collect();

    assert_eq!(results, vec!["66 6f 6f", "62 61 72"]); // "foo", "bar" in hex
    server.await.unwrap();
}

#[tokio::test]
async fn tcp_length_hex_little_endian() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(&3u16.to_le_bytes()).await.unwrap();
        stream.write_all(b"xyz").await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = LengthPrefixedFramer::new(LengthConfig {
        endian: Endian::Little,
        ..LengthConfig::default()
    });
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();

    assert_eq!(frames.len(), 1);
    let decoder = HexDecoder::new(HexConfig::default());
    let result = decoder.decode(&frames[0]).unwrap();
    assert!(matches!(result, DecodedData::Hex(ref s) if s == "78 79 7a"));
    server.await.unwrap();
}

// ══════════════════════════════════════════════════════════════════════
// TCP + FixedLengthFramer + PlotDecoder
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tcp_fixed_plot_f32() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // 3 samples × 4 bytes each = 12 bytes per frame
        let samples: [f32; 3] = [1.0, -2.5, 3.14];
        let mut data = Vec::new();
        for s in &samples {
            data.extend_from_slice(&s.to_le_bytes());
        }
        stream.write_all(&data).await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = FixedLengthFramer::new(12); // 3 × f32
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();

    let decoder = PlotDecoder::new(PlotConfig {
        sample_type: SampleType::F32,
        endian: Endian::Little,
        channels: 1,
        format: PlotFormat::Interleaved,
    });

    assert_eq!(frames.len(), 1);
    let result = decoder.decode(&frames[0]).unwrap();
    match result {
        DecodedData::Plot(frame) => {
            assert_eq!(frame.channels.len(), 1);
            assert_eq!(frame.channels[0].len(), 3);
            assert!((frame.channels[0][0] - 1.0).abs() < 1e-5);
            assert!((frame.channels[0][1] - (-2.5)).abs() < 1e-5);
            assert!((frame.channels[0][2] - 3.14).abs() < 1e-5);
        }
        other => panic!("expected Plot, got {:?}", other),
    }
    server.await.unwrap();
}

#[tokio::test]
async fn tcp_mixed_text_and_plot_single_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let payload = [
            0x00, 0x00, 0x80, 0x3f, // 1.0
            0x00, 0x00, 0x00, 0x40, // 2.0
        ];
        let packet = MixedTextPlotDecoder::build_plot_packet(
            SampleType::F32,
            Endian::Little,
            1,
            PlotFormat::Interleaved,
            2,
            &payload,
        );
        let mut mixed = b"status ok\n".to_vec();
        mixed.push(MIXED_PLOT_ESCAPE);
        mixed.push(MIXED_PLOT_MARKER);
        mixed.extend_from_slice(&raw_cobs_encode(&packet));
        mixed.push(0x00);
        mixed.extend_from_slice(b"done\n");
        stream.write_all(&mixed).await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = MixedTextPlotFramer::new(MixedFramerConfig::default());
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();

    let decoder = MixedTextPlotDecoder::new(MixedTextPlotConfig::default());
    let results: Vec<DecodedData> = frames
        .iter()
        .filter_map(|f| decoder.decode(f))
        .collect();

    assert_eq!(results.len(), 3);
    assert!(matches!(&results[0], DecodedData::Text(s) if s == "status ok"));
    match &results[1] {
        DecodedData::Plot(frame) => assert_eq!(frame.channels[0], vec![1.0, 2.0]),
        other => panic!("expected Plot, got {other:?}"),
    }
    assert!(matches!(&results[2], DecodedData::Text(s) if s == "done"));

    server.await.unwrap();
}

#[tokio::test]
async fn tcp_fixed_plot_two_channel_u16() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // 2 channels × 2 samples each × 2 bytes = 8 bytes
        let samples: [u16; 4] = [100, 200, 300, 400];
        let mut data = Vec::new();
        for s in &samples {
            data.extend_from_slice(&s.to_le_bytes());
        }
        stream.write_all(&data).await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = FixedLengthFramer::new(8);
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();

    let decoder = PlotDecoder::new(PlotConfig {
        sample_type: SampleType::U16,
        endian: Endian::Little,
        channels: 2,
        format: PlotFormat::Interleaved,
    });

    assert_eq!(frames.len(), 1);
    let result = decoder.decode(&frames[0]).unwrap();
    match result {
        DecodedData::Plot(frame) => {
            assert_eq!(frame.channels.len(), 2);
            assert_eq!(frame.channels[0], vec![100.0, 300.0]);
            assert_eq!(frame.channels[1], vec![200.0, 400.0]);
        }
        other => panic!("expected Plot, got {:?}", other),
    }
    server.await.unwrap();
}

// ══════════════════════════════════════════════════════════════════════
// UDP + CobsFramer + TextDecoder
// ══════════════════════════════════════════════════════════════════════

fn cobs_encode(data: &[u8]) -> Vec<u8> {
    if data.is_empty() {
        return vec![0x01, 0x00];
    }
    let mut out = Vec::new();
    let mut block_start = 0;
    for (i, &byte) in data.iter().enumerate() {
        if byte == 0x00 || i - block_start == 254 {
            let len = i - block_start + 1;
            out.push(len as u8);
            out.extend_from_slice(&data[block_start..i]);
            block_start = i + if byte == 0x00 { 1 } else { 0 };
        }
    }
    if block_start < data.len() {
        let len = data.len() - block_start + 1;
        out.push(len as u8);
        out.extend_from_slice(&data[block_start..]);
    }
    out.push(0x00);
    out
}

#[tokio::test]
async fn udp_cobs_text() {
    let remote = UdpSocket::bind("127.0.0.1:0").await.unwrap();
    let remote_addr = remote.local_addr().unwrap().to_string();

    let mut conn = Connection::new(TransportConfig::Udp {
        bind_addr: "127.0.0.1:0".into(),
        remote_addr: Some(remote_addr.clone()),
    });
    conn.connect().await.unwrap();

    // Send a probe so the remote learns our actual port
    AsyncWriteExt::write_all(&mut conn, b"x").await.unwrap();
    let mut probe_buf = [0u8; 1];
    let (_, conn_actual_addr) = remote.recv_from(&mut probe_buf).await.unwrap();

    // Send COBS-encoded frames to the connection's actual port
    let packet1 = cobs_encode(b"hello");
    let packet2 = cobs_encode(b"world");
    let mut combined = packet1;
    combined.extend_from_slice(&packet2);
    remote.send_to(&combined, conn_actual_addr).await.unwrap();

    let mut framer = CobsFramer::default();
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();

    let decoder = TextDecoder::new(TextEncoding::Utf8);
    let results: Vec<String> = frames
        .iter()
        .filter_map(|f| decoder.decode(f))
        .filter_map(|d| match d {
            DecodedData::Text(s) => Some(s),
            _ => None,
        })
        .collect();

    assert_eq!(results, vec!["hello", "world"]);
}

// ══════════════════════════════════════════════════════════════════════
// Connection lifecycle: connect → use → disconnect → reconnect → use
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn connection_reconnect_tcp() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    // First connection
    let server1 = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"first\n").await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr: addr.clone() });
    conn.connect().await.unwrap();

    let mut framer = LineFramer::new(LineConfig::default());
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();
    server1.await.unwrap();

    let decoder = TextDecoder::new(TextEncoding::Utf8);
    let results: Vec<String> = frames
        .iter()
        .filter_map(|f| decoder.decode(f))
        .filter_map(|d| match d {
            DecodedData::Text(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(results, vec!["first"]);

    // Framer state should not carry over (it was fully consumed)
    framer.reset();

    // Second connection
    let listener2 = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr2 = listener2.local_addr().unwrap().to_string();

    let server2 = tokio::spawn(async move {
        let (mut stream, _) = listener2.accept().await.unwrap();
        stream.write_all(b"second\n").await.unwrap();
    });

    let mut conn2 = Connection::new(TransportConfig::Tcp { addr: addr2 });
    conn2.connect().await.unwrap();

    let frames = read_to_framer(&mut conn2, &mut framer).await;
    conn2.disconnect().await.unwrap();
    server2.await.unwrap();

    let results: Vec<String> = frames
        .iter()
        .filter_map(|f| decoder.decode(f))
        .filter_map(|d| match d {
            DecodedData::Text(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(results, vec!["second"]);
}

// ══════════════════════════════════════════════════════════════════════
// Framer reset mid-stream
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn framer_reset_mid_stream() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"garbage__data\nvalid\n").await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = LineFramer::new(LineConfig::default());

    // Read exactly "garbage_" (8 bytes) — no newline yet
    let mut buf = [0u8; 8];
    let n = timeout(TEST_TIMEOUT, AsyncReadExt::read(&mut conn, &mut buf))
        .await
        .unwrap()
        .unwrap();
    let frames = framer.feed(&buf[..n]);
    assert!(frames.is_empty());
    assert!(framer.pending_len() > 0);

    // Reset — discard the incomplete "garbage_"
    framer.reset();
    assert_eq!(framer.pending_len(), 0);

    // Read remaining data
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();
    server.await.unwrap();

    let decoder = TextDecoder::new(TextEncoding::Utf8);
    let results: Vec<String> = frames
        .iter()
        .filter_map(|f| decoder.decode(f))
        .filter_map(|d| match d {
            DecodedData::Text(s) => Some(s),
            _ => None,
        })
        .collect();

    assert_eq!(results, vec!["_data", "valid"]);
}

// ══════════════════════════════════════════════════════════════════════
// Multi-frame burst + edge cases
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tcp_many_frames_burst() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // 50 lines in one write
        let mut data = String::new();
        for i in 0..50 {
            data.push_str(&format!("line_{}\n", i));
        }
        stream.write_all(data.as_bytes()).await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = LineFramer::new(LineConfig::default());
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();
    server.await.unwrap();

    assert_eq!(frames.len(), 50);

    let decoder = TextDecoder::new(TextEncoding::Utf8);
    let results: Vec<String> = frames
        .iter()
        .filter_map(|f| decoder.decode(f))
        .filter_map(|d| match d {
            DecodedData::Text(s) => Some(s),
            _ => None,
        })
        .collect();

    assert_eq!(results.len(), 50);
    for (i, line) in results.iter().enumerate() {
        assert_eq!(line, &format!("line_{}", i));
    }
}

#[tokio::test]
async fn tcp_trailing_data_flushed_on_disconnect() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"no_newline_at_end").await.unwrap();
        // Close without sending \n
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = LineFramer::new(LineConfig::default());
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();
    server.await.unwrap();

    // flush() in read_to_framer should capture the trailing data
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0], b"no_newline_at_end");
}

#[tokio::test]
async fn empty_data_produces_no_frames() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        drop(stream);
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = LineFramer::new(LineConfig::default());
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();
    server.await.unwrap();

    assert!(frames.is_empty());
}

#[tokio::test]
async fn protocol_switching_on_same_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Send text line then length-prefixed binary
        stream.write_all(b"text_mode\n").await.unwrap();
        stream.write_all(&3u16.to_be_bytes()).await.unwrap();
        stream.write_all(b"bin").await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    // Phase 1: text protocol
    let mut line_framer = LineFramer::new(LineConfig::default());
    let buf = read_chunk(&mut conn).await;
    let frames = line_framer.feed(&buf);

    let text_decoder = TextDecoder::new(TextEncoding::Utf8);
    let results: Vec<_> = frames
        .iter()
        .filter_map(|f| text_decoder.decode(f))
        .collect();
    assert_eq!(results.len(), 1);
    assert!(matches!(&results[0], DecodedData::Text(s) if s == "text_mode"));

    // Phase 2: feed leftovers + remaining data to length-prefixed framer
    let mut len_framer = LengthPrefixedFramer::new(LengthConfig::default());
    let mut frames = if let Some(rest) = line_framer.flush() {
        len_framer.feed(&rest)
    } else {
        Vec::new()
    };
    let rest_buf = read_remaining(&mut conn).await;
    frames.extend(len_framer.feed(&rest_buf));

    let hex_decoder = HexDecoder::new(HexConfig::default());
    let results: Vec<_> = frames
        .iter()
        .filter_map(|f| hex_decoder.decode(f))
        .collect();
    assert_eq!(results.len(), 1);
    assert!(matches!(&results[0], DecodedData::Hex(s) if s == "62 69 6e"));
    server.await.unwrap();
}

// ══════════════════════════════════════════════════════════════════════
// TransportType dispatch via Connection enum
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn connection_transport_type_dispatch() {
    let serial = Connection::new(TransportConfig::Serial {
        port: "COM1".into(),
        baud_rate: 115200,
        data_bits: SerialDataBits::Eight,
        parity: SerialParity::None,
        stop_bits: SerialStopBits::One,
        flow_control: SerialFlowControl::None,
    });
    let tcp = Connection::new(TransportConfig::Tcp {
        addr: "127.0.0.1:8080".into(),
    });
    let udp = Connection::new(TransportConfig::Udp {
        bind_addr: "0.0.0.0:0".into(),
        remote_addr: None,
    });

    assert_eq!(serial.transport_type(), TransportType::Serial);
    assert_eq!(tcp.transport_type(), TransportType::Tcp);
    assert_eq!(udp.transport_type(), TransportType::Udp);
}

// ══════════════════════════════════════════════════════════════════════
// Multiple framers, one connection
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn multiple_framers_one_connection() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"a\nbb\nccc\n").await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut buf = [0u8; 256];
    let n = AsyncReadExt::read(&mut conn, &mut buf).await.unwrap();
    conn.disconnect().await.unwrap();
    server.await.unwrap();

    let data = &buf[..n];

    // Framer 1: LineFramer
    let mut line = LineFramer::new(LineConfig::default());
    let line_frames = line.feed(data);
    assert_eq!(line_frames.len(), 3);

    // Framer 2: FixedLengthFramer (same bytes, different interpretation)
    let mut fixed = FixedLengthFramer::new(3);
    let fixed_frames = fixed.feed(data);
    assert_eq!(fixed_frames.len(), 3);
    assert_eq!(fixed_frames[0], b"a\nb");
    assert_eq!(fixed_frames[1], b"b\nc");
    assert_eq!(fixed_frames[2], b"cc\n");
}

// ══════════════════════════════════════════════════════════════════════
// Robustness: partial writes, slow consumer
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tcp_partial_writes_line_framing() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        // Simulate fragmented writes
        stream.write_all(b"hel").await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        stream.write_all(b"lo\nwor").await.unwrap();
        tokio::time::sleep(Duration::from_millis(10)).await;
        stream.write_all(b"ld\n").await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = LineFramer::new(LineConfig::default());
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();
    server.await.unwrap();

    let decoder = TextDecoder::new(TextEncoding::Utf8);
    let results: Vec<String> = frames
        .iter()
        .filter_map(|f| decoder.decode(f))
        .filter_map(|d| match d {
            DecodedData::Text(s) => Some(s),
            _ => None,
        })
        .collect();

    assert_eq!(results, vec!["hello", "world"]);
}

#[tokio::test]
async fn decode_summary_on_full_pipeline() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();

    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        stream.write_all(b"hello\n").await.unwrap();
    });

    let mut conn = Connection::new(TransportConfig::Tcp { addr });
    conn.connect().await.unwrap();

    let mut framer = LineFramer::new(LineConfig::default());
    let frames = read_to_framer(&mut conn, &mut framer).await;
    conn.disconnect().await.unwrap();
    server.await.unwrap();

    let decoder = TextDecoder::new(TextEncoding::Utf8);
    let decoded = decoder.decode(&frames[0]).unwrap();
    assert_eq!(decoded.summary(), "hello");
}

// ══════════════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════════════

async fn read_chunk(conn: &mut Connection) -> Vec<u8> {
    let mut buf = [0u8; 4096];
    match timeout(TEST_TIMEOUT, AsyncReadExt::read(conn, &mut buf)).await {
        Ok(Ok(n)) => buf[..n].to_vec(),
        _ => Vec::new(),
    }
}

async fn read_remaining(conn: &mut Connection) -> Vec<u8> {
    let mut all = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        match timeout(TEST_TIMEOUT, AsyncReadExt::read(conn, &mut buf)).await {
            Ok(Ok(0)) => break,
            Ok(Ok(n)) => all.extend_from_slice(&buf[..n]),
            _ => break,
        }
    }
    all
}
