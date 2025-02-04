use clap::{Parser, Subcommand};
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::{fs, io};
use zstd::DEFAULT_COMPRESSION_LEVEL;

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
        if !(a[1].starts_with("-")
            || a[1] == "r"
            || a[1] == "res"
            || a[1] == "restore"
            || a[1] == "b"
            || a[1] == "bak"
            || a[1] == "backup")
        {
            let slice = &["".to_string()];
            a.splice(1..1, slice.iter().cloned());
        }
        cli = Cli::parse_from(a.iter());
        cli.command.unwrap()
    };

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
                } else if path.is_file() {
                    backup_file(&path, compress)
                } else {
                    panic!("this is neither a file nor a directory, don't know what to do")
                };

                if let Err(e) = result {
                    eprintln!("Error backing up {:?}: {}", path, e);
                }
            }
        }
        Commands::Restore { path, delete } => {
            println!("Restoring from {:?}", path);
            restore(&path)?;
            if delete && (cli.confirm || confirm(format!("delete {}?", path.display()))?) {
                fs::remove_dir_all(path)?;
            }
        }
    }

    Ok(())
}

fn confirm(prompt: String) -> io::Result<bool> {
    print!("{prompt} - y/N");
    io::stdout().flush()?;
    let mut buf = String::new();
    loop {
        io::stdin().read_line(&mut buf)?;
        match buf.to_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "" | "n" | "no" => return Ok(false),
            _ => println!("That is neither yes or no"),
        }
    }
}

fn add_extension(path: &Path, postfix: &str) -> PathBuf {
    // TODO: do something if the same extension already exists
    let parts = [
        path.file_name()
            .expect("this string is weird, no file name"),
        OsStr::new(postfix),
    ];
    let newname: OsString = parts.iter().copied().collect();
    path.with_file_name(newname)
}

fn remove_extension(path: &Path, suffix: &str) -> PathBuf {
    let r = path.display().to_string();
    match r.strip_suffix(&format!(".{suffix}")) {
        None => panic!("that path did not have that suffix"),
        Some(short) => PathBuf::from(short),
    }
}

fn restore(path: &Path) -> io::Result<()> {
    let path_s: String = path.display().to_string();
    if path_s.ends_with("tar.zstd") | path_s.ends_with("tar.zst") {
        if !path.is_file() {
            panic!("archive name but not an archive")
        }

        let target = if let Some(p) = path.parent() {
            p
        } else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("path has no parent path: {}", path_s),
            ));
        };

        read_archive(path, |a| a.unpack(target))?;
        Ok(())
    } else if path_s.ends_with("bak") {
        if !path.is_file() {
            panic!("bak name but not a file")
        }

        let target = remove_extension(path, "bak");
        fs::copy(path, target)?;
        Ok(())
    } else if path_s.ends_with("bak.d") {
        if path.is_file() {
            panic!("bak.d name but not a directory")
        }
        let target = remove_extension(path, "bak.d");
        copy_dir_all(path, &target)?;
        Ok(())
    } else {
        panic!("unknown file {}", path_s)
    }
}

fn backup_file(path: &Path, compress: bool) -> io::Result<PathBuf> {
    if compress {
        let archive_path = add_extension(path, ".tar.zstd");
        make_archive(&archive_path, |a| a.append_path(path))?;
        Ok(archive_path)
    } else {
        let backup_path = add_extension(path, ".bak");
        fs::copy(path, &backup_path)?;
        Ok(backup_path)
    }
}

fn backup_dir(path: &Path, compress: bool) -> io::Result<PathBuf> {
    if compress {
        let archive_path = add_extension(path, ".tar.zstd");
        make_archive(&archive_path, |a| a.append_dir_all("", path))?;
        Ok(archive_path)
    } else {
        let backup_path = add_extension(path, ".bak.d");
        copy_dir_all(path, &backup_path)?;
        Ok(backup_path)
    }
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
        } else {
            eprintln!(
                "neither a file nor a directory, skipping: {}",
                entry.path().display()
            );
        }
    }
    Ok(())
}

fn make_archive<F>(archive_path: &Path, do_this: F) -> std::io::Result<()>
where
    F: FnOnce(
        &mut tar::Builder<zstd::stream::AutoFinishEncoder<std::fs::File>>,
    ) -> std::io::Result<()>,
{
    let compressed_file = fs::File::create(archive_path)?;

    let compressor = zstd::Encoder::new(compressed_file, DEFAULT_COMPRESSION_LEVEL)?.auto_finish();
    let mut archiver = tar::Builder::new(compressor);

    do_this(&mut archiver)?;

    archiver.finish()?;

    Ok(())
}

