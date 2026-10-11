use std::fs::File;
use std::io::Read;
use std::path::Path;

/// Reads a file to a string without updating its access time.
///
/// Plain reads are self-sustaining under inotify on filesystems where an
/// atime bump emits an event (tmpfs does): gitplay reads a watched file,
/// the read bumps its atime, the bump arrives as a new watch event, and the
/// loop never ends. Opening with `O_NOATIME` leaves no trace, breaking the
/// cycle. When the flag is unavailable or not permitted (a file owned by
/// someone else), the read falls back to the ordinary path.
///
/// The caller still sees the file's content either way; only the side
/// effect on the filesystem differs.
pub fn read_to_string(path: impl AsRef<Path>) -> std::io::Result<String> {
    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // O_NOATIME (01000000 octal) without pulling in a libc dependency.
        const O_NOATIME: i32 = 0o1000000;
        match File::options()
            .read(true)
            .custom_flags(O_NOATIME)
            .open(&path)
        {
            Ok(mut file) => {
                let mut content = String::new();
                file.read_to_string(&mut content)?;
                Ok(content)
            }
            Err(_) => std::fs::read_to_string(&path),
        }
    }
    #[cfg(not(target_os = "linux"))]
    std::fs::read_to_string(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_file_content_regardless_of_the_atime_flag() {
        let dir = std::env::temp_dir().join(format!("gitplay-test-noatime-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("sample.txt"), "content\n").unwrap();
        assert_eq!(read_to_string(dir.join("sample.txt")).unwrap(), "content\n");
        // Missing files surface the same error the ordinary read gives.
        assert!(read_to_string(dir.join("missing.txt")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
