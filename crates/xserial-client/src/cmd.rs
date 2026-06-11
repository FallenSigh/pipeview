use crate::config::SessionConfig;

pub enum SessionCmd {
    Send(Vec<u8>),
    Connect,
    Disconnect,
    Reconnect,
    SetAutoReconnect(bool),
    Close,
    Reconfigure(SessionConfig),
    SetDtr(bool),
    SetRts(bool),
}
