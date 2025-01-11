use std::fs;

use bytesize::ByteSize;
use clap::{Parser, Subcommand};
use fs_extra::dir::get_size;
#[derive(Parser)]
#[command(author,version,about,long_about = None)]
struct Args {
    path: String,
}
struct Entry {
    path: String,
    size: u64,
    is_dir: bool,
}
fn main() {
    let args = Args::parse();
    let paths = fs::read_dir(args.path.clone()).unwrap();
    let mut entries: Vec<Entry> = Vec::new();
    for path in paths {
        if let Ok(entry) = path {
            if entry.path().is_dir() {
                if let Ok(folder_size) = get_size(format!("{}", entry.path().display())) {
                    entries.push(Entry {
                        is_dir: true,
                        size: folder_size,
                        path: format!("{}", entry.path().display()),
                    });
                }
            } else {
                let file_size = fs::metadata(format!("{}", entry.path().display()))
                    .unwrap()
                    .len();
                entries.push(Entry {
                    is_dir: false,
                    size: file_size,
                    path: format!("{}", entry.path().display()),
                });
            }
        }
    }
    entries.sort_by(|a, b| b.size.cmp(&a.size));
    let mut total_size: u64 = 0;
    for entry in entries {
        total_size += entry.size;
        if entry.is_dir {
            println!("{}: {}", entry.path, ByteSize(entry.size));
        } else {
            println!("{}: {}", entry.path, ByteSize(entry.size));
        }
    }
    if let Ok(folder_size) = get_size(args.path.clone()) {
        println!("{}: {}", ByteSize(folder_size), args.path);
    } else {
        println!("{}: {}", ByteSize(total_size), args.path);
    }
}
