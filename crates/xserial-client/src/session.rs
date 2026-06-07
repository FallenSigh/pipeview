use tokio::io::AsyncReadExt;
use tokio::select;
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use xserial_core::pipeline::{MultiPipeline, Pipeline};
use xserial_core::transport::Connection;

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

pub struct SessionHandle {
    pub id: SessionId,
    pub(crate) cmd_tx: mpsc::Sender<SessionCmd>,
    pub(crate) event_tx: broadcast::Sender<SessionEvent>,
    pub(crate) _task: Option<JoinHandle<()>>,
}

impl SessionHandle {
    pub fn id(&self) -> SessionId { self.id }

    pub async fn send(&self, data: Vec<u8>) -> Result<(), String> {
        self.cmd_tx.send(SessionCmd::Send(data)).await.map_err(|_| "session closed".into())
    }

    pub async fn close(&self) -> Result<(), String> {
        self.cmd_tx.send(SessionCmd::Close).await.map_err(|_| "session closed".into())
    }

    pub async fn reconfigure(&self, config: SessionConfig) -> Result<(), String> {
        self.cmd_tx.send(SessionCmd::Reconfigure(config)).await.map_err(|_| "session closed".into())
    }

    pub async fn read(&self, timeout_ms: u64) -> Option<DecodedEntry> {
        let mut rx = self.event_tx.subscribe();
        let deadline = std::time::Duration::from_millis(timeout_ms);
        tokio::time::timeout(deadline, async {
            loop {
                match rx.recv().await {
                    Ok(SessionEvent::Data(_, e)) => return Some(e),
                    Ok(SessionEvent::Error(_, _) | SessionEvent::Closed(_)) => return None,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => return None,
                    _ => continue,
                }
            }
        }).await.ok().flatten()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.event_tx.subscribe()
    }
}

impl Drop for SessionHandle {
    fn drop(&mut self) {
        let _ = self.cmd_tx.try_send(SessionCmd::Close);
        if let Some(t) = self._task.take() { t.abort(); }
    }
}

pub struct Session {
    id: SessionId,
    config: SessionConfig,
    conn: Option<Connection>,
    pipeline: MultiPipeline,
    history: RingBuffer<DecodedEntry>,
    cmd_rx: mpsc::Receiver<SessionCmd>,
    event_tx: broadcast::Sender<SessionEvent>,
}

impl Session {
    pub fn spawn(id: SessionId, config: SessionConfig) -> SessionHandle {
        let (cmd_tx, cmd_rx) = mpsc::channel(32);
        let (event_tx, _) = broadcast::channel(256);
        let mut session = Self {
            id, conn: None,
            pipeline: Self::build_pipeline(&config),
            history: RingBuffer::new(config.history_limit),
            config, cmd_rx, event_tx: event_tx.clone(),
        };
        let task = tokio::spawn(async move { session.run().await });
        SessionHandle { id, cmd_tx, event_tx, _task: Some(task) }
    }

    fn build_pipeline(config: &SessionConfig) -> MultiPipeline {
        let mut mp = MultiPipeline::new();
        for pc in &config.pipelines {
            mp.add(Pipeline::new(
                pc.name.clone(),
                pc.framer.clone().build(),
                pc.decoder.clone().build(),
            ));
        }
        mp
    }

    async fn run(&mut self) {
        info!(session_id = self.id, "session started");

        // Initial connect
        let conn = &mut self.conn;
        if let Err(e) = do_connect(conn, &self.config, &self.event_tx, self.id).await {
            let _ = self.event_tx.send(SessionEvent::Error(self.id, format!("connect failed: {}", e)));
            let _ = self.event_tx.send(SessionEvent::Closed(self.id));
            return;
        }

        loop {
            let cmd_rx = &mut self.cmd_rx;
            let conn = &mut self.conn;
            let pipeline = &mut self.pipeline;
            let history = &mut self.history;
            let event_tx = &self.event_tx;
            let id = self.id;

            select! {
                cmd = cmd_rx.recv() => {
                    match cmd {
                        Some(SessionCmd::Close) => {
                            do_disconnect(conn, pipeline, event_tx, id).await;
                            let _ = event_tx.send(SessionEvent::Closed(id));
                            break;
                        }
                        Some(SessionCmd::Send(data)) => {
                            if let Some(c) = conn {
                                use tokio::io::AsyncWriteExt;
                                if let Err(e) = c.write_all(&data).await {
                                    let _ = event_tx.send(SessionEvent::Error(id, format!("write: {}", e)));
                                }
                            }
                        }
                        Some(SessionCmd::Reconfigure(new_cfg)) => {
                            do_disconnect(conn, pipeline, event_tx, id).await;
                            self.config = new_cfg;
                            *pipeline = Self::build_pipeline(&self.config);
                            *history = RingBuffer::new(self.config.history_limit);
                            if let Err(e) = do_connect(conn, &self.config, event_tx, id).await {
                                let _ = event_tx.send(SessionEvent::Error(id, format!("reconnect: {}", e)));
                            }
                        }
                        None => break,
                    }
                }
                result = try_read(conn, pipeline) => {
                    match result {
                        Ok(entries) => {
                            for entry in entries {
                                history.push(entry.clone());
                                let _ = event_tx.send(SessionEvent::Data(id, entry));
                            }
                        }
                        Err(e) => {
                            error!(session_id = id, error = %e, "read error");
                            let _ = event_tx.send(SessionEvent::Error(id, e.to_string()));
                            do_disconnect(conn, pipeline, event_tx, id).await;
                            if self.config.auto_reconnect {
                                warn!(session_id = id, "auto-reconnecting");
                                tokio::time::sleep(std::time::Duration::from_secs(1)).await;
                                if let Err(e) = do_connect(conn, &self.config, event_tx, id).await {
                                    let _ = event_tx.send(SessionEvent::Error(id, format!("reconnect: {}", e)));
                                }
                            }
                        }
                    }
                }
            }
        }
        info!(session_id = self.id, "session ended");
    }
}

async fn try_read(
    conn: &mut Option<Connection>,
    pipeline: &mut MultiPipeline,
) -> Result<Vec<DecodedEntry>, std::io::Error> {
    let c = match conn.as_mut() { Some(c) => c, None => return Ok(Vec::new()) };
    let mut buf = [0u8; 4096];
    match c.read(&mut buf).await {
        Ok(0) => Ok(Vec::new()),
        Ok(n) => {
            Ok(pipeline.feed(&buf[..n])
                .into_iter()
                .map(|r| DecodedEntry { pipeline_name: r.pipeline_name, data: r.data })
                .collect())
        }
        Err(e) => Err(e),
    }
}

async fn do_connect(
    conn: &mut Option<Connection>,
    config: &SessionConfig,
    event_tx: &broadcast::Sender<SessionEvent>,
    id: SessionId,
) -> Result<(), std::io::Error> {
    let mut c = Connection::new(config.transport.clone());
    c.connect().await.map_err(|e| std::io::Error::new(std::io::ErrorKind::NotConnected, format!("{}", e)))?;
    *conn = Some(c);
    let _ = event_tx.send(SessionEvent::Connected(id));
    Ok(())
}

async fn do_disconnect(
    conn: &mut Option<Connection>,
    pipeline: &mut MultiPipeline,
    event_tx: &broadcast::Sender<SessionEvent>,
    id: SessionId,
) {
    if let Some(mut c) = conn.take() {
        let _ = c.disconnect().await;
        pipeline.reset();
        let _ = event_tx.send(SessionEvent::Disconnected(id));
    }
}
