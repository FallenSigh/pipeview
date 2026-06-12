use std::fs;
use std::sync::Mutex;

use mlua::{Function, Lua, RegistryKey, Table, Value};
use tracing::warn;
use pipeview_core::frame::Framer;
use pipeview_core::protocol::plot::{PlotFormat, PlotFrame, SampleType};
use pipeview_core::protocol::{DecodedData, ProtocolDecoder};

use crate::error::{Error, Result};

pub struct LuaFramer {
    lua: Lua,
    feed: RegistryKey,
    flush: RegistryKey,
    reset: RegistryKey,
    pending_len: RegistryKey,
    script_path: String,
}

impl LuaFramer {
    pub fn from_script_path(script_path: impl Into<String>) -> Result<Self> {
        let script_path = script_path.into();
        let lua = Lua::new();
        let table = load_script_table(&lua, &script_path)?;
        let feed = registry_function(&lua, &table, "feed", &script_path)?;
        let flush = registry_function(&lua, &table, "flush", &script_path)?;
        let reset = registry_function(&lua, &table, "reset", &script_path)?;
        let pending_len = registry_function(&lua, &table, "pending_len", &script_path)?;

        Ok(Self {
            lua,
            feed,
            flush,
            reset,
            pending_len,
            script_path,
        })
    }
}

impl Framer for LuaFramer {
    fn feed(&mut self, data: &[u8]) -> Vec<Vec<u8>> {
        let result = (|| -> mlua::Result<Vec<Vec<u8>>> {
            let function: Function = self.lua.registry_value(&self.feed)?;
            let bytes = self.lua.create_string(data)?;
            frames_from_value(function.call::<Value>(bytes)?)
        })();

        match result {
            Ok(frames) => frames,
            Err(err) => {
                warn!(script = %self.script_path, error = %err, "lua framer feed failed");
                Vec::new()
            }
        }
    }

    fn flush(&mut self) -> Option<Vec<u8>> {
        let result = (|| -> mlua::Result<Option<Vec<u8>>> {
            let function: Function = self.lua.registry_value(&self.flush)?;
            optional_bytes_from_value(function.call::<Value>(())?)
        })();

        match result {
            Ok(frame) => frame,
            Err(err) => {
                warn!(script = %self.script_path, error = %err, "lua framer flush failed");
                None
            }
        }
    }

    fn reset(&mut self) {
        let result = (|| -> mlua::Result<()> {
            let function: Function = self.lua.registry_value(&self.reset)?;
            function.call::<()>(())
        })();

        if let Err(err) = result {
            warn!(script = %self.script_path, error = %err, "lua framer reset failed");
        }
    }

    fn pending_len(&self) -> usize {
        let result = (|| -> mlua::Result<usize> {
            let function: Function = self.lua.registry_value(&self.pending_len)?;
            function.call::<usize>(())
        })();

        match result {
            Ok(len) => len,
            Err(err) => {
                warn!(script = %self.script_path, error = %err, "lua framer pending_len failed");
                0
            }
        }
    }
}

pub struct LuaDecoder {
    lua: Lua,
    decode: RegistryKey,
    script_path: String,
    call_lock: Mutex<()>,
}

impl LuaDecoder {
    pub fn from_script_path(script_path: impl Into<String>) -> Result<Self> {
        let script_path = script_path.into();
        let lua = Lua::new();
        let table = load_script_table(&lua, &script_path)?;
        let decode = registry_function(&lua, &table, "decode", &script_path)?;

        Ok(Self {
            lua,
            decode,
            script_path,
            call_lock: Mutex::new(()),
        })
    }
}

impl ProtocolDecoder for LuaDecoder {
    fn name(&self) -> &str {
        "Lua"
    }

    fn decode(&self, frame: &[u8]) -> Option<DecodedData> {
        let _guard = self
            .call_lock
            .lock()
            .expect("lua decoder call mutex poisoned");
        let result = (|| -> mlua::Result<Option<DecodedData>> {
            let function: Function = self.lua.registry_value(&self.decode)?;
            let bytes = self.lua.create_string(frame)?;
            decoded_from_value(function.call::<Value>(bytes)?, frame)
        })();

        match result {
            Ok(decoded) => decoded,
            Err(err) => {
                warn!(script = %self.script_path, error = %err, "lua decoder decode failed");
                None
            }
        }
    }
}

