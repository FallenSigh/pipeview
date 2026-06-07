use mlua::{UserData, UserDataMethods, Variadic, LuaSerdeExt};
use xserial_core::protocol::DecodedData;
use crate::session::SessionHandle;

pub struct LuaSessionHandle { pub handle: SessionHandle }

impl LuaSessionHandle {
    pub fn new(handle: SessionHandle) -> Self { Self { handle } }
}

impl UserData for LuaSessionHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_async_method("send", |_, this, data: mlua::String| async move {
            this.handle.send(data.as_bytes().to_vec()).await
                .map_err(mlua::Error::RuntimeError)
        });

        methods.add_async_method("read", |lua, this, timeout_ms: Option<u64>| async move {
            let ms = timeout_ms.unwrap_or(1000);
            match this.handle.read(ms).await {
                Some(entry) => {
                    let t = lua.create_table()?;
                    t.set("pipeline", entry.pipeline_name)?;
                    match entry.data {
                        DecodedData::Text(s) => { t.set("kind", "text")?; t.set("data", s)?; }
                        DecodedData::Hex(s) => { t.set("kind", "hex")?; t.set("data", s)?; }
                        DecodedData::Binary(v) => { t.set("kind", "binary")?; t.set("data", lua.create_string(&v)?)?; }
                        DecodedData::Plot(frame) => {
                            t.set("kind", "plot")?;
                            let chs = lua.create_table()?;
                            for (i, ch) in frame.channels.iter().enumerate() {
                                chs.set(i + 1, lua.create_sequence_from(ch.iter().copied())?)?;
                            }
                            t.set("channels", chs)?;
                            t.set("sample_count", frame.sample_count())?;
                        }
                    }
                    Ok(Some(t))
                }
                None => Ok(None),
            }
        });

        methods.add_async_method("close", |_, this, (): ()| async move {
            this.handle.close().await.map_err(mlua::Error::RuntimeError)
        });

        methods.add_async_method("reconfigure", |lua, this, config: mlua::Table| async move {
            let cfg: crate::config::SessionConfig = lua.from_value(mlua::Value::Table(config))?;
            this.handle.reconfigure(cfg).await.map_err(mlua::Error::RuntimeError)
        });

        methods.add_async_method("on_data", |_, this, (callback, pipelines): (mlua::Function, Variadic<String>)| async move {
            let filter: Vec<String> = pipelines.into_iter().collect();
            let mut rx = this.handle.subscribe();
            tokio::spawn(async move {
                loop {
                    match rx.recv().await {
                        Ok(crate::session::SessionEvent::Data(_, entry))
                            if filter.is_empty() || filter.iter().any(|f| f == &entry.pipeline_name) =>
                        {
                                let data_str = match &entry.data {
                                    DecodedData::Text(s) => s.clone(),
                                    DecodedData::Hex(s) => s.clone(),
                                    _ => continue,
                                };
                                let _ = callback.call_async::<()>((entry.pipeline_name, data_str)).await;
                            }
                        Ok(crate::session::SessionEvent::Closed(_)) => break,
                        Err(_) => break,
                        _ => {}
                    }
                }
            });
            Ok(())
        });
    }
}
