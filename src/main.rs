use structopt::StructOpt;

use cli_gen::generator::gen_cfg;


#[derive(StructOpt, Debug)]
pub enum RunMode {
    /// Generate terminal configuration files
    Generate {
        /// List of warehouse IDs
        #[structopt(short, long)]
        warehouse_id_list: Vec<u32>,

        /// Generate configuration for this many terminals
        #[structopt(short, long)]
        terminal_count: u32,
    },
    /// Build test reports
    TestReport {
        #[structopt(short, long)]
        reports: Vec<String>,
    }
}

#[derive(StructOpt, Debug)]
pub struct Opt {
    /// Program run mode. Either 'Generate' or 'TestReports' are allowed
    #[structopt(subcommand)]  // Note that we mark a field as a subcommand
    pub mode: RunMode
}

fn main() {
    let opt = Opt::from_args();
    println!("{:?}", opt);
    match opt.mode {
        RunMode::Generate { warehouse_id_list, terminal_count } => {
            gen_cfg(warehouse_id_list, terminal_count);
        },
        RunMode::TestReport { reports } => {
            println!("TestReport not implemented");
        }
    }
}
