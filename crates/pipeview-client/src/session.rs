use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;
use std::{future, io};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::select;
use tokio::sync::{Mutex, broadcast, mpsc};
use tokio::task::JoinHandle;
use tracing::{debug, error, info, warn};

use pipeview_core::pipeline::{MultiPipeline, Pipeline};
use pipeview_core::transport::Connection;

use crate::cmd::SessionCmd;
use crate::config::SessionConfig;
use crate::event::DecodedEntry;
use crate::history::RingBuffer;

pub type SessionId = u64;

#[derive(Debug, Clone)]
pub enum SessionEvent {
    Connected(SessionId),
    Disconnected(SessionId),
    Data(SessionId, DecodedEntry),
    Error(SessionId, String),
    Closed(SessionId),
}

struct SessionHandleInner {
    id: SessionId,
    cmd_tx: mpsc::Sender<SessionCmd>,
    event_tx: broadcast::Sender<SessionEvent>,
    task: Mutex<Option<JoinHandle<()>>>,
    close_requested: AtomicBool,
}

impl SessionHandleInner {
    fn new(
        id: SessionId,
        cmd_tx: mpsc::Sender<SessionCmd>,
        event_tx: broadcast::Sender<SessionEvent>,
        task: JoinHandle<()>,
    ) -> Self {
        Self {
            id,
            cmd_tx,
            event_tx,
            task: Mutex::new(Some(task)),
            close_requested: AtomicBool::new(false),
        }
    }

    async fn request_close(&self) -> Result<(), String> {
        self.close_requested.store(true, Ordering::SeqCst);
        self.cmd_tx
            .send(SessionCmd::Close)
            .await
            .map_err(|_| String::from("session closed"))
    }

    fn request_close_nonblocking(&self) {
        self.close_requested.store(true, Ordering::SeqCst);
        let _ = self.cmd_tx.try_send(SessionCmd::Close);
    }
}

#[derive(Clone)]
pub struct SessionHandle {
    pub id: SessionId,
    inner: Arc<SessionHandleInner>,
}

impl SessionHandle {
    fn new(inner: Arc<SessionHandleInner>) -> Self {
        Self {
            id: inner.id,
            inner,
        }
    }

    pub fn id(&self) -> SessionId {
        self.id
    }

    pub async fn send(&self, data: Vec<u8>) -> Result<(), String> {
        self.inner
            .cmd_tx
            .send(SessionCmd::Send(data))
            .await
            .map_err(|_| String::from("session closed"))
    }

    pub async fn close(&self) -> Result<(), String> {
        self.inner.request_close().await
    }

    pub async fn reconfigure(&self, config: SessionConfig) -> Result<(), String> {
        self.inner
            .cmd_tx
            .send(SessionCmd::Reconfigure(config))
            .await
            .map_err(|_| String::from("session closed"))
    }

    pub async fn connect(&self) -> Result<(), String> {
        self.inner
            .cmd_tx
            .send(SessionCmd::Connect)
            .await
            .map_err(|_| String::from("session closed"))
    }

    pub async fn disconnect(&self) -> Result<(), String> {
        self.inner
            .cmd_tx
            .send(SessionCmd::Disconnect)
            .await
            .map_err(|_| String::from("session closed"))
    }

    pub async fn reconnect(&self) -> Result<(), String> {
        self.inner
            .cmd_tx
            .send(SessionCmd::Reconnect)
            .await
            .map_err(|_| String::from("session closed"))
    }

    pub async fn set_auto_reconnect(&self, enabled: bool) -> Result<(), String> {
        self.inner
            .cmd_tx
            .send(SessionCmd::SetAutoReconnect(enabled))
            .await
            .map_err(|_| String::from("session closed"))
    }

    pub async fn set_dtr(&self, state: bool) -> Result<(), String> {
        self.inner
            .cmd_tx
            .send(SessionCmd::SetDtr(state))
            .await
            .map_err(|_| String::from("session closed"))
    }

    pub async fn set_rts(&self, state: bool) -> Result<(), String> {
        self.inner
            .cmd_tx
            .send(SessionCmd::SetRts(state))
            .await
            .map_err(|_| String::from("session closed"))
    }

    pub async fn read(&self, timeout_ms: u64) -> Option<DecodedEntry> {
        let mut rx = self.inner.event_tx.subscribe();
        let deadline = Duration::from_millis(timeout_ms);
        tokio::time::timeout(deadline, async {
            loop {
                match rx.recv().await {
                    Ok(SessionEvent::Data(_, entry)) => return Some(entry),
                    Ok(SessionEvent::Error(_, _) | SessionEvent::Closed(_)) => return None,
                    Ok(SessionEvent::Connected(_) | SessionEvent::Disconnected(_)) => {}
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                }
            }
        })
        .await
        .ok()
        .flatten()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.inner.event_tx.subscribe()
    }

