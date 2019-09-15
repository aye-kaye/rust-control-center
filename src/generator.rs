extern crate chrono;
extern crate rayon;

use std::collections::{HashSet, HashMap};
use std::error::Error;
use std::fs::File;
use std::io::ErrorKind;
use std::option::Iter;
use std::time::Instant;

use rayon::prelude::*;

use self::chrono::{DateTime, Local};
use std::iter::Map;
use itertools::Itertools;

use crate::cfg::*;

pub fn gen_cfg(warehouse_id_list: Vec<u32>, terminal_count: u32, transaction_count: u32) -> () {
    let wh_id_set: HashSet<_> = warehouse_id_list.clone().drain(..).collect(); // distinct values
    let now: DateTime<Local> = Local::now();
    let ts = now.format("%Y%m%d_%H%M%S");
    wh_id_set.iter().cartesian_product((1..terminal_count + 1)).for_each(|(w, t)| {
        File::create(format!("W{}_T{}_{}.cfg", w, t, &ts)).unwrap_or_else(|error| {
            if error.kind() == ErrorKind::AlreadyExists {
                File::create("tmp.cfg").unwrap_or_else(|error| {
                    panic!("Error creating substitute file {:?}", error);
                })
            } else {
                panic!("Error creating configuration file {:?}", error);
            }
        });
    });
}

fn tx_breakdown(transaction_count: u32) -> HashMap<TransactionType, u32> {
    let mut map = HashMap::new();
    let tc_f64 = transaction_count as f64;
    map.insert(TransactionType::Payment, (tc_f64 * 0.44) as u32);
    map.insert(TransactionType::Status, (tc_f64 * 0.4) as u32);
    map.insert(TransactionType::Delivery, (tc_f64 * 0.4) as u32);
    map.insert(TransactionType::Threshold, (tc_f64 * 0.4) as u32);
    map.insert(TransactionType::NewOrder, transaction_count - map.values().sum::<u32>());

    map
}


