use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "nolgia", version, about = "Nolgia CLI")]
struct Cli {
    #[arg(long)]
    verbose: bool,
}

fn main() {
    let cli = Cli::parse();
    if cli.verbose {
        println!("nolgia running in verbose mode");
    }
}
