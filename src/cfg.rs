use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct TermControlCfg {
    pub home_warehouse_id: u32,
    pub this_terminal_id: u32,
    pub transactions_to_run: Vec<TransactionParams>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TransactionParams {
    pub typ: TransactionType,
    pub key_time_ms: u32,
    pub think_time_ms: u32,
    pub is_rbk: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Hash, Clone)]
pub enum TransactionType {
    NewOrder,
    Payment,
    Status,
    Delivery,
    Threshold,
}