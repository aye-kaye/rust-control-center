use structopt::StructOpt;

mod cfg;
mod generator;
mod terminal;

#[derive(StructOpt, Debug)]
pub enum RunMode {
    /// Generate terminal configuration files
    Generate {
        /// List of warehouse IDs
        #[structopt(short = "w", long)]
        warehouse_id_list: Vec<u32>,

        /// Generate configuration for this many terminals
        #[structopt(short = "t", long)]
        terminal_count: u32,

        /// Number of transactions per terminal
        #[structopt(short = "x", long)]
        transaction_count: u32,
    },
    /// Build test reports
    TestReport {
        #[structopt(short, long)]
        reports: Vec<String>,
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
            generator::gen_cfg(warehouse_id_list, terminal_count, transaction_count);
        }
        RunMode::TestReport { reports } => {
            println!("TestReport not implemented");
        }
        RunMode::SampleLogFiles {
            terminal_count,
            iter_count,
        } => {
            generator::gen_sample_data(terminal_count, iter_count);
        }
    }
}
