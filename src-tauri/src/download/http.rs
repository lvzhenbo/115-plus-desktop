use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU16, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use log::{debug, error, info, warn};
use tokio::sync::{Semaphore, mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::{Duration, interval};

use tauri::AppHandle;

use super::persistence::ProgressFile;
use super::segment::compute_segments;
use super::throttle::get_throttle;
use super::types::{
    DownloadConfig, DownloadError, DownloadTask, ProgressUpdate, RangeInfo, Segment, SegmentStatus,
    TaskAbortReason, TaskStatus,
};
use super::writer::FileWriter;
use crate::download::events::{
    DownloadProgressEvent, DownloadSegmentEvent, DownloadTaskEvent, ProgressItem, ProgressRegistry,
    SpeedCalculator, UrlExpiredEvent, emit_progress, emit_segment_status, emit_task_status,
    emit_url_expired,
};

/// 下载信号枚举，控制任务的运行、暂停、取消
#[derive(Clone, Debug, PartialEq)]
pub enum DownloadSignal {
    Running,
    Paused,
    Cancelled,
}

/// 自适应连接数控制器
///
/// 限流时降速：检测到 CDN 403 后将有效连接数减半（最小为 1），
/// 通过消耗信号量 permit 实现。
/// 空闲时提速：冷却 10s 后每 3s 恢复 1 个连接，直到恢复到用户配置上限。
pub struct ConnectionController {
    configured_max: u16,
    effective_max: AtomicU16,
    /// 已回收的 permit 数量
    stolen_permits: AtomicU16,
    /// 上次限流时间戳 (epoch ms)
    last_rate_limit_ms: AtomicU64,
    /// 上次恢复时间戳 (epoch ms)，用于控制恢复频率
    last_restore_ms: AtomicU64,
}

/// 限流后冷却时间 (ms)，冷却期内不恢复连接数
const RATE_LIMIT_COOLDOWN_MS: u64 = 10_000;
/// 连接恢复最小间隔 (ms)，每次最多恢复 1 个连接
const RESTORE_INTERVAL_MS: u64 = 3_000;

fn current_epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl ConnectionController {
    pub fn new(max: u16) -> Self {
        Self {
            configured_max: max,
            effective_max: AtomicU16::new(max),
            stolen_permits: AtomicU16::new(0),
            last_rate_limit_ms: AtomicU64::new(0),
            last_restore_ms: AtomicU64::new(0),
        }
    }

    /// CDN 限流时调用，将有效连接数减半（最小为 1）
    ///
    /// 通过消耗空闲 permit 减少信号量容量，活跃下载不受影响，自然完成后释放。
    pub fn on_rate_limit(&self, semaphore: &Semaphore) {
        self.last_rate_limit_ms
            .store(current_epoch_ms(), Ordering::SeqCst);
        loop {
            let current = self.effective_max.load(Ordering::SeqCst);
            if current <= 1 {
                return;
            }
            let new_val = (current / 2).max(1);
            match self.effective_max.compare_exchange_weak(
                current,
                new_val,
                Ordering::SeqCst,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    let to_steal = (current - new_val) as usize;
                    let mut actually_stolen = 0u16;
                    for _ in 0..to_steal {
                        match semaphore.try_acquire() {
                            Ok(permit) => {
                                permit.forget();
                                actually_stolen += 1;
                            }
                            Err(_) => break,
                        }
                    }
                    self.stolen_permits
                        .fetch_add(actually_stolen, Ordering::SeqCst);
                    info!(
                        "CDN限流降速: 连接数 {} → {} (回收{}/{}个permit, 累计回收={})",
                        current,
                        new_val,
                        actually_stolen,
                        to_steal,
                        self.stolen_permits.load(Ordering::Relaxed)
                    );
                    return;
                }
                Err(_) => continue,
            }
        }
    }

    /// 分片下载成功时调用，冷却期后逐步恢复连接数
    ///
    /// 限流后等待冷却期，之后按恢复间隔每次恢复 1 个连接，直到达到配置上限。
    pub fn on_success(&self, semaphore: &Semaphore) {
        let stolen = self.stolen_permits.load(Ordering::SeqCst);
        if stolen == 0 {
            return;
        }

        let current = self.effective_max.load(Ordering::SeqCst);
        if current >= self.configured_max {
            return;
        }

        let now = current_epoch_ms();

        // 冷却期内不恢复
        let last_rl = self.last_rate_limit_ms.load(Ordering::SeqCst);
        if now.saturating_sub(last_rl) < RATE_LIMIT_COOLDOWN_MS {
            return;
        }

        // 恢复间隔限制
        let last_restore = self.last_restore_ms.load(Ordering::SeqCst);
        if now.saturating_sub(last_restore) < RESTORE_INTERVAL_MS {
            return;
        }

        // CAS 抢占恢复权
        if self
            .last_restore_ms
            .compare_exchange(last_restore, now, Ordering::SeqCst, Ordering::Relaxed)
            .is_err()
        {
            return;
        }

        // 恢复 1 个 permit
        match self.stolen_permits.compare_exchange(
            stolen,
            stolen - 1,
            Ordering::SeqCst,
            Ordering::Relaxed,
        ) {
            Ok(_) => {
                semaphore.add_permits(1);
                let new_effective = self.effective_max.fetch_add(1, Ordering::SeqCst) + 1;
                debug!(
                    "CDN限流恢复: 连接数 {} → {} (剩余可恢复={})",
                    new_effective - 1,
                    new_effective,
                    stolen - 1
                );
            }
            Err(_) => {
                // 回滚 last_restore_ms
                self.last_restore_ms.store(last_restore, Ordering::SeqCst);
            }
        }
    }

    /// 获取当前有效连接数
    #[allow(dead_code)]
    pub fn effective_limit(&self) -> u16 {
        self.effective_max.load(Ordering::Relaxed)
    }
}

/// HEAD 请求探测服务器是否支持 Range 分片下载
///
/// 解析 Accept-Ranges 和 ETag 响应头。
/// 遇到 CDN 限流 (HTTP 403) 时指数退避重试，最多 5 次。
pub async fn detect_range_support(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    user_agent: &str,
) -> Result<RangeInfo, DownloadError> {
    const MAX_RETRIES: u32 = 5;
    let mut last_error = None;

    for attempt in 0..=MAX_RETRIES {
        let resp = client
            .head(url)
            .header("Authorization", format!("Bearer {}", token))
            .header("User-Agent", user_agent)
            .send()
            .await
            .map_err(DownloadError::Http)?;

        if resp.status().is_success() {
            let supports_range = resp
                .headers()
                .get("accept-ranges")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.eq_ignore_ascii_case("bytes"));

            let etag = resp
                .headers()
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_string());

            return Ok(RangeInfo {
                supports_range,
                etag,
            });
        }

        let status = resp.status().as_u16();
        let message = resp
            .status()
            .canonical_reason()
            .unwrap_or("未知状态")
            .to_string();

        // 仅对 403 限流进行退避重试
        if status == 403 && attempt < MAX_RETRIES {
            let backoff_ms = 1000 * 2u64.pow(attempt);
            let jitter_ms = (attempt as u64) * 200;
            let delay = Duration::from_millis(backoff_ms + jitter_ms);
            warn!(
                "HEAD请求403 疑似CDN限流, 退避重试#{} (延迟{:.1}s)",
                attempt + 1,
                delay.as_secs_f64()
            );
            tokio::time::sleep(delay).await;
            last_error = Some(DownloadError::HttpStatus { status, message });
            continue;
        }

        return Err(DownloadError::HttpStatus { status, message });
    }

    Err(last_error.unwrap_or(DownloadError::HttpStatus {
        status: 403,
        message: "HEAD 请求多次遭遇 CDN 限流，未能获取有效响应".to_string(),
    }))
}

