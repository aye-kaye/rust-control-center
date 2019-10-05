extern crate rand;

use std::cmp::max;
use std::collections::{HashMap, HashSet};
use std::fs;

use chrono::{DateTime, Local};
use itertools::Itertools;
use rand::rngs::{SmallRng, StdRng};
use rand::{thread_rng, Rng, SeedableRng};
use rayon::prelude::*;

use crate::cfg::*;
use crate::terminal::*;
use rand::seq::SliceRandom;
use rand_distr::{Distribution, Poisson};

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
    wh_id_set
        .iter()
        .cartesian_product(1..terminal_count + 1)
        .collect::<Vec<(&u32, u32)>>()
        .par_iter_mut()
        .for_each(|(w, t)| {
            let mut tx_vec: Vec<TransactionParams> = Vec::new();
            tx_breakdown(transaction_count)
                .iter()
                .for_each(|(typ, dflts)| {
                    let mut rbk_count_left = dflts.rbk_count;
                    for _ in 0..dflts.tx_count {
                        tx_vec.push(TransactionParams {
                            typ: typ.clone(),
                            keying_time_ms: dflts.keying_time_ms,
                            think_time_ms: dflts.think_time_ms,
                            is_rbk: match rbk_count_left {
                                0 => false,
                                _ => {
                                    rbk_count_left -= 1;
                                    true
                                }
                            },
                        });
                    }
                });

            let cfgz = TermControlCfg {
                home_warehouse_id: w.clone(),
                this_terminal_id: t.clone(),
                transactions_to_run: tx_vec,
            };
            let str = serde_yaml::to_string(&cfgz).expect("Unsupported configuration format");
            let cfg_file_name = format!("{}_W{}_T{}.cfg", &ts, w, t);
            fs::write(&cfg_file_name, &str)
                .expect(&format!("Error writing cfg file {}", &cfg_file_name));
        });
}

/// Generate sample log files with fixed deck size and configurable terminal count and iteration count
pub fn gen_sample_data(terminal_count: u32, iteration_count: u32) -> () {
    if terminal_count == 0 {
        panic!("Terminal count must be more than 0");
    }
    if iteration_count == 0 {
        panic!("Iteration count must be more than 0");
    }

    const TRANSACTION_COUNT: u32 = 100;

    let now: DateTime<Local> = Local::now();
    let start_ts = now.format("%Y%m%d_%H%M%S");

    (0..terminal_count)
        .collect::<Vec<u32>>()
        .par_iter_mut()
        .for_each(|t| {
            let log_file_name = format!("{}_T{}.csv", start_ts, t);
            let mut wtr = csv::Writer::from_path(&log_file_name).unwrap();

            let tx_bkdwn = tx_breakdown(TRANSACTION_COUNT);

            // Initialize deck with the generated distribution of transaction types
            let mut deck: Vec<TransactionType> = tx_bkdwn
                .iter()
                .map(|(tx_type, tx_def)| {
                    (0..tx_def.tx_count)
                        .collect::<Vec<u32>>()
                        .iter()
                        .map(|_| tx_type.clone())
                        .collect()
                })
                .flat_map(|t_t: Vec<TransactionType>| t_t)
                .collect();
            let deck_slice: &mut [TransactionType] = &mut deck;

            let mut rng = thread_rng();
            let mut small_rng = SmallRng::from_entropy();
            let poi = Poisson::new(2.0).unwrap();

            let mut term_running_time = Local::now().timestamp_millis() as u64;

            (0..iteration_count)
                .collect::<Vec<u32>>()
                .iter()
                .for_each(|_| {
                    deck_slice.shuffle(&mut rng);

                    deck_slice
                        .iter()
                        .cloned()
                        .for_each(|tx_type: TransactionType| {
                            let tx_def = tx_bkdwn.get(&tx_type).unwrap();

                            let keying_time = tx_def.keying_time_ms;

                            let mut tx_rt_smpl_f: f64 = poi.sample(&mut small_rng);
                            tx_rt_smpl_f += 1.;
                            let rt_smpl_f: f64 = tx_rt_smpl_f * small_rng.gen_range(1.05, 1.15);
                            let tx_rt_smpl = (tx_rt_smpl_f * 1000.) as u32;
                            let rt_smpl = (rt_smpl_f * 1000.) as u32;

                            let think_time = tx_def.think_time_ms;

                            wtr.serialize(TermLogRecord {
                                time_started: term_running_time,
                                typ: tx_type.clone(),
                                running_time: rt_smpl,
                                tx_running_time: tx_rt_smpl,
                                think_time_ms: think_time,
                                is_rbk: false,
                            })
                            .expect(&format!(
                                "Error writing sample record to the file {}",
                                &log_file_name
                            ));

                            term_running_time += (keying_time + rt_smpl + think_time) as u64;
                        });
                });
        });
}

struct TransactionDefaults {
    tx_count: u32,
    keying_time_ms: u32,
    think_time_ms: u32,
    rbk_count: u32,
}

fn tx_breakdown(transaction_count: u32) -> HashMap<TransactionType, TransactionDefaults> {
    let mut map = HashMap::new();
    let tc_f64 = transaction_count as f64;
    map.insert(
        TransactionType::Payment,
        TransactionDefaults {
            tx_count: fraction_non_zero(tc_f64, 0.44),
            keying_time_ms: 3_000,
            think_time_ms: gen_think_time(12),
            rbk_count: 0,
        },
    );
    map.insert(
        TransactionType::OrderStatus,
        TransactionDefaults {
            tx_count: fraction_non_zero(tc_f64, 0.04),
            keying_time_ms: 2_000,
            think_time_ms: gen_think_time(10),
            rbk_count: 0,
        },
    );
    map.insert(
        TransactionType::Delivery,
        TransactionDefaults {
            tx_count: fraction_non_zero(tc_f64, 0.04),
            keying_time_ms: 2_000,
            think_time_ms: gen_think_time(5),
            rbk_count: 0,
        },
    );
    map.insert(
        TransactionType::StockLevel,
        TransactionDefaults {
            tx_count: fraction_non_zero(tc_f64, 0.04),
            keying_time_ms: 2_000,
            think_time_ms: gen_think_time(5),
            rbk_count: 0,
        },
    );
    let new_order_count = transaction_count - map.iter().map(|(_, v)| v.tx_count).sum::<u32>();
    map.insert(
        TransactionType::NewOrder,
        TransactionDefaults {
            tx_count: new_order_count,
            keying_time_ms: 18_000,
            think_time_ms: gen_think_time(12),
            rbk_count: fraction_non_zero(new_order_count as f64, 0.01),
        },
    );

    map
}

fn fraction_non_zero(base: f64, fraction: f64) -> u32 {
    max(1, (base * fraction) as u32)
}

fn gen_think_time(mean_time_s: u32) -> u32 {
    (-StdRng::from_entropy().gen::<f64>().ln() * (mean_time_s as f64) * 1000.0) as u32
}