fn load_script_table(lua: &Lua, script_path: &str) -> Result<Table> {
    let script = fs::read_to_string(script_path)?;
    match lua.load(&script).eval::<Value>()? {
        Value::Table(table) => Ok(table),
        other => Err(Error::Other(format!(
            "lua script '{script_path}' must return a table, got {}",
            other.type_name()
        ))),
    }
}

fn registry_function(
    lua: &Lua,
    table: &Table,
    name: &str,
    script_path: &str,
) -> Result<RegistryKey> {
    let value: Value = table.get(name)?;
    match value {
        Value::Function(function) => Ok(lua.create_registry_value(function)?),
        other => Err(Error::Other(format!(
            "lua script '{script_path}' field '{name}' must be a function, got {}",
            other.type_name()
        ))),
    }
}

fn frames_from_value(value: Value) -> mlua::Result<Vec<Vec<u8>>> {
    match value {
        Value::Nil => Ok(Vec::new()),
        Value::String(bytes) => Ok(vec![bytes.as_bytes().to_vec()]),
        Value::Table(table) => {
            let mut frames = Vec::new();
            for value in table.sequence_values::<mlua::String>() {
                frames.push(value?.as_bytes().to_vec());
            }
            Ok(frames)
        }
        other => Err(mlua::Error::RuntimeError(format!(
            "framer feed must return nil, string, or table of strings, got {}",
            other.type_name()
        ))),
    }
}

fn optional_bytes_from_value(value: Value) -> mlua::Result<Option<Vec<u8>>> {
    match value {
        Value::Nil => Ok(None),
        Value::String(bytes) => Ok(Some(bytes.as_bytes().to_vec())),
        other => Err(mlua::Error::RuntimeError(format!(
            "framer flush must return nil or string, got {}",
            other.type_name()
        ))),
    }
}

fn decoded_from_value(value: Value, raw: &[u8]) -> mlua::Result<Option<DecodedData>> {
    match value {
        Value::Nil => Ok(None),
        Value::String(text) => Ok(Some(DecodedData::Text(text.to_str()?.to_string()))),
        Value::Table(table) => decoded_from_table(table, raw).map(Some),
        other => Err(mlua::Error::RuntimeError(format!(
            "decoder decode must return nil, string, or table, got {}",
            other.type_name()
        ))),
    }
}

fn decoded_from_table(table: Table, raw: &[u8]) -> mlua::Result<DecodedData> {
    let kind: String = table.get("kind")?;
    match kind.as_str() {
        "text" => Ok(DecodedData::Text(table.get("data")?)),
        "hex" => Ok(DecodedData::Hex(table.get("data")?)),
        "binary" => {
            let bytes: mlua::String = table.get("data")?;
            Ok(DecodedData::Binary(bytes.as_bytes().to_vec()))
        }
        "plot" => Ok(DecodedData::Plot(plot_from_table(table, raw)?)),
        other => Err(mlua::Error::RuntimeError(format!(
            "unknown lua decoder kind '{other}'"
        ))),
    }
}

fn plot_from_table(table: Table, raw: &[u8]) -> mlua::Result<PlotFrame> {
    let channels_table: Table = table.get("channels")?;
    let mut channels = Vec::new();
    for channel in channels_table.sequence_values::<Table>() {
        let channel = channel?;
        let mut samples = Vec::new();
        for sample in channel.sequence_values::<f64>() {
            samples.push(sample?);
        }
        channels.push(samples);
    }

    Ok(PlotFrame {
        channels,
        raw: raw.to_vec(),
        sample_type: match table.get::<Option<String>>("sample_type")?.as_deref() {
            Some(value) => parse_sample_type(value)?,
            None => SampleType::F64,
        },
        format: match table.get::<Option<String>>("format")?.as_deref() {
            Some(value) => parse_plot_format(value)?,
            None => PlotFormat::Interleaved,
        },
    })
}

