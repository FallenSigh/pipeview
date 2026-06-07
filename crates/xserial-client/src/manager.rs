use std::collections::HashMap;
use crate::config::SessionConfig;
use crate::session::{Session, SessionEvent, SessionHandle, SessionId};

pub struct SessionManager {
    sessions: HashMap<SessionId, SessionHandle>,
    event_tx: tokio::sync::broadcast::Sender<SessionEvent>,
}

impl SessionManager {
    pub fn new() -> Self {
        let (event_tx, _) = tokio::sync::broadcast::channel(256);
        Self { sessions: HashMap::new(), event_tx }
    }

    pub fn create(&mut self, config: SessionConfig) -> SessionHandle {
        let id = self.sessions.len() as u64 + 1;
        let handle = Session::spawn(id, config);
        let event_relay = self.event_tx.clone();
        let mut event_rx = handle.subscribe();
        tokio::spawn(async move {
            while let Ok(event) = event_rx.recv().await { let _ = event_relay.send(event); }
        });
        self.sessions.insert(id, SessionHandle {
            id: handle.id, cmd_tx: handle.cmd_tx.clone(), event_tx: handle.event_tx.clone(), _task: None,
        });
        handle
    }

    pub fn remove(&mut self, id: SessionId) { self.sessions.remove(&id); }
    pub fn get(&self, id: SessionId) -> Option<&SessionHandle> { self.sessions.get(&id) }
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<SessionEvent> { self.event_tx.subscribe() }
    pub fn list_ids(&self) -> Vec<SessionId> { self.sessions.keys().copied().collect() }
    pub fn count(&self) -> usize { self.sessions.len() }
}

impl Default for SessionManager {
    fn default() -> Self { Self::new() }
}
