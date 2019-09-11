use structopt::StructOpt;


#[derive(StructOpt, Debug)]
pub enum RunMode2 {
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
struct Opt {
    /// Program run mode. Either 'Generate' or 'TestReports' are allowed
    #[structopt(subcommand)]  // Note that we mark a field as a subcommand
    mode: RunMode2
}

fn main() {
    let opt = Opt::from_args();
    println!("{:?}", opt);
}
