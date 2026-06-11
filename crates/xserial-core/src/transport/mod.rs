use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::info;

use crate::error::Result;
use crate::transport::serial::{
    SerialDataBits, SerialFlowControl, SerialParity, SerialStopBits, SerialTransport,
};
use crate::transport::tcp::TcpTransport;
use crate::transport::udp::UdpTransport;

pub mod serial;
pub mod tcp;
pub mod udp;

fn default_serial_data_bits() -> SerialDataBits {
    SerialDataBits::Eight
}

fn default_serial_parity() -> SerialParity {
    SerialParity::None
}

fn default_serial_stop_bits() -> SerialStopBits {
    SerialStopBits::One
}

fn default_serial_flow_control() -> SerialFlowControl {
    SerialFlowControl::None
}

fn default_serial_dtr() -> bool {
    false
}

fn default_serial_rts() -> bool {
    false
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransportType {
    Serial,
    Tcp,
    Udp,
}

impl std::fmt::Display for TransportType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportType::Serial => write!(f, "Serial"),
            TransportType::Tcp => write!(f, "Tcp"),
            TransportType::Udp => write!(f, "Udp"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TransportConfig {
    Serial {
        port: String,
        baud_rate: u32,
        #[serde(default = "default_serial_data_bits")]
        data_bits: SerialDataBits,
        #[serde(default = "default_serial_parity")]
        parity: SerialParity,
        #[serde(default = "default_serial_stop_bits")]
        stop_bits: SerialStopBits,
        #[serde(default = "default_serial_flow_control")]
        flow_control: SerialFlowControl,
        #[serde(default = "default_serial_dtr")]
        dtr: bool,
        #[serde(default = "default_serial_rts")]
        rts: bool,
    },
    Tcp {
        addr: String,
    },
    Udp {
        bind_addr: String,
        remote_addr: Option<String>,
    },
}

#[async_trait]
pub trait Transport: AsyncRead + AsyncWrite + Send + Sync + Unpin {
    fn name(&self) -> &str;
    fn transport_type(&self) -> TransportType;
    fn is_connected(&self) -> bool;
    async fn connect(&mut self) -> Result<()>;
    async fn disconnect(&mut self) -> Result<()>;
}

#[derive(Debug)]
pub enum Connection {
    Serial(SerialTransport),
    Tcp(TcpTransport),
    Udp(UdpTransport),
}

impl Connection {
    pub fn new(config: TransportConfig) -> Self {
        match config {
            TransportConfig::Serial {
                port,
                baud_rate,
                data_bits,
                parity,
                stop_bits,
                flow_control,
                dtr,
                rts,
            } => Connection::Serial(SerialTransport::new(
                port,
                baud_rate,
                data_bits,
                parity,
                stop_bits,
                flow_control,
                dtr,
                rts,
            )),
            TransportConfig::Tcp { addr } => Connection::Tcp(TcpTransport::new(addr)),
            TransportConfig::Udp {
                bind_addr,
                remote_addr,
            } => Connection::Udp(UdpTransport::new(bind_addr, remote_addr)),
        }
    }

    pub fn transport_type(&self) -> TransportType {
        match self {
            Connection::Serial(_) => TransportType::Serial,
            Connection::Tcp(_) => TransportType::Tcp,
            Connection::Udp(_) => TransportType::Udp,
        }
    }

    pub fn name(&self) -> &str {
        match self {
            Connection::Serial(t) => t.name(),
            Connection::Tcp(t) => t.name(),
            Connection::Udp(t) => t.name(),
        }
    }

    pub fn is_connected(&self) -> bool {
        match self {
            Connection::Serial(t) => t.is_connected(),
            Connection::Tcp(t) => t.is_connected(),
            Connection::Udp(t) => t.is_connected(),
        }
    }

    pub async fn connect(&mut self) -> Result<()> {
        info!(transport = ?self.transport_type(), name = self.name(), "Connection connecting");
        match self {
            Connection::Serial(t) => t.connect().await,
            Connection::Tcp(t) => t.connect().await,
            Connection::Udp(t) => t.connect().await,
        }
    }

    pub async fn disconnect(&mut self) -> Result<()> {
        info!(name = self.name(), "Connection disconnecting");
        match self {
            Connection::Serial(t) => t.disconnect().await,
            Connection::Tcp(t) => t.disconnect().await,
            Connection::Udp(t) => t.disconnect().await,
        }
    }

    pub fn set_dtr(&mut self, state: bool) -> Result<()> {
        match self {
            Connection::Serial(t) => t.set_dtr(state),
            Connection::Tcp(_) | Connection::Udp(_) => {
                Err(crate::error::Error::ConnectionFailed(
                    "DTR only supported on Serial connections".into(),
                ))
            }
        }
    }

    pub fn set_rts(&mut self, state: bool) -> Result<()> {
        match self {
            Connection::Serial(t) => t.set_rts(state),
            Connection::Tcp(_) | Connection::Udp(_) => {
                Err(crate::error::Error::ConnectionFailed(
                    "RTS only supported on Serial connections".into(),
                ))
            }
        }
    }
}

impl AsyncRead for Connection {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Connection::Serial(t) => std::pin::Pin::new(t).poll_read(cx, buf),
            Connection::Tcp(t) => std::pin::Pin::new(t).poll_read(cx, buf),
            Connection::Udp(t) => std::pin::Pin::new(t).poll_read(cx, buf),
        }
    }
}