/// 下载单个分片
///
/// 支持 Range 时发送 `Range: bytes=start-end` 分段请求，
/// 否则回退为全文件 GET 请求。流式接收数据并写入对应偏移位置。
pub async fn download_segment(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    user_agent: &str,
    segment: &Segment,
    writer: &FileWriter,
    supports_range: bool,
    progress_tx: Option<tokio::sync::mpsc::Sender<ProgressUpdate>>,
    task_id: &str,
    mut signal_rx: watch::Receiver<DownloadSignal>,
) -> Result<u64, DownloadError> {
    // 分片已完成，直接返回（防止构建无效 Range 导致 416 错误）
    if supports_range && segment.downloaded >= segment.end - segment.start + 1 {
        return Ok(segment.downloaded);
    }

    let mut request = client
        .get(url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", user_agent);

    if supports_range {
        let start = segment.start + segment.downloaded;
        request = request.header("Range", format!("bytes={}-{}", start, segment.end));
    }

    let resp = request.send().await.map_err(DownloadError::Http)?;

    if !resp.status().is_success() {
        let status = resp.status().as_u16();
        if is_url_expired(status) {
            return Err(DownloadError::UrlExpired {
                status,
                message: format!("下载地址已失效（HTTP {}）", status),
            });
        }
        return Err(DownloadError::HttpStatus {
            status,
            message: resp
                .status()
                .canonical_reason()
                .unwrap_or("未知状态")
                .to_string(),
        });
    }

    // 验证 Range 请求返回 206 Partial Content
    // 若服务器忽略 Range 返回 200，写入偏移会导致文件损坏
    if supports_range && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(DownloadError::HttpStatus {
            status: resp.status().as_u16(),
            message: format!(
                "Range 请求期望返回 206 Partial Content，实际收到 {}",
                resp.status().as_u16()
            ),
        });
    }

    // 验证 Content-Range 起始偏移与请求一致
    // 防止 CDN/代理返回错误范围导致数据写偏
    if supports_range {
        let expected_start = segment.start + segment.downloaded;
        if let Some(cr) = resp
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
        {
            // 格式: "bytes START-END/TOTAL"
            if let Some(range_part) = cr.strip_prefix("bytes ")
                && let Some(dash_pos) = range_part.find('-')
                && let Ok(actual_start) = range_part[..dash_pos].parse::<u64>()
                && actual_start != expected_start
            {
                return Err(DownloadError::HttpStatus {
                    status: 206,
                    message: format!(
                        "Content-Range 起始偏移不匹配：期望 {}，实际 {}",
                        expected_start, actual_start
                    ),
                });
            }
        }
    }

    let mut stream = resp.bytes_stream();
    // 不支持 Range 时从文件起始位置写入
    let mut offset = if supports_range {
        segment.start + segment.downloaded
    } else {
        0
    };
    let mut total_written: u64 = if supports_range {
        segment.downloaded
    } else {
        0
    };

    // 诊断计时器
    let seg_start_time = std::time::Instant::now();
    let mut last_log_time = std::time::Instant::now();
    let mut last_log_bytes: u64 = total_written;
    let mut write_ns: u64 = 0;
    let mut throttle_ns: u64 = 0;
    let mut chunk_count: u64 = 0;

    debug!(
        "[分片{}][{}] 开始 range={}..{} 已下载={} 预期={}",
        segment.index,
        task_id,
        segment.start + segment.downloaded,
        segment.end,
        segment.downloaded,
        segment.end - segment.start + 1
    );

    // 写缓冲区，累积到阈值后批量刷盘，减少 I/O 系统调用次数
    const WRITE_BUFFER_SIZE: usize = 2 * 1024 * 1024; // 2MB
    let mut write_buffer: Vec<u8> = Vec::with_capacity(WRITE_BUFFER_SIZE);
    let mut buffer_start_offset = offset;

    // 分片边界保护，防止服务器返回超范围数据覆盖相邻分片
    let max_bytes_expected = segment.end - segment.start + 1 - segment.downloaded;

    // 检查当前信号，如果已暂停/取消则立即返回（防止暂停后新生成的分片漏检）
    {
        let current = signal_rx.borrow_and_update().clone();
        match current {
            DownloadSignal::Paused => {
                return Err(DownloadError::TaskAborted(TaskAbortReason::Paused));
            }
            DownloadSignal::Cancelled => {
                return Err(DownloadError::TaskAborted(TaskAbortReason::Cancelled));
            }
            DownloadSignal::Running => {}
        }
    }

    /// 刷盘缓冲区并上报进度，确保 DB 记录不超过实际磁盘数据
    #[inline]
    fn flush_buffer(
        writer: &FileWriter,
        buf: &mut Vec<u8>,
        buf_offset: u64,
        progress_tx: &Option<tokio::sync::mpsc::Sender<ProgressUpdate>>,
        task_id: &str,
        segment_index: u16,
        total_written: u64,
    ) -> Result<(), DownloadError> {
        if buf.is_empty() {
            return Ok(());
        }
        writer.write_at(buf_offset, buf)?;
        buf.clear();
        // 刷盘后报告进度，确保 DB 记录不超过磁盘数据
        if let Some(tx) = progress_tx {
            let _ = tx.try_send(ProgressUpdate {
                task_id: task_id.to_string(),
                segment_index,
                downloaded: total_written,
            });
        }
        Ok(())
    }

    loop {
        tokio::select! {
            biased;

            // 优先检查暂停/取消信号
            result = signal_rx.changed() => {
                if result.is_err() {
                    // 通道关闭，刷盘保留已下载进度
                    flush_buffer(&writer, &mut write_buffer, buffer_start_offset,
                        &progress_tx, task_id, segment.index, total_written)?;
                    return Err(DownloadError::TaskAborted(TaskAbortReason::SignalChannelClosed));
                }
                let signal = signal_rx.borrow_and_update().clone();
                match signal {
                    DownloadSignal::Paused => {
                        // 暂停前刷盘，防止恢复时 DB 进度超过磁盘数据
                        flush_buffer(&writer, &mut write_buffer, buffer_start_offset,
                            &progress_tx, task_id, segment.index, total_written)?;
                        return Err(DownloadError::TaskAborted(TaskAbortReason::Paused));
                    }
                    DownloadSignal::Cancelled => {
                        // 取消不刷盘，文件将被删除
                        return Err(DownloadError::TaskAborted(TaskAbortReason::Cancelled));
                    }
                    DownloadSignal::Running => {}
                }
            }

            // 接收数据块并写入缓冲区
            chunk_result = tokio::time::timeout(Duration::from_secs(60), stream.next()) => {
                let chunk = match chunk_result {
                    Ok(Some(chunk)) => chunk,
                    Ok(None) => break, // 流结束
                    Err(_) => {
                        // 超时，刷盘保留部分进度
                        flush_buffer(&writer, &mut write_buffer, buffer_start_offset,
                            &progress_tx, task_id, segment.index, total_written)?;
                        warn!("[分片{}][{}] 读取超时60s, 已下载={}", segment.index, task_id, total_written);
                        return Err(DownloadError::TaskAborted(TaskAbortReason::ReadTimeout { seconds: 60 }));
                    }
                };
                let bytes = chunk.map_err(DownloadError::Http)?;

                // 分片边界保护，截断超出范围的多余数据
                let bytes_remaining = max_bytes_expected.saturating_sub(total_written - segment.downloaded);
                if bytes_remaining == 0 {
                    break; // 已收到足够数据
                }
                let effective_bytes = if (bytes.len() as u64) > bytes_remaining {
                    &bytes[..bytes_remaining as usize]
                } else {
                    &bytes[..]
                };

                chunk_count += 1;
                write_buffer.extend_from_slice(effective_bytes);
                offset += effective_bytes.len() as u64;
                total_written += effective_bytes.len() as u64;

                // 缓冲区满，批量刷盘
                if write_buffer.len() >= WRITE_BUFFER_SIZE {
                    let t_write_start = std::time::Instant::now();
                    writer.write_at(buffer_start_offset, &write_buffer)?;
                    write_ns += t_write_start.elapsed().as_nanos() as u64;
                    write_buffer.clear();
                    buffer_start_offset = offset;

                    // 只在刷盘后报告进度
                    if let Some(ref tx) = progress_tx {
                        let _ = tx.try_send(ProgressUpdate {
                            task_id: task_id.to_string(),
                            segment_index: segment.index,
                            downloaded: total_written,
                        });
                    }
                }

                // aria2-style: chunk 处理后立即检查暂停/取消信号（同步检查，零延迟）
                {
                    let signal = signal_rx.borrow().clone();
                    match signal {
                        DownloadSignal::Paused => {
                            flush_buffer(&writer, &mut write_buffer, buffer_start_offset,
                                &progress_tx, task_id, segment.index, total_written)?;
                            return Err(DownloadError::TaskAborted(TaskAbortReason::Paused));
                        }
                        DownloadSignal::Cancelled => {
                            return Err(DownloadError::TaskAborted(TaskAbortReason::Cancelled));
                        }
                        DownloadSignal::Running => {}
                    }
                }

                // 全局带宽限速（限速等待期间也监听暂停/取消信号，aria2-style）
                let t_throttle_start = std::time::Instant::now();
                tokio::select! {
                    biased;
                    result = signal_rx.changed() => {
                        if result.is_ok() {
                            let sig = signal_rx.borrow_and_update().clone();
                            match sig {
                                DownloadSignal::Paused => {
                                    flush_buffer(&writer, &mut write_buffer, buffer_start_offset,
                                        &progress_tx, task_id, segment.index, total_written)?;
                                    return Err(DownloadError::TaskAborted(TaskAbortReason::Paused));
                                }
                                DownloadSignal::Cancelled => {
                                    return Err(DownloadError::TaskAborted(TaskAbortReason::Cancelled));
                                }
                                DownloadSignal::Running => {} // 跳过本次限速
                            }
                        } else {
                            flush_buffer(&writer, &mut write_buffer, buffer_start_offset,
                                &progress_tx, task_id, segment.index, total_written)?;
                            return Err(DownloadError::TaskAborted(TaskAbortReason::SignalChannelClosed));
                        }
                    }
                    _ = get_throttle().consume(effective_bytes.len()) => {}
                }
                throttle_ns += t_throttle_start.elapsed().as_nanos() as u64;

                // 每 5 秒输出诊断日志
                if last_log_time.elapsed().as_secs() >= 5 {
                    let elapsed = last_log_time.elapsed().as_secs_f64();
                    let delta_bytes = total_written - last_log_bytes;
                    let speed_mbps = delta_bytes as f64 / elapsed / 1024.0 / 1024.0;
                    debug!(
                        "[分片{}][{}] 速度={:.1}MB/s 已下载={:.1}MB 块数={} 写盘={:.0}ms 限速={:.0}ms",
                        segment.index, task_id,
                        speed_mbps,
                        total_written as f64 / 1024.0 / 1024.0,
                        chunk_count,
                        write_ns as f64 / 1_000_000.0,
                        throttle_ns as f64 / 1_000_000.0,
                    );
                    last_log_bytes = total_written;
                    last_log_time = std::time::Instant::now();
                }
            }
        }
    }

    // 最后一批缓冲区刷盘
    if !write_buffer.is_empty() {
        writer.write_at(buffer_start_offset, &write_buffer)?;
        if let Some(ref tx) = progress_tx {
            let _ = tx.try_send(ProgressUpdate {
                task_id: task_id.to_string(),
                segment_index: segment.index,
                downloaded: total_written,
            });
        }
    }

    // 验证分片下载字节数
    // 防止服务器截断响应导致预分配区域留有零字节空洞
    let expected_total = segment.end - segment.start + 1;
    if total_written != expected_total {
        warn!(
            "[分片{}][{}] 字节数不匹配: 预期={} 实际={} 耗时={:.1}s",
            segment.index,
            task_id,
            expected_total,
            total_written,
            seg_start_time.elapsed().as_secs_f64()
        );
        return Err(DownloadError::TaskAborted(
            TaskAbortReason::SegmentSizeMismatch {
                segment_index: segment.index,
                expected: expected_total,
                actual: total_written,
            },
        ));
    }

    debug!(
        "[分片{}][{}] 完成 {:.1}MB 耗时={:.1}s 均速={:.1}MB/s 块数={} 写盘={:.0}ms 限速={:.0}ms",
        segment.index,
        task_id,
        total_written as f64 / 1024.0 / 1024.0,
        seg_start_time.elapsed().as_secs_f64(),
        total_written as f64 / seg_start_time.elapsed().as_secs_f64() / 1024.0 / 1024.0,
        chunk_count,
        write_ns as f64 / 1_000_000.0,
        throttle_ns as f64 / 1_000_000.0,
    );

    Ok(total_written)
}
const MAX_SEGMENT_RETRIES: u32 = 3;
/// 重试基准延迟 (ms)，指数退避 1s → 2s → 4s
const RETRY_BASE_DELAY_MS: u64 = 1000;

