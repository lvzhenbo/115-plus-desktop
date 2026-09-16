/** 代理输入中的强制直连关键词。 */
export const DIRECT_PROXY_KEYWORD = 'direct';

/** 代理地址支持的协议前缀（小写，带冒号）。 */
const SUPPORTED_PROXY_PROTOCOLS = ['http:', 'https:', 'socks4:', 'socks4a:', 'socks5:', 'socks5h:'];

const SCHEME_PATTERN = /^([a-zA-Z][a-zA-Z0-9+.-]*):\/\//;

/**
 * 规范化代理输入：识别已有协议前缀，未带前缀时按 HTTP 代理补全；
 * `direct` 关键词原样保留，表示强制直连。
 *
 * 输入无效（未知协议、缺少主机名、携带路径或查询）或留空时返回 `null`。
 * 注意：返回值为规范化后的地址字符串，校验规则与 Rust 侧 `normalize_proxy_url` 保持一致。
 */
export function normalizeProxyInput(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;

  if (trimmed.toLowerCase() === DIRECT_PROXY_KEYWORD) return DIRECT_PROXY_KEYWORD;

  const schemeMatch = SCHEME_PATTERN.exec(trimmed);
  let candidate: string;

  if (schemeMatch) {
    const protocol = `${schemeMatch[1].toLowerCase()}:`;
    if (!SUPPORTED_PROXY_PROTOCOLS.includes(protocol)) return null;
    candidate = `${protocol}//${trimmed.slice(schemeMatch[0].length).trim()}`;
  } else {
    candidate = `http://${trimmed}`;
  }

  try {
    const parsed = new URL(candidate);
    if (!parsed.hostname) return null;

    // 代理地址不应包含路径、查询或片段，避免把 `http:/host` 之类的手误输入当成合法地址。
    if (parsed.pathname !== '' && parsed.pathname !== '/') return null;
    if (parsed.search || parsed.hash) return null;

    return candidate;
  } catch {
    return null;
  }
}

/**
 * 校验代理输入，返回错误提示；合法或留空时返回 `undefined`。
 */
export function validateProxyInput(raw: string): string | undefined {
  if (!raw.trim()) return undefined;

  if (normalizeProxyInput(raw) === null) {
    return '代理地址无效，示例：http://127.0.0.1:7890、socks5://127.0.0.1:1080，或填写 direct 强制直连';
  }

  return undefined;
}
