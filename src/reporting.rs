use std::{fs, io, thread};
use std::cell::{RefCell, RefMut};
use std::cmp::*;
use std::collections::HashMap;
use std::error::Error;
use std::ffi::OsStr;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::iter::Enumerate;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier, Mutex};
use std::time::Duration;

use average::{Estimate, Max, Mean, Quantile};
use chrono::{DateTime, Local};
use crossbeam_channel::unbounded;
use crossbeam_deque::{Steal, Worker};
use hdrhistogram::Histogram;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use structopt::StructOpt;

use glob::glob;

use crate::cfg::*;
use crate::cfg::TransactionType::*;
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

arg_enum! {
    /// Report building mode. Either 'New' or 'Append' modes are supported
    #[derive(StructOpt, Debug)]
    pub enum ReportMode {
        New,
        Append
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ReportingData {
    tx_data: Box<Vec<TransactionData>>,
    total_tx_data: Box<ThroughputData>,
    tx_rt_tpm_series: Vec<[u64; 2]>,
    total_tpmc: u64,
    total_tx_count: u64,
    terminal_count: usize,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TransactionData {
    tx_type: TransactionType,
    tx_rt_data: TxRtData,
    throughput_data: ThroughputData,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TxRtData {
    tx_rt_p90: u64,
    tx_rt_mean: u64,
    tx_rt_max: u64,
    tx_rt_high: u64,
    tx_rt_tx_count: u64,
    tt_mean: u64,
    tx_rt_series: Vec<[u64; 2]>,
    tt_series: Vec<[u64; 2]>,
    tpmc: u64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ThroughputData {
    steady_begin_time: u64,
    steady_end_time: u64,
    tpm_series: Vec<[u64; 2]>,
    tx_count_series: Vec<[u64; 2]>,
}

struct TxStatsNewOrderContainer {
    tx_type: TransactionType,
    tx_rt_histo: Histogram<u64>,
    tt_histo: Histogram<u64>,
    //TODO response time vs tpmc
    tx_cnt_histo: Histogram<u64>,
    steady_count: u64,
}

impl TxStatsNewOrderContainer {
    fn new(transaction_type: &TransactionType) -> Self {
        TxStatsNewOrderContainer {
            tx_type: transaction_type.clone(),
            tx_rt_histo: Histogram::<u64>::new(5).unwrap(),
            tt_histo: Histogram::<u64>::new(5).unwrap(),
            tx_cnt_histo: Histogram::<u64>::new(5).unwrap(),
            steady_count: 0,
        }
    }

    fn record_tx_rt(&mut self, value: u64) -> () {
        self.tx_rt_histo.record(value);
    }

    fn record_tt(&mut self, value: u64) -> () {
        self.tt_histo.record(value);
    }

    fn record_tx_cnt(&mut self, value: u64) -> () {
        self.tx_cnt_histo.record(value);
    }

    fn record_steady(&mut self) -> () {
        self.steady_count += 1;
    }
}

pub const TX_RT_INTERVAL_COUNT: u64 = 20;
pub const TT_INTERVAL_COUNT: u64 = 20;
pub const PERCENTILE_90: f64 = 90.;
pub const DATA_VAR_PREFIX: &str = "var data=";
pub const DATA_FILE_NAME: &str = "data.js";
pub const REPORT_FILE_NAME: &str = "report.html";

const TX_SAMPLING_INTERVAL_SEC: u64 = 1;
const TX_SAMPLING_INTERVAL_MSEC: u64 = TX_SAMPLING_INTERVAL_SEC * 100;
const TX_SAMPLING_INTERVAL_MSEC_F: f64 = TX_SAMPLING_INTERVAL_MSEC as f64;

const TX_COUNT_SAMPLING_INTERVAL_SEC: u64 = 30;
const TX_COUNT_SAMPLING_INTERVAL_MSEC: u64 = TX_COUNT_SAMPLING_INTERVAL_SEC * 1000;
const TX_COUNT_SAMPLING_INTERVAL_MSEC_F: f64 = TX_COUNT_SAMPLING_INTERVAL_MSEC as f64;

const TPM_SAMPLING_INTERVAL_SEC: u64 = 60;
const TPM_SAMPLING_INTERVAL_MSEC: u64 = TPM_SAMPLING_INTERVAL_SEC * 1000;

pub fn analyze_term_group(
    paths: &Vec<String>,
    steady_begin_offset: Duration,
    steady_length: Duration,
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
        .for_each(drop);
    barrier.wait();
    let lfv = log_files_valid.lock().unwrap().clone();
    let mut est = earliest_start_time_ms.lock().unwrap();
    let lst = latest_start_time_ms.lock().unwrap();
    if let Ordering::Greater = est.cmp(&lst) {
        *est = lst.clone();
    }
    Ok(TermGroupParams {
        term_count: lfv.len() as u32,
        earliest_start_time_ms: *est,
        latest_start_time_ms: *lst,
        log_files_valid: lfv,
        steady_begin_time_ms: *lst + steady_begin_offset.as_millis() as u64,
        steady_end_time_ms: *lst + steady_length.as_millis() as u64,
    })
}

pub fn build_reports(
    paths: &Vec<String>,
    steady_begin_offset: Duration,
    steady_length: Duration,
    report_mode: ReportMode,
    report_path: Option<String>,
) {
    if let ReportMode::Append = report_mode {
        if report_path.is_none() {
            panic!("report-mode 'Append' requires non-empty path")
        } else if ![&report_path.clone().unwrap(), DATA_FILE_NAME]
            .iter()
            .collect::<PathBuf>()
            .exists()
        {
            panic!("Report path or data file (data.js) does not exist")
        }
    }

    let now: DateTime<Local> = Local::now();
    let start_ts = now.format("%Y%m%d_%H%M%S");

    let final_report_path = report_path.clone().unwrap_or(String::from(
        ["test-reports", &format!("{}", &start_ts)]
            .iter()
            .collect::<PathBuf>()
            .to_str()
            .unwrap(),
    ));

    let group_params = analyze_term_group(paths, steady_begin_offset, steady_length).unwrap();

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
                    let mut record: TermLogRecord;
                    for result in rdr.deserialize() {
                        record = result.unwrap();
                        sr.send(record).unwrap();
                    }
                }
                b.wait();
            });
        })
        .for_each(drop);

    thread::spawn(move || {
        barrier.clone().wait();
        drop(sender);
    });

    let earliest_start_time_ms: &u64 = &group_params.earliest_start_time_ms;
    let latest_start_time_ms: &u64 = &group_params.latest_start_time_ms;
    let steady_begin_time_ms: &u64 = &group_params.steady_begin_time_ms;
    let steady_end_time_ms: &u64 = &group_params.steady_end_time_ms;
    let steady_legth_ms: u64 = match steady_begin_time_ms.cmp(steady_end_time_ms) {
        Ordering::Greater => 0,
        _ => *steady_end_time_ms - *steady_begin_time_ms,
    };

    let mut total_tpmc = 0;
    let mut total_tx_count = 0;

    let mut txsg: HashMap<&TransactionType, RefCell<TxStatsNewOrderContainer>> = HashMap::new();
    TransactionType::iter().for_each(|tx_type| {
        txsg.insert(
            &tx_type,
            RefCell::new(TxStatsNewOrderContainer::new(&tx_type)),
        );
    });

    loop {
        match receiver.recv() {
            Ok(record) => {
                let cycle_start_time = record.time_started;
                //TODO суммировать остальные части: keying_time + menu_time
                let cycle_finish_time =
//                    cycle_start_time + record.running_time as u64 + record.think_time_ms as u64;
                    cycle_start_time + record.tx_running_time as u64 + record.think_time_ms as u64;
                let mut is_steady = false;

                let mut tx_stats_container: RefMut<TxStatsNewOrderContainer> =
                    txsg.get(&record.typ).unwrap().borrow_mut();

                // Gathering metrics within steady interval
                if cycle_start_time >= *steady_begin_time_ms
                    && cycle_start_time < *steady_end_time_ms
                    && cycle_finish_time >= *steady_begin_time_ms
                    && cycle_finish_time < *steady_end_time_ms
                {
                    is_steady = true;
                    let tx_interval_value =
                        libm::ceil(record.tx_running_time as f64 / TX_SAMPLING_INTERVAL_MSEC_F)
                            as u64
                            * TX_SAMPLING_INTERVAL_MSEC;
                    tx_stats_container.record_tx_rt(tx_interval_value);

                    let tt_interval_value =
                        libm::ceil(record.think_time_ms as f64 / TX_SAMPLING_INTERVAL_MSEC_F)
                            as u64
                            * TX_SAMPLING_INTERVAL_MSEC;
                    tx_stats_container.record_tt(tt_interval_value);

                    tx_stats_container.record_steady();
                }

                let tx_cnt_interval_num = libm::ceil(
                    (cycle_finish_time - *earliest_start_time_ms) as f64
                        / TX_COUNT_SAMPLING_INTERVAL_MSEC_F,
                ) as u64
                    * TX_COUNT_SAMPLING_INTERVAL_MSEC;

                tx_stats_container.record_tx_cnt(tx_cnt_interval_num);
            }
            Err(RecvError) => {
                println!("Done reading files");

                // All transaction types statistical variables
                let total_running_time_ms =
                    txsg.get(&NewOrder).unwrap().borrow_mut().tx_cnt_histo.max();
                let tx_count_interval_count =
                    total_running_time_ms / TX_COUNT_SAMPLING_INTERVAL_MSEC;
                let steady_begin_time = *steady_begin_time_ms - *earliest_start_time_ms;
                let steady_end_time = *steady_end_time_ms - *earliest_start_time_ms;

                // NewOrder transaction' parameters define Tx_Runtime graph scales
                let tx_rt_1x = txsg
                    .get(&NewOrder)
                    .unwrap()
                    .borrow_mut()
                    .tx_rt_histo
                    .value_at_percentile(PERCENTILE_90);
                let tx_rt_4x = tx_rt_1x * 4;
                let tx_rt_interval_size = tx_rt_4x / TX_RT_INTERVAL_COUNT;

                // and Think_Time graph scales
                let tt_1x = txsg.get(&NewOrder).unwrap().borrow_mut().tt_histo.mean() as u64;
                let tt_4x = tt_1x * 4;
                let tt_interval_size = tt_4x / TT_INTERVAL_COUNT;

                // and Throughput variables
                let mut tx_rt_tpm_series: Vec<[u64; 2]> = Vec::new();
                let mut curr_tpmc: u64 = 0;
                {
                    let curr_tx_cont_ref = txsg.get(&TransactionType::NewOrder).unwrap();
                    let curr_tx_cont = curr_tx_cont_ref.borrow_mut();
                    let curr_steady_count = curr_tx_cont.steady_count.clone();
                    curr_tpmc = calculate_tpmc(&steady_legth_ms, &curr_steady_count);
                }

                if let ReportMode::Append = report_mode {
                    let prev_reporting_data_path: PathBuf =
                        [&report_path.clone().unwrap(), DATA_FILE_NAME]
                            .iter()
                            .collect();
                    let mut prev_data_file =
                        File::open(&prev_reporting_data_path).expect(&format!(
                            "Error opening previous report data file {:?}",
                            prev_reporting_data_path
                        ));
                    let mut prev_reporting_data_full = String::new();
                    prev_data_file
                        .read_to_string(&mut prev_reporting_data_full)
                        .expect(&format!(
                            "Error reading previous report data file {:?}",
                            prev_reporting_data_path
                        ));
                    let prev_reporting_data_str =
                        &prev_reporting_data_full[DATA_VAR_PREFIX.len()..];
                    let prev_reporting_data: ReportingData =
                        serde_json::from_str(prev_reporting_data_str).unwrap();

                    let prev_tx_rt_tpm_series = &prev_reporting_data.tx_rt_tpm_series;
                    // Скопировать старые значения
                    prev_tx_rt_tpm_series
                        .iter()
                        .for_each(|pair| tx_rt_tpm_series.push([pair[0].clone(), pair[1].clone()]));

                    let prev_max = prev_tx_rt_tpm_series
                        .iter()
                        .find(|pair| pair[0] > curr_tpmc as u64);
                    // Старый файл уже содержит значение больше текущего tpmC, надо просто дописать в него текущее tpmC
                    if let Some(pair) = prev_max {
                        println!("Appending a new tpmC reading");
                        tx_rt_tpm_series.push([curr_tpmc, tx_rt_1x]);

                        let new_reporting_data = ReportingData {
                            tx_data: prev_reporting_data.tx_data,
                            total_tx_data: prev_reporting_data.total_tx_data,
                            tx_rt_tpm_series,
                            total_tpmc: prev_reporting_data.total_tpmc,
                            total_tx_count: prev_reporting_data.total_tx_count,
                            terminal_count: prev_reporting_data.terminal_count,
                        };
                        write_report_file(&final_report_path, &new_reporting_data);
                        return;
                    } else {
                        println!("Rebuilding a report respecting the previous tpmC readings");
                    }
                }

                tx_rt_tpm_series.push([curr_tpmc, tx_rt_1x]);

                let mut tx_data: Vec<TransactionData> = Vec::new();
                let mut total_tpm_map: HashMap<u64, u64> = HashMap::new();
                let mut total_tx_count_map: HashMap<u64, u64> = HashMap::new();

                TransactionType::iter().for_each(|tx_type| {
                    let tx_cont_ref = txsg.get(tx_type).unwrap();
                    let tx_cont = tx_cont_ref.borrow_mut();
                    let tx_rt_histo = &tx_cont.tx_rt_histo;
                    let tx_cnt_histo = &tx_cont.tx_cnt_histo;
                    let tt_histo = &tx_cont.tt_histo;
                    let steady_count = tx_cont.steady_count;

                    let mut tx_rt_high = 0;
                    let tx_rt_series: Vec<[u64; 2]> = (1..TX_RT_INTERVAL_COUNT + 1)
                        .map(|i| {
                            let value = i * tx_rt_interval_size;
                            let count =
                                tx_rt_histo.count_between((i - 1) * tx_rt_interval_size + 1, value);
                            tx_rt_high = max(tx_rt_high, count);

                            [value, count]
                        })
                        .collect();
                    let tt_series: Vec<[u64; 2]> = (1..TT_INTERVAL_COUNT + 1)
                        .map(|i| {
                            let value = i * tt_interval_size;
                            let count =
                                tt_histo.count_between((i - 1) * tt_interval_size + 1, value);

                            [value, count]
                        })
                        .collect();

                    let mut tx_count_series: Vec<[u64; 2]> = Vec::new();
                    let tpm_series = (1..tx_count_interval_count + 1)
                        .map(|i| {
                            let value = i * TX_COUNT_SAMPLING_INTERVAL_MSEC;
                            let lower_tpm_bound = match value.cmp(&TPM_SAMPLING_INTERVAL_MSEC) {
                                Ordering::Greater => value - TPM_SAMPLING_INTERVAL_MSEC + 1,
                                _ => 1,
                            };
                            let count = tx_cnt_histo.count_between(lower_tpm_bound, value);
                            let count_at = tx_cnt_histo.count_at(value);

                            let mut total_tpm_at = total_tpm_map.entry(value).or_insert(0);
                            *total_tpm_at += count;
                            let total_count_at = total_tx_count_map.entry(value).or_insert(0);
                            *total_count_at += count_at;

                            tx_count_series.push([value, count_at]);
                            [value, count]
                        })
                        .collect();

                    let tpmc = calculate_tpmc(&steady_legth_ms, &steady_count);

                    tx_data.push(TransactionData {
                        tx_type: tx_type.clone(),
                        tx_rt_data: TxRtData {
                            tx_rt_p90: tx_rt_histo.value_at_percentile(PERCENTILE_90),
                            tx_rt_mean: tx_rt_histo.mean() as u64,
                            tx_rt_max: tx_rt_histo.max() as u64,
                            tx_rt_high,
                            tx_rt_tx_count: steady_count,
                            tt_mean: tt_histo.mean() as u64,
                            tx_rt_series,
                            tt_series,
                            tpmc: tpmc as u64,
                        },
                        throughput_data: ThroughputData {
                            steady_begin_time,
                            steady_end_time,
                            tpm_series,
                            tx_count_series,
                        },
                    });

                    total_tx_count += steady_count;
                }); // End of TransactionType loop

                let mut total_tx_data: ThroughputData = ThroughputData {
                    steady_begin_time,
                    steady_end_time,
                    tpm_series: total_tpm_map
                        .iter()
                        .sorted_by_key(|e| e.0)
                        .map(|(k, v)| [*k, *v])
                        .collect::<Vec<[u64; 2]>>(),
                    tx_count_series: total_tx_count_map
                        .iter()
                        .sorted_by_key(|e| e.0)
                        .map(|(k, v)| [*k, *v])
                        .collect::<Vec<[u64; 2]>>(),
                };

                total_tpmc = match steady_legth_ms.cmp(&TPM_SAMPLING_INTERVAL_MSEC) {
                    Ordering::Greater => {
                        total_tx_count as f64
                            / (steady_legth_ms as f64 / TPM_SAMPLING_INTERVAL_MSEC as f64)
                    }
                    _ => total_tx_count as f64 / TPM_SAMPLING_INTERVAL_SEC as f64,
                } as u64;

                let reporting_data = ReportingData {
                    tx_data: Box::new(tx_data),
                    total_tx_data: Box::new(total_tx_data),
                    tx_rt_tpm_series,
                    total_tpmc,
                    total_tx_count,
                    terminal_count: group_params.log_files_valid.len(),
                };

                write_report_file(&final_report_path, &reporting_data);
                copy_assets(&final_report_path);

                println!(
                    "Total running time {}",
                    humantime::format_duration(Duration::new(total_running_time_ms / 1000, 0))
                );

                return;
            }
        }
    }

    fn calculate_tpmc(steady_length_ms: &u64, tx_count: &u64) -> u64 {
        return match steady_length_ms.cmp(&TPM_SAMPLING_INTERVAL_MSEC) {
            Ordering::Greater => {
                *tx_count as f64 / (*steady_length_ms as f64 / TPM_SAMPLING_INTERVAL_MSEC as f64)
            }
            _ => *tx_count as f64 / TPM_SAMPLING_INTERVAL_SEC as f64,
        } as u64;
    }

    fn write_report_file(report_path: &str, reporting_data: &ReportingData) {
        fs::create_dir_all(&report_path).expect(&format!(
            "Error creating test report directory {:?}",
            &report_path
        ));
        let data_str =
            serde_json::to_string(&reporting_data).expect("Unsupported reporting data format");
        let data_file_path = [&report_path, DATA_FILE_NAME].iter().collect::<PathBuf>();
        let mut data_file = File::create(&data_file_path)
            .expect(&format!("Error creating data file {:?}", &data_file_path));
        let data_file_err_msg = format!("Error writing data file {:?}", &data_file_path);
        data_file
            .write(DATA_VAR_PREFIX.as_bytes())
            .expect(&data_file_err_msg);
        data_file
            .write(data_str.as_bytes())
            .expect(&data_file_err_msg);
    }

    fn copy_assets(report_path: &str) {
        fs::create_dir_all(&report_path).expect(&format!(
            "Error creating test report directory {:?}",
            &report_path
        ));
        let copy_err_msg = "Error copying an asset file";
        ["css", "js"].iter().for_each(|dir| {
            let dir_path = ["assets", *dir, "*.*"].iter().collect::<PathBuf>();
            let mut dir_target_path = [report_path, *dir].iter().collect::<PathBuf>();
            fs::create_dir_all(&dir_target_path).expect(&format!(
                "Error creating asset directory {:?}",
                &dir_target_path
            ));
            for entry in glob(dir_path.to_str().unwrap())
                .unwrap()
                .filter_map(Result::ok)
            {
                let file_name: &OsStr = entry.file_name().unwrap();
                dir_target_path.push(file_name);
                fs::copy(entry.to_str().unwrap(), dir_target_path.to_str().unwrap())
                    .expect(copy_err_msg);
                dir_target_path.pop();
            }
        });
        fs::copy(
            ["assets", "html", REPORT_FILE_NAME]
                .iter()
                .collect::<PathBuf>()
                .to_str()
                .unwrap(),
            [report_path, REPORT_FILE_NAME]
                .iter()
                .collect::<PathBuf>()
                .to_str()
                .unwrap(),
        )
        .expect(copy_err_msg);
    }
}
