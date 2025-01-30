use clap::{Parser, Subcommand};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::{fs, io};

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
    about = "Simple local backups with a bit of compression",
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

#[derive(Debug, Subcommand)]
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

fn help_and_exit() -> ! {
    use clap::CommandFactory;
    let mut cmd = Cli::command();
    cmd.print_help().expect("could not print");
    std::process::exit(1)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli;
    let command = {
        let mut a: Vec<String> = std::env::args().collect();
        if a.len() < 2 {
            help_and_exit()
        }
        if !a[1].starts_with("-")
            || a[1] == "r"
            || a[1] == "res"
            || a[1] == "restore"
            || a[1] == "b"
            || a[1] == "bak"
            || a[1] == "backup"
        {
            let slice = &["".to_string()];
            a.splice(1..1, slice.iter().cloned());
        }
        cli = Cli::parse_from(a.iter());
        cli.command.unwrap()
    };
    dbg!(&command);

    match command {
        Commands::Backup { paths, compress } => {
            if paths.is_empty() {
                help_and_exit()
            }
            for path in paths {
                if !path.exists() {
                    eprintln!("Error: {:?} does not exist", path);
                    continue;
                }

                let result = if path.is_dir() {
                    backup_dir(&path, compress)
                } else {
                    backup_file(&path, compress)
                };

                if let Err(e) = result {
                    eprintln!("Error backing up {:?}: {}", path, e);
                }
            }
        }
        Commands::Restore { path, delete } => {
            println!("Restoring from {:?}", path);
            if delete {
                println!("Will remove backup after restore");
            }
        }
    }

    Ok(())
}

fn add_extension(path: &Path, postfix: &str) -> PathBuf {
    let parts = [
        path.file_name()
            .expect("this string is weird, no file name"),
        OsStr::new(postfix),
    ];
    let newname: OsString = parts.iter().copied().collect();
    path.with_file_name(newname)
}

fn backup_file(path: &Path, compress: bool) -> io::Result<()> {
    if compress {
        compress_file(path)?;
    } else {
        let backup_path = add_extension(path, ".bak");
        fs::copy(path, backup_path)?;
    }
    Ok(())
}

fn backup_dir(path: &Path, compress: bool) -> io::Result<()> {
    if compress {
        compress_file(path)?;
    } else {
        let backup_path = add_extension(path, ".bak.d");
        copy_dir_all(path, &backup_path)?;
    }
    Ok(())
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_all(&entry.path(), &dst_path)?;
        } else if ty.is_file() {
            fs::copy(entry.path(), dst_path)?;
        }
        // Ignore other types like symlinks for simplicity
    }
    Ok(())
}

fn compress_file(path: &Path) -> io::Result<()> {
    let file = fs::File::open(path)?;
    let compressed_path = add_extension(path, ".tar.zstd");
    let compressed_file = fs::File::create(compressed_path)?;

    let mut encoder = zstd::Encoder::new(compressed_file, 3)?;
    io::copy(&mut io::BufReader::new(file), &mut encoder)?;
    encoder.finish()?;

    Ok(())
}
