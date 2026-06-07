use crate::config::SessionConfig;

pub enum SessionCmd {
    Send(Vec<u8>),
    Close,
    Reconfigure(SessionConfig),
}
