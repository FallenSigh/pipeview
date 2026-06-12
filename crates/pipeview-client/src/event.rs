use pipeview_core::protocol::DecodedData;

#[derive(Debug, Clone)]
pub struct DecodedEntry {
    pub pipeline_name: String,
    pub data: DecodedData,
}
