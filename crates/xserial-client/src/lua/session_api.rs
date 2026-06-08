use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;

use mlua::{Function, Lua, LuaSerdeExt, RegistryKey, UserData, UserDataMethods, Value, Variadic};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use xserial_core::protocol::DecodedData;

use crate::config::SessionConfig;
use crate::lua::LuaRuntime;
use crate::session::{SessionEvent, SessionHandle};

pub struct LuaSessionHandle {
    handle: SessionHandle,
    runtime: LuaRuntime,
    callbacks: Arc<CallbackState>,
}

pub struct CallbackState {
    queue_tx: mpsc::UnboundedSender<QueuedEvent>,
    queue_rx: Mutex<mpsc::UnboundedReceiver<QueuedEvent>>,
    subscriptions: Mutex<HashMap<u64, Subscription>>,
    next_subscription_id: AtomicU64,
    relay_task: Mutex<Option<JoinHandle<()>>>,
}

struct Subscription {
    callback: Arc<RegistryKey>,
    filters: Vec<String>,
}

#[derive(Clone)]
enum QueuedEvent {
    Data {
        pipeline_name: String,
        data: QueuedData,
    },
}

#[derive(Clone)]
enum QueuedData {
    Text(String),
    Hex(String),
    Binary(Vec<u8>),
    Plot {
        channels: Vec<Vec<f64>>,
        sample_count: usize,
    },
}

impl LuaSessionHandle {
    pub fn new(handle: SessionHandle, runtime: LuaRuntime) -> Self {
        let callbacks = Arc::new(CallbackState::new(handle.clone()));
        runtime.register_callback_state(handle.id(), &callbacks);
        Self {
            handle,
            runtime,
            callbacks,
        }
    }
}

impl Drop for LuaSessionHandle {
    fn drop(&mut self) {
        self.runtime.unregister_callback_state(self.handle.id());
    }
}

impl CallbackState {
    pub fn new(handle: SessionHandle) -> Self {
        let (queue_tx, queue_rx) = mpsc::unbounded_channel();
        let state = Self {
            queue_tx,
            queue_rx: Mutex::new(queue_rx),
            subscriptions: Mutex::new(HashMap::new()),
            next_subscription_id: AtomicU64::new(1),
            relay_task: Mutex::new(None),
        };
        state.ensure_relay(handle);
        state
    }

