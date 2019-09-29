use std::cmp::*;
use std::collections::HashMap;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU64;
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;
use std::{fs, thread};

use average::{concatenate, Estimate, Max, Mean, Quantile};
use crossbeam_channel::{unbounded, Sender};
use crossbeam_deque::{Steal, Worker};
use hdrhistogram::Histogram;
use serde::{Deserialize, Serialize, Serializer};

use crate::cfg::TransactionType::*;
use crate::cfg::*;
use crate::terminal::*;

#[derive(Debug)]
pub struct TermGroupParams {
    pub term_count: u32,
    pub log_files_valid: Vec<String>,
    pub earliest_start_time_ms: u64,
    pub latest_start_time_ms: u64,
    pub steady_begin_time_ms: u64,
    pub steady_end_time_ms: u64,
}

#[derive(Serialize, Debug)]
pub struct ReportingData {
    tx_rt_data_new_order: TxRtData,
}

#[derive(Serialize, Debug)]
pub struct TxRtData {
    tx_type: TransactionType,
    tx_rt_p90: u64,
    tx_rt_mean: u64,
    tx_rt_max: u64,
    tx_rt_high: u64,
    tx_rt_series: Vec<[u64; 2]>,
}

struct FreqDistr {
    quantile90: Quantile,
    mean: Mean,
    max: Max,
}

impl FreqDistr {
    pub fn new() -> FreqDistr {
        FreqDistr {
            quantile90: Quantile::new(0.9),
            mean: Mean::default(),
            max: Max::default(),
        }
    }

    pub fn add(&mut self, x: f64) {
        self.quantile90.add(x);
        self.mean.add(x);
        self.max.add(x);
    }

    pub fn quantile(&self) -> f64 {
        self.quantile90.quantile()
    }

    pub fn mean(&self) -> f64 {
        self.mean.mean()
    }

    pub fn max(&self) -> f64 {
        self.max.max()
    }
}

struct TxStatsNewOrderContainer {
    tx_type: TransactionType,
    fqd: FreqDistr,
    tx_rt_histo: Histogram<u64>,
    tt_histo: Histogram<u64>,
    //TODO response time vs tpmc
    tx_cnt_histo: Histogram<u64>,
}

struct TxStatsCommonContainer {
    tx_type: TransactionType,
    fqd: FreqDistr,
    tx_rt_histo: Histogram<u64>,
}

impl TxStatsNewOrderContainer {
    fn new() -> Self {
        TxStatsNewOrderContainer {
            tx_type: TransactionType::NewOrder,
            fqd: FreqDistr::new(),
            tx_rt_histo: Histogram::<u64>::new(5).unwrap(),
            tt_histo: Histogram::<u64>::new(5).unwrap(),
            tx_cnt_histo: Histogram::<u64>::new(5).unwrap(),
        }
    }
}

impl TxStatsCommonContainer {
    fn new(tx_type: &TransactionType) -> Self {
        TxStatsCommonContainer {
            tx_type: tx_type.clone(),
            fqd: FreqDistr::new(),
            tx_rt_histo: Histogram::<u64>::new(5).unwrap(),
        }
    }
}

struct TxStatsGroup {
    new_order: TxStatsNewOrderContainer,
    payment: TxStatsCommonContainer,
    order_status: TxStatsCommonContainer,
    delivery: TxStatsCommonContainer,
    stock_level: TxStatsCommonContainer,
}