impl AsyncWrite for Connection {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match self.get_mut() {
            Connection::Serial(t) => std::pin::Pin::new(t).poll_write(cx, buf),
            Connection::Tcp(t) => std::pin::Pin::new(t).poll_write(cx, buf),
            Connection::Udp(t) => std::pin::Pin::new(t).poll_write(cx, buf),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Connection::Serial(t) => std::pin::Pin::new(t).poll_flush(cx),
            Connection::Tcp(t) => std::pin::Pin::new(t).poll_flush(cx),
            Connection::Udp(t) => std::pin::Pin::new(t).poll_flush(cx),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.get_mut() {
            Connection::Serial(t) => std::pin::Pin::new(t).poll_shutdown(cx),
            Connection::Tcp(t) => std::pin::Pin::new(t).poll_shutdown(cx),
            Connection::Udp(t) => std::pin::Pin::new(t).poll_shutdown(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::serial::{
        SerialDataBits, SerialFlowControl, SerialParity, SerialStopBits,
    };

    fn serial_config(port: &str, baud_rate: u32) -> TransportConfig {
        TransportConfig::Serial {
            port: port.into(),
            baud_rate,
            data_bits: SerialDataBits::Eight,
            parity: SerialParity::None,
            stop_bits: SerialStopBits::One,
            flow_control: SerialFlowControl::None,
            dtr: false,
            rts: false,
        }
    }

    #[test]
    fn test_transport_type_display() {
        assert_eq!(TransportType::Serial.to_string(), "Serial");
        assert_eq!(TransportType::Tcp.to_string(), "Tcp");
        assert_eq!(TransportType::Udp.to_string(), "Udp");
    }

    #[test]
    fn test_transport_type_debug() {
        let _ = format!("{:?}", TransportType::Serial);
        let _ = format!("{:?}", TransportType::Tcp);
        let _ = format!("{:?}", TransportType::Udp);
    }

    #[test]
    fn test_transport_type_eq() {
        assert_eq!(TransportType::Serial, TransportType::Serial);
        assert_eq!(TransportType::Tcp, TransportType::Tcp);
        assert_eq!(TransportType::Udp, TransportType::Udp);
        assert_ne!(TransportType::Serial, TransportType::Tcp);
        assert_ne!(TransportType::Serial, TransportType::Udp);
        assert_ne!(TransportType::Tcp, TransportType::Udp);
    }

    #[test]
    fn test_transport_type_copy_clone() {
        let original = TransportType::Serial;
        let cloned = original;
        assert_eq!(original, cloned);

        let original = TransportType::Tcp;
        let cloned = original.clone();
        assert_eq!(original, cloned);

        let original = TransportType::Udp;
        let cloned = original.clone();
        assert_eq!(original, cloned);
    }

    #[test]
    fn test_transport_type_serialize() {
        assert_eq!(
            serde_json::to_string(&TransportType::Serial).unwrap(),
            "\"Serial\""
        );
        assert_eq!(
            serde_json::to_string(&TransportType::Tcp).unwrap(),
            "\"Tcp\""
        );
        assert_eq!(
            serde_json::to_string(&TransportType::Udp).unwrap(),
            "\"Udp\""
        );
    }

    #[test]
    fn test_transport_type_deserialize() {
        let t: TransportType = serde_json::from_str("\"Serial\"").unwrap();
        assert_eq!(t, TransportType::Serial);

        let t: TransportType = serde_json::from_str("\"Tcp\"").unwrap();
        assert_eq!(t, TransportType::Tcp);

        let t: TransportType = serde_json::from_str("\"Udp\"").unwrap();
        assert_eq!(t, TransportType::Udp);
    }

    #[test]
    fn test_transport_type_roundtrip() {
        for original in [
            TransportType::Serial,
            TransportType::Tcp,
            TransportType::Udp,
        ] {
            let json = serde_json::to_string(&original).unwrap();
            let roundtripped: TransportType = serde_json::from_str(&json).unwrap();
            assert_eq!(original, roundtripped);
        }
    }

    #[test]
    fn test_transport_config_debug() {
        let cfg = TransportConfig::Serial {
            port: "COM1".into(),
            baud_rate: 115200,
            data_bits: SerialDataBits::Eight,
            parity: SerialParity::None,
            stop_bits: SerialStopBits::One,
            flow_control: SerialFlowControl::None,
            dtr: false,
            rts: false,
        };
        let _ = format!("{:?}", cfg);

        let cfg = TransportConfig::Tcp {
            addr: "127.0.0.1:8080".into(),
        };
        let _ = format!("{:?}", cfg);

        let cfg = TransportConfig::Udp {
            bind_addr: "0.0.0.0:9000".into(),
            remote_addr: Some("192.168.1.1:9001".into()),
        };
        let _ = format!("{:?}", cfg);
    }

    #[test]
    fn test_transport_config_clone() {
        let original = serial_config("COM1", 115200);
        let cloned = original.clone();
        assert_eq!(format!("{:?}", original), format!("{:?}", cloned));

        let original = TransportConfig::Tcp {
            addr: "127.0.0.1:8080".into(),
        };
        let cloned = original.clone();
        assert_eq!(format!("{:?}", original), format!("{:?}", cloned));

        let original = TransportConfig::Udp {
            bind_addr: "0.0.0.0:9000".into(),
            remote_addr: Some("192.168.1.1:9001".into()),
        };
        let cloned = original.clone();
        assert_eq!(format!("{:?}", original), format!("{:?}", cloned));
    }

    #[test]
    fn test_transport_config_serialize_serial() {
        let cfg = serial_config("COM1", 115200);
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("COM1"));
        assert!(json.contains("115200"));
        assert!(json.contains("data_bits"));
        assert!(json.contains("parity"));
        assert!(json.contains("stop_bits"));
        assert!(json.contains("flow_control"));
    }

    #[test]
    fn test_transport_config_deserialize_serial() {
        let json = r#"{"Serial":{"port":"COM1","baud_rate":9600}}"#;
        let cfg: TransportConfig = serde_json::from_str(json).unwrap();
        match cfg {
            TransportConfig::Serial {
                port,
                baud_rate,
                data_bits,
                parity,
                stop_bits,
                flow_control,
                dtr,
                rts,
            } => {
                assert_eq!(port, "COM1");
                assert_eq!(baud_rate, 9600);
                assert_eq!(data_bits, SerialDataBits::Eight);
                assert_eq!(parity, SerialParity::None);
                assert_eq!(stop_bits, SerialStopBits::One);
                assert_eq!(flow_control, SerialFlowControl::None);
                assert!(!dtr);
                assert!(!rts);
            }
            _ => panic!("expected Serial variant"),
        }
    }

    #[test]
    fn test_transport_config_deserialize_serial_with_explicit_options() {
        let json = r#"{"Serial":{"port":"COM2","baud_rate":57600,"data_bits":"Seven","parity":"Even","stop_bits":"Two","flow_control":"Hardware"}}"#;
        let cfg: TransportConfig = serde_json::from_str(json).unwrap();
        match cfg {
            TransportConfig::Serial {
                port,
                baud_rate,
                data_bits,
                parity,
                stop_bits,
                flow_control,
                dtr,
                rts,
            } => {
                assert_eq!(port, "COM2");
                assert_eq!(baud_rate, 57600);
                assert_eq!(data_bits, SerialDataBits::Seven);
                assert_eq!(parity, SerialParity::Even);
                assert_eq!(stop_bits, SerialStopBits::Two);
                assert_eq!(flow_control, SerialFlowControl::Hardware);
                assert!(!dtr);
                assert!(!rts);
            }
            _ => panic!("expected Serial variant"),
        }
    }

    #[test]
    fn test_transport_config_serialize_tcp() {
        let cfg = TransportConfig::Tcp {
            addr: "127.0.0.1:8080".into(),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("127.0.0.1:8080"));
    }

    #[test]
    fn test_transport_config_deserialize_tcp() {
        let json = r#"{"Tcp":{"addr":"127.0.0.1:8080"}}"#;
        let cfg: TransportConfig = serde_json::from_str(json).unwrap();
        match cfg {
            TransportConfig::Tcp { addr } => {
                assert_eq!(addr, "127.0.0.1:8080");
            }
            _ => panic!("expected Tcp variant"),
        }
    }

    #[test]
    fn test_transport_config_serialize_udp_with_remote() {
        let cfg = TransportConfig::Udp {
            bind_addr: "0.0.0.0:9000".into(),
            remote_addr: Some("192.168.1.1:9001".into()),
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("0.0.0.0:9000"));
        assert!(json.contains("192.168.1.1:9001"));
    }

    #[test]
    fn test_transport_config_serialize_udp_without_remote() {
        let cfg = TransportConfig::Udp {
            bind_addr: "0.0.0.0:9000".into(),
            remote_addr: None,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        assert!(json.contains("0.0.0.0:9000"));
        assert!(json.contains("null"));
    }

    #[test]
    fn test_transport_config_deserialize_udp() {
        let json = r#"{"Udp":{"bind_addr":"0.0.0.0:9000","remote_addr":"192.168.1.1:9001"}}"#;
        let cfg: TransportConfig = serde_json::from_str(json).unwrap();
        match cfg {
            TransportConfig::Udp {
                bind_addr,
                remote_addr,
            } => {
                assert_eq!(bind_addr, "0.0.0.0:9000");
                assert_eq!(remote_addr, Some("192.168.1.1:9001".into()));
            }
            _ => panic!("expected Udp variant"),
        }

        let json = r#"{"Udp":{"bind_addr":"0.0.0.0:9000","remote_addr":null}}"#;
        let cfg: TransportConfig = serde_json::from_str(json).unwrap();
        match cfg {
            TransportConfig::Udp {
                bind_addr,
                remote_addr,
            } => {
                assert_eq!(bind_addr, "0.0.0.0:9000");
                assert_eq!(remote_addr, None);
            }
            _ => panic!("expected Udp variant"),
        }
    }

    #[test]
    fn test_transport_config_roundtrip() {
        let configs = vec![
            serial_config("COM3", 57600),
            TransportConfig::Tcp {
                addr: "10.0.0.1:9999".into(),
            },
            TransportConfig::Udp {
                bind_addr: "0.0.0.0:7000".into(),
                remote_addr: Some("10.0.0.2:7001".into()),
            },
            TransportConfig::Udp {
                bind_addr: "127.0.0.1:8000".into(),
                remote_addr: None,
            },
        ];

        for original in &configs {
            let json = serde_json::to_string(original).unwrap();
            let roundtripped: TransportConfig = serde_json::from_str(&json).unwrap();
            assert_eq!(
                serde_json::to_string(&roundtripped).unwrap(),
                json,
                "roundtrip serialized forms must match"
            );
        }
    }

    // ── Connection tests ──────────────────────────────────────────────

    #[test]
    fn test_connection_new_serial() {
        let conn = Connection::new(serial_config("COM1", 115200));
        assert_eq!(conn.transport_type(), TransportType::Serial);
        assert_eq!(conn.name(), "COM1");
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_connection_new_tcp() {
        let conn = Connection::new(TransportConfig::Tcp {
            addr: "192.168.1.1:8080".into(),
        });
        assert_eq!(conn.transport_type(), TransportType::Tcp);
        assert_eq!(conn.name(), "192.168.1.1:8080");
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_connection_new_udp_with_remote() {
        let conn = Connection::new(TransportConfig::Udp {
            bind_addr: "0.0.0.0:9000".into(),
            remote_addr: Some("192.168.1.1:9001".into()),
        });
        assert_eq!(conn.transport_type(), TransportType::Udp);
        assert_eq!(conn.name(), "0.0.0.0:9000");
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_connection_new_udp_without_remote() {
        let conn = Connection::new(TransportConfig::Udp {
            bind_addr: "127.0.0.1:0".into(),
            remote_addr: None,
        });
        assert_eq!(conn.transport_type(), TransportType::Udp);
        assert_eq!(conn.name(), "127.0.0.1:0");
        assert!(!conn.is_connected());
    }

    #[test]
    fn test_connection_is_connected_default() {
        let serial = Connection::new(serial_config("COM1", 9600));
        let tcp = Connection::new(TransportConfig::Tcp {
            addr: "127.0.0.1:8080".into(),
        });
        let udp = Connection::new(TransportConfig::Udp {
            bind_addr: "0.0.0.0:0".into(),
            remote_addr: None,
        });
        assert!(!serial.is_connected());
        assert!(!tcp.is_connected());
        assert!(!udp.is_connected());
    }

    #[test]
    fn test_connection_debug() {
        let serial = Connection::new(serial_config("COM1", 115200));
        let tcp = Connection::new(TransportConfig::Tcp {
            addr: "127.0.0.1:8080".into(),
        });
        let udp = Connection::new(TransportConfig::Udp {
            bind_addr: "0.0.0.0:0".into(),
            remote_addr: None,
        });
        let _ = format!("{:?}", serial);
        let _ = format!("{:?}", tcp);
        let _ = format!("{:?}", udp);
    }

    fn noop_waker() -> std::task::Waker {
        use std::task::{RawWaker, RawWakerVTable};
        unsafe fn clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &VTABLE)
        }
        unsafe fn wake(_: *const ()) {}
        unsafe fn wake_by_ref(_: *const ()) {}
        unsafe fn drop(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, wake, wake_by_ref, drop);
        unsafe { std::task::Waker::from_raw(RawWaker::new(std::ptr::null(), &VTABLE)) }
    }

    fn serial_conn() -> Connection {
        Connection::new(serial_config("COM1", 115200))
    }

    fn tcp_conn() -> Connection {
        Connection::new(TransportConfig::Tcp {
            addr: "127.0.0.1:8080".into(),
        })
    }

    fn udp_conn() -> Connection {
        Connection::new(TransportConfig::Udp {
            bind_addr: "127.0.0.1:0".into(),
            remote_addr: None,
        })
    }

    #[test]
    fn test_connection_poll_read_not_connected() {
        use std::pin::Pin;
        use std::task::{Context, Poll};
        use tokio::io::ReadBuf;

        for mut conn in [serial_conn(), tcp_conn(), udp_conn()] {
            let pinned = Pin::new(&mut conn);
            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);
            let mut buf_data = [0u8; 16];
            let mut buf = ReadBuf::new(&mut buf_data);
            match pinned.poll_read(&mut cx, &mut buf) {
                Poll::Ready(Err(e)) => {
                    assert_eq!(e.kind(), std::io::ErrorKind::NotConnected);
                }
                other => panic!("expected Poll::Ready(Err(NotConnected)), got {:?}", other),
            }
        }
    }

    #[test]
    fn test_connection_poll_write_not_connected() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        for mut conn in [serial_conn(), tcp_conn(), udp_conn()] {
            let pinned = Pin::new(&mut conn);
            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);
            match pinned.poll_write(&mut cx, b"hello") {
                Poll::Ready(Err(e)) => {
                    assert_eq!(e.kind(), std::io::ErrorKind::NotConnected);
                }
                other => panic!("expected Poll::Ready(Err(NotConnected)), got {:?}", other),
            }
        }
    }