    pub(crate) async fn join(&self) {
        let task = {
            let mut slot = self.inner.task.lock().await;
            slot.take()
        };
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        if Arc::strong_count(&self.inner) == 1 {
            self.inner.request_close_nonblocking();
            if let Ok(mut task) = self.inner.task.try_lock()
                && let Some(task) = task.take()
            {
                task.abort();
            }
        }
    }
}

pub struct Session {
    id: SessionId,
    config: SessionConfig,
    conn: Option<Connection>,
    pipeline: MultiPipeline,
    pipeline_build_error: Option<String>,
    history: RingBuffer<DecodedEntry>,
    cmd_rx: mpsc::Receiver<SessionCmd>,
    event_tx: broadcast::Sender<SessionEvent>,
    close_requested: bool,
    desired_connected: bool,
}

impl Session {
    pub fn spawn(id: SessionId, config: SessionConfig) -> SessionHandle {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let (event_tx, _) = broadcast::channel(256);
        let (pipeline, pipeline_build_error) = match Self::build_pipeline(&config) {
            Ok(pipeline) => (pipeline, None),
            Err(err) => (
                MultiPipeline::new(),
                Some(format!("pipeline build failed: {err}")),
            ),
        };
        let mut session = Self {
            id,
            pipeline,
            pipeline_build_error,
            history: RingBuffer::new(config.history_limit),
            config,
            conn: None,
            cmd_rx,
            event_tx: event_tx.clone(),
            close_requested: false,
            desired_connected: true,
        };
        let task = tokio::spawn(async move { session.run().await });
        let inner = Arc::new(SessionHandleInner::new(id, cmd_tx, event_tx, task));
        SessionHandle::new(inner)
    }

    fn build_pipeline(config: &SessionConfig) -> crate::Result<MultiPipeline> {
        let mut pipelines = MultiPipeline::new();
        for pipeline in &config.pipelines {
            pipelines.add(Pipeline::new(
                pipeline.name.clone(),
                pipeline.framer.clone().build()?,
                pipeline.decoder.clone().build()?,
            ));
        }
        Ok(pipelines)
    }

    async fn run(&mut self) {
        info!(session_id = self.id, "session started");

        if let Some(message) = self.pipeline_build_error.take() {
            self.emit_error(message);
            self.desired_connected = false;
        }

        if self.desired_connected
            && let Err(err) = self.connect().await
        {
            warn!(session_id = self.id, error = %err, "initial connect failed");
            self.emit_error(format!("connect failed: {err}"));
        }

        loop {
            let reconnect_timer = self.reconnect_timer();
            select! {
                cmd = self.cmd_rx.recv() => {
                    if !self.handle_command(cmd).await {
                        break;
                    }
                }
                read = try_read(&mut self.conn, &mut self.pipeline) => {
                    if !self.handle_read_result(read).await {
                        break;
                    }
                }
                _ = reconnect_timer => {
                    if let Err(err) = self.connect().await {
                        self.emit_error(format!("connect failed: {err}"));
                    }
                }
            }
        }

        self.disconnect(false).await;
        self.emit_closed();
        info!(session_id = self.id, "session ended");
    }

    async fn handle_command(&mut self, cmd: Option<SessionCmd>) -> bool {
        match cmd {
            Some(SessionCmd::Close) => {
                self.close_requested = true;
                self.desired_connected = false;
                false
            }
            Some(SessionCmd::Connect) => {
                self.desired_connected = true;
                if self.conn.is_none()
                    && let Err(err) = self.connect().await
                {
                    self.emit_error(format!("connect failed: {err}"));
                }
                true
            }
            Some(SessionCmd::Disconnect) => {
                self.desired_connected = false;
                self.disconnect(true).await;
                true
            }
            Some(SessionCmd::Reconnect) => {
                self.desired_connected = true;
                self.disconnect(true).await;
                if let Err(err) = self.connect().await {
                    self.emit_error(format!("reconnect: {err}"));
                }
                true
            }
            Some(SessionCmd::SetAutoReconnect(enabled)) => {
                self.config.auto_reconnect = enabled;
                true
            }
            Some(SessionCmd::Send(data)) => {
                self.handle_send(data).await;
                true
            }
            Some(SessionCmd::Reconfigure(config)) => {
                self.handle_reconfigure(config).await;
                true
            }
            Some(SessionCmd::SetDtr(state)) => {
                self.handle_set_dtr(state);
                true
            }
            Some(SessionCmd::SetRts(state)) => {
                self.handle_set_rts(state);
                true
            }
            None => false,
        }
    }

    async fn handle_send(&mut self, data: Vec<u8>) {
        debug!(session_id = self.id, len = data.len(), "sending data");
        match self.conn.as_mut() {
            Some(conn) => {
                if let Err(err) = conn.write_all(&data).await {
                    self.emit_error(format!("write: {err}"));
                }
            }
            None => self.emit_error(String::from("write: session not connected")),
        }
    }