/// 分片下载 + 指数退避重试
///
/// 对瞬态错误（网络异常、5xx）最多重试 MAX_SEGMENT_RETRIES 次。
/// 不重试：用户信号中断、磁盘 I/O 错误、4xx 客户端错误。
/// 重试时携带已下载进度，从断点继续。
pub async fn download_segment_with_retry(
    client: &reqwest::Client,
    url_rx: watch::Receiver<String>,
    token: &str,
    user_agent: &str,
    segment: &Segment,
    writer: &FileWriter,
    supports_range: bool,
    progress_tx: Option<tokio::sync::mpsc::Sender<ProgressUpdate>>,
    task_id: &str,
    pick_code: &str,
    signal_rx: watch::Receiver<DownloadSignal>,
    url_refresh_requested: Arc<AtomicBool>,
    app: &AppHandle,
) -> Result<u64, DownloadError> {
    let mut retry_count: u32 = 0;
    let mut url_refresh_count: u32 = 0;
    const MAX_URL_REFRESHES: u32 = 5;
    const URL_REFRESH_TIMEOUT_SECS: u64 = 30;
    let mut _last_error: Option<DownloadError> = None;

    // 可变分片副本，重试时更新 downloaded 字段实现断点续传
    let mut local_seg = segment.clone();
    // 原子计数器，跨重试追踪最新已下载字节
    let last_downloaded = Arc::new(std::sync::atomic::AtomicU64::new(segment.downloaded));
    let last_downloaded_for_tx = last_downloaded.clone();

    // 包装进度通道，拦截更新同步写入原子计数器
    let (intercepted_tx, mut intercepted_rx) = tokio::sync::mpsc::channel::<ProgressUpdate>(256);
    let forwarding_handle = {
        let orig_tx = progress_tx.clone();
        tokio::spawn(async move {
            while let Some(update) = intercepted_rx.recv().await {
                last_downloaded_for_tx.store(update.downloaded, Ordering::Relaxed);
                if let Some(ref tx) = orig_tx {
                    let _ = tx.try_send(update);
                }
            }
        })
    };

    /// 从原子计数器回读最新进度到分片副本
    fn sync_partial_progress(
        supports_range: bool,
        last_downloaded: &std::sync::atomic::AtomicU64,
        local_seg: &mut Segment,
    ) {
        if supports_range {
            let latest = last_downloaded.load(Ordering::Relaxed);
            if latest > local_seg.downloaded {
                local_seg.downloaded = latest;
            }
        }
    }

    // 主重试循环，退出路径统一走 break + 尾部清理
    let result: Result<u64, DownloadError> = 'retry_loop: {
        loop {
            let current_url = url_rx.borrow().clone();

            if retry_count > 0 {
                let base_delay_ms = RETRY_BASE_DELAY_MS * 2u64.pow(retry_count - 1);
                // 按分片索引错开重试时间，避免雷群效应
                let jitter_ms = ((local_seg.index as u64) % 16) * 150;
                let delay_ms = base_delay_ms + jitter_ms;
                debug!(
                    "[分片{}][{}] 重试#{} (延迟{}ms, 含抖动{}ms) 已下载={}",
                    local_seg.index,
                    task_id,
                    retry_count,
                    delay_ms,
                    jitter_ms,
                    local_seg.downloaded
                );
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;

                {
                    let signal = signal_rx.borrow().clone();
                    if signal != DownloadSignal::Running {
                        break 'retry_loop Err(DownloadError::TaskAborted(
                            if signal == DownloadSignal::Paused {
                                TaskAbortReason::Paused
                            } else {
                                TaskAbortReason::Cancelled
                            },
                        ));
                    }
                }

                emit_segment_status(
                    app,
                    &DownloadSegmentEvent {
                        task_id: task_id.to_string(),
                        segment_index: local_seg.index,
                        status: SegmentStatus::Downloading,
                        downloaded: local_seg.downloaded,
                    },
                );
            }

            match download_segment(
                client,
                &current_url,
                token,
                user_agent,
                &local_seg,
                writer,
                supports_range,
                Some(intercepted_tx.clone()),
                task_id,
                signal_rx.clone(),
            )
            .await
            {
                Ok(bytes) => {
                    break 'retry_loop Ok(bytes);
                }
                Err(DownloadError::UrlExpired { status, message }) => {
                    sync_partial_progress(supports_range, &last_downloaded, &mut local_seg);

                    // HTTP 403 可能是 CDN 并发限流而非 URL 过期
                    // 先指数退避重试，自然错开并发请求
                    if status == 403 && retry_count < 2 {
                        retry_count += 1;
                        warn!(
                            "[分片{}][{}] HTTP 403 疑似CDN限流, 退避重试#{} 已下载={:.1}MB",
                            local_seg.index,
                            task_id,
                            retry_count,
                            local_seg.downloaded as f64 / 1024.0 / 1024.0
                        );
                        _last_error = Some(DownloadError::UrlExpired { status, message });
                        continue; // 回到循环顶部，触发指数退避+抖动
                    }

                    // 退避后仍 403，确认为 CDN 限流，交由编排层降速重排
                    if status == 403 {
                        warn!(
                            "[分片{}][{}] CDN限流确认, 释放连接等待重新调度 已下载={:.1}MB",
                            local_seg.index,
                            task_id,
                            local_seg.downloaded as f64 / 1024.0 / 1024.0
                        );
                        break 'retry_loop Err(DownloadError::CdnRateLimit);
                    }

                    // 非 403 状态码（如 401/410），判定为 URL 真正过期
                    url_refresh_count += 1;
                    warn!(
                        "[分片{}][{}] URL过期 (HTTP {}) (第{}次刷新) 已下载={:.1}MB",
                        local_seg.index,
                        task_id,
                        status,
                        url_refresh_count,
                        local_seg.downloaded as f64 / 1024.0 / 1024.0
                    );
                    if url_refresh_count > MAX_URL_REFRESHES {
                        break 'retry_loop Err(DownloadError::TaskAborted(
                            TaskAbortReason::UrlRefreshExhausted {
                                max_attempts: MAX_URL_REFRESHES,
                            },
                        ));
                    }

                    if url_refresh_requested
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        emit_url_expired(
                            app,
                            &UrlExpiredEvent {
                                task_id: task_id.to_string(),
                                pick_code: pick_code.to_string(),
                            },
                        );
                    }

                    let mut rx = url_rx.clone();
                    let wait_result = tokio::time::timeout(
                        Duration::from_secs(URL_REFRESH_TIMEOUT_SECS),
                        async {
                            loop {
                                match rx.changed().await {
                                    Ok(()) => {
                                        let new_url = rx.borrow().clone();
                                        if new_url != current_url {
                                            return Ok(());
                                        }
                                    }
                                    Err(_) => {
                                        return Err(DownloadError::TaskAborted(
                                            TaskAbortReason::UrlChannelClosed,
                                        ));
                                    }
                                }
                            }
                        },
                    )
                    .await;

                    match wait_result {
                        Ok(Ok(())) => {
                            let new_url = url_rx.borrow().clone();
                            debug!(
                                "[分片{}][{}] URL已刷新 new_url={}...",
                                local_seg.index,
                                task_id,
                                &new_url[..new_url.len().min(80)]
                            );
                            url_refresh_requested.store(false, Ordering::SeqCst);
                            retry_count = 0;
                            continue;
                        }
                        Ok(Err(e)) => {
                            break 'retry_loop Err(e);
                        }
                        Err(_) => {
                            break 'retry_loop Err(DownloadError::TaskAborted(
                                TaskAbortReason::UrlRefreshTimeout {
                                    seconds: URL_REFRESH_TIMEOUT_SECS,
                                },
                            ));
                        }
                    }
                }
                Err(e) => {
                    if !is_retryable_error(&e) {
                        // 暂停/取消是正常操作，不作为 ERROR 记录
                        if e.is_user_abort() {
                            warn!("[分片{}][{}] 任务已中止: {:?}", local_seg.index, task_id, e);
                        } else {
                            error!(
                                "[分片{}][{}] 不可重试错误: {:?}",
                                local_seg.index, task_id, e
                            );
                        }
                        break 'retry_loop Err(e);
                    }
                    sync_partial_progress(supports_range, &last_downloaded, &mut local_seg);
                    retry_count += 1;
                    if retry_count > MAX_SEGMENT_RETRIES {
                        error!(
                            "[分片{}][{}] 重试耗尽 ({}/{}): {:?}",
                            local_seg.index, task_id, retry_count, MAX_SEGMENT_RETRIES, e
                        );
                        break 'retry_loop Err(e);
                    }
                    _last_error = Some(e);
                }
            }
        }
    };

    // 清理：关闭拦截通道，等待转发任务结束
    drop(intercepted_tx);
    let _ = forwarding_handle.await;

    result
}