    #[test]
    fn test_connection_poll_flush_not_connected() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        for mut conn in [serial_conn(), tcp_conn(), udp_conn()] {
            let pinned = Pin::new(&mut conn);
            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);
            match pinned.poll_flush(&mut cx) {
                Poll::Ready(Ok(())) => {}
                other => panic!("expected Poll::Ready(Ok(())), got {:?}", other),
            }
        }
    }

    #[test]
    fn test_connection_poll_shutdown_not_connected() {
        use std::pin::Pin;
        use std::task::{Context, Poll};

        for mut conn in [serial_conn(), tcp_conn(), udp_conn()] {
            let pinned = Pin::new(&mut conn);
            let waker = noop_waker();
            let mut cx = Context::from_waker(&waker);
            match pinned.poll_shutdown(&mut cx) {
                Poll::Ready(Ok(())) => {}
                other => panic!("expected Poll::Ready(Ok(())), got {:?}", other),
            }
        }
    }

    #[tokio::test]
    async fn test_connection_tcp_connect_and_disconnect() {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let mut conn = Connection::new(TransportConfig::Tcp { addr });
        assert!(!conn.is_connected());

        conn.connect().await.unwrap();
        assert!(conn.is_connected());

        conn.disconnect().await.unwrap();
        assert!(!conn.is_connected());
    }

    #[tokio::test]
    async fn test_connection_udp_connect_and_disconnect_without_remote() {
        let mut conn = Connection::new(TransportConfig::Udp {
            bind_addr: "127.0.0.1:0".into(),
            remote_addr: None,
        });
        assert!(!conn.is_connected());

        conn.connect().await.unwrap();
        assert!(conn.is_connected());

        conn.disconnect().await.unwrap();
        assert!(!conn.is_connected());
    }

    #[tokio::test]
    async fn test_connection_udp_connect_and_disconnect_with_remote() {
        use tokio::net::UdpSocket;
        let remote = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let remote_addr = remote.local_addr().unwrap().to_string();

        let mut conn = Connection::new(TransportConfig::Udp {
            bind_addr: "127.0.0.1:0".into(),
            remote_addr: Some(remote_addr),
        });
        assert!(!conn.is_connected());

        conn.connect().await.unwrap();
        assert!(conn.is_connected());

        conn.disconnect().await.unwrap();
        assert!(!conn.is_connected());
    }

    #[tokio::test]
    async fn test_connection_tcp_double_connect_is_noop() {
        use tokio::net::TcpListener;
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let mut conn = Connection::new(TransportConfig::Tcp { addr });
        conn.connect().await.unwrap();
        assert!(conn.is_connected());

        let result = conn.connect().await;
        assert!(result.is_ok(), "double connect should be a no-op");
        assert!(conn.is_connected());
    }

    #[tokio::test]
    async fn test_connection_tcp_disconnect_without_connect() {
        let mut conn = Connection::new(TransportConfig::Tcp {
            addr: "127.0.0.1:8080".into(),
        });
        let result = conn.disconnect().await;
        assert!(result.is_ok(), "disconnect without connect should be safe");
        assert!(!conn.is_connected());
    }

    #[tokio::test]
    async fn test_connection_tcp_connect_to_unreachable() {
        let mut conn = Connection::new(TransportConfig::Tcp {
            addr: "127.0.0.1:1".into(),
        });
        let result = conn.connect().await;
        assert!(result.is_err());
        assert!(!conn.is_connected());
    }

    #[tokio::test]
    async fn test_connection_udp_connect_bind_failure() {
        let mut conn = Connection::new(TransportConfig::Udp {
            bind_addr: "invalid_addr".into(),
            remote_addr: None,
        });
        let result = conn.connect().await;
        assert!(result.is_err());
        assert!(!conn.is_connected());
    }

    #[tokio::test]
    async fn test_connection_udp_double_connect_is_noop() {
        let mut conn = Connection::new(TransportConfig::Udp {
            bind_addr: "127.0.0.1:0".into(),
            remote_addr: None,
        });
        conn.connect().await.unwrap();
        assert!(conn.is_connected());

        let result = conn.connect().await;
        assert!(result.is_ok());
        assert!(conn.is_connected());
    }

    #[tokio::test]
    async fn test_connection_tcp_async_read_write() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            AsyncWriteExt::write_all(&mut stream, b"hello")
                .await
                .unwrap();
            let mut buf = [0u8; 5];
            AsyncReadExt::read_exact(&mut stream, &mut buf)
                .await
                .unwrap();
            assert_eq!(&buf, b"world");
        });

        let mut conn = Connection::new(TransportConfig::Tcp { addr });
        conn.connect().await.unwrap();

        let mut buf = [0u8; 5];
        AsyncReadExt::read_exact(&mut conn, &mut buf).await.unwrap();
        assert_eq!(&buf, b"hello");

        AsyncWriteExt::write_all(&mut conn, b"world").await.unwrap();

        conn.disconnect().await.unwrap();
        server.await.unwrap();
    }

    #[tokio::test]
    async fn test_connection_udp_async_read_write_with_remote() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio::net::UdpSocket;

        let remote = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let remote_addr = remote.local_addr().unwrap().to_string();

        let mut conn = Connection::new(TransportConfig::Udp {
            bind_addr: "127.0.0.1:0".into(),
            remote_addr: Some(remote_addr),
        });
        conn.connect().await.unwrap();

        AsyncWriteExt::write_all(&mut conn, b"hello").await.unwrap();

        let mut buf = [0u8; 1024];
        let (n, from) = remote.recv_from(&mut buf).await.unwrap();
        assert_eq!(&buf[..n], b"hello");

        remote.send_to(b"world", from).await.unwrap();

        let mut read_buf = [0u8; 5];
        AsyncReadExt::read_exact(&mut conn, &mut read_buf)
            .await
            .unwrap();
        assert_eq!(&read_buf, b"world");
    }
}
