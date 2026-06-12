use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use std::time::{SystemTime, UNIX_EPOCH};

use xserial_client::event::DecodedEntry;
use xserial_core::protocol::DecodedData;

/// Manages background file logging via a dedicated writer thread.
///
/// Lines are sent through an mpsc channel to a background thread that
/// appends to the log file using buffered I/O, keeping the GUI thread
/// non-blocking.
pub struct LogWriter {
    sender: mpsc::Sender<String>,
}

impl LogWriter {
    /// Spawns a background thread that opens `path` in append mode and
    /// writes every line it receives through the returned `LogWriter`.
    pub fn open(path: impl Into<PathBuf>) -> std::io::Result<Self> {
        let path: PathBuf = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        let mut writer = BufWriter::with_capacity(1024, file);
        let (sender, receiver) = mpsc::channel::<String>();

        let thread_name = format!("xserial-log-{}", path.display());
        thread::Builder::new().name(thread_name).spawn(move || {
            while let Ok(line) = receiver.recv() {
                if writeln!(writer, "{line}").is_err() {
                    break;
                }
            }
            let _ = writer.flush();
        })?;

        Ok(Self { sender })
    }

    /// Send a line to the background writer. Non-blocking — drops the line
    /// silently if the writer thread has stopped.
    pub fn write_line(&self, line: &str) {
        let _ = self.sender.send(line.to_owned());
    }
}

impl Drop for LogWriter {
    fn drop(&mut self) {
        // Sender is dropped here, which causes the background thread's
        // receiver.recv() to return Err, triggering flush + exit.
    }
}

/// Format a [`DecodedEntry`] into a log line.
///
/// Format: `[HH:MM:SS.mmm] [IN] [pipeline] payload`
///
/// Returns `None` for Binary and Plot entries (not meaningful as text logs).
pub fn format_data_log(
    entry: &DecodedEntry,
    direction: &str,
    show_timestamp: bool,
    show_direction: bool,
    show_pipeline: bool,
) -> Option<String> {
    let data = match &entry.data {
        DecodedData::Text(text) => text.clone(),
        DecodedData::Hex(hex) => hex.clone(),
        DecodedData::Binary(_) | DecodedData::Plot(_) => return None,
    };

    let mut parts = Vec::with_capacity(4);
    if show_timestamp {
        parts.push(utc_timestamp());
    }
    if show_direction {
        parts.push(format!("[{direction}]"));
    }
    if show_pipeline {
        parts.push(format!("[{}]", entry.pipeline_name));
    }
    parts.push(data);

    Some(parts.join(" "))
}

/// Format a sent text string into a log line (no `DecodedEntry` available
/// for outbound data).
pub fn format_sent_log(text: &str) -> String {
    format!("{} [OUT] {text}", utc_timestamp())
}

fn utc_timestamp() -> String {
    let since_epoch = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = since_epoch.as_secs();
    let ms = since_epoch.subsec_millis();
    let h = (secs / 3600) % 24;
    let m = (secs / 60) % 60;
    let s = secs % 60;
    format!("{h:02}:{m:02}:{s:02}.{ms:03}")
}
