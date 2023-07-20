use clap::Parser;
use std::collections::HashMap;
use std::ffi::OsString;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::{fs, io};

/// File deduplication tool
#[derive(Parser, Debug)]
struct Args {
    /// Perform a trial run with no changes made
    #[arg(short('n'), long("dry-run"))]
    dry_run: bool,
    /// Path to a reference directory
    reference: PathBuf,
    /// Path to a target directory to be deduplicated
    target: PathBuf,
}

/// Returns a list of files in a directory
///
/// # Arguments
/// * `path` - A path to a directory
fn scan_dir(path: impl AsRef<Path>) -> io::Result<Vec<PathBuf>> {
    let mut items = Vec::new();
    for entry in path.as_ref().read_dir()? {
        let path = entry?.path();
        if path.is_dir() {
            let dir_items = scan_dir(path)?;
            items.extend(dir_items);
        } else if path.is_file() && !path.is_symlink() {
            items.push(path);
        }
    }
    Ok(items)
}

/// Compare two files
/// # Arguments
/// * `path1` - A path to a file
/// * `path2` - A path to a file
/// # Returns
/// * `Ok(true)` if the files are the same
/// * `Ok(false)` if the files are different
/// * `Err` if the comparison failed
fn compare_files(path1: impl AsRef<Path>, path2: impl AsRef<Path>) -> io::Result<bool> {
    let path1 = path1.as_ref();
    let path2 = path2.as_ref();

    let meta1 = path1.metadata()?;
    let meta2 = path2.metadata()?;
    if meta1.len() != meta2.len() {
        return Ok(false);
    }
    let len = meta1.len();

    let mut f1 = File::open(path1)?;
    let mut f2 = File::open(path2)?;

    const BUFFER_SIZE: usize = 4096;
    let mut buffer1 = [0; BUFFER_SIZE];
    let mut buffer2 = [0; BUFFER_SIZE];

    let buffer_count = len / BUFFER_SIZE as u64;
    for _ in 0..buffer_count {
        f1.read_exact(&mut buffer1)?;
        f2.read_exact(&mut buffer2)?;
        if buffer1 != buffer2 {
            return Ok(false);
        }
    }

    let mut buffer1 = vec![];
    let mut buffer2 = vec![];
    f1.read_to_end(&mut buffer1)?;
    f2.read_to_end(&mut buffer2)?;
    if buffer1 != buffer2 {
        return Ok(false);
    }

    Ok(true)
}

struct ReferenceData {
    files: HashMap<OsString, Vec<PathBuf>>,
}

impl ReferenceData {
    fn new(paths: Vec<PathBuf>) -> Self {
        let mut files = HashMap::with_capacity(paths.len());
        for path in paths {
            let file_name = path.file_name().unwrap().to_owned();
            let entry = files.entry(file_name).or_insert_with(Vec::new);
            entry.push(path);
        }
        Self { files }
    }

    fn find_duplicate(&self, file: impl AsRef<Path>) -> io::Result<Option<&Path>> {
        let file = file.as_ref();
        let file_name = file.file_name().unwrap().to_owned();
        if let Some(candidates) = self.files.get(&file_name) {
            for candidate in candidates {
                if compare_files(file, candidate)? {
                    return Ok(Some(candidate));
                }
            }
        }
        Ok(None)
    }
}

fn find_duplicates(
    reference_files: Vec<PathBuf>,
    target_files: Vec<PathBuf>,
) -> io::Result<Vec<(PathBuf, PathBuf)>> {
    let reference = ReferenceData::new(reference_files);

    let mut duplicates = Vec::new();
    for target_file in target_files {
        if let Some(ref_file) = reference.find_duplicate(&target_file)? {
            duplicates.push((target_file, ref_file.to_owned()));
        }
    }
    Ok(duplicates)
}

fn dedup(reference: impl AsRef<Path>, target: impl AsRef<Path>, dry_run: bool) -> io::Result<()> {
    println!("Scanning reference directory...");
    let ref_contents = scan_dir(&reference)?;
    println!("Scanning target directory...");
    let target_contents = scan_dir(&target)?;
    println!("Comparing files...");
    let duplicates = find_duplicates(ref_contents, target_contents)?;
    for (target_file, ref_file) in duplicates {
        println!("Duplicate found: {target_file:?} -> {ref_file:?}");
        if !dry_run {
            fs::remove_file(target_file)?;
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args = Args::parse();
    println!("{:?}", args);

    if let Err(e) = dedup(args.reference, args.target, args.dry_run) {
        eprintln!("Error: {}", e);
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::Rng;
    use std::fs;
    use std::io::Write;
    use tempdir::TempDir;

    fn create_file(path: impl AsRef<Path>) {
        let mut rng = rand::thread_rng();
        let size: usize = rng.gen_range(0..=1024);

        let mut buf = vec![0; size];
        rng.fill(buf.as_mut_slice());

        let mut file = File::create(path).unwrap();
        file.write_all(&buf).unwrap();
        file.flush().unwrap();
    }

    #[test]
    fn test_scan_dir() {
        let tmp = TempDir::new("test_scan_dir").unwrap();
        let tmp_path = tmp.path();

        create_file(tmp_path.join("file1"));
        fs::create_dir(tmp_path.join("dir1")).unwrap();
        create_file(tmp_path.join("dir1").join("file2"));
        fs::create_dir(tmp_path.join("dir1").join("dir2")).unwrap();
        create_file(tmp_path.join("dir1").join("dir2").join("file3"));

        let mut files = scan_dir(tmp_path).unwrap();
        files.sort();
        assert_eq!(
            files,
            [
                tmp_path.join("dir1").join("dir2").join("file3"),
                tmp_path.join("dir1").join("file2"),
                tmp_path.join("file1"),
            ]
        );
    }

    #[test]
    fn test_find_duplicates() {
        let tmp = TempDir::new("test_find_duplicates").unwrap();
        let tmp_path = tmp.path();

        let ref_dir = tmp_path.join("ref");
        let target_dir = tmp_path.join("target");
        fs::create_dir(&ref_dir).unwrap();
        fs::create_dir(&target_dir).unwrap();
        fs::create_dir(ref_dir.join("dir2")).unwrap();

        create_file(ref_dir.join("file1"));
        create_file(ref_dir.join("dir2").join("file2"));
        create_file(ref_dir.join("file3"));
        create_file(ref_dir.join("file4"));
        create_file(ref_dir.join("file5"));
        let ref_files = scan_dir(&ref_dir).unwrap();

        create_file(target_dir.join("file1"));
        create_file(target_dir.join("file3"));
        create_file(target_dir.join("file5"));
        create_file(target_dir.join("file6"));
        fs::copy(ref_dir.join("dir2").join("file2"), target_dir.join("file2")).unwrap();
        fs::copy(ref_dir.join("file4"), target_dir.join("file4")).unwrap();
        let target_files = scan_dir(&target_dir).unwrap();

        let mut duplicates = find_duplicates(ref_files, target_files).unwrap();
        duplicates.sort();
        assert_eq!(
            duplicates,
            [
                (target_dir.join("file2"), ref_dir.join("dir2").join("file2"),),
                (target_dir.join("file4"), ref_dir.join("file4"),),
            ]
        );
    }
}
