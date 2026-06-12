use std::collections::HashMap;
use std::sync::{
    Arc, RwLock,
    atomic::{AtomicU64, Ordering},
};

use tokio::sync::broadcast;
use tracing::info;

use crate::config::SessionConfig;
use crate::session::{Session, SessionEvent, SessionHandle, SessionId};

#[derive(Clone)]
pub struct SessionManager {
    inner: Arc<SessionManagerInner>,
}

struct SessionManagerInner {
    sessions: RwLock<HashMap<SessionId, SessionHandle>>,
    event_tx: broadcast::Sender<SessionEvent>,
    next_session_id: AtomicU64,
}

impl SessionManager {
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(256);
        Self {
            inner: Arc::new(SessionManagerInner {
                sessions: RwLock::new(HashMap::new()),
                event_tx,
                next_session_id: AtomicU64::new(1),
            }),
        }
    }

    pub fn create(&self, config: SessionConfig) -> SessionHandle {
        let id = self.inner.next_session_id.fetch_add(1, Ordering::SeqCst);

        let pipeline_names: Vec<&str> = config.pipelines.iter().map(|p| p.name.as_str()).collect();
        info!(
            session_id = id,
            transport = ?config.transport,
            pipelines = ?pipeline_names,
            history_limit = config.history_limit,
            auto_reconnect = config.auto_reconnect,
            "session created"
        );

        let handle = Session::spawn(id, config);
        let stored = handle.clone();
        let mut session_events = handle.subscribe();
        let relay = self.inner.event_tx.clone();

        tokio::spawn(async move {
            loop {
                match session_events.recv().await {
                    Ok(event) => {
                        let _ = relay.send(event);
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        self.inner
            .sessions
            .write()
            .expect("session store poisoned")
            .insert(id, stored);

        handle
    }

    pub fn remove(&self, id: SessionId) {
        let removed = self
            .inner
            .sessions
            .write()
            .expect("session store poisoned")
            .remove(&id);

        if let Some(handle) = removed {
            info!(session_id = id, "session removed");
            tokio::spawn(async move {
                let _ = handle.close().await;
                handle.join().await;
            });
        }
    }

    pub fn get(&self, id: SessionId) -> Option<SessionHandle> {
        self.inner
            .sessions
            .read()
            .expect("session store poisoned")
            .get(&id)
            .cloned()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<SessionEvent> {
        self.inner.event_tx.subscribe()
    }

    pub fn list_ids(&self) -> Vec<SessionId> {
        self.inner
            .sessions
            .read()
            .expect("session store poisoned")
            .keys()
            .copied()
            .collect()
    }

    pub fn count(&self) -> usize {
        self.inner
            .sessions
            .read()
            .expect("session store poisoned")
            .len()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_keeps_monotonic_ids_after_remove() {
        let manager = SessionManager::new();
        let first = manager.create(SessionConfig::default());
        assert_eq!(first.id(), 1);
        manager.remove(first.id());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let second = manager.create(SessionConfig::default());
        assert_eq!(second.id(), 2);
    }

    #[tokio::test]
    async fn create_multiple_sessions() {
        let manager = SessionManager::new();
        let h1 = manager.create(SessionConfig::default());
        let h2 = manager.create(SessionConfig::default());
        let h3 = manager.create(SessionConfig::default());
        assert_eq!(h1.id(), 1);
        assert_eq!(h2.id(), 2);
        assert_eq!(h3.id(), 3);
    }

    #[tokio::test]
    async fn list_ids_returns_all() {
        let manager = SessionManager::new();
        manager.create(SessionConfig::default());
        manager.create(SessionConfig::default());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let ids = manager.list_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&1));
        assert!(ids.contains(&2));
    }

    #[tokio::test]
    async fn get_returns_correct_session() {
        let manager = SessionManager::new();
        let h = manager.create(SessionConfig::default());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let found = manager.get(h.id());
        assert!(found.is_some());
        assert_eq!(found.unwrap().id(), h.id());
    }

    #[tokio::test]
    async fn get_returns_none_for_missing() {
        let manager = SessionManager::new();
        assert!(manager.get(999).is_none());
    }

    #[tokio::test]
    async fn remove_decrements_count() {
        let manager = SessionManager::new();
        let h = manager.create(SessionConfig::default());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        manager.remove(h.id());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(manager.get(h.id()).is_none());
    }

    #[tokio::test]
    async fn subscribe_receives_events() {
        let manager = SessionManager::new();
        let mut rx = manager.subscribe();
        manager.create(SessionConfig::default());
        let event = tokio::time::timeout(std::time::Duration::from_secs(10), rx.recv()).await;
        if let Ok(Ok(event)) = event {
            assert!(matches!(
                event,
                SessionEvent::Connected(_) | SessionEvent::Error(_, _)
            ));
        }
    }

    #[tokio::test]
    async fn count_starts_at_zero() {
        let manager = SessionManager::new();
        assert_eq!(manager.count(), 0);
        assert!(manager.list_ids().is_empty());
    }

    #[tokio::test]
    async fn default_creates_empty_manager() {
        let manager = SessionManager::default();
        assert_eq!(manager.count(), 0);
    }
}
