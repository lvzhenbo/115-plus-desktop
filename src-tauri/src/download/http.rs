use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use futures_util::StreamExt;
use log::{error, info, warn};
use tokio::sync::{Semaphore, mpsc, watch};
use tokio::task::JoinSet;
use tokio::time::{Duration, interval};

use tauri::AppHandle;

use super::events::{
    DownloadProgressEvent, DownloadSegmentEvent, DownloadTaskEvent, SpeedCalculator,
    UrlExpiredEvent, emit_progress, emit_segment_status, emit_task_status, emit_url_expired,
};
use super::persistence::ProgressFile;
use super::segment::compute_segments;
use super::throttle::get_throttle;
use super::types::{
    DownloadConfig, DownloadError, DownloadTask, ProgressUpdate, RangeInfo, Segment, SegmentStatus,
    TaskStatus,
};
use super::writer::FileWriter;

/// 下载信号枚举 — 控制下载任务的暂停/取消 (per LC-01, LC-03)
#[derive(Clone, Debug, PartialEq)]
pub enum DownloadSignal {
    Running,
    Paused,
    Cancelled,
}

// 全局下载信号注册表 — 用于跨 Tauri command 发送暂停/取消信号
lazy_static::lazy_static! {
    static ref DOWNLOAD_SIGNALS: Arc<Mutex<HashMap<String, watch::Sender<DownloadSignal>>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

// 全局 URL channel 注册表 — 用于将前端获取的新 URL 传播给所有分片 (per URL-02, URL-03)
lazy_static::lazy_static! {
    static ref URL_CHANNELS: Arc<Mutex<HashMap<String, watch::Sender<String>>>> =
        Arc::new(Mutex::new(HashMap::new()));
}

/// HEAD 请求检测服务器 Range 支持情况 (per D-03, D-04, D-05, D-07)
///
/// 发送 HEAD 请求，解析 Accept-Ranges、ETag、Content-Length 响应头。
pub async fn detect_range_support(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    user_agent: &str,
) -> Result<RangeInfo, DownloadError> {
    let resp = client
        .head(url)
        .header("Authorization", format!("Bearer {}", token))
        .header("User-Agent", user_agent)
        .send()
        .await
        .map_err(DownloadError::Http)?;

    if !resp.status().is_success() {
        return Err(DownloadError::HttpStatus {
            status: resp.status().as_u16(),
            message: resp
                .status()
                .canonical_reason()
                .unwrap_or("Unknown")
                .to_string(),
        });
    }

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

    Ok(RangeInfo {
        supports_range,
        etag,
    })
}

/// 下载单个分片 (per D-06, D-07)
///
/// 当 `supports_range` 为 true 时发送 `Range: bytes=start-end`，
/// 否则发送不带 Range 的 GET 请求（全文件回退）。
/// 通过流式写入将数据写到文件的正确偏移位置。
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
    // 分片已完全下载 — 直接返回 (处理暂停后恢复时分片实际已完成但状态未更新的情况)
    // 避免构建无效 Range: bytes={end+1}-{end} 导致 416 错误
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
                message: format!("URL expired (HTTP {})", status),
            });
        }
        return Err(DownloadError::HttpStatus {
            status,
            message: resp
                .status()
                .canonical_reason()
                .unwrap_or("Unknown")
                .to_string(),
        });
    }

    // 验证 Range 请求返回 206 Partial Content (类似 aria2 的 HttpResponse::validateResponse)
    // 如果服务器忽略 Range 返回 200（完整内容），写入分片偏移会导致文件损坏
    if supports_range && resp.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(DownloadError::HttpStatus {
            status: resp.status().as_u16(),
            message: format!(
                "Expected 206 Partial Content for Range request, got {}",
                resp.status().as_u16()
            ),
        });
    }

    // 验证 Content-Range 头 — 确保服务器返回的范围与请求一致 (类似 aria2 的 HttpHeader 解析)
    // 防止 CDN/代理返回错误范围导致数据写偏
    if supports_range {
        let expected_start = segment.start + segment.downloaded;
        if let Some(cr) = resp
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
        {
            // 格式: "bytes START-END/TOTAL" 或 "bytes START-END/*"
            if let Some(range_part) = cr.strip_prefix("bytes ")
                && let Some(dash_pos) = range_part.find('-')
                && let Ok(actual_start) = range_part[..dash_pos].parse::<u64>()
                && actual_start != expected_start
            {
                return Err(DownloadError::HttpStatus {
                    status: 206,
                    message: format!(
                        "Content-Range mismatch: expected start={}, got start={}",
                        expected_start, actual_start
                    ),
                });
            }
        }
    }

    let mut stream = resp.bytes_stream();
    // supports_range=false 时服务器从 byte 0 发送完整文件，偏移必须从 0 开始
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

    // === 诊断计时器 ===
    let seg_start_time = std::time::Instant::now();
    let mut last_log_time = std::time::Instant::now();
    let mut last_log_bytes: u64 = total_written;
    let mut write_ns: u64 = 0;
    let mut throttle_ns: u64 = 0;
    let mut chunk_count: u64 = 0;

    info!(
        "[seg-{}][{}] 开始下载 range={}..{} downloaded={} expected={}",
        segment.index,
        task_id,
        segment.start + segment.downloaded,
        segment.end,
        segment.downloaded,
        segment.end - segment.start + 1
    );

    // 写缓冲区 — 累积到阈值后批量刷盘，类似 aria2 DiskAdaptor
    // 减少 syscall 次数: 从 ~6400次/段 降至 ~50次/段 (100MB 文件)
    const WRITE_BUFFER_SIZE: usize = 2 * 1024 * 1024; // 2MB
    let mut write_buffer: Vec<u8> = Vec::with_capacity(WRITE_BUFFER_SIZE);
    let mut buffer_start_offset = offset;

    // 分片边界保护 — 类似 aria2 的 piece boundary enforcement
    // 防止服务器返回超出请求范围的数据覆盖相邻分片
    let max_bytes_expected = segment.end - segment.start + 1 - segment.downloaded;

    // 消除首次 changed() 立即触发的问题 — tokio watch 首次调用 changed() 会立即返回
    signal_rx.borrow_and_update();

    /// 将缓冲区数据刷盘并报告进度 (写后报告，确保 DB 进度 ≤ 磁盘数据)
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
        // 只在刷盘后报告进度 — 确保 DB 记录 ≤ 实际磁盘数据，防止 resume 时出现零字节空洞
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

            // 优先检查暂停/取消信号 — 即使在等待数据块时也能立即响应
            result = signal_rx.changed() => {
                if result.is_err() {
                    // 信号通道关闭前，将缓冲区数据刷盘保留部分进度
                    flush_buffer(&writer, &mut write_buffer, buffer_start_offset,
                        &progress_tx, task_id, segment.index, total_written)?;
                    return Err(DownloadError::TaskAborted("signal channel closed".to_string()));
                }
                let signal = signal_rx.borrow_and_update().clone();
                match signal {
                    DownloadSignal::Paused => {
                        // 暂停前必须刷盘 — 否则 resume 时 DB 进度 > 磁盘数据 → 零字节空洞
                        flush_buffer(&writer, &mut write_buffer, buffer_start_offset,
                            &progress_tx, task_id, segment.index, total_written)?;
                        return Err(DownloadError::TaskAborted("paused".to_string()));
                    }
                    DownloadSignal::Cancelled => {
                        // 取消无需刷盘 — 文件将被删除
                        return Err(DownloadError::TaskAborted("cancelled".to_string()));
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
                        // 超时前刷盘保留部分进度
                        flush_buffer(&writer, &mut write_buffer, buffer_start_offset,
                            &progress_tx, task_id, segment.index, total_written)?;
                        warn!("[seg-{}][{}] 读取超时 60s, downloaded={}", segment.index, task_id, total_written);
                        return Err(DownloadError::TaskAborted("read timeout (60s)".to_string()));
                    }
                };
                let bytes = chunk.map_err(DownloadError::Http)?;

                // 分片边界保护 — 截断超出分片范围的数据 (类似 aria2 piece boundary enforcement)
                let bytes_remaining = max_bytes_expected.saturating_sub(total_written - segment.downloaded);
                if bytes_remaining == 0 {
                    break; // 已收到足够数据，丢弃多余字节
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

                // 缓冲区满时批量刷盘 — 大幅减少 seek+write syscall 次数
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

                // Global bandwidth throttle (per D-02: Token Bucket, each chunk consumes tokens)
                let t_throttle_start = std::time::Instant::now();
                get_throttle().consume(effective_bytes.len()).await;
                throttle_ns += t_throttle_start.elapsed().as_nanos() as u64;

                // 每 5 秒输出诊断日志
                if last_log_time.elapsed().as_secs() >= 5 {
                    let elapsed = last_log_time.elapsed().as_secs_f64();
                    let delta_bytes = total_written - last_log_bytes;
                    let speed_mbps = delta_bytes as f64 / elapsed / 1024.0 / 1024.0;
                    info!(
                        "[seg-{}][{}] 速度={:.1}MB/s downloaded={:.1}MB chunks={} write={:.0}ms throttle={:.0}ms",
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

    // 验证分片下载字节数 — 类似 aria2 piece-level verification
    // 防止服务器截断响应导致文件留零字节空洞（预分配区域）
    let expected_total = segment.end - segment.start + 1;
    if total_written != expected_total {
        warn!(
            "[seg-{}][{}] 字节数不匹配: expected={} got={} elapsed={:.1}s",
            segment.index,
            task_id,
            expected_total,
            total_written,
            seg_start_time.elapsed().as_secs_f64()
        );
        return Err(DownloadError::TaskAborted(format!(
            "segment {} size mismatch: expected {} bytes, got {}",
            segment.index, expected_total, total_written
        )));
    }

    info!(
        "[seg-{}][{}] 完成 {:.1}MB 耗时={:.1}s 平均速度={:.1}MB/s chunks={} write={:.0}ms throttle={:.0}ms",
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
/// 重试基准延迟 (毫秒) — 指数退避: 1s, 2s, 4s
const RETRY_BASE_DELAY_MS: u64 = 1000;

/// 带指数退避的分片下载重试包装器 (per D-03, LC-04)
///
/// 对瞬态错误 (Http, 5xx) 最多重试 MAX_SEGMENT_RETRIES 次。
/// 不重试: TaskAborted (信号), Io (磁盘), 4xx 客户端错误。
/// 重试时携带部分进度 — 从断点续传，类似 aria2 的 piece retry。
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
    let mut last_error: Option<DownloadError> = None;

    // 可变分片副本 — 重试时更新 downloaded 实现断点续传 (类似 aria2 piece completedLength)
    let mut local_seg = segment.clone();
    // 共享原子计数器 — 追踪本分片最新已下载字节，重试时回读 (per aria2 piece resume)
    let last_downloaded = Arc::new(std::sync::atomic::AtomicU64::new(segment.downloaded));
    let last_downloaded_for_tx = last_downloaded.clone();

    // 包装 progress_tx: 拦截进度更新，同步写入 last_downloaded
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

    /// 从共享计数器回读最新进度并更新分片副本 (类似 aria2 piece completedLength)
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

    // 主重试循环 — 所有退出路径统一走 break + 尾部清理
    let result: Result<u64, DownloadError> = 'retry_loop: {
        loop {
            let current_url = url_rx.borrow().clone();

            if retry_count > 0 {
                let base_delay_ms = RETRY_BASE_DELAY_MS * 2u64.pow(retry_count - 1);
                // 按分片索引错开重试时间，防止所有分片同时重试导致CDN再次拒绝
                let jitter_ms = ((local_seg.index as u64) % 16) * 150;
                let delay_ms = base_delay_ms + jitter_ms;
                warn!(
                    "[seg-{}][{}] 重试 #{} (延迟{}ms, 含抖动{}ms) downloaded={}",
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
                        break 'retry_loop Err(last_error.unwrap_or(DownloadError::TaskAborted(
                            "signal_during_retry".to_string(),
                        )));
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

                    // HTTP 403 可能是CDN并发连接数限制（非真正URL过期）
                    // 先用指数退避+分片索引抖动重试，自然错开并发请求
                    if status == 403 && retry_count < 2 {
                        retry_count += 1;
                        warn!(
                            "[seg-{}][{}] HTTP 403 疑似CDN限流, 退避重试#{} downloaded={:.1}MB",
                            local_seg.index,
                            task_id,
                            retry_count,
                            local_seg.downloaded as f64 / 1024.0 / 1024.0
                        );
                        last_error = Some(DownloadError::UrlExpired { status, message });
                        continue; // 回到循环顶部，触发指数退避+抖动
                    }

                    // 退避2次后仍然403 → CDN并发限流，释放permit让编排层重新调度
                    if status == 403 {
                        warn!(
                            "[seg-{}][{}] CDN限流确认, 释放连接等待重新调度 downloaded={:.1}MB",
                            local_seg.index,
                            task_id,
                            local_seg.downloaded as f64 / 1024.0 / 1024.0
                        );
                        break 'retry_loop Err(DownloadError::CdnRateLimit);
                    }

                    // 非403状态码(401/410) → 真正的URL过期
                    url_refresh_count += 1;
                    warn!(
                        "[seg-{}][{}] URL过期 (HTTP {}) (第{}次刷新) downloaded={:.1}MB",
                        local_seg.index,
                        task_id,
                        status,
                        url_refresh_count,
                        local_seg.downloaded as f64 / 1024.0 / 1024.0
                    );
                    if url_refresh_count > MAX_URL_REFRESHES {
                        break 'retry_loop Err(DownloadError::TaskAborted(
                            "max URL refreshes exceeded".to_string(),
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
                                            "URL channel closed".to_string(),
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
                            info!(
                                "[seg-{}][{}] URL已刷新 new_url={}...",
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
                                "URL refresh timeout (30s)".to_string(),
                            ));
                        }
                    }
                }
                Err(e) => {
                    if !is_retryable_error(&e) {
                        error!(
                            "[seg-{}][{}] 不可重试错误: {:?}",
                            local_seg.index, task_id, e
                        );
                        break 'retry_loop Err(e);
                    }
                    sync_partial_progress(supports_range, &last_downloaded, &mut local_seg);
                    retry_count += 1;
                    if retry_count > MAX_SEGMENT_RETRIES {
                        error!(
                            "[seg-{}][{}] 重试耗尽 ({}/{}): {:?}",
                            local_seg.index, task_id, retry_count, MAX_SEGMENT_RETRIES, e
                        );
                        break 'retry_loop Err(e);
                    }
                    last_error = Some(e);
                }
            }
        }
    };

    // 统一清理: 关闭 interceptor channel，等待转发任务结束
    drop(intercepted_tx);
    let _ = forwarding_handle.await;

    result
}

