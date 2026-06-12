use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::TcpStream;
use tracing::{debug, info};

use super::{Transport, TransportType};
use crate::error::Result;

#[derive(Debug)]
pub struct TcpTransport {
    stream: Option<TcpStream>,
    addr: String,
}

impl TcpTransport {
    pub fn new(addr: String) -> Self {
        Self { stream: None, addr }
    }

    pub fn addr(&self) -> &str {
        &self.addr
    }
}

impl AsyncRead for TcpTransport {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.get_mut().stream {
            Some(stream) => std::pin::Pin::new(stream).poll_read(cx, buf),
            None => std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "TCP not connected",
            ))),
        }
    }
}

impl AsyncWrite for TcpTransport {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match &mut self.get_mut().stream {
            Some(stream) => std::pin::Pin::new(stream).poll_write(cx, buf),
            None => std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "TCP not connected",
            ))),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.get_mut().stream {
            Some(stream) => std::pin::Pin::new(stream).poll_flush(cx),
            None => std::task::Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.get_mut().stream {
            Some(stream) => std::pin::Pin::new(stream).poll_shutdown(cx),
            None => std::task::Poll::Ready(Ok(())),
        }
    }
}

#[async_trait]
impl Transport for TcpTransport {
    fn name(&self) -> &str {
        &self.addr
    }

    fn transport_type(&self) -> TransportType {
        TransportType::Tcp
    }

    fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    async fn connect(&mut self) -> Result<()> {
        if self.is_connected() {
            return Ok(());
        }

        info!("Connecting to TCP {}", self.addr);

        let stream = TcpStream::connect(&self.addr).await.map_err(|e| {
            crate::error::Error::ConnectionFailed(format!(
                "Failed to connect to {}: {}",
                self.addr, e
            ))
        })?;

        debug!("TCP connection to {} established", self.addr);
        self.stream = Some(stream);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(stream) = self.stream.take() {
            debug!("Closing TCP connection {}", self.addr);
            drop(stream);
            info!("TCP connection {} closed", self.addr);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadBuf};
    use tokio::net::TcpListener;

    // ── Helper: noop waker for poll tests ─────────────────────────────

    fn noop_waker() -> Waker {
        unsafe fn noop_raw_clone(data: *const ()) -> RawWaker {
            RawWaker::new(data, &NOOP_VTABLE)
        }
        unsafe fn noop_raw_wake(_data: *const ()) {}
        unsafe fn noop_raw_wake_by_ref(_data: *const ()) {}
        unsafe fn noop_raw_drop(_data: *const ()) {}

        static NOOP_VTABLE: RawWakerVTable = RawWakerVTable::new(
            noop_raw_clone,
            noop_raw_wake,
            noop_raw_wake_by_ref,
            noop_raw_drop,
        );

        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &NOOP_VTABLE)) }
    }

    // ── Sync accessor tests ───────────────────────────────────────────

    #[test]
    fn test_new() {
        let transport = TcpTransport::new("127.0.0.1:8080".into());
        assert_eq!(transport.addr(), "127.0.0.1:8080");
    }

    #[test]
    fn test_name() {
        let transport = TcpTransport::new("127.0.0.1:8080".into());
        assert_eq!(transport.name(), transport.addr());
        assert_eq!(transport.name(), "127.0.0.1:8080");
    }

    #[test]
    fn test_transport_type() {
        let transport = TcpTransport::new("127.0.0.1:8080".into());
        assert_eq!(transport.transport_type(), TransportType::Tcp);
    }

    #[test]
    fn test_is_connected_false_by_default() {
        let transport = TcpTransport::new("127.0.0.1:8080".into());
        assert!(!transport.is_connected());
    }

    // ── Poll error-state tests (no real connection) ───────────────────

    #[test]
    fn test_poll_read_not_connected() {
        let mut transport = TcpTransport::new("127.0.0.1:8080".into());
        let pinned = Pin::new(&mut transport);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut buf_data = [0u8; 16];
        let mut buf = ReadBuf::new(&mut buf_data);

        match pinned.poll_read(&mut cx, &mut buf) {
            Poll::Ready(Err(e)) => assert_eq!(e.kind(), std::io::ErrorKind::NotConnected),
            other => panic!("expected Poll::Ready(Err(NotConnected)), got {:?}", other),
        }
    }

    #[test]
    fn test_poll_write_not_connected() {
        let mut transport = TcpTransport::new("127.0.0.1:8080".into());
        let pinned = Pin::new(&mut transport);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        match pinned.poll_write(&mut cx, b"hello") {
            Poll::Ready(Err(e)) => assert_eq!(e.kind(), std::io::ErrorKind::NotConnected),
            other => panic!("expected Poll::Ready(Err(NotConnected)), got {:?}", other),
        }
    }

    #[test]
    fn test_poll_flush_not_connected() {
        let mut transport = TcpTransport::new("127.0.0.1:8080".into());
        let pinned = Pin::new(&mut transport);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        match pinned.poll_flush(&mut cx) {
            Poll::Ready(Ok(())) => {} // expected
            other => panic!("expected Poll::Ready(Ok(())), got {:?}", other),
        }
    }

    #[test]
    fn test_poll_shutdown_not_connected() {
        let mut transport = TcpTransport::new("127.0.0.1:8080".into());
        let pinned = Pin::new(&mut transport);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);

        match pinned.poll_shutdown(&mut cx) {
            Poll::Ready(Ok(())) => {} // expected
            other => panic!("expected Poll::Ready(Ok(())), got {:?}", other),
        }
    }

    // ── Async connect/disconnect tests ────────────────────────────────

    #[tokio::test]
    async fn test_connect_and_disconnect() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let mut transport = TcpTransport::new(addr);
        assert!(!transport.is_connected());

        transport.connect().await.unwrap();
        assert!(transport.is_connected());

        transport.disconnect().await.unwrap();
        assert!(!transport.is_connected());
    }

    #[tokio::test]
    async fn test_double_connect_is_noop() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        let mut transport = TcpTransport::new(addr);
        transport.connect().await.unwrap();
        assert!(transport.is_connected());

        // Second connect should be a safe no-op
        let result = transport.connect().await;
        assert!(result.is_ok(), "double connect should return Ok");
        assert!(transport.is_connected());
    }

    #[tokio::test]
    async fn test_connect_to_unreachable_addr() {
        let mut transport = TcpTransport::new("127.0.0.1:1".into());
        let result = transport.connect().await;
        assert!(result.is_err());
        match result.unwrap_err() {
            Error::ConnectionFailed(_) => {} // expected
            other => panic!("expected ConnectionFailed, got {:?}", other),
        }
        assert!(!transport.is_connected());
    }

    #[tokio::test]
    async fn test_disconnect_without_connect() {
        let mut transport = TcpTransport::new("127.0.0.1:8080".into());
        let result = transport.disconnect().await;
        assert!(
            result.is_ok(),
            "disconnect without connect should be a safe no-op"
        );
        assert!(!transport.is_connected());
    }

    // ── Async read/write with real connection ─────────────────────────

    #[tokio::test]
    async fn test_async_read_write() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap().to_string();

        // Server: accept one connection, write "hello", read 5 bytes back, verify "world"
        let server_handle = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let written = tokio::io::AsyncWriteExt::write_all(&mut stream, b"hello").await;
            assert!(written.is_ok(), "server write_all hello failed");
            let mut buf = [0u8; 5];
            let read = tokio::io::AsyncReadExt::read_exact(&mut stream, &mut buf).await;
            assert!(read.is_ok(), "server read_exact failed");
            assert_eq!(&buf, b"world");
        });

        // Client: connect, read "hello", write "world", disconnect
        let mut transport = TcpTransport::new(addr);
        transport.connect().await.unwrap();

        let mut read_buf = [0u8; 5];
        AsyncReadExt::read_exact(&mut transport, &mut read_buf)
            .await
            .unwrap();
        assert_eq!(&read_buf, b"hello");

        AsyncWriteExt::write_all(&mut transport, b"world")
            .await
            .unwrap();

        transport.disconnect().await.unwrap();

        // Ensure server task completed without panic
        server_handle.await.unwrap();
    }
}
