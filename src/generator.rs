extern crate chrono;
extern crate rayon;

use std::cmp::max;
use std::collections::{HashMap, HashSet};
use std::fs;

use itertools::Itertools;
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

use crate::cfg::*;

use self::chrono::{DateTime, Local};

pub fn gen_cfg(warehouse_id_list: Vec<u32>, terminal_count: u32, transaction_count: u32) -> () {
    if warehouse_id_list.len() == 0 {
        panic!("Warehouse id list is empty");
    }
    if terminal_count == 0 {
        panic!("Terminal count must be more than 0");
    }
    if transaction_count < 23 {
        panic!("Minimum transaction count must be 23");
    }

    let wh_id_set: HashSet<_> = warehouse_id_list.clone().drain(..).collect(); // distinct values
    let now: DateTime<Local> = Local::now();
    let ts = now.format("%Y%m%d_%H%M%S");
    wh_id_set.iter().cartesian_product(1..terminal_count + 1).for_each(|(w, t)| {
        let mut tx_vec: Vec<TransactionParams> = Vec::new();
        tx_breakdown(transaction_count).iter().for_each(|(typ, dflts)| {
            let mut rbk_count_left = dflts.rbk_count;
            for _ in 0..dflts.tx_count {
                tx_vec.push(TransactionParams { typ: typ.clone(), keying_time_ms: dflts.keying_time_ms, think_time_ms: dflts.think_time_ms,
                                                is_rbk: match rbk_count_left { 0 => false,
                                                                               _ => { rbk_count_left -= 1; true }}
                });
            };
        });

        let cfgz = TermControlCfg { home_warehouse_id: w.clone(), this_terminal_id: t.clone(), transactions_to_run: tx_vec };
        let str = serde_yaml::to_string(&cfgz).expect("Unsupported configuration format");
        let cfg_file_name = format!("{}_W{}_T{}.cfg", &ts, w, t);
        fs::write(&cfg_file_name, &str).expect(&format!("Error writing cfg file {}", &cfg_file_name));
    });
}

struct TransactionDefaults {
    tx_count: u32,
    keying_time_ms: u32,
    think_time_ms: u32,
    rbk_count: u32
}

fn tx_breakdown(transaction_count: u32) -> HashMap<TransactionType, TransactionDefaults> {
    let mut map = HashMap::new();
    let tc_f64 = transaction_count as f64;
    map.insert(TransactionType::Payment,    TransactionDefaults { tx_count: fraction_non_zero(tc_f64, 0.44), keying_time_ms: 3_000, think_time_ms: gen_think_time(12), rbk_count: 0 });
    map.insert(TransactionType::Status,     TransactionDefaults { tx_count: fraction_non_zero(tc_f64, 0.04), keying_time_ms: 2_000, think_time_ms: gen_think_time(10), rbk_count: 0 });
    map.insert(TransactionType::Delivery,   TransactionDefaults { tx_count: fraction_non_zero(tc_f64, 0.04), keying_time_ms: 2_000, think_time_ms: gen_think_time(5), rbk_count: 0 });
    map.insert(TransactionType::Threshold,  TransactionDefaults { tx_count: fraction_non_zero(tc_f64, 0.04), keying_time_ms: 2_000, think_time_ms: gen_think_time(5), rbk_count: 0 });
    let new_order_count = transaction_count - map.iter().map(|(_,v)| v.tx_count).sum::<u32>();
    map.insert(TransactionType::NewOrder,   TransactionDefaults { tx_count: new_order_count,
                                                                  keying_time_ms: 18_000,
                                                                  think_time_ms: gen_think_time(12),
                                                                  rbk_count: fraction_non_zero(new_order_count as f64, 0.01) });

    map
}

fn fraction_non_zero(base: f64, fraction: f64) -> u32 {
    max(1, (base * fraction) as u32)
}

fn gen_think_time(mean_time_s: u32) -> u32 {
    (- StdRng::from_entropy().gen::<f64>().ln() * (mean_time_s as f64) * 1000.0) as u32
}
