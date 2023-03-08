const MAX_PEEK_SIZE: usize = 1024;
use std::{
    io::{self, Read},
    path::PathBuf,
};

use ignore::WalkBuilder;

pub fn read_sql_files(sql_dir: impl AsRef<std::path::Path>) -> Vec<String> {
    let paths: Vec<_> = ignore::WalkBuilder::new(sql_dir)
        .max_depth(Some(5))
        .filter_entry(|entry| {
            let path = entry.path();
            path.is_dir() || path.extension().map(|e| e == "sql").unwrap_or(false)
        })
        .build()
        .filter_map(|dir_result| dir_result.ok().map(|d| d.path().to_path_buf()))
        .collect();

    sort_paths(paths)
}

pub fn sort_paths(mut paths: Vec<PathBuf>) -> Vec<String> {
    paths.sort_by(|a, b| {
        let a_seq = get_sequence(a);
        let b_seq = get_sequence(b);
        a_seq.cmp(&b_seq)
    });
    paths
        .iter()
        .filter(|p| p.is_file())
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect()
}

pub fn read_extension_dir(extension_dir: impl Into<PathBuf>) -> Result<Vec<PathBuf>, io::Error> {
    let extension_dir = extension_dir.into();
    if !extension_dir.exists() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Directory {extension_dir:?} does not exist"),
        ));
    }
    let os_dir = std::env::consts::OS;
    let os_dir = extension_dir.join(os_dir);
    let paths: Vec<_> = WalkBuilder::new(extension_dir)
        .max_depth(Some(5))
        .filter_entry(move |entry| {
            !(entry.depth() == 1 && entry.path() != os_dir && entry.path().is_dir())
        })
        .build()
        .filter_map(|r| r.ok().map(|d| d.path().to_path_buf()))
        .collect();

    Ok(paths
        .iter()
        .filter_map(|p| {
            if p.is_file() {
                if let Ok(file) = std::fs::File::open(p) {
                    let mut buffer: Vec<u8> = vec![];

                    file.take(MAX_PEEK_SIZE as u64)
                        .read_to_end(&mut buffer)
                        .unwrap();

                    let content_type = content_inspector::inspect(&buffer);
                    if content_type.is_binary() {
                        return Some(PathBuf::from(p));
                    }
                }
            }
            None
        })
        .collect())
}

pub fn get_sequence(path: &std::path::Path) -> i32 {
    let path_str = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let seq = path_str.split('-').next();
    if let Some(first) = seq {
        if let Ok(seq_num) = first.parse::<i32>() {
            return seq_num;
        }
    }
    i32::MIN
}