    fn ensure_relay(&self, handle: SessionHandle) {
        let mut relay_task = self
            .relay_task
            .lock()
            .expect("lua relay task mutex poisoned");
        if relay_task.is_some() {
            return;
        }

        let tx = self.queue_tx.clone();
        let mut rx = handle.subscribe();
        *relay_task = Some(tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(SessionEvent::Data(_, entry)) => {
                        let queued = match entry.data {
                            DecodedData::Text(text) => QueuedEvent::Data {
                                pipeline_name: entry.pipeline_name,
                                data: QueuedData::Text(text),
                            },
                            DecodedData::Hex(hex) => QueuedEvent::Data {
                                pipeline_name: entry.pipeline_name,
                                data: QueuedData::Hex(hex),
                            },
                            DecodedData::Binary(bytes) => QueuedEvent::Data {
                                pipeline_name: entry.pipeline_name,
                                data: QueuedData::Binary(bytes),
                            },
                            DecodedData::Plot(frame) => QueuedEvent::Data {
                                pipeline_name: entry.pipeline_name,
                                data: QueuedData::Plot {
                                    sample_count: frame.sample_count(),
                                    channels: frame.channels,
                                },
                            },
                        };
                        if tx.send(queued).is_err() {
                            break;
                        }
                    }
                    Ok(SessionEvent::Closed(_)) => break,
                    Ok(_) => {}
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                }
            }
        }));
    }

    pub fn add_subscription(&self, callback: RegistryKey, filters: Vec<String>) -> u64 {
        let id = self.next_subscription_id.fetch_add(1, Ordering::SeqCst);
        self.subscriptions
            .lock()
            .expect("lua callback subscriptions mutex poisoned")
            .insert(
                id,
                Subscription {
                    callback: Arc::new(callback),
                    filters,
                },
            );
        id
    }

    pub fn remove_subscription(&self, lua: &Lua, id: u64) -> mlua::Result<bool> {
        let removed = self
            .subscriptions
            .lock()
            .expect("lua callback subscriptions mutex poisoned")
            .remove(&id);
        if let Some(subscription) = removed {
            if let Ok(key) = Arc::try_unwrap(subscription.callback) {
                lua.remove_registry_value(key)?;
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    pub async fn pump(&self, lua: &Lua, limit: Option<usize>) -> mlua::Result<usize> {
        let max = limit.unwrap_or(usize::MAX);
        let mut drained = Vec::new();
        {
            let mut rx = self
                .queue_rx
                .lock()
                .expect("lua callback queue mutex poisoned");
            while drained.len() < max {
                match rx.try_recv() {
                    Ok(event) => drained.push(event),
                    Err(tokio::sync::mpsc::error::TryRecvError::Empty) => break,
                    Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => break,
                }
            }
        }

        let mut called = 0;
        for event in drained {
            let callbacks = self.matching_callbacks(&event);
            for callback in callbacks {
                let function: Function = lua.registry_value(&callback)?;
                match &event {
                    QueuedEvent::Data {
                        pipeline_name,
                        data,
                    } => {
                        let payload = queued_data_to_value(lua, data)?;
                        function
                            .call_async::<()>((pipeline_name.clone(), payload))
                            .await?;
                        called += 1;
                    }
                }
            }
        }

        Ok(called)
    }

    fn matching_callbacks(&self, event: &QueuedEvent) -> Vec<Arc<RegistryKey>> {
        let subscriptions = self
            .subscriptions
            .lock()
            .expect("lua callback subscriptions mutex poisoned");
        subscriptions
            .values()
            .filter(|subscription| match event {
                QueuedEvent::Data { pipeline_name, .. } => {
                    subscription.filters.is_empty()
                        || subscription.filters.iter().any(|f| f == pipeline_name)
                }
            })
            .map(|subscription| Arc::clone(&subscription.callback))
            .collect()
    }
}

impl Drop for CallbackState {
    fn drop(&mut self) {
        if let Ok(mut relay_task) = self.relay_task.lock() {
            if let Some(task) = relay_task.take() {
                task.abort();
            }
        }
    }
}

impl UserData for LuaSessionHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("send", |_, this, data: mlua::String| async move {
            this.handle
                .send(data.as_bytes().to_vec())
                .await
                .map_err(mlua::Error::RuntimeError)
        });

        methods.add_async_method("read", |lua, this, timeout_ms: Option<u64>| async move {
            let ms = timeout_ms.unwrap_or(1000);
            match this.handle.read(ms).await {
                Some(entry) => Ok(Some(decoded_entry_to_table(
                    &lua,
                    entry.pipeline_name,
                    entry.data,
                )?)),
                None => Ok(None),
            }
        });

        methods.add_async_method(
            "next_event",
            |lua, this, timeout_ms: Option<u64>| async move {
                let mut rx = this.handle.subscribe();
                let timeout = Duration::from_millis(timeout_ms.unwrap_or(1000));
                let next = tokio::time::timeout(timeout, async move {
                    loop {
                        match rx.recv().await {
                            Ok(event) => return Some(event),
                            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                            Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                        }
                    }
                })
                .await
                .ok()
                .flatten();

                match next {
                    Some(event) => Ok(Some(session_event_to_table(&lua, event)?)),
                    None => Ok(None),
                }
            },
        );

        methods.add_async_method("close", |_, this, (): ()| async move {
            this.handle.close().await.map_err(mlua::Error::RuntimeError)
        });

        methods.add_async_method("reconfigure", |lua, this, config: mlua::Table| async move {
            let cfg: SessionConfig = lua.from_value(Value::Table(config))?;
            this.handle
                .reconfigure(cfg)
                .await
                .map_err(mlua::Error::RuntimeError)
        });

        methods.add_async_method(
            "on_data",
            |lua, this, (callback, pipelines): (Function, Variadic<String>)| async move {
                let filters: Vec<String> = pipelines.into_iter().collect();
                let key = lua.create_registry_value(callback)?;
                Ok(this.callbacks.add_subscription(key, filters))
            },
        );

        methods.add_method("off", |lua, this, token: u64| {
            this.callbacks.remove_subscription(lua, token)
        });

        methods.add_async_method(
            "pump_events",
            |lua, this, limit: Option<usize>| async move { this.callbacks.pump(&lua, limit).await },
        );
    }
}

