//! PID file management.
//!
//! The PID file is a secondary record: the socket bind is the primary
//! single-instance lock. The file lives at `<data_dir>/fergentsd.pid`.

use std::io;
use std::path::{Path, PathBuf};

/// A handle to the daemon PID file.
#[derive(Debug, Clone)]
pub struct PidFile {
    path: PathBuf,
}

impl PidFile {
    /// The PID file inside `data_dir`.
    pub fn in_dir(data_dir: &Path) -> Self {
        Self {
            path: data_dir.join("fergentsd.pid"),
        }
    }

    /// The PID file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Write the current process id.
    pub fn write_current(&self) -> io::Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&self.path, std::process::id().to_string())
    }

    /// Read the recorded PID, if the file exists and parses.
    pub fn read(&self) -> Option<u32> {
        let raw = std::fs::read_to_string(&self.path).ok()?;
        raw.trim().parse::<u32>().ok()
    }

    /// Remove the PID file. A missing file is not an error.
    pub fn remove(&self) -> io::Result<()> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_remove_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let pid = PidFile::in_dir(dir.path());
        assert!(pid.read().is_none());
        pid.write_current().unwrap();
        assert_eq!(pid.read(), Some(std::process::id()));
        pid.remove().unwrap();
        assert!(pid.read().is_none());
        // Removing again is a no-op.
        pid.remove().unwrap();
    }
}
