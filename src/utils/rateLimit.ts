/** 限流退避基础延迟(ms) */
export const BACKOFF_BASE = 3000;
/** 限流退避最大延迟(ms) */
export const BACKOFF_MAX = 60000;
/** 限流自动重试最大次数 */
export const MAX_RATE_LIMIT_RETRY = 5;

export const sleep = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** 检测错误是否为限流错误 */
export const isRateLimitError = (error: unknown): boolean => {
  if (!error) return false;
  const err = error as Record<string, unknown>;
  if (err.status === 429 || err.statusCode === 429) return true;
  if (err.code === 20130827 || err.errno === 20130827) return true;
  const msg = String(err.message || err.msg || '');
  if (/rate.?limit|too.?many|频繁|限流|请求过快/i.test(msg)) return true;
  return false;
};

/** 计算指数退避延迟（含 ±25% 抖动） */
export const getBackoffDelay = (retryCount: number): number => {
  const delay = Math.min(BACKOFF_BASE * Math.pow(2, retryCount), BACKOFF_MAX);
  const jitter = delay * 0.25 * (Math.random() * 2 - 1);
  return Math.round(delay + jitter);
};

/**
 * 创建令牌桶限流器
 *
 * 以可配置的速率控制请求频率。通过传入 getter 函数获取速率，支持运行时动态调整。
 * 当速率 ≤ 0 时不限制。桶容量 = max(1, 速率)，允许一定突发请求。
 */
export function createRateLimiter(getRefillRate: () => number) {
  let tokens = Math.max(1, getRefillRate());
  let lastRefill = Date.now();
  const pending: Array<() => void> = [];
  let processing = false;

  const refill = () => {
    const rate = getRefillRate();
    if (rate <= 0) return;
    const now = Date.now();
    const elapsed = (now - lastRefill) / 1000;
    const maxTokens = Math.max(1, rate);
    tokens = Math.min(maxTokens, tokens + elapsed * rate);
    lastRefill = now;
  };

  const drain = async () => {
    if (processing) return;
    processing = true;

    while (pending.length > 0) {
      const rate = getRefillRate();
      if (rate <= 0) {
        while (pending.length > 0) {
          pending.shift()!();
        }
        break;
      }

      refill();
      if (tokens >= 1) {
        tokens -= 1;
        pending.shift()!();
      } else {
        const waitTime = ((1 - tokens) / rate) * 1000;
        await sleep(waitTime);
      }
    }

    processing = false;
    // 防止竞态：drain 即将退出时新的 acquire 已入队但 drain() 因 processing=true 直接返回
    if (pending.length > 0) {
      drain();
    }
  };

  /** 获取一个令牌，桶为空时排队等待 */
  const acquire = (): Promise<void> => {
    const rate = getRefillRate();
    if (rate <= 0) return Promise.resolve();

    return new Promise<void>((resolve) => {
      pending.push(resolve);
      drain();
    });
  };

  return { acquire };
}