fn decoded_entry_to_table(
    lua: &Lua,
    pipeline_name: String,
    data: DecodedData,
) -> mlua::Result<mlua::Table> {
    let table = lua.create_table()?;
    table.set("type", "data")?;
    table.set("pipeline", pipeline_name)?;
    match data {
        DecodedData::Text(text) => {
            table.set("kind", "text")?;
            table.set("data", text)?;
        }
        DecodedData::Hex(hex) => {
            table.set("kind", "hex")?;
            table.set("data", hex)?;
        }
        DecodedData::Binary(bytes) => {
            table.set("kind", "binary")?;
            table.set("data", lua.create_string(&bytes)?)?;
        }
        DecodedData::Plot(frame) => {
            table.set("kind", "plot")?;
            table.set("channels", channels_to_table(lua, &frame.channels)?)?;
            table.set("sample_count", frame.sample_count())?;
        }
    }
    Ok(table)
}

fn session_event_to_table(lua: &Lua, event: SessionEvent) -> mlua::Result<mlua::Table> {
    match event {
        SessionEvent::Connected(session_id) => {
            let table = lua.create_table()?;
            table.set("type", "connected")?;
            table.set("session_id", session_id)?;
            Ok(table)
        }
        SessionEvent::Disconnected(session_id) => {
            let table = lua.create_table()?;
            table.set("type", "disconnected")?;
            table.set("session_id", session_id)?;
            Ok(table)
        }
        SessionEvent::Closed(session_id) => {
            let table = lua.create_table()?;
            table.set("type", "closed")?;
            table.set("session_id", session_id)?;
            Ok(table)
        }
        SessionEvent::Error(session_id, error) => {
            let table = lua.create_table()?;
            table.set("type", "error")?;
            table.set("session_id", session_id)?;
            table.set("error", error)?;
            Ok(table)
        }
        SessionEvent::Data(session_id, entry) => {
            let table = decoded_entry_to_table(lua, entry.pipeline_name, entry.data)?;
            table.set("session_id", session_id)?;
            Ok(table)
        }
    }
}

fn queued_data_to_value(lua: &Lua, data: &QueuedData) -> mlua::Result<Value> {
    match data {
        QueuedData::Text(text) => Ok(Value::String(lua.create_string(text)?)),
        QueuedData::Hex(hex) => Ok(Value::String(lua.create_string(hex)?)),
        QueuedData::Binary(bytes) => Ok(Value::String(lua.create_string(bytes)?)),
        QueuedData::Plot {
            channels,
            sample_count,
        } => {
            let table = lua.create_table()?;
            table.set("kind", "plot")?;
            table.set("channels", channels_to_table(lua, channels)?)?;
            table.set("sample_count", *sample_count)?;
            Ok(Value::Table(table))
        }
    }
}

