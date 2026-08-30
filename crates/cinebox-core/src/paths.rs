//! Portable data next to the running executable.

use std::io;
use std::path::PathBuf;

/// Directory that contains `cinebox.exe` (or the test binary).
///
/// # Errors
///
/// [`std::env::current_exe`] failed, or the path has no parent.
pub fn exe_dir() -> io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    match exe.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => Ok(dir.to_path_buf()),
        _ => Err(io::Error::new(
            io::ErrorKind::NotFound,
            "executable has no parent directory",
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exe_dir_exists() -> io::Result<()> {
        let dir = exe_dir()?;
        assert!(dir.is_dir());
        Ok(())
    }
}
