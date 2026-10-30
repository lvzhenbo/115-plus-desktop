use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::{Arc, Mutex};

use super::types::DownloadError;

/// 文件写入器 — 持有共享文件句柄，支持多分片并发写入
///
/// 通过 Arc<Mutex<File>> 共享文件句柄，避免每次 write_at 都 open/close。
/// Clone 后多个分片共享同一个文件句柄。
#[derive(Clone)]
pub struct FileWriter {
    file: Arc<Mutex<File>>,
}

impl FileWriter {
    /// 检查目标路径所在磁盘是否有足够空间
    pub fn check_disk_space(path: &str, needed: u64) -> Result<(), DownloadError> {
        let check_path = Path::new(path);
        let dir = check_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));

        fs::create_dir_all(dir).map_err(DownloadError::Io)?;
        let available = fs4::available_space(dir).map_err(DownloadError::Io)?;

        if available < needed {
            return Err(DownloadError::InsufficientDiskSpace { needed, available });
        }

        Ok(())
    }

    /// 创建目标文件、预分配空间并返回写入器。
    pub fn create(path: &str, file_size: u64) -> Result<Self, DownloadError> {
        let p = Path::new(path);
        if let Some(parent) = p.parent().filter(|parent| !parent.as_os_str().is_empty()) {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(DownloadError::Io)?;
            }
        }

        let file = File::options()
            .create_new(true)
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| match error.kind() {
                std::io::ErrorKind::AlreadyExists => DownloadError::TargetExists(path.to_string()),
                _ => DownloadError::Io(error),
            })?;
        Self::lock(&file, path)?;
        file.set_len(file_size).map_err(DownloadError::Io)?;

        Ok(Self {
            file: Arc::new(Mutex::new(file)),
        })
    }

    /// 打开已有文件用于续传写入
    pub fn open(path: &str) -> Result<Self, DownloadError> {
        let file = File::options()
            .read(true)
            .write(true)
            .open(path)
            .map_err(DownloadError::Io)?;
        Self::lock(&file, path)?;

        Ok(Self {
            file: Arc::new(Mutex::new(file)),
        })
    }

    fn lock(file: &File, path: &str) -> Result<(), DownloadError> {
        fs4::FileExt::try_lock(file).map_err(|error| match error {
            fs4::TryLockError::WouldBlock => DownloadError::TargetBusy(path.to_string()),
            fs4::TryLockError::Error(error) => DownloadError::Io(error),
        })
    }

    /// 仅供已确认归属的断点因 ETag 变化或不支持 Range 而重新下载。
    pub fn restart(path: &str, file_size: u64) -> Result<Self, DownloadError> {
        let writer = Self::open(path)?;
        {
            let file = writer.file.lock().unwrap();
            file.set_len(0).map_err(DownloadError::Io)?;
            file.set_len(file_size).map_err(DownloadError::Io)?;
        }
        Ok(writer)
    }

    /// 在指定偏移位置写入完整字节块。
    pub fn write_at(&self, offset: u64, data: &[u8]) -> Result<(), DownloadError> {
        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))
            .map_err(DownloadError::Io)?;
        file.write_all(data).map_err(DownloadError::Io)?;

        Ok(())
    }

    /// 使用持锁句柄校验文件，避免 Windows 排他锁阻止另开句柄读取。
    pub fn sha1(&self) -> Result<String, DownloadError> {
        use sha1::{Digest, Sha1};
        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(0)).map_err(DownloadError::Io)?;
        let mut hasher = Sha1::new();
        let mut buffer = vec![0u8; 8 * 1024 * 1024];
        loop {
            let n = file.read(&mut buffer).map_err(DownloadError::Io)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        Ok(hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect())
    }

    /// 将已写入数据刷盘，尽量保证断电或崩溃后进度记录仍与磁盘一致。
    pub fn sync_data(&self) -> Result<(), DownloadError> {
        let file = self.file.lock().unwrap();
        file.sync_data().map_err(DownloadError::Io)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(std::path::PathBuf);

    impl TestDirectory {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("115-download-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn existing_files_are_not_truncated() {
        let dir = TestDirectory::new();
        let path = dir.0.join("existing.txt");
        fs::write(&path, b"original content").unwrap();
        assert!(matches!(
            FileWriter::create(path.to_str().unwrap(), 4),
            Err(DownloadError::TargetExists(_))
        ));
        assert_eq!(fs::read(path).unwrap(), b"original content");
    }

    #[test]
    fn prepares_new_download_directories_before_checking_space() {
        let dir = TestDirectory::new();
        let path = dir.0.join("new/nested/file.bin");
        FileWriter::check_disk_space(path.to_str().unwrap(), 4).unwrap();
        let writer = FileWriter::create(path.to_str().unwrap(), 4).unwrap();
        writer.write_at(0, b"data").unwrap();
        assert_eq!(
            writer.sha1().unwrap(),
            "A17C9AAA61E80A1BF71D0D850AF4E5BAA9800BBD"
        );
        drop(writer);
        assert_eq!(fs::read(path).unwrap(), b"data");
    }

    #[test]
    fn writer_clones_keep_exclusive_ownership_until_last_drop() {
        let dir = TestDirectory::new();
        let path = dir.0.join("partial.bin");
        let path = path.to_str().unwrap();
        let writer = FileWriter::create(path, 4).unwrap();
        let clone = writer.clone();
        drop(writer);
        assert!(matches!(
            FileWriter::open(path),
            Err(DownloadError::TargetBusy(_))
        ));
        assert!(matches!(
            FileWriter::restart(path, 8),
            Err(DownloadError::TargetBusy(_))
        ));
        clone.write_at(0, b"data").unwrap();
        assert_eq!(
            clone.sha1().unwrap(),
            "A17C9AAA61E80A1BF71D0D850AF4E5BAA9800BBD"
        );
        drop(clone);
        let resumed = FileWriter::open(path).unwrap();
        resumed.write_at(2, b"OK").unwrap();
        drop(resumed);
        assert_eq!(fs::read(path).unwrap(), b"daOK");
        let restarted = FileWriter::restart(path, 2).unwrap();
        drop(restarted);
        assert_eq!(fs::read(path).unwrap(), [0, 0]);
    }

    #[test]
    fn concurrent_new_downloads_cannot_share_a_target() {
        let dir = TestDirectory::new();
        let path = dir.0.join("target.bin");
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = (0..2)
            .map(|_| {
                let path = path.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    FileWriter::create(path.to_str().unwrap(), 4).is_ok()
                })
            })
            .collect();
        assert_eq!(
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .filter(|created| *created)
                .count(),
            1
        );
    }
}
