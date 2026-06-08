use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use mlua::{Lua, LuaSerdeExt, Result as LuaResult, Table, Value};

use crate::config::SessionConfig;
use crate::manager::SessionManager;

mod session_api;

#[derive(Clone)]
pub struct LuaRuntime {
    manager: SessionManager,
    callback_states: Arc<Mutex<HashMap<u64, Weak<session_api::CallbackState>>>>,
}

impl LuaRuntime {
    pub fn new(manager: SessionManager) -> Self {
        Self {
            manager,
            callback_states: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn manager(&self) -> &SessionManager {
        &self.manager
    }

    pub fn register_callback_state(
        &self,
        session_id: u64,
        state: &Arc<session_api::CallbackState>,
    ) {
        self.callback_states
            .lock()
            .expect("lua callback state map poisoned")
            .insert(session_id, Arc::downgrade(state));
    }

    pub fn unregister_callback_state(&self, session_id: u64) {
        self.callback_states
            .lock()
            .expect("lua callback state map poisoned")
            .remove(&session_id);
    }

    pub async fn pump_callbacks(
        &self,
        lua: &Lua,
        limit_per_session: Option<usize>,
    ) -> LuaResult<usize> {
        let states: Vec<(u64, Arc<session_api::CallbackState>)> = {
            let mut states = self
                .callback_states
                .lock()
                .expect("lua callback state map poisoned");
            let mut live = Vec::with_capacity(states.len());
            states.retain(|session_id, weak| {
                if let Some(state) = weak.upgrade() {
                    live.push((*session_id, state));
                    true
                } else {
                    false
                }
            });
            live
        };

        let mut pumped = 0;
        for (_session_id, state) in states {
            pumped += state.pump(lua, limit_per_session).await?;
        }
        Ok(pumped)
    }
}

pub fn register(lua: &Lua) -> LuaResult<()> {
    register_with_manager(lua, SessionManager::new())
}

pub fn register_with_manager(lua: &Lua, manager: SessionManager) -> LuaResult<()> {
    let runtime = LuaRuntime::new(manager);
    let xserial = lua.create_table()?;

    xserial.set("open", {
        let runtime = runtime.clone();
        lua.create_async_function(move |lua, config: Table| {
            let runtime = runtime.clone();
            async move {
                let cfg: SessionConfig = lua.from_value(Value::Table(config))?;
                let handle = runtime.manager().create(cfg);
                let userdata = session_api::LuaSessionHandle::new(handle, runtime);
                lua.create_userdata(userdata)
            }
        })?
    })?;

    xserial.set(
        "list_ports",
        lua.create_function(|_, ()| {
            let ports = xserial_core::transport::serial::SerialTransport::list_ports();
            Ok(ports.into_iter().map(|p| p.port_name).collect::<Vec<_>>())
        })?,
    )?;

    xserial.set("sleep", {
        let runtime = runtime.clone();
        lua.create_async_function(move |lua, ms: u64| {
            let runtime = runtime.clone();
            async move {
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
                let _ = runtime.pump_callbacks(&lua, None).await?;
                Ok(())
            }
        })?
    })?;

    xserial.set("poll", {
        let runtime = runtime.clone();
        lua.create_async_function(move |lua, limit_per_session: Option<usize>| {
            let runtime = runtime.clone();
            async move { runtime.pump_callbacks(&lua, limit_per_session).await }
        })?
    })?;

    xserial.set(
        "log",
        lua.create_function(|_, msg: String| {
            tracing::info!("[lua] {}", msg);
            Ok(())
        })?,
    )?;

    lua.globals().set("xserial", xserial)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Value;

    #[test]
    fn register_exposes_expected_api() {
        let lua = Lua::new();
        register(&lua).unwrap();

        let xserial: Table = lua.globals().get("xserial").unwrap();
        for name in ["open", "list_ports", "sleep", "poll", "log"] {
            let value: Value = xserial.get(name).unwrap();
            assert!(
                matches!(value, Value::Function(_)),
                "{name} should be a function"
            );
        }
    }

    #[test]
    fn register_with_manager_uses_supplied_manager() {
        let lua = Lua::new();
        let manager = SessionManager::new();
        register_with_manager(&lua, manager.clone()).unwrap();

        let xserial: Table = lua.globals().get("xserial").unwrap();
        let open: Value = xserial.get("open").unwrap();
        assert!(matches!(open, Value::Function(_)));
        assert_eq!(manager.count(), 0);
    }
}
