use crate::cfg::TransactionType;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct TermLogRecord {
    pub time_started: u64,
    #[serde(rename(serialize = "type"))]
    pub typ: TransactionType,
    pub running_time: u32,
    pub tx_running_time: u32,
    pub think_time_ms: u32,
    pub is_rbk: bool,
}