/// 判断错误是否为瞬态错误，值得重试
fn is_retryable_error(err: &DownloadError) -> bool {
    match err {
        DownloadError::Http(_) => true,
        DownloadError::HttpStatus { status, .. } => *status >= 500,
        DownloadError::UrlExpired { .. } => false,
        DownloadError::CdnRateLimit => false, // 编排层处理
        _ if err.is_user_abort() => false,
        DownloadError::TaskAborted(_) => false,
        DownloadError::VerificationFailed(_) => false,
        DownloadError::Io(_) => false,
        _ => false,
    }
}

/// 校验下载文件 SHA1 完整性
///
/// 匹配或未提供期望值返回 Ok(true)，不匹配返回 Ok(false)。
async fn verify_file_sha1(
    file_path: &str,
    expected_sha1: Option<&str>,
) -> Result<bool, DownloadError> {
    let Some(expected) = expected_sha1 else {
        return Ok(true); // 未提供期望 SHA1，跳过校验
    };
    let expected = expected.to_uppercase();
    let path = file_path.to_string();
    let computed = tokio::task::spawn_blocking(move || -> Result<String, DownloadError> {
        use sha1::{Digest, Sha1};
        use std::io::Read;
        let mut file = std::fs::File::open(&path).map_err(DownloadError::Io)?;
        let mut hasher = Sha1::new();
        let mut buffer = vec![0u8; 8 * 1024 * 1024]; // 8MB 读取缓冲
        loop {
            let n = file.read(&mut buffer).map_err(DownloadError::Io)?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }
        let hash: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{:02X}", b))
            .collect();
        Ok(hash)
    })
    .await
    .map_err(|e| DownloadError::JoinError(e.to_string()))??;
    Ok(computed == expected)
}

/// 判断 HTTP 状态码是否表示 CDN 预签名 URL 过期
fn is_url_expired(status: u16) -> bool {
    status == 401 || status == 403 || status == 410
}

/// 同一任务的最大分片重分配次数
const MAX_REALLOCATIONS: u32 = 3;
/// 重分配子分片索引偏移基数，避免与原始分片索引冲突
const REALLOC_INDEX_BASE: u16 = 1000;

/// 将失败分片的剩余范围拆分为新子分片
///
/// `active_count`: 当前运行中的分片数，决定拆分数量
/// `realloc_counter`: 已执行的重分配次数，用于计算子分片索引偏移
///
/// 无剩余字节时返回 None。
fn reallocate_failed_segment(
    failed_segment: &Segment,
    active_count: usize,
    realloc_counter: u32,
) -> Option<Vec<Segment>> {
    let remaining_start = failed_segment.start + failed_segment.downloaded;
    let remaining_end = failed_segment.end;
    if remaining_start > remaining_end {
        return None; // 无剩余字节
    }
    let remaining_bytes = remaining_end - remaining_start + 1;
    if remaining_bytes == 0 {
        return None;
    }

    // 拆分为 N 个子分片，N 上限为 4
    let split_count = (active_count.max(1)).min(4) as u16;
    let chunk_size = remaining_bytes / split_count as u64;
    if chunk_size == 0 {
        // 范围太小，不拆分
        let idx = REALLOC_INDEX_BASE + realloc_counter as u16 * 10;
        return Some(vec![Segment {
            index: idx,
            start: remaining_start,
            end: remaining_end,
            status: SegmentStatus::Pending,
            downloaded: 0,
        }]);
    }

    let mut sub_segments = Vec::with_capacity(split_count as usize);
    let base_idx = REALLOC_INDEX_BASE + realloc_counter as u16 * 10;
    for i in 0..split_count {
        let seg_start = remaining_start + i as u64 * chunk_size;
        let seg_end = if i == split_count - 1 {
            remaining_end // 末尾子分片吸收余量
        } else {
            remaining_start + (i as u64 + 1) * chunk_size - 1
        };
        sub_segments.push(Segment {
            index: base_idx + i,
            start: seg_start,
            end: seg_end,
            status: SegmentStatus::Pending,
            downloaded: 0,
        });
    }
    Some(sub_segments)
}

