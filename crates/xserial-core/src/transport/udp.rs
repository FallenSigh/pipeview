use std::pin::Pin;
use std::task::{Context, Poll};

use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::UdpSocket;
use tracing::{debug, info};

use super::{Transport, TransportType};
use crate::error::Result;

#[derive(Debug)]
pub struct UdpTransport {
    socket: Option<UdpSocket>,
    bind_addr: String,
    remote_addr: Option<String>,
    read_buf: Vec<u8>,
    read_pos: usize,

    temp_recv_buf: Vec<u8>,
}

impl UdpTransport {
    pub fn new(bind_addr: String, remote_addr: Option<String>) -> Self {
        Self {
            socket: None,
            bind_addr,
            remote_addr,
            read_buf: Vec::new(),
            read_pos: 0,
            temp_recv_buf: vec![0u8; 65536], // 初始化一次，重复使用
        }
    }

    pub fn bind_addr(&self) -> &str {
        &self.bind_addr
    }
}

impl AsyncRead for UdpTransport {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();

        // 优先处理遗留的内部缓冲数据
        if this.read_pos < this.read_buf.len() {
            let remaining = &this.read_buf[this.read_pos..];
            let to_copy = std::cmp::min(remaining.len(), buf.remaining());

            buf.put_slice(&remaining[..to_copy]);
            this.read_pos += to_copy;

            // 如果缓冲区数据已被全部读完，清空它以便后续复用，避免无止境增长
            if this.read_pos == this.read_buf.len() {
                this.read_buf.clear();
                this.read_pos = 0;
            }
            return Poll::Ready(Ok(()));
        }

        // 如果内部缓冲为空，尝试从 Socket 读取新的 UDP 包
        let socket = match this.socket.as_ref() {
            Some(s) => s,
            None => {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "UDP socket is not connected",
                )));
            }
        };

        // 使用复用的 temp_recv_buf，包裹为 tokio 需要的 ReadBuf
        let mut temp_buf = ReadBuf::new(&mut this.temp_recv_buf);

        // 如果配置了 remote_addr，说明 socket 已经 connect 过，可以用 poll_recv
        // 否则只能用 poll_recv_from，丢弃掉远端地址信息
        let poll_result = if this.remote_addr.is_some() {
            socket.poll_recv(cx, &mut temp_buf)
        } else {
            // poll_recv_from 会返回 (usize, SocketAddr)，我们将其映射回统一的 () 类型
            socket
                .poll_recv_from(cx, &mut temp_buf)
                .map(|res| res.map(|_| ()))
        };

        match poll_result {
            Poll::Ready(Ok(())) => {
                let filled = temp_buf.filled();
                let to_copy = std::cmp::min(filled.len(), buf.remaining());

                buf.put_slice(&filled[..to_copy]);

                // 核心逻辑：如果本次读取到的 UDP 包大于外面提供的 buf 的剩余空间
                // 将没装下的剩余部分放进内部的 read_buf 留作下次 poll_read 使用
                if to_copy < filled.len() {
                    this.read_buf.clear(); // 确保安全清空
                    this.read_buf.extend_from_slice(&filled[to_copy..]);
                    this.read_pos = 0;
                }

                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(e)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for UdpTransport {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        let this = self.get_mut();

        let socket = match this.socket.as_ref() {
            Some(s) => s,
            None => {
                return Poll::Ready(Err(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "UDP socket is not connected",
                )));
            }
        };

        // AsyncWrite 是不带目标地址的流式写入接口。
        // 因此必须绑定了远端地址 (remote_addr) 才能确切知道把 UDP 包发送给谁。
        if this.remote_addr.is_some() {
            socket.poll_send(cx, buf)
        } else {
            Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Remote address must be set to use AsyncWrite for UDP",
            )))
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // UDP 是无连接的数据报协议，没有缓冲区需要手动 flush
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        // UDP 没有 TCP 的四次挥手过程，直接 Ready
        Poll::Ready(Ok(()))
    }
}

#[async_trait]
impl Transport for UdpTransport {
    fn name(&self) -> &str {
        &self.bind_addr
    }

    fn transport_type(&self) -> TransportType {
        TransportType::Udp
    }

    fn is_connected(&self) -> bool {
        self.socket.is_some()
    }

