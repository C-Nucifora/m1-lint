//! Complete CLI discovery; the pinned workspace walker silently drops errors.

use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn find_scripts(root: &Path) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = pending.pop() {
        if depth > 64 {
            return Err(io::Error::other(format!(
                "{}: script discovery exceeds maximum directory depth 64",
                dir.display()
            )));
        }
        let at_dir =
            |error: io::Error| io::Error::new(error.kind(), format!("{}: {error}", dir.display()));
        for entry in std::fs::read_dir(&dir).map_err(at_dir)? {
            let entry = entry.map_err(at_dir)?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|error| {
                io::Error::new(error.kind(), format!("{}: {error}", path.display()))
            })?;
            // Match workspace discovery: do not follow descendant symlinks.
            if kind.is_dir() {
                pending.push((path, depth + 1));
            } else if kind.is_file()
                && path.extension().and_then(|ext| ext.to_str()) == Some(m1_workspace::SCRIPT_EXT)
            {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}