/// 进度刷盘+速度计算+事件发射 — download_file 和 resume_download 共用
///
/// 优化:
/// - sync_data 频率从 500ms 降低到 5s，减少 fdatasync 系统调用开销
/// - 通道关闭时执行最终 sync_data，确保数据一致性
fn spawn_flush_task(
    db: Arc<ProgressFile>,
    app: AppHandle,
    task_id: String,
    file_size: u64,
    file_name: String,
    progress_registry: Arc<ProgressRegistry>,
    snapshot: Arc<Mutex<HashMap<u16, u64>>>,
    writer: FileWriter,
    mut progress_rx: mpsc::Receiver<ProgressUpdate>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = interval(Duration::from_millis(500));
        let mut pending: HashMap<(String, u16), u64> = HashMap::new();
        let mut speed_calc = SpeedCalculator::new(0.3);
        let mut tick_count: u64 = 0;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    tick_count += 1;
                    if !pending.is_empty() {
                        let updates: Vec<(String, u16, u64)> = pending
                            .drain()
                            .map(|((task_id, idx), downloaded)| (task_id, idx, downloaded))
                            .collect();
                        let _ = db.batch_update_downloaded(&updates);
                    }
                    // 每 5 秒 (10个tick) 刷盘一次，减少 fdatasync 系统调用开销
                    // 最多丢失 5 秒进度，.oofp 始终保守于磁盘数据，断点续传安全
                    if tick_count % 10 == 0 {
                        let _ = writer.sync_data();
                    }
                    let cumulative_downloaded: u64 = snapshot.lock().unwrap().values().sum();
                    let speed = speed_calc.update(cumulative_downloaded);
                    let remaining = file_size.saturating_sub(cumulative_downloaded);
                    let eta = speed_calc.eta(remaining);

                    if tick_count % 10 == 0 {
                        log::debug!(
                            "[刷盘][{}] 进度: {:.1}/{:.1}MB ({:.0}%) 速度={:.1}MB/s 剩余={:.0}s",
                            task_id,
                            cumulative_downloaded as f64 / 1024.0 / 1024.0,
                            file_size as f64 / 1024.0 / 1024.0,
                            cumulative_downloaded as f64 / file_size as f64 * 100.0,
                            speed / 1024.0 / 1024.0,
                            eta.unwrap_or(0.0),
                        );
                    }

                    emit_progress(&app, &DownloadProgressEvent {
                        task_id: task_id.clone(),
                        downloaded_bytes: cumulative_downloaded,
                        total_bytes: file_size,
                        speed,
                        eta_secs: eta,
                    });
                    progress_registry.update(ProgressItem {
                        task_id: task_id.clone(),
                        downloaded_bytes: cumulative_downloaded,
                        total_bytes: file_size,
                        speed,
                        eta_secs: eta,
                        status: "active".to_string(),
                        name: file_name.clone(),
                        is_folder: false,
                        completed_files: None,
                        failed_files: None,
                        total_files: None,
                    });
                }
                msg = progress_rx.recv() => {
                    match msg {
                        Some(update) => {
                            pending.insert(
                                (update.task_id.clone(), update.segment_index),
                                update.downloaded,
                            );
                            snapshot.lock().unwrap().insert(update.segment_index, update.downloaded);
                        }
                        None => {
                            // 通道关闭，最终刷盘
                            if !pending.is_empty() {
                                let updates: Vec<(String, u16, u64)> = pending
                                    .drain()
                                    .map(|((task_id, idx), downloaded)| (task_id, idx, downloaded))
                                    .collect();
                                let _ = db.batch_update_downloaded(&updates);
                            }
                            // 确保最终数据落盘
                            let _ = writer.sync_data();
                            progress_registry.remove(&task_id);
                            break;
                        }
                    }
                }
            }
        }
    })
}

// ============================================================================
// 共用下载编排基础设施 — download_file 和 resume_download 共享
// ============================================================================

/// 下载编排上下文 — 封装 download_file 和 resume_download 共用的状态
///
/// 避免在两个函数中重复声明和管理相同的变量集合
struct DownloadContext<'a> {
    task_id: &'a str,
    file_name: &'a str,
    file_size: u64,
    save_path: &'a str,
    pick_code: &'a str,
    expected_sha1: Option<&'a str>,
    supports_range: bool,
    segments: &'a mut Vec<Segment>,
    db: &'a Arc<ProgressFile>,
    app: &'a AppHandle,
    writer: &'a FileWriter,
    progress_snapshot: &'a Arc<Mutex<HashMap<u16, u64>>>,
    signal_rx: &'a mut watch::Receiver<DownloadSignal>,
    url_rx: &'a watch::Receiver<String>,
    url_refresh_requested: &'a Arc<AtomicBool>,
    semaphore: &'a Arc<Semaphore>,
    conn_controller: &'a Arc<ConnectionController>,
}

/// 分片 spawn 参数包 — 避免 spawn 闭包捕获过多局部变量
struct SegmentSpawnParams {
    client: reqwest::Client,
    url_rx: watch::Receiver<String>,
    token: String,
    user_agent: String,
    segment: Segment,
    writer: FileWriter,
    progress_tx: mpsc::Sender<ProgressUpdate>,
    task_id: String,
    pick_code: String,
    signal_rx: watch::Receiver<DownloadSignal>,
    url_refresh_requested: Arc<AtomicBool>,
    app: AppHandle,
    supports_range: bool,
}

/// 将分片 spawn 到 JoinSet — 统一 download_file 和 resume_download 的 spawn 逻辑
fn spawn_segment_task(
    join_set: &mut JoinSet<Result<(u16, u64), (Segment, DownloadError)>>,
    semaphore: &Arc<Semaphore>,
    params: SegmentSpawnParams,
) {
    let permit = semaphore.clone();
    join_set.spawn(async move {
        let p = match permit.acquire_owned().await {
            Ok(p) => p,
            Err(e) => {
                log::warn!(
                    "[分片{}][{}] 获取并发许可失败: {}",
                    params.segment.index,
                    params.task_id,
                    e
                );
                return Err((
                    params.segment,
                    DownloadError::TaskAborted(TaskAbortReason::SemaphoreClosed),
                ));
            }
        };
        match download_segment_with_retry(
            &params.client,
            params.url_rx,
            &params.token,
            &params.user_agent,
            &params.segment,
            &params.writer,
            params.supports_range,
            Some(params.progress_tx),
            &params.task_id,
            &params.pick_code,
            params.signal_rx,
            params.url_refresh_requested,
            &params.app,
        )
        .await
        {
            Ok(bytes) => {
                drop(p);
                Ok((params.segment.index, bytes))
            }
            Err(e) => {
                drop(p);
                Err((params.segment, e))
            }
        }
    });
}

/// CDN 限流重排队结果
enum CdnRetryAction {
    /// 正常重排队
    Retry { delay: Duration },
    /// 所有分片停滞，触发任务级 URL 刷新
    AllStuck,
    /// 重试耗尽，标记失败
    Exhausted,
}

/// 处理 CDN 限流 — 自适应降速 + 延迟重排队
fn handle_cdn_rate_limit(
    failed_seg: &Segment,
    cdn_retry_counts: &mut HashMap<u16, u32>,
    last_success_time: std::time::Instant,
    join_set_len: usize,
    task_url_refresh_count: u32,
    max_cdn_retries: u32,
    all_stuck_threshold_secs: u64,
    max_task_url_refreshes: u32,
) -> CdnRetryAction {
    let count = cdn_retry_counts.entry(failed_seg.index).or_insert(0);
    *count += 1;

    let all_stuck_duration = last_success_time.elapsed().as_secs();
    if all_stuck_duration > all_stuck_threshold_secs && join_set_len == 0 {
        if task_url_refresh_count > max_task_url_refreshes {
            return CdnRetryAction::Exhausted;
        }
        return CdnRetryAction::AllStuck;
    }

    if *count > max_cdn_retries {
        return CdnRetryAction::Exhausted;
    }

    let backoff_secs = 2u64.pow((*count).min(6));
    let jitter_ms = ((failed_seg.index as u64) % 16) * 300;
    let delay = Duration::from_secs(backoff_secs) + Duration::from_millis(jitter_ms);

    CdnRetryAction::Retry { delay }
}

/// 分片重分配结果
enum ReallocResult {
    Success(Vec<Segment>),
    Failed,
}

/// 处理分片失败 — 尝试重分配剩余范围
fn handle_segment_failure(
    failed_seg: &Segment,
    progress_snapshot: &Arc<Mutex<HashMap<u16, u64>>>,
    realloc_counter: u32,
    join_set_len: usize,
) -> ReallocResult {
    let actual_downloaded = progress_snapshot
        .lock()
        .unwrap()
        .get(&failed_seg.index)
        .copied()
        .unwrap_or(failed_seg.downloaded);
    let mut updated_seg = failed_seg.clone();
    updated_seg.downloaded = actual_downloaded;

    if realloc_counter < MAX_REALLOCATIONS {
        if let Some(sub_segs) =
            reallocate_failed_segment(&updated_seg, join_set_len, realloc_counter)
        {
            return ReallocResult::Success(sub_segs);
        }
    }

    ReallocResult::Failed
}