    async fn connect(&mut self) -> Result<()> {
        if self.is_connected() {
            return Ok(());
        }

        info!("Binding UDP socket to {}", self.bind_addr);

        let socket = UdpSocket::bind(&self.bind_addr).await.map_err(|e| {
            crate::error::Error::ConnectionFailed(format!(
                "Failed to bind UDP {}: {}",
                self.bind_addr, e
            ))
        })?;

        if let Some(ref remote) = self.remote_addr {
            socket.connect(remote).await.map_err(|e| {
                crate::error::Error::ConnectionFailed(format!(
                    "Failed to connect UDP to {}: {}",
                    remote, e
                ))
            })?;
            debug!("UDP connected to {}", remote);
        }

        debug!("UDP socket bound to {}", self.bind_addr);
        self.socket = Some(socket);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(socket) = self.socket.take() {
            debug!("Closing UDP socket {}", self.bind_addr);
            drop(socket);
            info!("UDP socket {} closed", self.bind_addr);
        }
        self.read_buf.clear();
        self.read_pos = 0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Error;
    use std::sync::LazyLock;
    use std::task::{RawWaker, RawWakerVTable, Waker};
    use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadBuf};
    use tokio::net::UdpSocket;

    static NOOP_VTABLE: RawWakerVTable = RawWakerVTable::new(
        |_| RawWaker::new(std::ptr::null(), &NOOP_VTABLE),
        |_| {},
        |_| {},
        |_| {},
    );