fn read_archive<F>(archive_path: &Path, do_this: F) -> std::io::Result<()>
where
    F: FnOnce(
        &mut tar::Archive<zstd::Decoder<'_, std::io::BufReader<std::fs::File>>>,
    ) -> std::io::Result<()>,
{
    let compressed_file = fs::File::open(archive_path)?;

    let decompressor = zstd::Decoder::new(compressed_file)?;
    let mut unarchiver = tar::Archive::new(decompressor);

    do_this(&mut unarchiver)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::MetadataExt;
    use std::path::{Path, PathBuf};
    use std::{fs, io};

    use serial_test::serial;
    use tempfile::tempdir;

    use crate::{backup_dir, backup_file, make_archive, read_archive, restore};

    const CONTENT: &[u8] = b"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

    fn filesize(p: &Path) -> io::Result<u64> {
        Ok(fs::metadata(p)?.size())
    }

    #[test]
    #[serial]
    fn test_make_archive() -> io::Result<()> {
        let t = tempdir()?;
        let tdir = t.path();
        std::env::set_current_dir(tdir).unwrap(); // NOTE: if multiple tests use this, this
                                                  // creates a race condition
        let tfile = PathBuf::from("foo");
        let tfile_a = PathBuf::from("foo.tar.zstd");

        fs::write(&tfile, CONTENT).unwrap();
        assert!(tfile.exists());
        assert!(tfile.is_file());
        assert_eq!(fs::read(&tfile).unwrap(), CONTENT);
        let raw_size = fs::metadata(&tfile).unwrap().size();
        assert!(raw_size > 1, "raw size was {raw_size}");

        // NOTE: append_path needs a relative path
        make_archive(&tfile_a, |a| a.append_path(&tfile)).unwrap();
        assert!(tfile_a.exists());
        assert!(tfile_a.is_file());
        let arch_size = fs::metadata(&tfile_a).unwrap().size();
        assert!(arch_size > 1, "archive size was {arch_size}");

        fs::remove_file(&tfile).unwrap();
        assert!(!tfile.exists());

        read_archive(&tfile_a, |a| a.unpack(tdir)).unwrap();
        assert!(tfile.exists());
        assert!(!tfile.is_dir());
        assert!(tfile.is_file());
        let copy_size = fs::metadata(&tfile).unwrap().size();
        assert!(copy_size > 1, "archive size was {arch_size}");

        let copy_content = fs::read(&tfile).unwrap();
        assert_eq!(CONTENT, copy_content);

        Ok(())
    }

    #[test]
    fn test_simple_bak_restore() -> io::Result<()> {
        let t = tempdir()?;
        let tdir = t.path();
        let tfile = tdir.join("foo");
        let tfile_b = tdir.join("foo.bak");

        fs::write(&tfile, CONTENT).unwrap();
        assert!(tfile.exists());
        assert!(tfile.is_file());
        assert_eq!(fs::read(&tfile).unwrap(), CONTENT);
        let raw_size = filesize(&tfile)?;
        assert!(raw_size > 1, "raw size was {raw_size}");

        backup_file(&tfile, false).unwrap();

        assert!(tfile_b.exists());
        assert!(tfile_b.is_file());
        assert_eq!(fs::read(&tfile_b).unwrap(), CONTENT);
        let raw_size = filesize(&tfile)?;
        assert!(raw_size > 1, "raw size was {raw_size}");

        fs::remove_file(&tfile).unwrap();
        assert!(!tfile.exists());

        restore(&tfile_b).unwrap();

        assert!(tfile.exists());
        assert!(tfile.is_file());
        assert_eq!(fs::read(&tfile).unwrap(), CONTENT);
        let raw_size = filesize(&tfile)?;
        assert!(raw_size > 1, "raw size was {raw_size}");

        Ok(())
    }

    #[test]
    fn test_dir_bak_restore() -> io::Result<()> {
        let t = tempdir()?;
        let tdir = t.path();
        let tdir_a = tdir.join("ichi");
        let tdir_b = tdir_a.join("ni");
        let dirs = [&tdir_a, &tdir_b];
        let names = ["foo", "bar", "qux"];
        fastrand::seed(133719);

        let mut contents: Vec<[u8; 16]> = vec![];
        for _ in 0..(dirs.len() * names.len()) {
            contents.push(fastrand::u128(0..u128::MAX).to_le_bytes());
        }

        let mut i = 0;
        for sdir in dirs {
            fs::create_dir_all(sdir)?;
            assert!(sdir.exists());
            assert!(sdir.is_dir());
            for fname in names {
                let p = sdir.join(fname);
                fs::write(&p, contents[i])?;
                assert!(p.exists());
                assert!(p.is_file());
                assert!(p.is_file());
                let raw_size = filesize(&p)?;
                assert!(raw_size > 1, "raw size of {} was {raw_size}", p.display());
                i += 1;
            }
        }

        let backup = backup_dir(&tdir_a, false)?;
        dbg!(&tdir_a);
        dbg!(fs::metadata(&tdir_a)?);
        fs::remove_dir_all(&tdir_a)?;
        dbg!(&backup);
        dbg!(fs::metadata(&backup)?);
        restore(&backup)?;
        dbg!(&tdir_a);
        dbg!(fs::metadata(&tdir_a)?);

        let mut i = 0;
        for sdir in dirs {
            assert!(sdir.exists());
            assert!(sdir.is_dir());
            for fname in names {
                let p = sdir.join(fname);
                assert!(p.exists());
                assert!(p.is_file());
                let actual = fs::read(&p)?;
                assert_eq!(actual, contents[i]);
                i += 1;
            }
        }

        Ok(())
    }
}
