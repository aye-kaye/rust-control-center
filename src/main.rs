use std::time::Duration;

use humantime::parse_duration;
use structopt::StructOpt;

use glob::glob;

use self::util::parse_nums;

mod cfg;
mod generator;
mod reporting;
mod terminal;
mod util;

#[derive(StructOpt, Debug)]
pub enum RunMode {
    /// Generate terminal configuration files
    Generate {
        /// List of warehouse IDs. Can be a single value, a comma separated list or a range (both ends are included)
        /// Example: `-w 1..5` will generate configuration for five warehouses starting from 1
        #[structopt(short = "w", long, parse(try_from_str = parse_nums))]
        warehouse_id_list: Box<Vec<u32>>,

        /// Generate configuration for this many terminals
        #[structopt(short = "t", long)]
        terminal_count: u32,

        /// Number of transactions per terminal
        #[structopt(short = "x", long)]
        transaction_count: u32,
    },
    /// Build test reports
    TestReport {
        /// Glob pattern for consuming log files with INTERNAL csv format
        #[structopt(short, long)]
        log_files_glob: String,
        /// Begin of the measurement (steady) interval starting from the latest `time_started` value throughout the log files provided.
        /// Accepts values in a human readable format, e.g. `1m` or `1h 15m`
        #[structopt(short = "b", long, parse(try_from_str = parse_duration))]
        steady_begin_offset: Duration,
        /// Length of the measurement (steady) interval.
        /// Accepts values in a human readable format, e.g. `1m` or `1h 15m`
        #[structopt(short = "e", long, parse(try_from_str = parse_duration))]
        steady_length: Duration,
    },
    /// Generate sample log files
    SampleLogFiles {
        /// Generate sample log data files for this many terminals
        #[structopt(short = "t", long)]
        terminal_count: u32,
        /// Run this many iterations per terminal
        #[structopt(short, long)]
        iter_count: u32,
    },
}

#[derive(StructOpt, Debug)]
pub struct Opt {
    /// Program run mode. Either 'generate' or 'testreports' are allowed
    #[structopt(subcommand)]
    pub mode: RunMode,
}

fn main() {
    let opt = Opt::from_args();
    println!("{:?}", opt);
    match opt.mode {
        RunMode::Generate {
            warehouse_id_list,
            terminal_count,
            transaction_count,
        } => {
            generator::gen_cfg(*warehouse_id_list, terminal_count, transaction_count);
        }
        RunMode::TestReport {
            log_files_glob,
            steady_begin_offset,
            steady_length,
        } => {
            let mut log_files_paths: Vec<String> = Vec::new();
            for entry in glob(&log_files_glob).expect("Failed to read glob pattern") {
                match entry {
                    Ok(path) => {
                        if let Some(path_str) = path.to_str() {
                            log_files_paths.push(path_str.to_string())
                        }
                    }
                    Err(e) => println!("{:?}", e),
                }
            }
            reporting::build_reports(&log_files_paths, steady_begin_offset, steady_length);
        }
        RunMode::SampleLogFiles {
            terminal_count,
            iter_count,
        } => {
            generator::gen_sample_data(terminal_count, iter_count);
        }
    }
}