    fn noop_waker() -> Waker {
        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &NOOP_VTABLE)) }
    }

    static NOOP_WAKER: LazyLock<Waker> = LazyLock::new(noop_waker);

    fn noop_context() -> Context<'static> {
        Context::from_waker(&NOOP_WAKER)
    }

    #[test]
    fn test_new_with_remote() {
        let t = UdpTransport::new("127.0.0.1:9000".into(), Some("127.0.0.1:9001".into()));
        assert_eq!(t.bind_addr(), "127.0.0.1:9000");
    }

    #[test]
    fn test_new_without_remote() {
        let t = UdpTransport::new("0.0.0.0:0".into(), None);
        assert_eq!(t.bind_addr(), "0.0.0.0:0");
    }

    #[test]
    fn test_name() {
        let t = UdpTransport::new("192.168.1.1:8888".into(), None);
        assert_eq!(t.name(), "192.168.1.1:8888");
    }

    #[test]
    fn test_transport_type() {
        let t = UdpTransport::new("127.0.0.1:0".into(), None);
        assert_eq!(t.transport_type(), TransportType::Udp);
    }

    #[test]
    fn test_is_connected_false_by_default() {
        let t = UdpTransport::new("127.0.0.1:0".into(), None);
        assert!(!t.is_connected());
    }

    #[test]
    fn test_poll_read_not_connected() {
        let mut t = UdpTransport::new("127.0.0.1:0".into(), None);
        let mut buf_data = [0u8; 64];
        let mut buf = ReadBuf::new(&mut buf_data);
        let mut cx = noop_context();
        let result = Pin::new(&mut t).poll_read(&mut cx, &mut buf);
        match result {
            Poll::Ready(Err(ref e)) => assert_eq!(e.kind(), std::io::ErrorKind::NotConnected),
            other => panic!("expected Poll::Ready(Err(NotConnected)), got {other:?}"),
        }
    }

    #[test]
    fn test_poll_write_not_connected() {
        let mut t = UdpTransport::new("127.0.0.1:0".into(), None);
        let mut cx = noop_context();
        let data = b"test";
        let result = Pin::new(&mut t).poll_write(&mut cx, data);
        match result {
            Poll::Ready(Err(ref e)) => assert_eq!(e.kind(), std::io::ErrorKind::NotConnected),
            other => panic!("expected Poll::Ready(Err(NotConnected)), got {other:?}"),
        }
    }

    #[test]
    fn test_poll_flush_not_connected() {
        let mut t = UdpTransport::new("127.0.0.1:0".into(), None);
        let mut cx = noop_context();
        let result = Pin::new(&mut t).poll_flush(&mut cx);
        assert!(matches!(result, Poll::Ready(Ok(()))));
    }

    #[test]
    fn test_poll_shutdown_not_connected() {
        let mut t = UdpTransport::new("127.0.0.1:0".into(), None);
        let mut cx = noop_context();
        let result = Pin::new(&mut t).poll_shutdown(&mut cx);
        assert!(matches!(result, Poll::Ready(Ok(()))));
    }

    #[tokio::test]
    async fn test_connect_and_disconnect_without_remote() {
        let mut t = UdpTransport::new("127.0.0.1:0".into(), None);
        assert!(!t.is_connected());

        t.connect().await.expect("connect should succeed");
        assert!(t.is_connected());

        t.disconnect().await.expect("disconnect should succeed");
        assert!(!t.is_connected());
    }

    #[tokio::test]
    async fn test_connect_and_disconnect_with_remote() {
        let remote = UdpSocket::bind("127.0.0.1:0").await.expect("remote bind");
        let remote_addr = remote.local_addr().unwrap().to_string();

        let mut t = UdpTransport::new("127.0.0.1:0".into(), Some(remote_addr));
        t.connect()
            .await
            .expect("connect with remote should succeed");
        assert!(t.is_connected());

        t.disconnect().await.expect("disconnect should succeed");
        assert!(!t.is_connected());
    }

    #[tokio::test]
    async fn test_double_connect_is_noop() {
        let mut t = UdpTransport::new("127.0.0.1:0".into(), None);
        t.connect().await.expect("first connect");
        assert!(t.is_connected());

        t.connect().await.expect("second connect (idempotent)");
        assert!(t.is_connected());
    }

    #[tokio::test]
    async fn test_connect_bind_failure() {
        let mut t = UdpTransport::new("invalid_addr".into(), None);
        let result = t.connect().await;
        match result {
            Err(Error::ConnectionFailed(_)) => {}
            other => panic!("expected ConnectionFailed error, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_async_read_write_with_remote() {
        let remote = UdpSocket::bind("127.0.0.1:0").await.expect("remote bind");
        let remote_addr = remote.local_addr().unwrap().to_string();

        let mut t = UdpTransport::new("127.0.0.1:0".into(), Some(remote_addr));
        t.connect().await.expect("connect");

        AsyncWriteExt::write_all(&mut t, b"hello")
            .await
            .expect("write_all hello");

        let mut buf = [0u8; 1024];
        let (n, transport_addr) = remote.recv_from(&mut buf).await.expect("remote recv_from");
        assert_eq!(&buf[..n], b"hello");

        remote
            .send_to(b"world", transport_addr)
            .await
            .expect("remote send_to");

        let mut read_buf = [0u8; 5];
        AsyncReadExt::read_exact(&mut t, &mut read_buf)
            .await
            .expect("read_exact world");
        assert_eq!(&read_buf, b"world");
    }

    #[tokio::test]
    async fn test_write_fails_without_remote() {
        let mut t = UdpTransport::new("127.0.0.1:0".into(), None);
        t.connect().await.expect("connect (binds but no remote)");

        let result = AsyncWriteExt::write_all(&mut t, b"test").await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
    }

    #[tokio::test]
    async fn test_oversized_datagram_buffering() {
        let remote = UdpSocket::bind("127.0.0.1:0").await.expect("remote bind");
        let remote_addr = remote.local_addr().unwrap().to_string();

        let mut t = UdpTransport::new("127.0.0.1:0".into(), Some(remote_addr));
        t.connect().await.expect("connect");

        AsyncWriteExt::write_all(&mut t, b"x")
            .await
            .expect("write ping");
        let mut ping_buf = [0u8; 1];
        let (_, transport_addr) = remote
            .recv_from(&mut ping_buf)
            .await
            .expect("remote recv ping");

        let large_data: Vec<u8> = vec![b'A'; 100];
        remote
            .send_to(&large_data, transport_addr)
            .await
            .expect("send large datagram");

        let mut small_buf = [0u8; 10];
        AsyncReadExt::read_exact(&mut t, &mut small_buf)
            .await
            .expect("read 10 bytes");
        assert_eq!(&small_buf, b"AAAAAAAAAA");

        let mut rest_buf = [0u8; 90];
        AsyncReadExt::read_exact(&mut t, &mut rest_buf)
            .await
            .expect("read 90 bytes");
        assert_eq!(&rest_buf[..], &vec![b'A'; 90][..]);
    }
}
