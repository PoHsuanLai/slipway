//! Small durable file operations: atomic replace, symlink switch, directory flush.

use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(suffix);
    path.with_file_name(name)
}

/// Flushes a directory's entries to disk.
pub fn sync_dir(dir: &Path) -> io::Result<()> {
    File::open(dir)?.sync_all()
}

/// Writes `bytes` to `path` so a reader sees the old file or the whole new one: write beside it,
/// flush, rename over.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;
    let staged = sibling(path, ".new");
    let mut file = File::create(&staged)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(&staged, path)?;
    sync_dir(parent)
}

/// Points the symlink `link` at `target` with one atomic rename: `link.new` is made first and
/// renamed over `link`.
pub fn switch_link(link: &Path, target: &Path) -> io::Result<()> {
    let staged = sibling(link, ".new");
    match fs::remove_file(&staged) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    symlink(target, &staged)?;
    fs::rename(&staged, link)?;
    link.parent().map_or(Ok(()), sync_dir)
}

/// Removes a directory tree, treating "already gone" as done.
pub fn remove_tree(dir: &Path) -> io::Result<()> {
    match fs::remove_dir_all(dir) {
        Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

/// Flushes every directory under `dir` (and `dir` itself).
pub fn sync_tree(dir: &Path) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            sync_tree(&entry.path())?;
        }
    }
    sync_dir(dir)
}
