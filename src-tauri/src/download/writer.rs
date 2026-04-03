use std::fs::{self, File};
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Mutex;

use super::types::DownloadError;

/// 文件写入器 — 持有共享文件句柄，支持多分片并发写入
///
/// 通过 Arc<Mutex<File>> 共享文件句柄，避免每次 write_at 都 open/close。
/// Clone 后多个分片共享同一个文件句柄。
pub struct FileWriter {
    path: String,
    file: Mutex<File>,
}

impl Clone for FileWriter {
    /// 每次 clone 打开独立的文件句柄，避免多分片共享同一把 Mutex
    /// 导致 tokio 工作线程阻塞的性能问题
    fn clone(&self) -> Self {
        let file = File::options()
            .write(true)
            .open(&self.path)
            .expect("FileWriter::clone: failed to reopen file");
        Self {
            path: self.path.clone(),
            file: Mutex::new(file),
        }
    }
}

impl FileWriter {
    /// 检查目标路径所在磁盘是否有足够空间 (per D-07)
    pub fn check_disk_space(path: &str, needed: u64) -> Result<(), DownloadError> {
        let check_path = Path::new(path);
        let dir = check_path.parent().unwrap_or(check_path);

        let available = fs2::available_space(dir).map_err(DownloadError::Io)?;

        if available < needed {
            return Err(DownloadError::InsufficientDiskSpace { needed, available });
        }

        Ok(())
    }

    /// 创建文件、预分配大小并打开写入句柄 (per D-06)
    pub fn create(path: &str, file_size: u64) -> Result<Self, DownloadError> {
        let p = Path::new(path);
        if let Some(parent) = p.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(DownloadError::Io)?;
            }
        }

        let file = File::options()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(DownloadError::Io)?;
        file.set_len(file_size).map_err(DownloadError::Io)?;

        Ok(Self {
            path: path.to_string(),
            file: Mutex::new(file),
        })
    }

    /// 打开已有文件用于续传写入
    pub fn open(path: &str) -> Result<Self, DownloadError> {
        let file = File::options()
            .write(true)
            .open(path)
            .map_err(DownloadError::Io)?;

        Ok(Self {
            path: path.to_string(),
            file: Mutex::new(file),
        })
    }

    /// 在指定偏移位置写入全部数据 (per D-13)
    pub fn write_at(&self, offset: u64, data: &[u8]) -> Result<(), DownloadError> {
        let mut file = self.file.lock().unwrap();
        file.seek(SeekFrom::Start(offset))
            .map_err(DownloadError::Io)?;
        file.write_all(data).map_err(DownloadError::Io)?;

        Ok(())
    }

    /// 将缓冲写入刷盘，确保断电后 DB 记录的进度与文件数据一致
    pub fn sync_data(&self) -> Result<(), DownloadError> {
        let file = self.file.lock().unwrap();
        file.sync_data().map_err(DownloadError::Io)?;
        Ok(())
    }
}
