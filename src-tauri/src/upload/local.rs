//! 上传流程依赖的本地文件能力。
//!
//! 这里集中放置与本地文件系统直接交互的能力：
//! - 全量/部分 SHA1 计算
//! - 目录递归扫描
//! - 文件大小读取
//!
//! 这些能力主要供上传队列内部复用；前端当前只需要通过 Tauri command 读取文件大小。

use std::io::{Read, Seek, SeekFrom};
use std::num::TryFromIntError;

use sha1::{Digest, Sha1};

use super::error::{UploadResult, io_error, message_error};

/// 一个文件的完整 SHA1 和前 128KB SHA1。
///
/// 115 上传协议同时依赖完整哈希和预读哈希：
/// - 完整哈希用于文件标识/秒传判断
/// - 前 128KB 哈希用于初始化上传时的快速校验
#[derive(serde::Serialize, Clone)]
pub(super) struct FileHashResult {
    pub(super) sha1: String,
    pub(super) pre_sha1: String,
}

/// 目录扫描结果里的单个文件项。
#[derive(serde::Serialize, Clone)]
pub(super) struct LocalFileInfo {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) size: u64,
    pub(super) is_dir: bool,
}

/// 通过 Tauri command 计算文件完整 SHA1 与前 128KB SHA1。
/// 供上传队列内部复用的哈希计算入口。
///
/// 这里实际工作放进 `spawn_blocking`，避免大文件哈希计算阻塞 Tokio 异步线程池。
pub(super) async fn compute_file_hash_internal(file_path: String) -> UploadResult<FileHashResult> {
    tokio::task::spawn_blocking(move || {
        let mut file =
            std::fs::File::open(&file_path).map_err(|e| io_error("打开文件", &file_path, e))?;

        let metadata = file
            .metadata()
            .map_err(|e| io_error("读取文件元数据", &file_path, e))?;
        let file_size = metadata.len();

        // 115 初始化接口会消费前 128KB 的 SHA1，因此这里先计算预读哈希。
        let pre_size = std::cmp::min(file_size, 128 * 1024);
        let mut pre_buf = vec![0u8; pre_size as usize];
        file.read_exact(&mut pre_buf)
            .map_err(|e| io_error("读取文件", &file_path, e))?;

        let mut pre_hasher = Sha1::new();
        pre_hasher.update(&pre_buf);
        let pre_sha1: String = pre_hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect();

        file.seek(SeekFrom::Start(0))
            .map_err(|e| io_error("重置文件游标", &file_path, e))?;

        // 完整 SHA1 使用流式读取，避免一次性把大文件读入内存。
        let mut hasher = Sha1::new();
        let mut buffer = vec![0u8; 1024 * 1024];
        loop {
            let n = file
                .read(&mut buffer)
                .map_err(|e| io_error("读取文件", &file_path, e))?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        let sha1: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect();

        Ok(FileHashResult { sha1, pre_sha1 })
    })
    .await
    .map_err(|e| message_error("执行哈希计算任务", e))?
}

/// 计算文件指定闭区间的 SHA1，用于 115 上传的二次认证。
/// 供上传队列内部复用的部分哈希计算入口。
pub(super) async fn compute_partial_sha1_internal(
    file_path: String,
    start: u64,
    end: u64,
) -> UploadResult<String> {
    tokio::task::spawn_blocking(move || {
        if end < start {
            return Err(message_error(
                "校验文件区间",
                format!("非法区间: start={} end={}", start, end),
            ));
        }

        let mut file =
            std::fs::File::open(&file_path).map_err(|e| io_error("打开文件", &file_path, e))?;

        let file_size = file
            .metadata()
            .map_err(|e| io_error("读取文件元数据", &file_path, e))?
            .len();
        if end >= file_size {
            return Err(message_error(
                "校验文件区间",
                format!("区间越界: end={} file_size={}", end, file_size),
            ));
        }

        file.seek(SeekFrom::Start(start))
            .map_err(|e| io_error("定位文件区间", &file_path, e))?;

        let len = usize::try_from(end - start + 1).map_err(range_len_error)?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf)
            .map_err(|e| io_error("读取文件区间", &file_path, e))?;

        let mut hasher = Sha1::new();
        hasher.update(&buf);
        Ok(hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect())
    })
    .await
    .map_err(|e| message_error("执行部分哈希计算任务", e))?
}

fn range_len_error(error: TryFromIntError) -> super::error::UploadError {
    message_error("校验文件区间", format!("区间长度超出支持范围: {}", error))
}

/// 递归扫描目录下的所有文件。
/// 供上传队列内部复用的目录扫描入口。
pub(super) async fn scan_directory_internal(dir_path: String) -> UploadResult<Vec<LocalFileInfo>> {
    tokio::task::spawn_blocking(move || {
        let mut result = Vec::new();
        scan_dir_recursive(&dir_path, &mut result)?;
        Ok(result)
    })
    .await
    .map_err(|e| message_error("执行目录扫描任务", e))?
}

/// 深度优先遍历目录，把所有普通文件压平到结果集中。
fn scan_dir_recursive(dir_path: &str, result: &mut Vec<LocalFileInfo>) -> UploadResult<()> {
    let entries = std::fs::read_dir(dir_path).map_err(|e| io_error("读取目录", dir_path, e))?;

    for entry in entries {
        let entry = entry.map_err(|e| message_error("读取目录条目", e))?;
        let metadata = entry
            .metadata()
            .map_err(|e| io_error("读取文件元数据", entry.path().display().to_string(), e))?;
        let path = entry.path();
        let path_str = path.to_string_lossy().to_string();
        let name = entry.file_name().to_string_lossy().to_string();

        if metadata.is_dir() {
            scan_dir_recursive(&path_str, result)?;
        } else {
            result.push(LocalFileInfo {
                path: path_str,
                name,
                size: metadata.len(),
                is_dir: false,
            });
        }
    }

    Ok(())
}

/// 获取单个文件的字节大小。
#[tauri::command]
pub async fn upload_get_file_size(file_path: String) -> Result<u64, String> {
    get_file_size_impl(file_path)
        .await
        .map_err(|err| err.to_string())
}

/// 供内部流程复用的文件大小读取实现。
async fn get_file_size_impl(file_path: String) -> UploadResult<u64> {
    tokio::task::spawn_blocking(move || {
        let metadata =
            std::fs::metadata(&file_path).map_err(|e| io_error("读取文件元数据", &file_path, e))?;
        Ok(metadata.len())
    })
    .await
    .map_err(|e| message_error("执行文件大小读取任务", e))?
}