pub fn analyze_term_group(
    paths: &Vec<String>,
    steady_begin_offset: Duration,
    steady_end_offset: Duration,
) -> Result<TermGroupParams, Box<dyn Error>> {
    let w: Worker<String> = Worker::new_lifo();
    paths.iter().for_each(|f| w.push(f.clone()));

    let earliest_start_time_ms = Arc::new(Mutex::new(std::u64::MAX));
    let latest_start_time_ms = Arc::new(Mutex::new(0));
    let path_vec: Vec<String> = Vec::new();
    let log_files_valid = Arc::new(Mutex::new(path_vec));

    let num_cpus: usize = num_cpus::get();
    let barrier = Arc::new(Barrier::new(num_cpus + 1));
    (0..num_cpus)
        .map(|_| {
            let b = barrier.clone();
            let s = w.stealer().clone();
            let est = earliest_start_time_ms.clone();
            let lst = latest_start_time_ms.clone();
            let lfv = log_files_valid.clone();
            thread::spawn(move || {
                while let Steal::Success(file) = s.steal() {
                    //                    println!("anaylizing file {}", file);
                    let mut rdr = csv::Reader::from_path(&file).unwrap();
                    if let Some(result) = rdr.deserialize().next() {
                        let record: TermLogRecord = result.unwrap();
                        let current_start_time_ms = record.time_started;
                        let mut prev_est = est.lock().unwrap();
                        if let Ordering::Greater = prev_est.cmp(&current_start_time_ms) {
                            *prev_est = current_start_time_ms;
                        }
                        let mut prev_lst = lst.lock().unwrap();
                        if let Ordering::Less = prev_lst.cmp(&current_start_time_ms) {
                            *prev_lst = current_start_time_ms;
                        }

                        lfv.lock().unwrap().push(file);
                    }
                }
                b.wait();
            });
        })
        .collect::<()>();
    barrier.wait();
    let lfv = log_files_valid.lock().unwrap().clone();
    let est = earliest_start_time_ms.lock().unwrap();
    let lst = latest_start_time_ms.lock().unwrap();
    Ok(TermGroupParams {
        term_count: lfv.len() as u32,
        earliest_start_time_ms: *est,
        latest_start_time_ms: *lst,
        log_files_valid: lfv,
        steady_begin_time_ms: *lst + steady_begin_offset.as_millis() as u64,
        steady_end_time_ms: *lst + steady_end_offset.as_millis() as u64,
    })
}