/// 下载结果收集后的后处理 — 统一暂停/取消/失败/完成逻辑
///
/// 注意: 调用者需在调用此函数前 drop progress_tx 以关闭进度通道
async fn finalize_download<'a>(
    is_paused: bool,
    is_cancelled: bool,
    has_failure: bool,
    ctx: &DownloadContext<'a>,
    flush_handle: &mut tokio::task::JoinHandle<()>,
) -> Result<(), DownloadError> {
    if is_paused || is_cancelled {
        {
            let latest_progress = ctx.progress_snapshot.lock().unwrap();
            for seg in ctx.segments.iter() {
                if seg.status == SegmentStatus::Completed
                    || seg.status == SegmentStatus::Reallocated
                {
                    continue;
                }
                let downloaded = latest_progress
                    .get(&seg.index)
                    .copied()
                    .unwrap_or(seg.downloaded);
                let _ = ctx.db.update_segment_status(
                    ctx.task_id,
                    seg.index,
                    &SegmentStatus::Paused,
                    downloaded,
                );
                emit_segment_status(
                    ctx.app,
                    &DownloadSegmentEvent {
                        task_id: ctx.task_id.to_string(),
                        segment_index: seg.index,
                        status: SegmentStatus::Paused,
                        downloaded,
                    },
                );
            }
        }
        let _ = ctx.db.update_task_status(ctx.task_id, "paused");
        let _ = flush_handle.await;
        return Err(DownloadError::TaskAborted(if is_paused {
            TaskAbortReason::Paused
        } else {
            TaskAbortReason::Cancelled
        }));
    }

    let _ = flush_handle.await;

    if has_failure {
        let latest_progress = ctx.progress_snapshot.lock().unwrap();
        for seg in ctx.segments.iter() {
            if seg.status == SegmentStatus::Completed || seg.status == SegmentStatus::Reallocated {
                continue;
            }
            let downloaded = latest_progress
                .get(&seg.index)
                .copied()
                .unwrap_or(seg.downloaded);
            let _ = ctx.db.update_segment_status(
                ctx.task_id,
                seg.index,
                &SegmentStatus::Failed,
                downloaded,
            );
            emit_segment_status(
                ctx.app,
                &DownloadSegmentEvent {
                    task_id: ctx.task_id.to_string(),
                    segment_index: seg.index,
                    status: SegmentStatus::Failed,
                    downloaded,
                },
            );
        }
        let _ = ctx.db.update_task_status(ctx.task_id, "error");
        return Err(DownloadError::TaskAborted(TaskAbortReason::DownloadFailed));
    }

    Ok(())
}

/// CDN 限流后重新排队分片
fn respawn_cdn_segment(
    ctx: &DownloadContext<'_>,
    join_set: &mut JoinSet<Result<(u16, u64), (Segment, DownloadError)>>,
    failed_seg: &Segment,
    delay: Duration,
    client: &reqwest::Client,
    token: &str,
    user_agent: &str,
    progress_tx: &mpsc::Sender<ProgressUpdate>,
) {
    let mut seg = failed_seg.clone();
    let actual_downloaded = ctx
        .progress_snapshot
        .lock()
        .unwrap()
        .get(&seg.index)
        .copied()
        .unwrap_or(seg.downloaded);
    seg.downloaded = actual_downloaded;

    let semaphore = ctx.semaphore.clone();
    let client = client.clone();
    let url_rx = ctx.url_rx.clone();
    let token = token.to_string();
    let user_agent = user_agent.to_string();
    let writer = ctx.writer.clone();
    let tx = progress_tx.clone();
    let tid = ctx.task_id.to_string();
    let pick_code = ctx.pick_code.to_string();
    let mut sig_rx = ctx.signal_rx.clone();
    let url_refresh_req = ctx.url_refresh_requested.clone();
    let app_clone = ctx.app.clone();
    let supports_range = ctx.supports_range;

    join_set.spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(delay) => {}
            _ = sig_rx.changed() => {
                let signal = sig_rx.borrow_and_update().clone();
                if signal != DownloadSignal::Running {
                    return Err((
                        seg,
                        DownloadError::TaskAborted(if signal == DownloadSignal::Paused {
                            TaskAbortReason::Paused
                        } else {
                            TaskAbortReason::Cancelled
                        }),
                    ));
                }
            }
        }
        {
            let signal = sig_rx.borrow().clone();
            if signal != DownloadSignal::Running {
                return Err((
                    seg,
                    DownloadError::TaskAborted(if signal == DownloadSignal::Paused {
                        TaskAbortReason::Paused
                    } else {
                        TaskAbortReason::Cancelled
                    }),
                ));
            }
        }
        let permit = match semaphore.acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                return Err((
                    seg,
                    DownloadError::TaskAborted(TaskAbortReason::SemaphoreClosed),
                ));
            }
        };
        match download_segment_with_retry(
            &client,
            url_rx,
            &token,
            &user_agent,
            &seg,
            &writer,
            supports_range,
            Some(tx),
            &tid,
            &pick_code,
            sig_rx,
            url_refresh_req,
            &app_clone,
        )
        .await
        {
            Ok(bytes) => {
                drop(permit);
                Ok((seg.index, bytes))
            }
            Err(e) => {
                drop(permit);
                Err((seg, e))
            }
        }
    });
}

