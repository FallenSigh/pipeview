use std::sync::atomic::{AtomicU64, Ordering};
use mlua::{Lua, Result as LuaResult, Table, Value, LuaSerdeExt};
use crate::config::SessionConfig;
use crate::session::Session;
mod session_api;

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub fn register(lua: &Lua) -> LuaResult<()> {
    let xserial = lua.create_table()?;

    xserial.set("open", lua.create_async_function(move |lua, config: Table| {
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        async move {
            let cfg: SessionConfig = lua.from_value(Value::Table(config))?;
            let handle = Session::spawn(id, cfg);
            lua.create_userdata(session_api::LuaSessionHandle::new(handle))
        }
    })?)?;

    xserial.set("list_ports", lua.create_function(|_, ()| {
        let ports = xserial_core::transport::serial::SerialTransport::list_ports();
        Ok(ports.into_iter().map(|p| p.port_name).collect::<Vec<_>>())
    })?)?;

    xserial.set("sleep", lua.create_async_function(|_, ms: u64| async move {
        tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
        Ok(())
    })?)?;

    xserial.set("log", lua.create_function(|_, msg: String| {
        tracing::info!("[lua] {}", msg);
        Ok(())
    })?)?;

    lua.globals().set("xserial", xserial)?;
    Ok(())
}