pub fn build_reports(
    paths: &Vec<String>,
    steady_begin_offset: Duration,
    steady_end_offset: Duration,
) {
    let group_params = analyze_term_group(paths, steady_begin_offset, steady_end_offset).unwrap();

    let w: Worker<String> = Worker::new_lifo();
    paths.iter().for_each(|f| w.push(f.clone()));

    let num_cpus: usize = num_cpus::get();
    let barrier = Arc::new(Barrier::new(num_cpus + 1));
    let (sender, receiver) = unbounded();
    (0..num_cpus)
        .map(|_| {
            let b = barrier.clone();
            let st = w.stealer().clone();
            let sr = sender.clone();
            thread::spawn(move || {
                while let Steal::Success(file) = st.steal() {
                    let mut rdr = csv::Reader::from_path(&file).unwrap();
                    for result in rdr.deserialize() {
                        let record: TermLogRecord = result.unwrap();
                        sr.send(record).unwrap();
                    }
                    //                    println!("file processed {}", &file);
                }
                b.wait();
                //                println!("freed {:?}", thread::current().id());
            });
        })
        .collect::<()>();

    thread::spawn(move || {
        barrier.clone().wait();
        //        println!("dropping sender");
        drop(sender);
    });

    const TX_SAMPLING_INTERVAL_SEC: u64 = 1;
    const TX_SAMPLING_INTERVAL_MSEC: u64 = TX_SAMPLING_INTERVAL_SEC * 100;
    const TX_SAMPLING_INTERVAL_MSEC_F: f64 = TX_SAMPLING_INTERVAL_MSEC as f64;

    const TX_COUNT_SAMPLING_INTERVAL_SEC: u64 = 30;
    const TX_COUNT_SAMPLING_INTERVAL_MSEC: u64 = TX_COUNT_SAMPLING_INTERVAL_SEC * 1000;

    let mut fqd_new_order: FreqDistr = FreqDistr::new();

    let earliest_start_time_ms: &u64 = &group_params.earliest_start_time_ms;
    let latest_start_time_ms: &u64 = &group_params.latest_start_time_ms;
    let steady_begin_time_ms: &u64 = &group_params.steady_begin_time_ms;
    let steady_end_time_ms: &u64 = &group_params.steady_end_time_ms;

    let mut txsg = TxStatsGroup {
        new_order: TxStatsNewOrderContainer::new(),
        payment: TxStatsCommonContainer::new(&Payment),
        order_status: TxStatsCommonContainer::new(&OrderStatus),
        delivery: TxStatsCommonContainer::new(&Delivery),
        stock_level: TxStatsCommonContainer::new(&StockLevel),
    };

    loop {
        match receiver.recv() {
            Ok(record) => {
                let record_time = record.time_started;

                // Gathering metrics within steady interval
                if (record_time >= *steady_begin_time_ms && record_time < *steady_end_time_ms) {
                    let tx_interval_value =
                        libm::ceil(record.tx_running_time as f64 / TX_SAMPLING_INTERVAL_MSEC_F)
                            as u64
                            * TX_SAMPLING_INTERVAL_MSEC;
                    //XXX rewrite in idiomatic way
                    match record.typ {
                        TransactionType::NewOrder => {
                            txsg.new_order.fqd.add(tx_interval_value as f64);
                            txsg.new_order.tx_rt_histo.record(tx_interval_value);
                        }
                        TransactionType::Payment => {
                            txsg.payment.fqd.add(tx_interval_value as f64);
                            txsg.payment.tx_rt_histo.record(tx_interval_value);
                        }
                        TransactionType::OrderStatus => {
                            txsg.order_status.fqd.add(tx_interval_value as f64);
                            txsg.order_status.tx_rt_histo.record(tx_interval_value);
                        }
                        TransactionType::Delivery => {
                            txsg.delivery.fqd.add(tx_interval_value as f64);
                            txsg.delivery.tx_rt_histo.record(tx_interval_value);
                        }
                        TransactionType::StockLevel => {
                            txsg.stock_level.fqd.add(tx_interval_value as f64);
                            txsg.stock_level.tx_rt_histo.record(tx_interval_value);
                        }
                    };
                }
                // TODO append total iteration time
                let tx_cnt_interval_num =
                    (record_time - *earliest_start_time_ms) / TX_COUNT_SAMPLING_INTERVAL_MSEC;
                txsg.new_order.tx_cnt_histo.record(tx_cnt_interval_num);
            }
            Err(RecvError) => {
                println!("Done");

                const TX_RT_INTERVAL_COUNT: u64 = 20;
                //              New Order
                let p90 = txsg.new_order.tx_rt_histo.value_at_percentile(90.); //txsg.new_order.fqd.quantile() as u64;
                let mut high = 0;
                let tx_rt_4 = p90 * 4;
                let tx_rt_interval_size = tx_rt_4 / TX_RT_INTERVAL_COUNT;
                let mut tx_rt_series: Vec<[u64; 2]> = (1..TX_RT_INTERVAL_COUNT + 1)
                    .map(|i| {
                        let value = i * tx_rt_interval_size;
                        let count = txsg
                            .new_order
                            .tx_rt_histo
                            .count_between((i - 1) * tx_rt_interval_size, value);
                        high = max(high, count);
                        [value, count]
                    })
                    .collect();

                let mut new_order_data = TxRtData {
                    tx_type: TransactionType::NewOrder,
                    tx_rt_p90: p90,
                    tx_rt_mean: txsg.new_order.fqd.mean() as u64,
                    tx_rt_max: txsg.new_order.fqd.max() as u64,
                    tx_rt_high: high,
                    tx_rt_series,
                };

                let mut reporting_data = ReportingData {
                    tx_rt_data_new_order: new_order_data,
                };

                let data_str = serde_json::to_string(&reporting_data)
                    .expect("Unsupported reporting data format");
                let data_file_name = "data.json";
                fs::write(&data_file_name, &data_str)
                    .expect(&format!("Error writing data file {}", &data_file_name));

                println!(
                    "New order tx frequency stats q90: {} mean: {} max: {}",
                    txsg.new_order.fqd.quantile(),
                    txsg.new_order.fqd.mean(),
                    txsg.new_order.fqd.max()
                );

                return;
            }
        }
    }
}
