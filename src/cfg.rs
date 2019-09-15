pub struct TermControlCfg {
    pub home_warehouse_id: u32,
    pub this_terminal_id: u32,
    pub transactions_to_run: TransactionParams
}

pub struct TransactionParams {
    pub typ: TransactionType,
    pub key_time_ms: u32,
    pub wait_time_ms: u32,
    pub is_rbk: bool
}

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum TransactionType {
    NewOrder,
    Payment,
    Status,
    Delivery,
    Threshold
}