/// 判断下载错误是否为瞬态、值得重试
fn is_retryable_error(err: &DownloadError) -> bool {
    match err {
        DownloadError::Http(_) => true,
        DownloadError::HttpStatus { status, .. } => *status >= 500,
        DownloadError::UrlExpired { .. } => false,
        DownloadError::CdnRateLimit => false, // 由编排层处理重新排队
        DownloadError::TaskAborted(_) => false,
        DownloadError::Io(_) => false,
        _ => false,
    }
}

/// 下载完成后计算文件 SHA1 并与期望值对比 (per D-06, D-07, D-08)
///
/// Returns Ok(true) if SHA1 matches or no expected_sha1 (skip verification per D-08).
/// Returns Ok(false) if SHA1 mismatch.
async fn verify_file_sha1(
    file_path: &str,
    expected_sha1: Option<&str>,
) -> Result<bool, DownloadError> {
    let Some(expected) = expected_sha1 else {
        return Ok(true); // No expected SHA1 — skip verification (per D-08)
    };
    let expected = expected.to_uppercase();
    let path = file_path.to_string();
    let computed = tokio::task::spawn_blocking(move || -> Result<String, DownloadError> {
        use sha1::{Digest, Sha1};
        use std::io::Read;
        let mut file = std::fs::File::open(&path).map_err(DownloadError::Io)?;
        let mut hasher = Sha1::new();
        let mut buffer = vec![0u8; 1024 * 1024]; // 1MB buffer (matches upload.rs pattern)
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

/// 判断 HTTP 状态码是否表示 URL 过期（OSS 预签名 URL 失效）(per URL-01)
fn is_url_expired(status: u16) -> bool {
    status == 401 || status == 403 || status == 410
}

/// 最大重分配次数 — 同一任务中的累计重分配上限 (per D-11)
const MAX_REALLOCATIONS: u32 = 3;
/// 重分配子分片起始索引偏移 — 避免与原始分片索引冲突
const REALLOC_INDEX_BASE: u16 = 1000;

/// 计算失败分片的剩余范围并拆分为新子分片 (per D-10)
///
/// `active_count`: 当前仍在运行的分片数（用于决定拆分数）
/// `realloc_counter`: 已执行的重分配次数（用于子分片索引偏移）
///
/// Returns new sub-segments to spawn, or None if no remaining bytes.
fn reallocate_failed_segment(
    failed_segment: &Segment,
    active_count: usize,
    realloc_counter: u32,
) -> Option<Vec<Segment>> {
    let remaining_start = failed_segment.start + failed_segment.downloaded;
    let remaining_end = failed_segment.end;
    if remaining_start > remaining_end {
        return None; // No remaining bytes
    }
    let remaining_bytes = remaining_end - remaining_start + 1;
    if remaining_bytes == 0 {
        return None;
    }

    // Split remaining range into N parts (N = max(active_count, 1), capped at 4)
    let split_count = (active_count.max(1)).min(4) as u16;
    let chunk_size = remaining_bytes / split_count as u64;
    if chunk_size == 0 {
        // Too small to split — create single sub-segment
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
            remaining_end // Last sub-segment absorbs remainder
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

/// 高层下载编排 — 多分片并行下载
///
/// 流程: 磁盘空间检查 → Range 检测 → 分片计算 → 文件预分配 → Semaphore+JoinSet 并行下载
pub async fn download_file(
    client: &reqwest::Client,
    task: &mut DownloadTask,
    token: &str,
    user_agent: &str,
    config: &DownloadConfig,
    db: &Arc<ProgressFile>,
    app: &AppHandle,
) -> Result<(), DownloadError> {
    // Step 1: 检查磁盘空间
    FileWriter::check_disk_space(&task.save_path, task.file_size)?;

    // Apply speed limit from config (per D-03)
    if config.speed_limit > 0 {
        super::throttle::set_speed_limit(config.speed_limit);
    }

    // Step 2: 检测 Range 支持
    let range_info = detect_range_support(client, &task.url, token, user_agent).await?;

    // Step 3: 存储 ETag
    task.etag = range_info.etag;

    // Step 4: 根据 Range 支持决定分片数
    let split = if range_info.supports_range {
        config.split
    } else {
        1
    };
    task.segments = compute_segments(task.file_size, split);

    info!(
        "[task][{}] 开始下载 file={} size={:.1}MB segments={} range={} url={}...",
        task.task_id,
        task.file_name,
        task.file_size as f64 / 1024.0 / 1024.0,
        task.segments.len(),
        range_info.supports_range,
        &task.url[..task.url.len().min(80)]
    );

    // Step 5: 创建 FileWriter 并预分配文件
    let writer = FileWriter::create(&task.save_path, task.file_size)?;

    // Step 5.5: 保存任务和分片元数据到 SQLite
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

    // Step 5.6: 创建信号 channel 并注册到全局表
    let (signal_tx, signal_rx) = watch::channel(DownloadSignal::Running);
    {
        let mut signals = DOWNLOAD_SIGNALS.lock().unwrap();
        signals.insert(task.task_id.clone(), signal_tx);
    }

    // Create URL watch channel for URL refresh (per URL-02, URL-03)
    let (url_tx, url_rx) = watch::channel(task.url.clone());
    {
        let mut channels = URL_CHANNELS.lock().unwrap();
        channels.insert(task.task_id.clone(), url_tx);
    }
    let url_refresh_requested = Arc::new(AtomicBool::new(false));

    // Step 6: 创建进度 channel + 启动 flush collector
    let (progress_tx, mut progress_rx) = mpsc::channel::<ProgressUpdate>(1024);
    let flush_db = Arc::clone(db);
    let flush_app = app.clone();
    let flush_task_id = task.task_id.clone();
    let flush_file_size = task.file_size;

    let progress_snapshot: Arc<Mutex<HashMap<u16, u64>>> = Arc::new(Mutex::new(HashMap::new()));
    let flush_snapshot = progress_snapshot.clone();
    let flush_writer = writer.clone();

    let flush_handle = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_millis(500));
        let mut pending: std::collections::HashMap<(String, u16), u64> =
            std::collections::HashMap::new();
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
                        let _ = flush_db.batch_update_downloaded(&updates);
                        let _ = flush_writer.sync_data();
                    }
                    // Emit progress event with EMA speed + ETA
                    let cumulative_downloaded: u64 = flush_snapshot.lock().unwrap().values().sum();
                    let speed = speed_calc.update(cumulative_downloaded);
                    let remaining = flush_file_size.saturating_sub(cumulative_downloaded);
                    let eta = speed_calc.eta(remaining);

                    // 每 5 秒输出总体速度日志
                    if tick_count % 10 == 0 {
                        log::info!(
                            "[task][{}] 进度: {:.1}/{:.1}MB ({:.0}%) 速度={:.1}MB/s ETA={:.0}s",
                            flush_task_id,
                            cumulative_downloaded as f64 / 1024.0 / 1024.0,
                            flush_file_size as f64 / 1024.0 / 1024.0,
                            cumulative_downloaded as f64 / flush_file_size as f64 * 100.0,
                            speed / 1024.0 / 1024.0,
                            eta.unwrap_or(0.0),
                        );
                    }

                    emit_progress(&flush_app, &DownloadProgressEvent {
                        task_id: flush_task_id.clone(),
                        downloaded_bytes: cumulative_downloaded,
                        total_bytes: flush_file_size,
                        speed,
                        eta_secs: eta,
                    });
                }
                msg = progress_rx.recv() => {
                    match msg {
                        Some(update) => {
                            pending.insert(
                                (update.task_id.clone(), update.segment_index),
                                update.downloaded,
                            );
                            flush_snapshot.lock().unwrap().insert(update.segment_index, update.downloaded);
                        }
                        None => {
                            // Channel closed — final flush
                            if !pending.is_empty() {
                                let updates: Vec<(String, u16, u64)> = pending
                                    .drain()
                                    .map(|((task_id, idx), downloaded)| (task_id, idx, downloaded))
                                    .collect();
                                let _ = flush_db.batch_update_downloaded(&updates);
                            }
                            break;
                        }
                    }
                }
            }
        }
    });

    // Semaphore+JoinSet 并行下载
    task.status = TaskStatus::Active;
    emit_task_status(
        app,
        &DownloadTaskEvent {
            task_id: task.task_id.clone(),
            status: TaskStatus::Active,
        },
    );
    let semaphore = Arc::new(Semaphore::new(config.max_connections_per_server as usize));
    let mut join_set: JoinSet<Result<(u16, u64), (Segment, DownloadError)>> = JoinSet::new();

    let supports_range = range_info.supports_range;
    for segment in &task.segments {
        // 即时持久化: 标记分片进入 Downloading 状态
        let _ = db.update_segment_status(
            &task.task_id,
            segment.index,
            &SegmentStatus::Downloading,
            segment.downloaded,
        );
        emit_segment_status(
            app,
            &DownloadSegmentEvent {
                task_id: task.task_id.clone(),
                segment_index: segment.index,
                status: SegmentStatus::Downloading,
                downloaded: segment.downloaded,
            },
        );
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| DownloadError::TaskAborted(e.to_string()))?;
        let client = client.clone();
        let url_rx = url_rx.clone();
        let token = token.to_string();
        let user_agent = user_agent.to_string();
        let seg = segment.clone();
        let writer = writer.clone();
        let tx = progress_tx.clone();
        let tid = task.task_id.clone();
        let pick_code = task.pick_code.clone();
        let sig_rx = signal_rx.clone();
        let url_refresh_req = url_refresh_requested.clone();
        let app_clone = app.clone();

        join_set.spawn(async move {
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

    // Collect results — 按分片处理错误，支持暂停/取消 + 分片重分配 (per D-10)
    let mut has_failure = false;
    let mut is_paused = false;
    let mut is_cancelled = false;
    let mut realloc_counter: u32 = 0;
    let task_start_time = std::time::Instant::now();
    let mut completed_segments: u32 = 0;
    let total_segments = task.segments.len() as u32;
    // CDN限流重排队追踪 — 被动降低并发 (类似 aria2)
    let mut cdn_retry_counts: HashMap<u16, u32> = HashMap::new();
    const MAX_CDN_RETRIES: u32 = 50;
    // 任务级URL刷新追踪 — 只有当所有分片都卡住时才刷新URL
    let mut last_success_time = std::time::Instant::now();
    let mut task_url_refresh_count: u32 = 0;
    const MAX_TASK_URL_REFRESHES: u32 = 10;
    const ALL_STUCK_THRESHOLD_SECS: u64 = 60;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok((index, bytes))) => {
                completed_segments += 1;
                last_success_time = std::time::Instant::now();
                info!(
                    "[task][{}] 分片{} 完成 {:.1}MB ({}/{})",
                    task.task_id,
                    index,
                    bytes as f64 / 1024.0 / 1024.0,
                    completed_segments,
                    total_segments
                );
                if let Some(seg) = task.segments.iter_mut().find(|s| s.index == index) {
                    seg.status = SegmentStatus::Completed;
                    seg.downloaded = bytes;
                }
                let _ = db.update_segment_status(
                    &task.task_id,
                    index,
                    &SegmentStatus::Completed,
                    bytes,
                );
                emit_segment_status(
                    app,
                    &DownloadSegmentEvent {
                        task_id: task.task_id.clone(),
                        segment_index: index,
                        status: SegmentStatus::Completed,
                        downloaded: bytes,
                    },
                );
            }
            Ok(Err((_, DownloadError::TaskAborted(ref reason)))) if reason == "paused" => {
                info!("[task][{}] 任务暂停", task.task_id);
                is_paused = true;
                join_set.abort_all();
                break;
            }
            Ok(Err((_, DownloadError::TaskAborted(ref reason)))) if reason == "cancelled" => {
                info!("[task][{}] 任务取消", task.task_id);
                is_cancelled = true;
                join_set.abort_all();
                break;
            }
            Ok(Err((failed_seg, DownloadError::CdnRateLimit))) => {
                // CDN限流 — 被动降低并发: 释放permit后延迟重新排队 (类似 aria2)
                let count = cdn_retry_counts.entry(failed_seg.index).or_insert(0);
                *count += 1;

                // 检查是否所有分片都卡住 — 如果长时间无任何分片成功，可能URL真的过期了
                let all_stuck_duration = last_success_time.elapsed().as_secs();
                if all_stuck_duration > ALL_STUCK_THRESHOLD_SECS && join_set.is_empty() {
                    task_url_refresh_count += 1;
                    warn!(
                        "[task][{}] 所有分片停滞{}s, 触发任务级URL刷新 (第{}次)",
                        task.task_id, all_stuck_duration, task_url_refresh_count
                    );
                    if task_url_refresh_count > MAX_TASK_URL_REFRESHES {
                        warn!("[task][{}] 任务级URL刷新耗尽, 标记失败", task.task_id);
                        has_failure = true;
                        continue;
                    }
                    // 触发URL刷新
                    if url_refresh_requested
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        emit_url_expired(
                            app,
                            &UrlExpiredEvent {
                                task_id: task.task_id.clone(),
                                pick_code: task.pick_code.clone(),
                            },
                        );
                    }
                }

                if *count > MAX_CDN_RETRIES {
                    warn!(
                        "[task][{}] 分片{} CDN限流重试耗尽 ({}/{})",
                        task.task_id, failed_seg.index, count, MAX_CDN_RETRIES
                    );
                    has_failure = true;
                    continue;
                }

                // 指数退避 + 分片索引抖动，延迟后重新排队
                let backoff_secs = 2u64.pow((*count).min(6));
                let jitter_ms = ((failed_seg.index as u64) % 16) * 300;
                let delay = Duration::from_secs(backoff_secs) + Duration::from_millis(jitter_ms);

                info!(
                    "[task][{}] 分片{} CDN限流重排队 #{} (延迟{:.1}s, 当前活跃{})",
                    task.task_id,
                    failed_seg.index,
                    count,
                    delay.as_secs_f64(),
                    join_set.len()
                );

                // 重新排队 — 延迟后重新获取permit，被动降低并发
                // 用 progress_snapshot 获取最新已下载字节 (修复 CDN 重试时 seg.downloaded 过期导致从头重下)
                let mut seg = failed_seg;
                let actual_downloaded = progress_snapshot
                    .lock()
                    .unwrap()
                    .get(&seg.index)
                    .copied()
                    .unwrap_or(seg.downloaded);
                seg.downloaded = actual_downloaded;

                let semaphore = semaphore.clone();
                let client = client.clone();
                let url_rx = url_rx.clone();
                let token = token.to_string();
                let user_agent = user_agent.to_string();
                let writer = writer.clone();
                let tx = progress_tx.clone();
                let tid = task.task_id.clone();
                let pick_code = task.pick_code.clone();
                let sig_rx = signal_rx.clone();
                let url_refresh_req = url_refresh_requested.clone();
                let app_clone = app.clone();

                join_set.spawn(async move {
                    // 先等待退避延迟 — 此时不持有permit，其他分片可以继续
                    tokio::time::sleep(delay).await;
                    // 检查是否已被取消/暂停
                    {
                        let signal = sig_rx.borrow().clone();
                        if signal != DownloadSignal::Running {
                            return Err((
                                seg,
                                DownloadError::TaskAborted(
                                    if signal == DownloadSignal::Paused {
                                        "paused"
                                    } else {
                                        "cancelled"
                                    }
                                    .to_string(),
                                ),
                            ));
                        }
                    }
                    // 重新获取permit — 如果其他分片正在下载，这里会排队等待
                    let permit = match semaphore.acquire_owned().await {
                        Ok(p) => p,
                        Err(_) => {
                            return Err((
                                seg,
                                DownloadError::TaskAborted("semaphore closed".to_string()),
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
            Ok(Err((failed_seg, _e))) => {
                // 分片重试耗尽后失败 — 尝试重分配 (per D-10, D-11)
                warn!(
                    "[task][{}] 分片{} 失败: {:?} downloaded={:.1}MB",
                    task.task_id,
                    failed_seg.index,
                    _e,
                    failed_seg.downloaded as f64 / 1024.0 / 1024.0
                );
                // 用 progress_snapshot 获取最新已下载字节 (修复过期 downloaded 导致重复下载)
                let actual_downloaded = progress_snapshot
                    .lock()
                    .unwrap()
                    .get(&failed_seg.index)
                    .copied()
                    .unwrap_or(failed_seg.downloaded);
                let mut updated_seg = failed_seg.clone();
                updated_seg.downloaded = actual_downloaded;

                if realloc_counter < MAX_REALLOCATIONS {
                    let active = join_set.len();
                    if let Some(sub_segs) =
                        reallocate_failed_segment(&updated_seg, active, realloc_counter)
                    {
                        realloc_counter += 1;
                        // Mark original segment as Reallocated — 同步更新内存 + DB
                        if let Some(orig) = task
                            .segments
                            .iter_mut()
                            .find(|s| s.index == updated_seg.index)
                        {
                            orig.status = SegmentStatus::Reallocated;
                            orig.downloaded = actual_downloaded;
                        }
                        let _ = db.update_segment_status(
                            &task.task_id,
                            updated_seg.index,
                            &SegmentStatus::Reallocated,
                            actual_downloaded,
                        );
                        // Persist and spawn sub-segments — 同时追踪到内存向量
                        let _ = db.insert_segments(&task.task_id, &sub_segs);
                        let sub_segs_tracking: Vec<Segment> = sub_segs
                            .iter()
                            .map(|s| {
                                let mut ts = s.clone();
                                ts.status = SegmentStatus::Downloading;
                                ts
                            })
                            .collect();
                        let mut realloc_ok = true;
                        for sub_seg in sub_segs {
                            let _ = db.update_segment_status(
                                &task.task_id,
                                sub_seg.index,
                                &SegmentStatus::Downloading,
                                0,
                            );
                            let permit = match semaphore.clone().acquire_owned().await {
                                Ok(p) => p,
                                Err(_) => {
                                    realloc_ok = false;
                                    break;
                                }
                            };
                            let client = client.clone();
                            let url_rx = url_rx.clone();
                            let token = token.to_string();
                            let user_agent = user_agent.to_string();
                            let seg = sub_seg;
                            let writer = writer.clone();
                            let tx = progress_tx.clone();
                            let tid = task.task_id.clone();
                            let pick_code = task.pick_code.clone();
                            let sig_rx = signal_rx.clone();
                            let url_refresh_req = url_refresh_requested.clone();
                            let app_clone = app.clone();

                            join_set.spawn(async move {
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
                        if realloc_ok {
                            // 追踪子分片到内存向量 — 确保暂停/失败处理能遍历到它们
                            task.segments.extend(sub_segs_tracking);
                            continue; // Reallocation successful
                        }
                        // Fall through to has_failure = true
                    }
                }
                has_failure = true;
            }
            Err(_e) => {
                // JoinError (task panic) — 标记为失败，继续
                has_failure = true;
            }
        }
    }

    // 处理暂停结果
    if is_paused {
        {
            let latest_progress = progress_snapshot.lock().unwrap();
            for seg in &task.segments {
                if seg.status == SegmentStatus::Completed
                    || seg.status == SegmentStatus::Reallocated
                {
                    continue;
                }
                let downloaded = latest_progress
                    .get(&seg.index)
                    .copied()
                    .unwrap_or(seg.downloaded);
                let _ = db.update_segment_status(
                    &task.task_id,
                    seg.index,
                    &SegmentStatus::Paused,
                    downloaded,
                );
                emit_segment_status(
                    app,
                    &DownloadSegmentEvent {
                        task_id: task.task_id.clone(),
                        segment_index: seg.index,
                        status: SegmentStatus::Paused,
                        downloaded,
                    },
                );
            }
        }
        task.status = TaskStatus::Paused;
        let _ = db.update_task_status(&task.task_id, "paused");
        emit_task_status(
            app,
            &DownloadTaskEvent {
                task_id: task.task_id.clone(),
                status: TaskStatus::Paused,
            },
        );
        drop(progress_tx);
        let _ = flush_handle.await;
        {
            let mut signals = DOWNLOAD_SIGNALS.lock().unwrap();
            signals.remove(&task.task_id);
        }
        {
            let mut channels = URL_CHANNELS.lock().unwrap();
            channels.remove(&task.task_id);
        }
        return Ok(());
    }

    // 处理取消结果
    if is_cancelled {
        drop(progress_tx);
        let _ = flush_handle.await;
        let _ = db.delete_task(&task.task_id);
        let _ = std::fs::remove_file(&task.save_path);
        task.status = TaskStatus::Error;
        emit_task_status(
            app,
            &DownloadTaskEvent {
                task_id: task.task_id.clone(),
                status: TaskStatus::Error,
            },
        );
        {
            let mut signals = DOWNLOAD_SIGNALS.lock().unwrap();
            signals.remove(&task.task_id);
        }
        {
            let mut channels = URL_CHANNELS.lock().unwrap();
            channels.remove(&task.task_id);
        }
        return Ok(());
    }

    // Drop sender to close channel, then await final flush
    drop(progress_tx);
    let _ = flush_handle.await;

    // Step 7: 更新任务状态
    if has_failure {
        let latest_progress = progress_snapshot.lock().unwrap();
        for seg in &task.segments {
            if seg.status == SegmentStatus::Completed || seg.status == SegmentStatus::Reallocated {
                continue;
            }
            let downloaded = latest_progress
                .get(&seg.index)
                .copied()
                .unwrap_or(seg.downloaded);
            let _ = db.update_segment_status(
                &task.task_id,
                seg.index,
                &SegmentStatus::Failed,
                downloaded,
            );
            emit_segment_status(
                app,
                &DownloadSegmentEvent {
                    task_id: task.task_id.clone(),
                    segment_index: seg.index,
                    status: SegmentStatus::Failed,
                    downloaded,
                },
            );
        }
        task.status = TaskStatus::Error;
        let _ = db.update_task_status(&task.task_id, "error");
        error!(
            "[task][{}] 下载失败 file={} elapsed={:.1}s",
            task.task_id,
            task.file_name,
            task_start_time.elapsed().as_secs_f64()
        );
        emit_task_status(
            app,
            &DownloadTaskEvent {
                task_id: task.task_id.clone(),
                status: TaskStatus::Error,
            },
        );
    } else {
        // SHA1 verification before marking complete (per D-06)
        let sha1_ok = verify_file_sha1(&task.save_path, task.expected_sha1.as_deref())
            .await
            .unwrap_or(false); // IO error during verify → treat as failed

        if sha1_ok {
            task.status = TaskStatus::Complete;
            // 下载完成，删除 .oofp 进度文件（类似 aria2 删除 .aria2）
            db.delete_task(&task.task_id)?;
            let elapsed = task_start_time.elapsed().as_secs_f64();
            info!(
                "[task][{}] 下载完成 file={} size={:.1}MB elapsed={:.1}s avg_speed={:.1}MB/s",
                task.task_id,
                task.file_name,
                task.file_size as f64 / 1024.0 / 1024.0,
                elapsed,
                task.file_size as f64 / elapsed / 1024.0 / 1024.0,
            );
            emit_task_status(
                app,
                &DownloadTaskEvent {
                    task_id: task.task_id.clone(),
                    status: TaskStatus::Complete,
                },
            );
        } else {
            task.status = TaskStatus::VerifyFailed;
            let _ = db.update_task_status(&task.task_id, "verify_failed");
            emit_task_status(
                app,
                &DownloadTaskEvent {
                    task_id: task.task_id.clone(),
                    status: TaskStatus::VerifyFailed,
                },
            );
        }
    }

    // 清理信号注册
    {
        let mut signals = DOWNLOAD_SIGNALS.lock().unwrap();
        signals.remove(&task.task_id);
    }
    {
        let mut channels = URL_CHANNELS.lock().unwrap();
        channels.remove(&task.task_id);
    }

    Ok(())
}

/// 恢复中断的下载任务 (per D-06, D-07)
///
/// 流程:
/// 1. 从 .oofp 文件加载分片 → 2. HEAD + ETag 验证 →
/// 3a. ETag 匹配: 跳过已完成分片，从 downloaded 字节恢复 →
/// 3b. ETag 不匹配: 清除分片进度，从头重新下载
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
) -> Result<(), DownloadError> {
    // Step 1: 从 .oofp 文件加载任务元数据
    let task_meta = db.load_task(save_path)?;
    if task_meta.task_id != task_id {
        return Err(DownloadError::FileNotFound(format!(
            "Task ID mismatch: expected {}, found {}",
            task_id, task_meta.task_id
        )));
    }

    // Step 2: If-Range + ETag 验证 (per D-07)
    let mut need_restart = false;
    let range_info = detect_range_support(client, url, token, user_agent).await?;
    let supports_range = range_info.supports_range;

    // 无 Range 支持则无法续传，必须从头下载
    if !supports_range {
        need_restart = true;
    }

    if let Some(ref stored_etag) = task_meta.etag {
        match range_info.etag {
            Some(ref server_etag) if server_etag == stored_etag => {
                // ETag matches — safe to resume
            }
            _ => {
                // ETag mismatch or missing — file changed, restart
                need_restart = true;
            }
        }
    }

    // Step 3: ETag 不匹配 — 静默清除并从头下载 (per D-07)
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
        return download_file(client, &mut fresh_task, token, user_agent, config, db, app).await;
    }

    // Step 4: 验证下载文件存在
    if !std::path::Path::new(&task_meta.save_path).exists() {
        return Err(DownloadError::FileNotFound(format!(
            "Download file missing: {}",
            task_meta.save_path
        )));
    }

    // 打开已有文件用于续传写入
    let writer = FileWriter::open(&task_meta.save_path)?;

    // Step 5: 构建恢复任务 — 跳过已完成分片
    let mut segments = task_meta.segments;
    let completed_count = segments
        .iter()
        .filter(|s| s.status == SegmentStatus::Completed)
        .count();
    let already_downloaded: u64 = segments.iter().map(|s| s.downloaded).sum();
    info!(
        "[resume][{}] 恢复下载 file={} 已完成分片={}/{} 已下载={:.1}MB/{:.1}MB",
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

    // 创建信号 channel 并注册到全局表
    let (signal_tx, signal_rx) = watch::channel(DownloadSignal::Running);
    {
        let mut signals = DOWNLOAD_SIGNALS.lock().unwrap();
        signals.insert(task_id.to_string(), signal_tx);
    }

    // Create URL watch channel for URL refresh
    let (url_tx, url_rx) = watch::channel(url.to_string());
    {
        let mut channels = URL_CHANNELS.lock().unwrap();
        channels.insert(task_id.to_string(), url_tx);
    }
    let url_refresh_requested = Arc::new(AtomicBool::new(false));

    // 创建进度 channel + flush collector
    let (progress_tx, mut progress_rx) = mpsc::channel::<ProgressUpdate>(1024);
    let flush_db = Arc::clone(db);
    let flush_app = app.clone();
    let flush_task_id = task_id.to_string();
    let flush_file_size = task_meta.file_size;

    // Seed progress snapshot with existing segment progress for resume
    let progress_snapshot: Arc<Mutex<HashMap<u16, u64>>> = Arc::new(Mutex::new(
        segments
            .iter()
            .filter(|s| s.downloaded > 0)
            .map(|s| (s.index, s.downloaded))
            .collect(),
    ));
    let flush_snapshot = progress_snapshot.clone();
    let flush_writer = writer.clone();

    let flush_handle = tokio::spawn(async move {
        let mut ticker = interval(Duration::from_millis(500));
        let mut pending: std::collections::HashMap<(String, u16), u64> =
            std::collections::HashMap::new();
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
                        let _ = flush_db.batch_update_downloaded(&updates);
                        let _ = flush_writer.sync_data();
                    }
                    let cumulative_downloaded: u64 = flush_snapshot.lock().unwrap().values().sum();
                    let speed = speed_calc.update(cumulative_downloaded);
                    let remaining = flush_file_size.saturating_sub(cumulative_downloaded);
                    let eta = speed_calc.eta(remaining);

                    // 每 5 秒输出总体速度日志
                    if tick_count % 10 == 0 {
                        log::info!(
                            "[resume][{}] 进度: {:.1}/{:.1}MB ({:.0}%) 速度={:.1}MB/s ETA={:.0}s",
                            flush_task_id,
                            cumulative_downloaded as f64 / 1024.0 / 1024.0,
                            flush_file_size as f64 / 1024.0 / 1024.0,
                            cumulative_downloaded as f64 / flush_file_size as f64 * 100.0,
                            speed / 1024.0 / 1024.0,
                            eta.unwrap_or(0.0),
                        );
                    }

                    emit_progress(&flush_app, &DownloadProgressEvent {
                        task_id: flush_task_id.clone(),
                        downloaded_bytes: cumulative_downloaded,
                        total_bytes: flush_file_size,
                        speed,
                        eta_secs: eta,
                    });
                }
                msg = progress_rx.recv() => {
                    match msg {
                        Some(update) => {
                            pending.insert(
                                (update.task_id.clone(), update.segment_index),
                                update.downloaded,
                            );
                            flush_snapshot.lock().unwrap().insert(update.segment_index, update.downloaded);
                        }
                        None => {
                            if !pending.is_empty() {
                                let updates: Vec<(String, u16, u64)> = pending
                                    .drain()
                                    .map(|((task_id, idx), downloaded)| (task_id, idx, downloaded))
                                    .collect();
                                let _ = flush_db.batch_update_downloaded(&updates);
                            }
                            break;
                        }
                    }
                }
            }
        }
    });

    // Semaphore+JoinSet 并行恢复下载（跳过已完成分片）
    let semaphore = Arc::new(Semaphore::new(config.max_connections_per_server as usize));
    let mut join_set: JoinSet<Result<(u16, u64), (Segment, DownloadError)>> = JoinSet::new();

    for segment in &segments {
        if segment.status == SegmentStatus::Completed
            || segment.status == SegmentStatus::Reallocated
        {
            continue; // 跳过已完成和已重分配的分片 (已重分配的范围由子分片覆盖)
        }

        // 即时持久化: 标记分片进入 Downloading 状态
        let _ = db.update_segment_status(
            task_id,
            segment.index,
            &SegmentStatus::Downloading,
            segment.downloaded,
        );
        emit_segment_status(
            app,
            &DownloadSegmentEvent {
                task_id: task_id.to_string(),
                segment_index: segment.index,
                status: SegmentStatus::Downloading,
                downloaded: segment.downloaded,
            },
        );
        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| DownloadError::TaskAborted(e.to_string()))?;
        let client = client.clone();
        let url_rx = url_rx.clone();
        let token = token.to_string();
        let user_agent = user_agent.to_string();
        let seg = segment.clone();
        let writer = writer.clone();
        let tx = progress_tx.clone();
        let tid = task_id.to_string();
        let pick_code = task_meta.pick_code.clone();
        let sig_rx = signal_rx.clone();
        let url_refresh_req = url_refresh_requested.clone();
        let app_clone = app.clone();

        join_set.spawn(async move {
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

    // Collect results — 按分片处理错误，支持暂停/取消 + 分片重分配 (per D-10)
    let mut has_failure = false;
    let mut is_paused = false;
    let mut is_cancelled = false;
    // 从已有重分配子分片推断计数器起始值 — 避免 resume 时索引碰撞覆盖旧子分片
    let mut realloc_counter: u32 = segments
        .iter()
        .filter(|s| s.index >= REALLOC_INDEX_BASE)
        .map(|s| (s.index - REALLOC_INDEX_BASE) / 10 + 1)
        .max()
        .map(|v| v as u32)
        .unwrap_or(0);
    // CDN限流重排队追踪 — 被动降低并发 (类似 aria2)
    let mut cdn_retry_counts: HashMap<u16, u32> = HashMap::new();
    const MAX_CDN_RETRIES: u32 = 50;
    let mut last_success_time = std::time::Instant::now();
    let mut task_url_refresh_count: u32 = 0;
    const MAX_TASK_URL_REFRESHES: u32 = 10;
    const ALL_STUCK_THRESHOLD_SECS: u64 = 60;

    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Ok((index, bytes))) => {
                last_success_time = std::time::Instant::now();
                if let Some(seg) = segments.iter_mut().find(|s| s.index == index) {
                    seg.status = SegmentStatus::Completed;
                    seg.downloaded = bytes;
                }
                let _ = db.update_segment_status(task_id, index, &SegmentStatus::Completed, bytes);
                emit_segment_status(
                    app,
                    &DownloadSegmentEvent {
                        task_id: task_id.to_string(),
                        segment_index: index,
                        status: SegmentStatus::Completed,
                        downloaded: bytes,
                    },
                );
            }
            Ok(Err((_, DownloadError::TaskAborted(ref reason)))) if reason == "paused" => {
                is_paused = true;
                join_set.abort_all();
                break;
            }
            Ok(Err((_, DownloadError::TaskAborted(ref reason)))) if reason == "cancelled" => {
                is_cancelled = true;
                join_set.abort_all();
                break;
            }
            Ok(Err((failed_seg, DownloadError::CdnRateLimit))) => {
                // CDN限流 — 被动降低并发: 释放permit后延迟重新排队 (类似 aria2)
                let count = cdn_retry_counts.entry(failed_seg.index).or_insert(0);
                *count += 1;

                let all_stuck_duration = last_success_time.elapsed().as_secs();
                if all_stuck_duration > ALL_STUCK_THRESHOLD_SECS && join_set.is_empty() {
                    task_url_refresh_count += 1;
                    warn!(
                        "[resume][{}] 所有分片停滞{}s, 触发任务级URL刷新 (第{}次)",
                        task_id, all_stuck_duration, task_url_refresh_count
                    );
                    if task_url_refresh_count > MAX_TASK_URL_REFRESHES {
                        warn!("[resume][{}] 任务级URL刷新耗尽, 标记失败", task_id);
                        has_failure = true;
                        continue;
                    }
                    if url_refresh_requested
                        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                        .is_ok()
                    {
                        emit_url_expired(
                            app,
                            &UrlExpiredEvent {
                                task_id: task_id.to_string(),
                                pick_code: task_meta.pick_code.clone(),
                            },
                        );
                    }
                }

                if *count > MAX_CDN_RETRIES {
                    warn!(
                        "[resume][{}] 分片{} CDN限流重试耗尽 ({}/{})",
                        task_id, failed_seg.index, count, MAX_CDN_RETRIES
                    );
                    has_failure = true;
                    continue;
                }

                let backoff_secs = 2u64.pow((*count).min(6));
                let jitter_ms = ((failed_seg.index as u64) % 16) * 300;
                let delay = Duration::from_secs(backoff_secs) + Duration::from_millis(jitter_ms);

                info!(
                    "[resume][{}] 分片{} CDN限流重排队 #{} (延迟{:.1}s, 当前活跃{})",
                    task_id,
                    failed_seg.index,
                    count,
                    delay.as_secs_f64(),
                    join_set.len()
                );

                // 用 progress_snapshot 获取最新已下载字节 (修复 CDN 重试时 seg.downloaded 过期导致从头重下)
                let mut seg = failed_seg;
                let actual_downloaded = progress_snapshot
                    .lock()
                    .unwrap()
                    .get(&seg.index)
                    .copied()
                    .unwrap_or(seg.downloaded);
                seg.downloaded = actual_downloaded;

                let semaphore = semaphore.clone();
                let client = client.clone();
                let url_rx = url_rx.clone();
                let token = token.to_string();
                let user_agent = user_agent.to_string();
                let writer = writer.clone();
                let tx = progress_tx.clone();
                let tid = task_id.to_string();
                let pick_code = task_meta.pick_code.clone();
                let sig_rx = signal_rx.clone();
                let url_refresh_req = url_refresh_requested.clone();
                let app_clone = app.clone();

                join_set.spawn(async move {
                    tokio::time::sleep(delay).await;
                    {
                        let signal = sig_rx.borrow().clone();
                        if signal != DownloadSignal::Running {
                            return Err((
                                seg,
                                DownloadError::TaskAborted(
                                    if signal == DownloadSignal::Paused {
                                        "paused"
                                    } else {
                                        "cancelled"
                                    }
                                    .to_string(),
                                ),
                            ));
                        }
                    }
                    let permit = match semaphore.acquire_owned().await {
                        Ok(p) => p,
                        Err(_) => {
                            return Err((
                                seg,
                                DownloadError::TaskAborted("semaphore closed".to_string()),
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
            Ok(Err((failed_seg, _e))) => {
                // 分片重试耗尽后失败 — 尝试重分配 (per D-10, D-11)
                // 用 progress_snapshot 获取最新已下载字节 (修复过期 downloaded 导致重复下载)
                let actual_downloaded = progress_snapshot
                    .lock()
                    .unwrap()
                    .get(&failed_seg.index)
                    .copied()
                    .unwrap_or(failed_seg.downloaded);
                let mut updated_seg = failed_seg.clone();
                updated_seg.downloaded = actual_downloaded;

                if realloc_counter < MAX_REALLOCATIONS {
                    let active = join_set.len();
                    if let Some(sub_segs) =
                        reallocate_failed_segment(&updated_seg, active, realloc_counter)
                    {
                        realloc_counter += 1;
                        // Mark original segment as Reallocated — 同步更新内存 + DB
                        if let Some(orig) =
                            segments.iter_mut().find(|s| s.index == updated_seg.index)
                        {
                            orig.status = SegmentStatus::Reallocated;
                            orig.downloaded = actual_downloaded;
                        }
                        let _ = db.update_segment_status(
                            task_id,
                            updated_seg.index,
                            &SegmentStatus::Reallocated,
                            actual_downloaded,
                        );
                        // Persist and spawn sub-segments — 同时追踪到内存向量
                        let _ = db.insert_segments(task_id, &sub_segs);
                        let sub_segs_tracking: Vec<Segment> = sub_segs
                            .iter()
                            .map(|s| {
                                let mut ts = s.clone();
                                ts.status = SegmentStatus::Downloading;
                                ts
                            })
                            .collect();
                        let mut realloc_ok = true;
                        for sub_seg in sub_segs {
                            let _ = db.update_segment_status(
                                task_id,
                                sub_seg.index,
                                &SegmentStatus::Downloading,
                                0,
                            );
                            let permit = match semaphore.clone().acquire_owned().await {
                                Ok(p) => p,
                                Err(_) => {
                                    realloc_ok = false;
                                    break;
                                }
                            };
                            let client = client.clone();
                            let url_rx = url_rx.clone();
                            let token = token.to_string();
                            let user_agent = user_agent.to_string();
                            let seg = sub_seg;
                            let writer = writer.clone();
                            let tx = progress_tx.clone();
                            let tid = task_id.to_string();
                            let pick_code = task_meta.pick_code.clone();
                            let sig_rx = signal_rx.clone();
                            let url_refresh_req = url_refresh_requested.clone();
                            let app_clone = app.clone();

                            join_set.spawn(async move {
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
                        if realloc_ok {
                            // 追踪子分片到内存向量 — 确保暂停/失败处理能遍历到它们
                            segments.extend(sub_segs_tracking);
                            continue; // Reallocation successful
                        }
                        // Fall through to has_failure = true
                    }
                }
                has_failure = true;
            }
            Err(_e) => {
                has_failure = true;
            }
        }
    }

    // 处理暂停结果
    if is_paused {
        {
            let latest_progress = progress_snapshot.lock().unwrap();
            for seg in &segments {
                if seg.status == SegmentStatus::Completed
                    || seg.status == SegmentStatus::Reallocated
                {
                    continue;
                }
                let downloaded = latest_progress
                    .get(&seg.index)
                    .copied()
                    .unwrap_or(seg.downloaded);
                let _ = db.update_segment_status(
                    task_id,
                    seg.index,
                    &SegmentStatus::Paused,
                    downloaded,
                );
                emit_segment_status(
                    app,
                    &DownloadSegmentEvent {
                        task_id: task_id.to_string(),
                        segment_index: seg.index,
                        status: SegmentStatus::Paused,
                        downloaded,
                    },
                );
            }
        }
        let _ = db.update_task_status(task_id, "paused");
        emit_task_status(
            app,
            &DownloadTaskEvent {
                task_id: task_id.to_string(),
                status: TaskStatus::Paused,
            },
        );
        drop(progress_tx);
        let _ = flush_handle.await;
        {
            let mut signals = DOWNLOAD_SIGNALS.lock().unwrap();
            signals.remove(task_id);
        }
        {
            let mut channels = URL_CHANNELS.lock().unwrap();
            channels.remove(task_id);
        }
        return Ok(());
    }

    // 处理取消结果
    if is_cancelled {
        drop(progress_tx);
        let _ = flush_handle.await;
        let _ = db.delete_task(task_id);
        let _ = std::fs::remove_file(&task_meta.save_path);
        emit_task_status(
            app,
            &DownloadTaskEvent {
                task_id: task_id.to_string(),
                status: TaskStatus::Error,
            },
        );
        {
            let mut signals = DOWNLOAD_SIGNALS.lock().unwrap();
            signals.remove(task_id);
        }
        {
            let mut channels = URL_CHANNELS.lock().unwrap();
            channels.remove(task_id);
        }
        return Ok(());
    }

    // Final flush
    drop(progress_tx);
    let _ = flush_handle.await;

    // 更新任务状态
    if has_failure {
        let latest_progress = progress_snapshot.lock().unwrap();
        for seg in &segments {
            if seg.status == SegmentStatus::Completed || seg.status == SegmentStatus::Reallocated {
                continue;
            }
            let downloaded = latest_progress
                .get(&seg.index)
                .copied()
                .unwrap_or(seg.downloaded);
            let _ =
                db.update_segment_status(task_id, seg.index, &SegmentStatus::Failed, downloaded);
            emit_segment_status(
                app,
                &DownloadSegmentEvent {
                    task_id: task_id.to_string(),
                    segment_index: seg.index,
                    status: SegmentStatus::Failed,
                    downloaded,
                },
            );
        }
        let _ = db.update_task_status(task_id, "error");
        emit_task_status(
            app,
            &DownloadTaskEvent {
                task_id: task_id.to_string(),
                status: TaskStatus::Error,
            },
        );
    } else {
        // SHA1 verification before marking complete (per D-06)
        let sha1_ok = verify_file_sha1(&task_meta.save_path, task_meta.expected_sha1.as_deref())
            .await
            .unwrap_or(false);

        if sha1_ok {
            // 下载完成，删除 .oofp 进度文件
            db.delete_task(task_id)?;
            emit_task_status(
                app,
                &DownloadTaskEvent {
                    task_id: task_id.to_string(),
                    status: TaskStatus::Complete,
                },
            );
        } else {
            let _ = db.update_task_status(task_id, "verify_failed");
            emit_task_status(
                app,
                &DownloadTaskEvent {
                    task_id: task_id.to_string(),
                    status: TaskStatus::VerifyFailed,
                },
            );
        }
    }

    // 清理信号注册
    {
        let mut signals = DOWNLOAD_SIGNALS.lock().unwrap();
        signals.remove(task_id);
    }
    {
        let mut channels = URL_CHANNELS.lock().unwrap();
        channels.remove(task_id);
    }

    Ok(())
}

/// 设置全局下载速度上限 (per ADV-01, D-03)
///
/// limit: bytes/sec, 0 = 不限速 (per D-04)
#[tauri::command]
pub fn set_speed_limit(limit: u64) {
    info!("[cmd] set_speed_limit limit={}B/s", limit);
    super::throttle::set_speed_limit(limit);
}

/// 启动新下载任务 — fire-and-forget (per D-04)
///
/// 接受下载参数，生成 task_id，spawn tokio task 调用 download_file() 后立即返回 task_id。
#[tauri::command]
pub async fn start_download(
    url: String,
    file_name: String,
    file_size: u64,
    save_path: String,
    pick_code: String,
    expected_sha1: Option<String>,
    token: String,
    user_agent: String,
    split: u16,
    max_connections_per_server: u16,
    db: tauri::State<'_, Arc<ProgressFile>>,
    http_client: tauri::State<'_, reqwest::Client>,
    app: AppHandle,
) -> Result<String, String> {
    let task_id = uuid::Uuid::new_v4().to_string();
    info!(
        "[cmd] start_download task_id={} file={} size={:.1}MB",
        task_id,
        file_name,
        file_size as f64 / 1024.0 / 1024.0
    );
    let config = DownloadConfig {
        split,
        max_connections_per_server,
        speed_limit: 0,
    };
    let mut task = DownloadTask {
        task_id: task_id.clone(),
        file_name,
        file_size,
        save_path,
        url,
        pick_code,
        etag: None,
        expected_sha1,
        segments: Vec::new(),
        status: TaskStatus::Pending,
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64,
    };
    let db = Arc::clone(&db);
    let client = http_client.inner().clone();
    let app_clone = app.clone();
    tokio::spawn(async move {
        if let Err(e) = download_file(
            &client,
            &mut task,
            &token,
            &user_agent,
            &config,
            &db,
            &app_clone,
        )
        .await
        {
            error!("[cmd] download_file 失败 task_id={}: {}", task.task_id, e);
            emit_task_status(
                &app_clone,
                &DownloadTaskEvent {
                    task_id: task.task_id.clone(),
                    status: TaskStatus::Error,
                },
            );
        }
    });
    Ok(task_id)
}

/// 恢复中断的下载任务 — fire-and-forget
///
/// 接受 task_id、新 URL 和 save_path，spawn tokio task 调用 resume_download() 后立即返回。
#[tauri::command]
pub async fn resume_download_task(
    task_id: String,
    url: String,
    save_path: String,
    token: String,
    user_agent: String,
    split: u16,
    max_connections_per_server: u16,
    db: tauri::State<'_, Arc<ProgressFile>>,
    http_client: tauri::State<'_, reqwest::Client>,
    app: AppHandle,
) -> Result<(), String> {
    info!("[cmd] resume_download_task task_id={}", task_id);
    let config = DownloadConfig {
        split,
        max_connections_per_server,
        speed_limit: 0,
    };
    let db = Arc::clone(&db);
    let client = http_client.inner().clone();
    let app_clone = app.clone();
    let tid = task_id.clone();
    tokio::spawn(async move {
        if let Err(e) = resume_download(
            &client,
            &tid,
            &url,
            &save_path,
            &token,
            &user_agent,
            &config,
            &db,
            &app_clone,
        )
        .await
        {
            error!("[cmd] resume_download 失败 task_id={}: {}", tid, e);
            emit_task_status(
                &app_clone,
                &DownloadTaskEvent {
                    task_id: tid.clone(),
                    status: TaskStatus::Error,
                },
            );
        }
    });
    Ok(())
}

/// 暂停下载任务 (LC-01)
#[tauri::command]
pub fn pause_download(task_id: String) -> Result<(), String> {
    info!("[cmd] pause_download task_id={}", task_id);
    let signals = DOWNLOAD_SIGNALS.lock().unwrap();
    if let Some(tx) = signals.get(&task_id) {
        tx.send(DownloadSignal::Paused)
            .map_err(|e| format!("发送暂停信号失败: {}", e))?;
        Ok(())
    } else {
        warn!("[cmd] pause_download 未找到任务 task_id={}", task_id);
        Err("未找到下载任务".to_string())
    }
}

/// 取消下载任务 (LC-03)
#[tauri::command]
pub fn cancel_download(task_id: String) -> Result<(), String> {
    info!("[cmd] cancel_download task_id={}", task_id);
    let signals = DOWNLOAD_SIGNALS.lock().unwrap();
    if let Some(tx) = signals.get(&task_id) {
        tx.send(DownloadSignal::Cancelled)
            .map_err(|e| format!("发送取消信号失败: {}", e))?;
        Ok(())
    } else {
        warn!("[cmd] cancel_download 未找到任务 task_id={}", task_id);
        Err("未找到下载任务".to_string())
    }
}

/// 接收前端传回的新下载 URL (per URL-02)
///
/// 前端监听 `download:url-expired` 事件后调用 115 API 获取新 URL，
/// 然后通过此 command 传回 Rust 端。Rust 端更新 watch channel 和 DB。
#[tauri::command]
pub fn update_download_url(
    task_id: String,
    url: String,
    db: tauri::State<'_, Arc<ProgressFile>>,
) -> Result<(), String> {
    info!("[cmd] update_download_url task_id={}", task_id);
    // 1. Send new URL via watch channel to all waiting segments
    {
        let channels = URL_CHANNELS.lock().unwrap();
        if let Some(tx) = channels.get(&task_id) {
            tx.send(url.clone())
                .map_err(|e| format!("URL channel send failed: {}", e))?;
        } else {
            return Err(format!("No active download for task {}", task_id));
        }
    }

    // 2. Persist new URL to DB for crash recovery
    db.update_task_url(&task_id, &url)
        .map_err(|e| e.to_string())?;

    Ok(())
}