fn parse_sample_type(value: &str) -> mlua::Result<SampleType> {
    match value {
        "i8" | "I8" => Ok(SampleType::I8),
        "u8" | "U8" => Ok(SampleType::U8),
        "i16" | "I16" => Ok(SampleType::I16),
        "u16" | "U16" => Ok(SampleType::U16),
        "i32" | "I32" => Ok(SampleType::I32),
        "u32" | "U32" => Ok(SampleType::U32),
        "i64" | "I64" => Ok(SampleType::I64),
        "u64" | "U64" => Ok(SampleType::U64),
        "f32" | "F32" => Ok(SampleType::F32),
        "f64" | "F64" => Ok(SampleType::F64),
        other => Err(mlua::Error::RuntimeError(format!(
            "unknown plot sample_type '{other}'"
        ))),
    }
}

fn parse_plot_format(value: &str) -> mlua::Result<PlotFormat> {
    match value {
        "Interleaved" | "interleaved" => Ok(PlotFormat::Interleaved),
        "Block" | "block" => Ok(PlotFormat::Block),
        "XY" | "xy" => Ok(PlotFormat::XY),
        other => Err(mlua::Error::RuntimeError(format!(
            "unknown plot format '{other}'"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn write_script(name: &str, script: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("pipeview_{name}_{nonce}.lua"));
        fs::write(&path, script).unwrap();
        path
    }

    #[test]
    fn lua_framer_splits_lines_across_chunks() {
        let path = write_script(
            "framer",
            r#"
            local buffer = ""
            return {
                feed = function(bytes)
                    buffer = buffer .. bytes
                    local frames = {}
                    while true do
                        local i = buffer:find("\n", 1, true)
                        if not i then break end
                        frames[#frames + 1] = buffer:sub(1, i - 1)
                        buffer = buffer:sub(i + 1)
                    end
                    return frames
                end,
                flush = function()
                    if #buffer == 0 then return nil end
                    local frame = buffer
                    buffer = ""
                    return frame
                end,
                reset = function()
                    buffer = ""
                end,
                pending_len = function()
                    return #buffer
                end,
            }
            "#,
        );

        let mut framer = LuaFramer::from_script_path(path.to_string_lossy()).unwrap();
        assert_eq!(framer.feed(b"one\nt"), vec![b"one".to_vec()]);
        assert_eq!(framer.pending_len(), 1);
        assert_eq!(framer.feed(b"wo\nthree"), vec![b"two".to_vec()]);
        assert_eq!(framer.flush(), Some(b"three".to_vec()));
        assert_eq!(framer.pending_len(), 0);

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn lua_decoder_maps_text_binary_and_plot() {
        let path = write_script(
            "decoder",
            r#"
            return {
                decode = function(frame)
                    if frame == "skip" then
                        return nil
                    elseif frame == "bin" then
                        return { kind = "binary", data = "\1\2" }
                    elseif frame == "plot" then
                        return {
                            kind = "plot",
                            channels = { { 1.0, 2.0 }, { 3.5, 4.5 } },
                            sample_type = "F64",
                            format = "Block",
                        }
                    end
                    return { kind = "text", data = "decoded:" .. frame }
                end,
            }
            "#,
        );

        let decoder = LuaDecoder::from_script_path(path.to_string_lossy()).unwrap();
        assert!(decoder.decode(b"skip").is_none());
        assert!(matches!(
            decoder.decode(b"abc"),
            Some(DecodedData::Text(text)) if text == "decoded:abc"
        ));
        assert!(matches!(
            decoder.decode(b"bin"),
            Some(DecodedData::Binary(bytes)) if bytes == vec![1, 2]
        ));
        match decoder.decode(b"plot").unwrap() {
            DecodedData::Plot(frame) => {
                assert_eq!(frame.channels, vec![vec![1.0, 2.0], vec![3.5, 4.5]]);
                assert_eq!(frame.sample_type, SampleType::F64);
                assert_eq!(frame.format, PlotFormat::Block);
                assert_eq!(frame.raw, b"plot");
            }
            other => panic!("expected plot, got {other:?}"),
        }

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn lua_framer_requires_expected_functions() {
        let path = write_script("bad_framer", "return { feed = function() return {} end }");
        let result = LuaFramer::from_script_path(path.to_string_lossy());
        assert!(result.is_err());
        fs::remove_file(path).unwrap();
    }
}
