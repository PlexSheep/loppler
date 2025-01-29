use clap::{Parser, Subcommand};
use std::path::PathBuf;

const HELP_TEMPLATE: &str = r"{about-section}
{usage-heading} {usage}

{all-args}{tab}

{name}: {version}
Author: {author-with-newline}
";

#[derive(Parser)]
#[command(
    author = env!("CARGO_PKG_AUTHORS"),
    version = env!("CARGO_PKG_VERSION"),
    about = "Simple local backups with a bit of compression magic",
    help_template = HELP_TEMPLATE
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Do not confirm
    #[clap(short = 'y', long = "yes", global = true)]
    confirm: bool,

    /// Print out every action
    #[clap(short = 'v', long = "verbose", global = true)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Create backup of files or directories, default action
    #[clap(alias = "")]
    #[clap(visible_alias = "b")]
    #[clap(visible_alias = "bak")]
    Backup {
        /// Files or directories to backup
        paths: Vec<PathBuf>,

        /// Use zstd compression
        #[arg(short = 'z', long)]
        compress: bool,
    },

    /// Restore from backup
    #[clap(visible_alias = "r")]
    #[clap(visible_alias = "res")]
    Restore {
        /// Backup file to restore from
        path: PathBuf,

        /// Delete backup after successful restore
        #[arg(short = 'd', long)]
        delete: bool,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let command = {
        let mut a: Vec<String> = std::env::args().collect();
        if a.len() < 2 || a[1].contains('-') {
            let slice = &["".to_string()];
            a.splice(1..1, slice.iter().cloned());
        }
        let cli = Cli::parse_from(a.iter());
        cli.command.unwrap()
    };

    match command {
        Commands::Backup { paths, compress } => {
            if paths.is_empty() {
                println!("No paths specified for backup");
                return Ok(());
            }
            for path in paths {
                if compress {
                    println!("Compressing {:?} to .tar.zstd", path);
                } else {
                    println!("Backing up {:?} to .bak", path);
                }
            }
        }
        Commands::Restore {
            path,
            delete: remove,
        } => {
            println!("Restoring from {:?}", path);
            if remove {
                println!("Will remove backup after restore");
            }
        }
    }

    Ok(())
}
