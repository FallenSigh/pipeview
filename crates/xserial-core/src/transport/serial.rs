use async_trait::async_trait;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio_serial::{DataBits, FlowControl, Parity, SerialPortBuilderExt, SerialStream, StopBits};
use serialport::SerialPort;
use tracing::{debug, info, warn};

use super::{Transport, TransportType};
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SerialDataBits {
    Five,
    Six,
    Seven,
    Eight,
}

impl From<SerialDataBits> for DataBits {
    fn from(value: SerialDataBits) -> Self {
        match value {
            SerialDataBits::Five => DataBits::Five,
            SerialDataBits::Six => DataBits::Six,
            SerialDataBits::Seven => DataBits::Seven,
            SerialDataBits::Eight => DataBits::Eight,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SerialParity {
    None,
    Odd,
    Even,
}

impl From<SerialParity> for Parity {
    fn from(value: SerialParity) -> Self {
        match value {
            SerialParity::None => Parity::None,
            SerialParity::Odd => Parity::Odd,
            SerialParity::Even => Parity::Even,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SerialStopBits {
    One,
    Two,
}

impl From<SerialStopBits> for StopBits {
    fn from(value: SerialStopBits) -> Self {
        match value {
            SerialStopBits::One => StopBits::One,
            SerialStopBits::Two => StopBits::Two,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum SerialFlowControl {
    None,
    Software,
    Hardware,
}

impl From<SerialFlowControl> for FlowControl {
    fn from(value: SerialFlowControl) -> Self {
        match value {
            SerialFlowControl::None => FlowControl::None,
            SerialFlowControl::Software => FlowControl::Software,
            SerialFlowControl::Hardware => FlowControl::Hardware,
        }
    }
}

#[derive(Debug)]
pub struct SerialTransport {
    port: Option<SerialStream>,
    port_name: String,
    baud_rate: u32,
    data_bits: SerialDataBits,
    parity: SerialParity,
    stop_bits: SerialStopBits,
    flow_control: SerialFlowControl,
    dtr: bool,
    rts: bool,
}

impl SerialTransport {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        port_name: String,
        baud_rate: u32,
        data_bits: SerialDataBits,
        parity: SerialParity,
        stop_bits: SerialStopBits,
        flow_control: SerialFlowControl,
        dtr: bool,
        rts: bool,
    ) -> Self {
        Self {
            port: None,
            port_name,
            baud_rate,
            data_bits,
            parity,
            stop_bits,
            flow_control,
            dtr,
            rts,
        }
    }

    pub fn list_ports() -> Vec<serialport::SerialPortInfo> {
        match serialport::available_ports() {
            Ok(ports) => ports,
            Err(e) => {
                warn!("Failed to enumerate serial ports: {}", e);
                vec![]
            }
        }
    }

    pub fn port_name(&self) -> &str {
        &self.port_name
    }

    pub fn baud_rate(&self) -> u32 {
        self.baud_rate
    }

    pub fn data_bits(&self) -> SerialDataBits {
        self.data_bits
    }

    pub fn parity(&self) -> SerialParity {
        self.parity
    }

    pub fn stop_bits(&self) -> SerialStopBits {
        self.stop_bits
    }

    pub fn flow_control(&self) -> SerialFlowControl {
        self.flow_control
    }

    pub fn set_dtr(&mut self, state: bool) -> Result<()> {
        match &mut self.port {
            Some(port) => {
                port.write_data_terminal_ready(state)?;
                self.dtr = state;
                debug!("DTR set to {} on {}", state, self.port_name);
                Ok(())
            }
            None => Err(crate::error::Error::ConnectionFailed(format!(
                "port {} not open",
                self.port_name
            ))),
        }
    }

    pub fn set_rts(&mut self, state: bool) -> Result<()> {
        match &mut self.port {
            Some(port) => {
                port.write_request_to_send(state)?;
                self.rts = state;
                debug!("RTS set to {} on {}", state, self.port_name);
                Ok(())
            }
            None => Err(crate::error::Error::ConnectionFailed(format!(
                "port {} not open",
                self.port_name
            ))),
        }
    }
}

impl AsyncRead for SerialTransport {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.get_mut().port {
            Some(port) => std::pin::Pin::new(port).poll_read(cx, buf),
            None => std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Serial port not open",
            ))),
        }
    }
}

impl AsyncWrite for SerialTransport {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        match &mut self.get_mut().port {
            Some(port) => std::pin::Pin::new(port).poll_write(cx, buf),
            None => std::task::Poll::Ready(Err(std::io::Error::new(
                std::io::ErrorKind::NotConnected,
                "Serial port not open",
            ))),
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.get_mut().port {
            Some(port) => std::pin::Pin::new(port).poll_flush(cx),
            None => std::task::Poll::Ready(Ok(())),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut self.get_mut().port {
            Some(port) => std::pin::Pin::new(port).poll_shutdown(cx),
            None => std::task::Poll::Ready(Ok(())),
        }
    }
}

#[async_trait]
impl Transport for SerialTransport {
    fn name(&self) -> &str {
        &self.port_name
    }

    fn transport_type(&self) -> TransportType {
        TransportType::Serial
    }

    fn is_connected(&self) -> bool {
        self.port.is_some()
    }

    async fn connect(&mut self) -> Result<()> {
        if self.is_connected() {
            return Ok(());
        }

        info!(
            "Opening serial port {} at {} baud ({:?}, {:?}, {:?}, {:?})",
            self.port_name,
            self.baud_rate,
            self.data_bits,
            self.parity,
            self.stop_bits,
            self.flow_control
        );

        let mut port = tokio_serial::new(&self.port_name, self.baud_rate)
            .data_bits(self.data_bits.into())
            .parity(self.parity.into())
            .stop_bits(self.stop_bits.into())
            .flow_control(self.flow_control.into())
            .open_native_async()
            .map_err(|e| {
                crate::error::Error::ConnectionFailed(format!(
                    "Failed to open {}: {}",
                    self.port_name, e
                ))
            })?;

        // Linux kernel toggles DTR/RTS on every open() — restore configured
        // state immediately afterward. Many wireless serial modules (HC-12,
        // HC-15, HC-05, Bluetooth/UART bridges) need DTR asserted to stay in
        // transparent data mode and not fall into AT-command / reset state.
        if let Err(e) = port.write_data_terminal_ready(self.dtr) {
            warn!("Failed to set DTR({}) on {}: {}", self.dtr, self.port_name, e);
        }
        if let Err(e) = port.write_request_to_send(self.rts) {
            warn!("Failed to set RTS({}) on {}: {}", self.rts, self.port_name, e);
        }

        debug!("Serial port {} opened successfully", self.port_name);
        self.port = Some(port);
        Ok(())
    }

    async fn disconnect(&mut self) -> Result<()> {
        if let Some(port) = self.port.take() {
            debug!("Closing serial port {}", self.port_name);
            drop(port);
            debug!("Serial port {} closed", self.port_name);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::ErrorKind;
    use std::pin::Pin;
    use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    use tokio::io::ReadBuf;

    fn noop_waker() -> Waker {
        unsafe fn raw_clone(_: *const ()) -> RawWaker {
            RawWaker::new(std::ptr::null(), &RAW_VTABLE)
        }
        unsafe fn raw_wake(_: *const ()) {}
        unsafe fn raw_wake_by_ref(_: *const ()) {}
        unsafe fn raw_drop(_: *const ()) {}

        static RAW_VTABLE: RawWakerVTable =
            RawWakerVTable::new(raw_clone, raw_wake, raw_wake_by_ref, raw_drop);

        unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &RAW_VTABLE)) }
    }

    // ── Constructor and accessor tests ──

    #[test]
    fn test_new() {
        let transport = SerialTransport::new(
            "COM1".into(),
            115200,
            SerialDataBits::Eight,
            SerialParity::None,
            SerialStopBits::One,
            SerialFlowControl::None,
        true,
        true,
        );
        assert_eq!(transport.port_name(), "COM1");
        assert_eq!(transport.baud_rate(), 115200);
    }

    #[test]
    fn test_new_different_params() {
        let transport = SerialTransport::new(
            "COM3".into(),
            9600,
            SerialDataBits::Seven,
            SerialParity::Even,
            SerialStopBits::Two,
            SerialFlowControl::Hardware,
        true,
        true,
        );
        assert_eq!(transport.port_name(), "COM3");
        assert_eq!(transport.baud_rate(), 9600);
        assert_eq!(transport.data_bits(), SerialDataBits::Seven);
        assert_eq!(transport.parity(), SerialParity::Even);
        assert_eq!(transport.stop_bits(), SerialStopBits::Two);
        assert_eq!(transport.flow_control(), SerialFlowControl::Hardware);
    }

    #[test]
    fn test_name() {
        let transport = SerialTransport::new(
            "COM1".into(),
            115200,
            SerialDataBits::Eight,
            SerialParity::None,
            SerialStopBits::One,
            SerialFlowControl::None,
        true,
        true,
        );
        assert_eq!(transport.name(), "COM1");
    }

    #[test]
    fn test_transport_type() {
        let transport = SerialTransport::new(
            "COM1".into(),
            115200,
            SerialDataBits::Eight,
            SerialParity::None,
            SerialStopBits::One,
            SerialFlowControl::None,
        true,
        true,
        );
        assert_eq!(transport.transport_type(), TransportType::Serial);
    }

    #[test]
    fn test_is_connected_false_by_default() {
        let transport = SerialTransport::new(
            "COM1".into(),
            115200,
            SerialDataBits::Eight,
            SerialParity::None,
            SerialStopBits::One,
            SerialFlowControl::None,
        true,
        true,
        );
        assert!(!transport.is_connected());
    }

    // ── list_ports tests ──

    #[test]
    fn test_list_ports_does_not_panic() {
        let _ = SerialTransport::list_ports();
    }

    #[test]
    fn test_list_ports_returns_vec() {
        let ports: Vec<serialport::SerialPortInfo> = SerialTransport::list_ports();
        let _ = ports;
    }

    // ── AsyncRead tests (not connected) ──

    #[tokio::test]
    async fn test_poll_read_not_connected() {
        let mut transport = SerialTransport::new(
            "COM1".into(),
            115200,
            SerialDataBits::Eight,
            SerialParity::None,
            SerialStopBits::One,
            SerialFlowControl::None,
        true,
        true,
        );
        let pinned = Pin::new(&mut transport);
        let mut buf_data = [0u8; 16];
        let mut buf = ReadBuf::new(&mut buf_data);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        match pinned.poll_read(&mut cx, &mut buf) {
            Poll::Ready(Err(e)) => assert_eq!(e.kind(), ErrorKind::NotConnected),
            other => panic!("expected Poll::Ready(Err(NotConnected)), got {:?}", other),
        }
    }

    // ── AsyncWrite tests (not connected) ──

    #[tokio::test]
    async fn test_poll_write_not_connected() {
        let mut transport = SerialTransport::new(
            "COM1".into(),
            115200,
            SerialDataBits::Eight,
            SerialParity::None,
            SerialStopBits::One,
            SerialFlowControl::None,
        true,
        true,
        );
        let pinned = Pin::new(&mut transport);
        let data = b"hello";
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        match pinned.poll_write(&mut cx, data) {
            Poll::Ready(Err(e)) => assert_eq!(e.kind(), ErrorKind::NotConnected),
            other => panic!("expected Poll::Ready(Err(NotConnected)), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_poll_flush_not_connected() {
        let mut transport = SerialTransport::new(
            "COM1".into(),
            115200,
            SerialDataBits::Eight,
            SerialParity::None,
            SerialStopBits::One,
            SerialFlowControl::None,
        true,
        true,
        );
        let pinned = Pin::new(&mut transport);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        match pinned.poll_flush(&mut cx) {
            Poll::Ready(Ok(())) => {}
            other => panic!("expected Poll::Ready(Ok(())), got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_poll_shutdown_not_connected() {
        let mut transport = SerialTransport::new(
            "COM1".into(),
            115200,
            SerialDataBits::Eight,
            SerialParity::None,
            SerialStopBits::One,
            SerialFlowControl::None,
        true,
        true,
        );
        let pinned = Pin::new(&mut transport);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        match pinned.poll_shutdown(&mut cx) {
            Poll::Ready(Ok(())) => {}
            other => panic!("expected Poll::Ready(Ok(())), got {:?}", other),
        }
    }

    // ── Transport trait method tests ──

    #[test]
    fn test_transport_name() {
        let transport = SerialTransport::new(
            "COM1".into(),
            115200,
            SerialDataBits::Eight,
            SerialParity::None,
            SerialStopBits::One,
            SerialFlowControl::None,
        true,
        true,
        );
        assert_eq!(transport.name(), "COM1");
    }

    #[test]
    fn test_transport_transport_type() {
        let transport = SerialTransport::new(
            "COM1".into(),
            115200,
            SerialDataBits::Eight,
            SerialParity::None,
            SerialStopBits::One,
            SerialFlowControl::None,
        true,
        true,
        );
        assert_eq!(transport.transport_type(), TransportType::Serial);
    }

    #[test]
    fn test_transport_is_connected() {
        let transport = SerialTransport::new(
            "COM1".into(),
            115200,
            SerialDataBits::Eight,
            SerialParity::None,
            SerialStopBits::One,
            SerialFlowControl::None,
        true,
        true,
        );
        assert!(!transport.is_connected());
    }
}