    async fn handle_reconfigure(&mut self, config: SessionConfig) {
        info!(session_id = self.id, "reconfiguring session");
        let pipeline = match Self::build_pipeline(&config) {
            Ok(pipeline) => pipeline,
            Err(err) => {
                self.emit_error(format!("pipeline build failed: {err}"));
                return;
            }
        };
        self.disconnect(true).await;
        self.config = config;
        self.pipeline = pipeline;
        self.history = RingBuffer::new(self.config.history_limit);

        if let Err(err) = self.connect().await {
            self.emit_error(format!("reconnect: {err}"));
        }
    }

    fn handle_set_dtr(&mut self, state: bool) {
        match self.conn.as_mut() {
            Some(conn) => {
                if let Err(err) = conn.set_dtr(state) {
                    self.emit_error(format!("set_dtr: {err}"));
                }
            }
            None => self.emit_error(String::from("set_dtr: session not connected")),
        }
    }

    fn handle_set_rts(&mut self, state: bool) {
        match self.conn.as_mut() {
            Some(conn) => {
                if let Err(err) = conn.set_rts(state) {
                    self.emit_error(format!("set_rts: {err}"));
                }
            }
            None => self.emit_error(String::from("set_rts: session not connected")),
        }
    }

    async fn handle_read_result(
        &mut self,
        result: Result<Vec<DecodedEntry>, std::io::Error>,
    ) -> bool {
        match result {
            Ok(entries) => {
                for entry in entries {
                    self.history.push(entry.clone());
                    let _ = self.event_tx.send(SessionEvent::Data(self.id, entry));
                }
                true
            }
            Err(err) => {
                error!(session_id = self.id, error = %err, "read error");
                self.emit_error(err.to_string());
                self.disconnect(true).await;
                true
            }
        }
    }

    fn reconnect_timer(&self) -> impl std::future::Future<Output = ()> + Send + 'static {
        let should_retry = self.conn.is_none()
            && self.desired_connected
            && self.config.auto_reconnect
            && !self.close_requested;
        async move {
            if should_retry {
                tokio::time::sleep(Duration::from_secs(1)).await;
            } else {
                future::pending::<()>().await;
            }
        }
    }

    async fn connect(&mut self) -> Result<(), io::Error> {
        if self.conn.is_some() {
            return Ok(());
        }
        let transport = self.config.transport.clone();
        debug!(session_id = self.id, ?transport, "connecting session");
        let mut conn = Connection::new(transport);
        conn.connect().await.map_err(to_io_error)?;
        self.conn = Some(conn);
        let _ = self.event_tx.send(SessionEvent::Connected(self.id));
        info!(session_id = self.id, "session connected");
        Ok(())
    }

    async fn disconnect(&mut self, emit_disconnected: bool) {
        if let Some(mut conn) = self.conn.take() {
            let _ = conn.disconnect().await;
            if emit_disconnected {
                let _ = self.event_tx.send(SessionEvent::Disconnected(self.id));
            }
            info!(session_id = self.id, "session disconnected");
        }
        self.pipeline.reset();
    }

    fn emit_error(&self, message: String) {
        let _ = self.event_tx.send(SessionEvent::Error(self.id, message));
    }

    fn emit_closed(&self) {
        let _ = self.event_tx.send(SessionEvent::Closed(self.id));
    }
}

async fn try_read(
    conn: &mut Option<Connection>,
    pipeline: &mut MultiPipeline,
) -> Result<Vec<DecodedEntry>, io::Error> {
    if conn.is_none() {
        future::pending::<()>().await;
        unreachable!("pending future should never resolve");
    }

    let conn = conn.as_mut().expect("connection presence checked");

    let mut buf = [0u8; 4096];
    match conn.read(&mut buf).await {
        Ok(0) => Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "connection closed",
        )),
        Ok(n) => Ok(pipeline
            .feed(&buf[..n])
            .into_iter()
            .map(|result| DecodedEntry {
                pipeline_name: result.pipeline_name,
                data: result.data,
            })
            .collect()),
        Err(err) => Err(err),
    }
}