/// 结果收集循环 — 统一处理分片完成、CDN限流、失败重分配、暂停/取消
///
/// 接收 progress_tx 所有权，在 finalize_download 前自动 drop 关闭进度通道
async fn collect_results<'a>(
    ctx: &mut DownloadContext<'a>,
    join_set: &mut JoinSet<Result<(u16, u64), (Segment, DownloadError)>>,
    client: &reqwest::Client,
    token: &str,
    user_agent: &str,
    progress_tx: mpsc::Sender<ProgressUpdate>,
    flush_handle: &mut tokio::task::JoinHandle<()>,
    task_start_time: std::time::Instant,
    log_prefix: &str,
) -> Result<(), DownloadError> {
    let mut has_failure = false;
    let mut is_paused = false;
    let mut is_cancelled = false;
    let mut realloc_counter: u32 = 0;
    let mut completed_segments: u32 = 0;
    let total_segments = ctx.segments.len() as u32;
    let mut cdn_retry_counts: HashMap<u16, u32> = HashMap::new();
    const MAX_CDN_RETRIES: u32 = 50;
    let mut last_success_time = std::time::Instant::now();
    let mut task_url_refresh_count: u32 = 0;
    const MAX_TASK_URL_REFRESHES: u32 = 10;
    const ALL_STUCK_THRESHOLD_SECS: u64 = 60;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok((index, bytes))) => {
                completed_segments += 1;
                last_success_time = std::time::Instant::now();
                ctx.conn_controller.on_success(ctx.semaphore);
                debug!(
                    "[{}][{}] 分片{} 完成 {:.1}MB ({}/{})",
                    log_prefix,
                    ctx.task_id,
                    index,
                    bytes as f64 / 1024.0 / 1024.0,
                    completed_segments,
                    total_segments
                );
                if let Some(seg) = ctx.segments.iter_mut().find(|s| s.index == index) {
                    seg.status = SegmentStatus::Completed;
                    seg.downloaded = bytes;
                }
                let _ = ctx.db.update_segment_status(
                    ctx.task_id,
                    index,
                    &SegmentStatus::Completed,
                    bytes,
                );
                emit_segment_status(
                    ctx.app,
                    &DownloadSegmentEvent {
                        task_id: ctx.task_id.to_string(),
                        segment_index: index,
                        status: SegmentStatus::Completed,
                        downloaded: bytes,
                    },
                );
            }
            Ok(Err((_, DownloadError::TaskAborted(TaskAbortReason::Paused)))) => {
                info!("[{}][{}] 任务暂停", log_prefix, ctx.task_id);
                is_paused = true;
                join_set.abort_all();
                break;
            }
            Ok(Err((_, DownloadError::TaskAborted(TaskAbortReason::Cancelled)))) => {
                info!("[{}][{}] 任务取消", log_prefix, ctx.task_id);
                is_cancelled = true;
                join_set.abort_all();
                break;
            }
            Ok(Err((failed_seg, DownloadError::CdnRateLimit))) => {
                ctx.conn_controller.on_rate_limit(ctx.semaphore);
                match handle_cdn_rate_limit(
                    &failed_seg,
                    &mut cdn_retry_counts,
                    last_success_time,
                    join_set.len(),
                    task_url_refresh_count,
                    MAX_CDN_RETRIES,
                    ALL_STUCK_THRESHOLD_SECS,
                    MAX_TASK_URL_REFRESHES,
                ) {
                    CdnRetryAction::AllStuck => {
                        task_url_refresh_count += 1;
                        warn!(
                            "[{}][{}] 所有分片停滞, 触发URL刷新 (第{}次)",
                            log_prefix, ctx.task_id, task_url_refresh_count
                        );
                        if ctx
                            .url_refresh_requested
                            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            emit_url_expired(
                                ctx.app,
                                &UrlExpiredEvent {
                                    task_id: ctx.task_id.to_string(),
                                    pick_code: ctx.pick_code.to_string(),
                                },
                            );
                        }
                        let count = cdn_retry_counts.entry(failed_seg.index).or_insert(0);
                        let backoff_secs = 2u64.pow((*count).min(6));
                        let jitter_ms = ((failed_seg.index as u64) % 16) * 300;
                        let delay =
                            Duration::from_secs(backoff_secs) + Duration::from_millis(jitter_ms);
                        respawn_cdn_segment(
                            ctx,
                            join_set,
                            &failed_seg,
                            delay,
                            client,
                            token,
                            user_agent,
                            &progress_tx,
                        );
                    }
                    CdnRetryAction::Retry { delay } => {
                        debug!(
                            "[{}][{}] 分片{} CDN限流重排队 (延迟{:.1}s)",
                            log_prefix,
                            ctx.task_id,
                            failed_seg.index,
                            delay.as_secs_f64()
                        );
                        respawn_cdn_segment(
                            ctx,
                            join_set,
                            &failed_seg,
                            delay,
                            client,
                            token,
                            user_agent,
                            &progress_tx,
                        );
                    }
                    CdnRetryAction::Exhausted => {
                        warn!(
                            "[{}][{}] 分片{} CDN限流重试耗尽",
                            log_prefix, ctx.task_id, failed_seg.index
                        );
                        has_failure = true;
                    }
                }
            }
            Ok(Err((failed_seg, _e))) => {
                warn!(
                    "[{}][{}] 分片{} 失败: {:?}",
                    log_prefix, ctx.task_id, failed_seg.index, _e
                );
                match handle_segment_failure(
                    &failed_seg,
                    ctx.progress_snapshot,
                    realloc_counter,
                    join_set.len(),
                ) {
                    ReallocResult::Success(sub_segs) => {
                        realloc_counter += 1;
                        let actual_downloaded = ctx
                            .progress_snapshot
                            .lock()
                            .unwrap()
                            .get(&failed_seg.index)
                            .copied()
                            .unwrap_or(failed_seg.downloaded);
                        if let Some(orig) = ctx
                            .segments
                            .iter_mut()
                            .find(|s| s.index == failed_seg.index)
                        {
                            orig.status = SegmentStatus::Reallocated;
                            orig.downloaded = actual_downloaded;
                        }
                        let _ = ctx.db.update_segment_status(
                            ctx.task_id,
                            failed_seg.index,
                            &SegmentStatus::Reallocated,
                            actual_downloaded,
                        );
                        let _ = ctx.db.insert_segments(ctx.task_id, &sub_segs);
                        let sub_segs_tracking: Vec<Segment> = sub_segs
                            .iter()
                            .map(|s| {
                                let mut ts = s.clone();
                                ts.status = SegmentStatus::Downloading;
                                ts
                            })
                            .collect();
                        let mut realloc_ok = true;
                        for sub_seg in &sub_segs {
                            let _ = ctx.db.update_segment_status(
                                ctx.task_id,
                                sub_seg.index,
                                &SegmentStatus::Downloading,
                                0,
                            );
                            match ctx.semaphore.clone().acquire_owned().await {
                                Ok(_permit) => {
                                    let params = SegmentSpawnParams {
                                        client: client.clone(),
                                        url_rx: ctx.url_rx.clone(),
                                        token: token.to_string(),
                                        user_agent: user_agent.to_string(),
                                        segment: sub_seg.clone(),
                                        writer: ctx.writer.clone(),
                                        progress_tx: progress_tx.clone(),
                                        task_id: ctx.task_id.to_string(),
                                        pick_code: ctx.pick_code.to_string(),
                                        signal_rx: ctx.signal_rx.clone(),
                                        url_refresh_requested: ctx.url_refresh_requested.clone(),
                                        app: ctx.app.clone(),
                                        supports_range: ctx.supports_range,
                                    };
                                    spawn_segment_task(join_set, ctx.semaphore, params);
                                }
                                Err(_) => {
                                    realloc_ok = false;
                                    break;
                                }
                            }
                        }
                        if realloc_ok {
                            ctx.segments.extend(sub_segs_tracking);
                            continue;
                        }
                    }
                    ReallocResult::Failed => {
                        has_failure = true;
                    }
                }
            }
            Err(_) => {
                has_failure = true;
            }
        }
    }

    // 关闭进度通道，触发 flush_handle 最终刷盘
    drop(progress_tx);
    finalize_download(is_paused, is_cancelled, has_failure, ctx, flush_handle).await?;

    // SHA1 完整性校验
    let sha1_ok = verify_file_sha1(ctx.save_path, ctx.expected_sha1)
        .await
        .unwrap_or(false);
    if sha1_ok {
        ctx.db.delete_task(ctx.task_id)?;
        let elapsed = task_start_time.elapsed().as_secs_f64();
        info!(
            "[{}][{}] 下载完成 文件={} 大小={:.1}MB 耗时={:.1}s 均速={:.1}MB/s",
            log_prefix,
            ctx.task_id,
            ctx.file_name,
            ctx.file_size as f64 / 1024.0 / 1024.0,
            elapsed,
            ctx.file_size as f64 / elapsed / 1024.0 / 1024.0,
        );
    } else {
        let _ = ctx.db.update_task_status(ctx.task_id, "verify_failed");
        return Err(DownloadError::VerificationFailed(
            "SHA1 与服务端返回值不一致".to_string(),
        ));
    }

    Ok(())
}

