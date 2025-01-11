use std::fs;
use bytesize::ByteSize;
use clap::{Parser, Subcommand};
use fs_extra::dir::get_size;
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use sha2::{Sha256, Digest};
use walkdir::WalkDir;

use rayon::prelude::*;
use std::sync::Mutex;
use std::ffi::OsStr;
use std::hash::{Hash, Hasher, SipHasher};
use chrono::{DateTime, Utc};
use std::fs::{ DirEntry};
use chrono::Datelike; 
#[derive(Parser)]
#[command(name = "dhs", about = "Disk Cleanup utiliity")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(name = "ls")]
    ListSizes {
        #[arg(required = true, help = "Path for dir which sizes needs to be listed")]
        path: String,
    },
    #[command(name = "org")]
    OrganizeFiles {
        #[arg(required = true, help = "Path for dir which needs to be organized")]
        path: String,
    },
    #[command(name = "dump")]
    Dump {
        #[arg(required = true, help = "Source Directory")]
        source: String,
        #[arg(required = true, help = "Destination Directory")]
        destination: String,
    },
    #[command(name = "dedupe")]
    Dedupe {
        #[arg(required = true, help = "Path for dedupe directory")]
        path: String
    },
}

fn main() {
    let cli = Cli::parse();

    match &cli.command {
        Commands::ListSizes { path } => {
            list_sizes(path.as_str());
        }
        Commands::OrganizeFiles { path } => {
            organize_files(path.as_str());
        }
        Commands::Dump { source, destination } =>{
            dump(source, destination);
        }
        Commands::Dedupe { path } =>{
            delete_duplicates_and_keep_one(Path::new(path.as_str()));
        }
    }
}
struct Entry {
    path: String,
    size: u64,
    is_dir: bool,
}
fn list_sizes(path:&str){
    let paths = fs::read_dir(path).unwrap();
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
    if let Ok(folder_size) = get_size(path) {
        println!("{}: {}", ByteSize(folder_size), path);
    } else {
        println!("{}: {}", ByteSize(total_size), path);
    }
}
fn organize_files(source_dir: &str) -> std::io::Result<()> {
    // Read the entries in the source directory
    let entries = fs::read_dir(source_dir)?
        .filter_map(|entry| entry.ok())
        .collect::<Vec<DirEntry>>();

    for entry in entries {
        let metadata = entry.metadata()?;
        let file_name = entry.file_name();
        let file_path = entry.path(); // Borrowed here for later use

        if metadata.is_file() {
            // Get the last modified time (or creation time, depending on your system)
            let modified_time = metadata.modified()?;
            let datetime: DateTime<Utc> = modified_time.into();

            // Extract year, month, and day from the datetime
            let year = datetime.year();
            let month = datetime.month();
            let day = datetime.day();

            // Create the new directory path
            let new_dir = Path::new(source_dir)
                .join(format!("{:04}", year))
                .join(format!("{:02}", month))
                .join(format!("{:02}", day));

            // Create the directories if they don't exist
            fs::create_dir_all(&new_dir)?;

            // Move the file into the new directory
            let new_file_path = new_dir.join(&file_name); // Borrowed here
            fs::rename(&file_path, &new_file_path)?; // Use references to avoid moving the file_path

            // Print the original file path
            println!("Moved {:?} to {:?}", file_path, new_file_path);
        }
    }

    Ok(())
}
fn calculate_file_hash(file_path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(file_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 4096];

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}
fn delete_duplicates_and_keep_one(dir: &Path) -> io::Result<()> {
    // Step 1: Group files by size
    let mut size_map: HashMap<u64, Vec<PathBuf>> = HashMap::new();

    // Gather files by size
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_file() {
            let metadata = fs::metadata(&path)?;
            let file_size = metadata.len();

            size_map.entry(file_size).or_default().push(path);
        }
    }

    // Step 2: Process files of the same size in parallel and hash them
    let hash_map = Mutex::new(HashMap::<String, PathBuf>::new());

    size_map
        .into_par_iter()
        .filter(|(_, files)| files.len() > 1) // Only process size groups with potential duplicates
        .for_each(|(_, files)| {
            let mut keep_file: Option<PathBuf> = None;
            for file_path in files {
                let file_hash = match calculate_file_hash(&file_path) {
                    Ok(hash) => hash,
                    Err(_) => continue, // Skip files with errors
                };

                let mut map = hash_map.lock().unwrap();
                if let Some(existing_file) = map.get(&file_hash) {
                    // If a duplicate is found, delete it
                    if let Err(e) = fs::remove_file(&file_path) {
                        eprintln!("Error deleting file {}: {}", file_path.display(), e);
                    } else {
                        println!("Deleted duplicate: {}", file_path.display());
                    }
                } else {
                    // Keep the first file with this hash
                    map.insert(file_hash, file_path.clone());
                    keep_file = Some(file_path);
                }
            }
        });

    // Step 3: Print files that were kept (optional)
    let kept_files = hash_map.lock().unwrap();
    for (hash, file) in kept_files.iter() {
        println!("Kept file with hash {}: {}", hash, file.display());
    }

    Ok(())
}
fn dump(source:&str,destination:&str)->std::io::Result<()> {
    let source_dir = source;
    let dest_dir = destination;

    fs::create_dir_all(dest_dir)?;

    // Supported file extensions for images and videos
    let valid_extensions = [
        "jpg", "jpeg", "png", "gif", "bmp", "tiff", // Image formats
        "mp4", "avi", "mkv", "mov", "wmv", "flv",   // Video formats
        "m4a", "mp3","3gp" //Audio formats
    ];

    // Move files and delete originals
    for entry in WalkDir::new(source_dir).into_iter().filter_map(Result::ok) {
        if entry.file_type().is_file() {
            let file_path = entry.path();
            if let Some(ext) = file_path.extension().and_then(|e| e.to_str()) {
                if valid_extensions.contains(&ext.to_lowercase().as_str()) {
                    let dest_file = generate_unique_filename(dest_dir, file_path.file_name().unwrap());
                    // Copy the file to the destination
                    if fs::copy(file_path, &dest_file).is_ok(){
                        fs::remove_file(file_path)?;
                    } else {
                        println!("{:?} not copied",file_path)
                    }
                    // Remove the original file
                    
                }
            }
        }
    }

    // Clean up empty directories
    clean_empty_dirs(Path::new(source_dir))?;

    println!("All files have been moved, and empty folders have been removed!");
    Ok(())
}
fn clean_empty_dirs(dir: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            // Recurse into subdirectories
            clean_empty_dirs(&path)?;
            // Attempt to remove the directory if it's empty
            if fs::read_dir(&path)?.next().is_none() {
                fs::remove_dir(&path)?;
            }
        }
    }
    Ok(())
}
fn generate_unique_filename(dest_dir: &str, file_name: &OsStr) -> PathBuf {
    let mut dest_path = Path::new(dest_dir).join(file_name);
    let mut counter = 1;

    // Check if file path length exceeds the limit (e.g., 255 characters for filename)
    let max_length = 255;

    if dest_path.to_str().map(|s| s.len()).unwrap_or(0) > max_length {
        // Shorten file name using a hash if it is too long
        let hashed_name = hash_filename(file_name);
        let extension = dest_path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        dest_path = Path::new(dest_dir).join(format!("{}_{}.{}", hashed_name, counter, extension));
    }

    while dest_path.exists() {
        let file_stem = dest_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("file");
        let extension = dest_path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        let new_file_name = format!("{}_{}.{}", file_stem, counter, extension);
        dest_path = Path::new(dest_dir).join(new_file_name);
        counter += 1;
    }

    dest_path
}

// Hash a long filename to shorten it
fn hash_filename(file_name: &OsStr) -> String {
    let mut hasher = SipHasher::new();
    file_name.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}