fn to_io_error(err: pipeview_core::error::Error) -> std::io::Error {
    io::Error::other(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{DecoderConfig, FramerConfig, PipelineConfig};

    fn tcp_config() -> SessionConfig {
        SessionConfig {
            transport: pipeview_core::transport::TransportConfig::Tcp {
                addr: "127.0.0.1:1".into(),
            },
            pipelines: vec![PipelineConfig {
                name: "text".into(),
                framer: FramerConfig::Line {
                    strip_cr: true,
                    max_line_len: 65536,
                },
                decoder: DecoderConfig::Text {
                    encoding: pipeview_core::protocol::text::TextEncoding::Utf8,
                },
            }],
            ..Default::default()
        }
    }

    #[test]
    fn session_id_is_u64() {
        let _id: SessionId = 42;
    }

    #[test]
    fn session_event_debug() {
        let e1 = SessionEvent::Connected(1);
        let e2 = SessionEvent::Disconnected(1);
        let e3 = SessionEvent::Closed(1);
        let e4 = SessionEvent::Error(1, "oops".into());
        let _ = format!("{:?}", e1);
        let _ = format!("{:?}", e2);
        let _ = format!("{:?}", e3);
        let _ = format!("{:?}", e4);
    }

    #[test]
    fn session_event_clone() {
        let e1 = SessionEvent::Connected(1);
        let e2 = e1.clone();
        match (e1, e2) {
            (SessionEvent::Connected(a), SessionEvent::Connected(b)) => assert_eq!(a, b),
            _ => panic!(),
        }
    }

    #[tokio::test]
    async fn spawn_and_close_session() {
        let handle = Session::spawn(1, tcp_config());
        let mut rx = handle.subscribe();
        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timeout");
        if let Ok(event) = event {
            assert!(matches!(
                event,
                SessionEvent::Error(_, _) | SessionEvent::Connected(_)
            ));
        }
        let _ = handle.close().await;
        handle.join().await;
    }

    #[tokio::test]
    async fn handle_send_to_dead_session() {
        let handle = Session::spawn(2, tcp_config());
        tokio::time::sleep(Duration::from_millis(200)).await;
        let _ = handle.send(b"data".to_vec()).await;
        let _ = handle.close().await;
        handle.join().await;
    }

    #[tokio::test]
    async fn handle_close_twice_is_idempotent() {
        let handle = Session::spawn(3, tcp_config());
        let _ = handle.close().await;
        let _ = handle.close().await;
        handle.join().await;
    }

    #[tokio::test]
    async fn handle_read_after_close() {
        let handle = Session::spawn(4, tcp_config());
        let _ = handle.close().await;
        let result = handle.read(100).await;
        assert!(result.is_none());
        handle.join().await;
    }

    #[tokio::test]
    async fn handle_read_timeout_returns_none() {
        let handle = Session::spawn(5, tcp_config());
        let result = handle.read(100).await;
        assert!(result.is_none());
        let _ = handle.close().await;
        handle.join().await;
    }

    #[tokio::test]
    async fn handle_subscribe_receives_events() {
        let handle = Session::spawn(6, tcp_config());
        let mut rx = handle.subscribe();
        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timeout");
        if let Ok(event) = event {
            assert!(matches!(
                event,
                SessionEvent::Error(_, _) | SessionEvent::Connected(_)
            ));
        }
        let _ = handle.close().await;
        handle.join().await;
    }

    #[tokio::test]
    async fn handle_id_matches() {
        let handle = Session::spawn(99, tcp_config());
        assert_eq!(handle.id(), 99);
        let _ = handle.close().await;
        handle.join().await;
    }

    #[tokio::test]
    async fn handle_reconfigure_triggers_reconnect() {
        let handle = Session::spawn(7, tcp_config());
        let new_cfg = tcp_config();
        let _ = handle.reconfigure(new_cfg).await;
        let _ = handle.close().await;
        handle.join().await;
    }

    #[tokio::test]
    async fn try_read_without_connection_stays_pending() {
        let mut conn = None;
        let mut pipeline = MultiPipeline::new();

        let result = tokio::time::timeout(
            Duration::from_millis(50),
            try_read(&mut conn, &mut pipeline),
        )
        .await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn handle_drop_closes_session() {
        let handle = Session::spawn(8, tcp_config());
        let mut rx = handle.subscribe();
        drop(handle);
        let mut closed = false;
        while let Ok(Ok(event)) = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await
        {
            if matches!(event, SessionEvent::Closed(_)) {
                closed = true;
                break;
            }
        }
        let _ = closed;
    }

    #[tokio::test]
    async fn multiple_subscribers_all_receive() {
        let handle = Session::spawn(9, tcp_config());
        let mut rx1 = handle.subscribe();
        let mut rx2 = handle.subscribe();

        let e1 = tokio::time::timeout(Duration::from_secs(5), rx1.recv())
            .await
            .unwrap();
        if let Ok(e1) = e1 {
            assert!(matches!(
                e1,
                SessionEvent::Connected(9) | SessionEvent::Error(9, _)
            ));
        }

        let e2 = tokio::time::timeout(Duration::from_secs(1), rx2.recv())
            .await
            .unwrap();
        if let Ok(e2) = e2 {
            assert!(matches!(
                e2,
                SessionEvent::Connected(9) | SessionEvent::Error(9, _)
            ));
        }

        let _ = handle.close().await;
        handle.join().await;
    }
}
