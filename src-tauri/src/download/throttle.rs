use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use tokio::sync::watch;

// 全局速度限制 channel — 广播速度上限变更给所有分片
static SPEED_LIMIT_CHANNEL: LazyLock<(watch::Sender<u64>, watch::Receiver<u64>)> =
    LazyLock::new(|| watch::channel(0u64));
static GLOBAL_THROTTLE: LazyLock<TokenBucket> = LazyLock::new(TokenBucket::new);

/// 令牌桶内部状态 — 合并为单个 Mutex 避免竞态条件
struct TokenBucketState {
    tokens: f64,
    last_refill: Instant,
}

/// Token Bucket 令牌桶限速器
///
/// 所有并发下载共享同一个全局 TokenBucket，
/// 每个 chunk 写入后调用 `consume(bytes)` 消耗令牌。
/// refill + consume 在同一个 Mutex 下原子执行，避免并发竞态。
pub struct TokenBucket {
    state: Mutex<TokenBucketState>,
}

impl TokenBucket {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(TokenBucketState {
                tokens: 0.0,
                last_refill: Instant::now(),
            }),
        }
    }

    /// 消耗令牌 — 如果令牌不足则等待
    ///
    /// limit=0 时立即返回（不限速）
    /// refill 和 consume 在同一把锁内完成，避免多分片并发重复补充令牌。
    pub async fn consume(&self, bytes: usize) {
        let limit = *SPEED_LIMIT_CHANNEL.1.borrow();
        if limit == 0 {
            return; // 不限速
        }

        let wait_time = {
            let mut state = self.state.lock().unwrap();

            // Refill: 根据经过的时间补充令牌
            let elapsed = state.last_refill.elapsed().as_secs_f64();
            state.last_refill = Instant::now();
            state.tokens += elapsed * limit as f64;
            // 令牌上限 = 1 秒的 burst buffer
            if state.tokens > limit as f64 {
                state.tokens = limit as f64;
            }

            // Consume
            let needed = bytes as f64;
            if state.tokens >= needed {
                state.tokens -= needed;
                return;
            }
            let deficit = needed - state.tokens;
            state.tokens = 0.0;
            deficit / limit as f64
        };

        tokio::time::sleep(tokio::time::Duration::from_secs_f64(wait_time)).await;
    }
}

/// 设置全局下载速度上限
///
/// limit: bytes/sec, 0 = 不限速
/// 通过 watch channel 广播给所有正在下载的分片
pub fn set_speed_limit(limit: u64) {
    let _ = SPEED_LIMIT_CHANNEL.0.send(limit);
}

/// 获取全局 TokenBucket 引用
pub fn get_throttle() -> &'static TokenBucket {
    &GLOBAL_THROTTLE
}