/// 分片 spawn 辅助 — 交错延迟 + 信号检查 + 状态标记 + spawn
async fn spawn_segments_with_stagger(
    ctx: &mut DownloadContext<'_>,
    join_set: &mut JoinSet<Result<(u16, u64), (Segment, DownloadError)>>,
    client: &reqwest::Client,
    token: &str,
    user_agent: &str,
    progress_tx: &mpsc::Sender<ProgressUpdate>,
    skip_completed: bool,
) {
    let mut spawn_count = 0u32;
    for segment in ctx.segments.iter() {
        if skip_completed
            && (segment.status == SegmentStatus::Completed
                || segment.status == SegmentStatus::Reallocated)
        {
            continue;
        }
        if spawn_count > 0 {
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
        spawn_count += 1;

        {
            let signal = ctx.signal_rx.borrow().clone();
            if signal != DownloadSignal::Running {
                break;
            }
        }

        let _ = ctx.db.update_segment_status(
            ctx.task_id,
            segment.index,
            &SegmentStatus::Downloading,
            segment.downloaded,
        );
        emit_segment_status(
            ctx.app,
            &DownloadSegmentEvent {
                task_id: ctx.task_id.to_string(),
                segment_index: segment.index,
                status: SegmentStatus::Downloading,
                downloaded: segment.downloaded,
            },
        );

        let params = SegmentSpawnParams {
            client: client.clone(),
            url_rx: ctx.url_rx.clone(),
            token: token.to_string(),
            user_agent: user_agent.to_string(),
            segment: segment.clone(),
            writer: ctx.writer.clone(),
            progress_tx: progress_tx.clone(),
            task_id: ctx.task_id.to_string(),
            pick_code: ctx.pick_code.to_string(),
            signal_rx: ctx.signal_rx.clone(),
            url_refresh_requested: ctx.url_refresh_requested.clone(),
            app: ctx.app.clone(),
            supports_range: ctx.supports_range,
        };
        spawn_segment_task(join_set, ctx.semaphore, params);
    }
}

/// 多分片并行下载编排
///
/// 磁盘空间检查 → Range 探测 → 分片计算 → 文件预分配 → 信号量控制并行下载
pub async fn download_file(
    client: &reqwest::Client,
    task: &mut DownloadTask,
    token: &str,
    user_agent: &str,
    config: &DownloadConfig,
    db: &Arc<ProgressFile>,
    app: &AppHandle,
    signal_rx: watch::Receiver<DownloadSignal>,
    url_rx: watch::Receiver<String>,
    url_refresh_requested: Arc<AtomicBool>,
    segment_semaphore: Arc<Semaphore>,
    conn_controller: Arc<ConnectionController>,
    progress_registry: Arc<ProgressRegistry>,
) -> Result<(), DownloadError> {
    FileWriter::check_disk_space(&task.save_path, task.file_size)?;
    if config.speed_limit > 0 {
        super::throttle::set_speed_limit(config.speed_limit);
    }

    let range_info = detect_range_support(client, &task.url, token, user_agent).await?;
    task.etag = range_info.etag;

    let split = if range_info.supports_range {
        config.split
    } else {
        1
    };
    task.segments = compute_segments(task.file_size, split);

    info!(
        "[任务][{}] 开始下载 文件={} 大小={:.1}MB 分片={} 支持断点={} url={}...",
        task.task_id,
        task.file_name,
        task.file_size as f64 / 1024.0 / 1024.0,
        task.segments.len(),
        range_info.supports_range,
        &task.url[..task.url.len().min(80)]
    );

    let writer = FileWriter::create(&task.save_path, task.file_size)?;
    db.save_task(
        &task.task_id,
        &task.file_name,
        task.file_size,
        &task.save_path,
        &task.url,
        task.etag.as_deref(),
        &task.pick_code,
        task.expected_sha1.as_deref(),
        task.created_at,
    )?;
    db.save_segments(&task.task_id, &task.segments)?;

    let (progress_tx, progress_rx) = mpsc::channel::<ProgressUpdate>(1024);
    let progress_snapshot: Arc<Mutex<HashMap<u16, u64>>> = Arc::new(Mutex::new(HashMap::new()));
    let mut flush_handle = spawn_flush_task(
        Arc::clone(db),
        app.clone(),
        task.task_id.clone(),
        task.file_size,
        task.file_name.clone(),
        Arc::clone(&progress_registry),
        progress_snapshot.clone(),
        writer.clone(),
        progress_rx,
    );

    task.status = TaskStatus::Active;
    emit_task_status(
        app,
        &DownloadTaskEvent {
            task_id: task.task_id.clone(),
            status: TaskStatus::Active,
        },
    );

    let task_start_time = std::time::Instant::now();
    let mut ctx = DownloadContext {
        task_id: &task.task_id,
        file_name: &task.file_name,
        file_size: task.file_size,
        save_path: &task.save_path,
        pick_code: &task.pick_code,
        expected_sha1: task.expected_sha1.as_deref(),
        supports_range: range_info.supports_range,
        segments: &mut task.segments,
        db,
        app,
        writer: &writer,
        progress_snapshot: &progress_snapshot,
        signal_rx: &mut signal_rx.clone(),
        url_rx: &url_rx,
        url_refresh_requested: &url_refresh_requested,
        semaphore: &segment_semaphore,
        conn_controller: &conn_controller,
    };

    let mut join_set: JoinSet<Result<(u16, u64), (Segment, DownloadError)>> = JoinSet::new();
    spawn_segments_with_stagger(
        &mut ctx,
        &mut join_set,
        client,
        token,
        user_agent,
        &progress_tx,
        false,
    )
    .await;
    collect_results(
        &mut ctx,
        &mut join_set,
        client,
        token,
        user_agent,
        progress_tx,
        &mut flush_handle,
        task_start_time,
        "task",
    )
    .await
}

/// 恢复中断的下载任务
///
/// 1. 从进度文件加载分片 → 2. HEAD + ETag 验证 →
/// 3a. ETag 匹配：跳过已完成分片，从断点恢复 →
/// 3b. ETag 不匹配：清除进度，从头重新下载
pub async fn resume_download(
    client: &reqwest::Client,
    task_id: &str,
    url: &str,
    save_path: &str,
    token: &str,
    user_agent: &str,
    config: &DownloadConfig,
    db: &Arc<ProgressFile>,
    app: &AppHandle,
    signal_rx: watch::Receiver<DownloadSignal>,
    url_rx: watch::Receiver<String>,
    url_refresh_requested: Arc<AtomicBool>,
    segment_semaphore: Arc<Semaphore>,
    conn_controller: Arc<ConnectionController>,
    progress_registry: Arc<ProgressRegistry>,
) -> Result<(), DownloadError> {
    let task_meta = db.load_task(save_path)?;
    if task_meta.task_id != task_id {
        return Err(DownloadError::FileNotFound(format!(
            "Task ID mismatch: expected {}, found {}",
            task_id, task_meta.task_id
        )));
    }

    let mut need_restart = false;
    let range_info = detect_range_support(client, url, token, user_agent).await?;
    let supports_range = range_info.supports_range;

    if !supports_range {
        need_restart = true;
    }
    if let Some(ref stored_etag) = task_meta.etag {
        match range_info.etag {
            Some(ref server_etag) if server_etag == stored_etag => {}
            _ => {
                need_restart = true;
            }
        }
    }

    if need_restart {
        db.delete_task(task_id)?;
        let mut fresh_task = DownloadTask {
            task_id: task_id.to_string(),
            file_name: task_meta.file_name,
            file_size: task_meta.file_size,
            save_path: task_meta.save_path,
            url: url.to_string(),
            pick_code: task_meta.pick_code,
            etag: None,
            expected_sha1: task_meta.expected_sha1,
            segments: Vec::new(),
            status: TaskStatus::Pending,
            created_at: task_meta.created_at,
        };
        return download_file(
            client,
            &mut fresh_task,
            token,
            user_agent,
            config,
            db,
            app,
            signal_rx,
            url_rx,
            url_refresh_requested,
            segment_semaphore,
            conn_controller,
            progress_registry,
        )
        .await;
    }

    if !std::path::Path::new(&task_meta.save_path).exists() {
        return Err(DownloadError::FileNotFound(format!(
            "Download file missing: {}",
            task_meta.save_path
        )));
    }

    let writer = FileWriter::open(&task_meta.save_path)?;
    let mut segments = task_meta.segments;
    let completed_count = segments
        .iter()
        .filter(|s| s.status == SegmentStatus::Completed)
        .count();
    let already_downloaded: u64 = segments.iter().map(|s| s.downloaded).sum();
    info!(
        "[续传][{}] 恢复下载 文件={} 已完成分片={}/{} 已下载={:.1}MB/{:.1}MB",
        task_id,
        task_meta.file_name,
        completed_count,
        segments.len(),
        already_downloaded as f64 / 1024.0 / 1024.0,
        task_meta.file_size as f64 / 1024.0 / 1024.0,
    );
    db.update_task_status(task_id, "active")?;
    emit_task_status(
        app,
        &DownloadTaskEvent {
            task_id: task_id.to_string(),
            status: TaskStatus::Active,
        },
    );

    let (progress_tx, progress_rx) = mpsc::channel::<ProgressUpdate>(1024);
    let progress_snapshot: Arc<Mutex<HashMap<u16, u64>>> = Arc::new(Mutex::new(
        segments
            .iter()
            .filter(|s| s.downloaded > 0)
            .map(|s| (s.index, s.downloaded))
            .collect(),
    ));
    let mut flush_handle = spawn_flush_task(
        Arc::clone(db),
        app.clone(),
        task_id.to_string(),
        task_meta.file_size,
        task_meta.file_name.clone(),
        Arc::clone(&progress_registry),
        progress_snapshot.clone(),
        writer.clone(),
        progress_rx,
    );

    let task_start_time = std::time::Instant::now();
    let mut ctx = DownloadContext {
        task_id,
        file_name: &task_meta.file_name,
        file_size: task_meta.file_size,
        save_path: &task_meta.save_path,
        pick_code: &task_meta.pick_code,
        expected_sha1: task_meta.expected_sha1.as_deref(),
        supports_range,
        segments: &mut segments,
        db,
        app,
        writer: &writer,
        progress_snapshot: &progress_snapshot,
        signal_rx: &mut signal_rx.clone(),
        url_rx: &url_rx,
        url_refresh_requested: &url_refresh_requested,
        semaphore: &segment_semaphore,
        conn_controller: &conn_controller,
    };

    let mut join_set: JoinSet<Result<(u16, u64), (Segment, DownloadError)>> = JoinSet::new();
    spawn_segments_with_stagger(
        &mut ctx,
        &mut join_set,
        client,
        token,
        user_agent,
        &progress_tx,
        true,
    )
    .await;
    collect_results(
        &mut ctx,
        &mut join_set,
        client,
        token,
        user_agent,
        progress_tx,
        &mut flush_handle,
        task_start_time,
        "resume",
    )
    .await
}