fn channels_to_table(lua: &Lua, channels: &[Vec<f64>]) -> mlua::Result<mlua::Table> {
    let table = lua.create_table()?;
    for (index, channel) in channels.iter().enumerate() {
        table.set(
            index + 1,
            lua.create_sequence_from(channel.iter().copied())?,
        )?;
    }
    Ok(table)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::DecodedEntry;
    use crate::session::SessionId;

    fn test_callback_state() -> Arc<CallbackState> {
        let (queue_tx, queue_rx) = mpsc::unbounded_channel();
        Arc::new(CallbackState {
            queue_tx,
            queue_rx: Mutex::new(queue_rx),
            subscriptions: Mutex::new(HashMap::new()),
            next_subscription_id: AtomicU64::new(1),
            relay_task: Mutex::new(None),
        })
    }

    fn push_data_event(state: &CallbackState, pipeline_name: &str, data: QueuedData) {
        state
            .queue_tx
            .send(QueuedEvent::Data {
                pipeline_name: pipeline_name.to_string(),
                data,
            })
            .unwrap();
    }

    #[tokio::test]
    async fn callback_pump_invokes_matching_subscribers() {
        let lua = Lua::new();
        let received = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        lua.set_app_data(received.clone());

        let callback = lua
            .create_function(|lua, (name, data): (String, String)| {
                let store = lua
                    .app_data_ref::<Arc<Mutex<Vec<(String, String)>>>>()
                    .unwrap();
                store.lock().unwrap().push((name, data));
                Ok(())
            })
            .unwrap();

        let state = test_callback_state();
        let token = state.add_subscription(lua.create_registry_value(callback).unwrap(), vec![]);
        assert_eq!(token, 1);

        push_data_event(&state, "text", QueuedData::Text("hello".into()));
        let called = state.pump(&lua, None).await.unwrap();

        assert_eq!(called, 1);
        assert_eq!(
            received.lock().unwrap().as_slice(),
            &[("text".to_string(), "hello".to_string())]
        );
    }

    #[tokio::test]
    async fn callback_pump_respects_pipeline_filters_and_off() {
        let lua = Lua::new();
        let calls = Arc::new(Mutex::new(0usize));
        lua.set_app_data(calls.clone());

        let callback = lua
            .create_function(|lua, (_name, _data): (String, String)| {
                let calls = lua.app_data_ref::<Arc<Mutex<usize>>>().unwrap();
                *calls.lock().unwrap() += 1;
                Ok(())
            })
            .unwrap();

        let state = test_callback_state();
        let token = state.add_subscription(
            lua.create_registry_value(callback).unwrap(),
            vec!["hex".into()],
        );

        push_data_event(&state, "text", QueuedData::Text("ignored".into()));
        assert_eq!(state.pump(&lua, None).await.unwrap(), 0);
        assert_eq!(*calls.lock().unwrap(), 0);

        assert!(state.remove_subscription(&lua, token).unwrap());
        push_data_event(&state, "hex", QueuedData::Hex("41".into()));
        assert_eq!(state.pump(&lua, None).await.unwrap(), 0);
        assert_eq!(*calls.lock().unwrap(), 0);
    }

    #[test]
    fn decoded_entry_to_table_has_expected_shape() {
        let lua = Lua::new();
        let table =
            decoded_entry_to_table(&lua, "text".into(), DecodedData::Text("payload".into()))
                .unwrap();

        assert_eq!(table.get::<String>("type").unwrap(), "data");
        assert_eq!(table.get::<String>("pipeline").unwrap(), "text");
        assert_eq!(table.get::<String>("kind").unwrap(), "text");
        assert_eq!(table.get::<String>("data").unwrap(), "payload");
    }

    #[test]
    fn session_event_to_table_maps_variants() {
        let lua = Lua::new();

        let error_table =
            session_event_to_table(&lua, SessionEvent::Error(7 as SessionId, "boom".into()))
                .unwrap();
        assert_eq!(error_table.get::<String>("type").unwrap(), "error");
        assert_eq!(error_table.get::<u64>("session_id").unwrap(), 7);
        assert_eq!(error_table.get::<String>("error").unwrap(), "boom");

        let data_table = session_event_to_table(
            &lua,
            SessionEvent::Data(
                9,
                DecodedEntry {
                    pipeline_name: "hex".into(),
                    data: DecodedData::Hex("41 42".into()),
                },
            ),
        )
        .unwrap();
        assert_eq!(data_table.get::<String>("type").unwrap(), "data");
        assert_eq!(data_table.get::<u64>("session_id").unwrap(), 9);
        assert_eq!(data_table.get::<String>("pipeline").unwrap(), "hex");
        assert_eq!(data_table.get::<String>("kind").unwrap(), "hex");
        assert_eq!(data_table.get::<String>("data").unwrap(), "41 42");
    }
}
