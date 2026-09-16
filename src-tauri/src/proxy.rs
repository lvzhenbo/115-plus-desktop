//! 全局代理配置。
//!
//! 更新 / 下载 / 上传三个网络场景共用一份代理配置，解析优先级为：
//!
//! 1. 场景级代理（`update_proxy` / `download_proxy` / `upload_proxy`）非空时使用它；
//! 2. 场景级代理留空时，回落到统一代理（`unified_proxy`）；
//! 3. 统一代理也留空时，由"跟随系统代理"开关决定：
//!    - 开启 → 交给 reqwest 自动探测并使用系统代理；
//!    - 关闭 → 直连，不经过任何代理。
//!
//! 代理地址不需要区分协议单独选择：带 `scheme://` 前缀时按前缀识别
//! （http / https / socks4 / socks4a / socks5 / socks5h），未带前缀时默认按 HTTP 代理处理；
//! 特殊值 `direct` 表示强制直连。

use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

/// 代理地址支持的协议前缀（小写、不含 `://`）。
const SUPPORTED_PROXY_SCHEMES: [&str; 6] =
    ["http", "https", "socks4", "socks4a", "socks5", "socks5h"];

/// 强制直连的特殊关键词。
const DIRECT_PROXY_KEYWORD: &str = "direct";

/// 前端 `setting` store 中 `proxySetting` 的 Rust 投影。
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProxyConfig {
    /// 未配置任何代理时是否跟随系统代理
    pub follow_system_proxy: bool,
    /// 统一代理（更新 / 下载 / 上传共用）
    pub unified_proxy: String,
    /// 更新代理
    pub update_proxy: String,
    /// 下载代理
    pub download_proxy: String,
    /// 上传代理
    pub upload_proxy: String,
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            follow_system_proxy: true,
            unified_proxy: String::new(),
            update_proxy: String::new(),
            download_proxy: String::new(),
            upload_proxy: String::new(),
        }
    }
}

impl ProxyConfig {
    /// 解析指定场景最终采用的代理决策。
    fn resolve(&self, feature: ProxyFeature) -> ProxyDecision {
        if let Some(decision) = parse_override(feature.override_value(self)) {
            return decision;
        }

        if let Some(decision) = parse_override(&self.unified_proxy) {
            return decision;
        }

        if self.follow_system_proxy {
            ProxyDecision::System
        } else {
            ProxyDecision::Direct
        }
    }
}

/// 代理的使用场景。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProxyFeature {
    Update,
    Download,
    Upload,
}

impl ProxyFeature {
    /// 场景对应的功能级代理字段。
    fn override_value(self, config: &ProxyConfig) -> &str {
        match self {
            Self::Update => &config.update_proxy,
            Self::Download => &config.download_proxy,
            Self::Upload => &config.upload_proxy,
        }
    }

    /// 供日志使用的场景名。
    fn label(self) -> &'static str {
        match self {
            Self::Update => "更新",
            Self::Download => "下载",
            Self::Upload => "上传",
        }
    }
}

/// 某一场景最终采用的代理决策。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "mode", rename_all = "lowercase")]
pub enum ProxyDecision {
    /// 使用显式配置的代理地址
    Explicit { url: String },
    /// 跟随系统代理（由 reqwest 自动探测系统或环境变量中的代理配置）
    System,
    /// 直连（不使用任何代理）
    Direct,
}

impl ProxyDecision {
    /// 供日志使用的简要描述。
    pub fn describe(&self) -> String {
        match self {
            Self::Explicit { url } => format!("显式代理 {url}"),
            Self::System => "跟随系统代理".to_string(),
            Self::Direct => "直连".to_string(),
        }
    }
}

/// 全局代理状态，通过 Tauri managed state 共享给下载 / 上传模块与命令层。
#[derive(Default)]
pub struct ProxyState {
    config: Mutex<ProxyConfig>,
}

impl ProxyState {
    pub fn new() -> Self {
        Self::default()
    }

    /// 用前端同步来的最新设置整体替换代理配置。
    pub fn update(&self, config: ProxyConfig) {
        // 无效地址会被静默忽略（回落到下一优先级），这里提前记录一次日志方便排查。
        for (feature, raw) in [
            (ProxyFeature::Update, &config.update_proxy),
            (ProxyFeature::Download, &config.download_proxy),
            (ProxyFeature::Upload, &config.upload_proxy),
        ] {
            if is_invalid_override(raw) {
                log::warn!(
                    "[代理] {}代理配置无效，已忽略：{}",
                    feature.label(),
                    raw.trim()
                );
            }
        }
        if is_invalid_override(&config.unified_proxy) {
            log::warn!(
                "[代理] 统一代理配置无效，已忽略：{}",
                config.unified_proxy.trim()
            );
        }

        *self.config.lock().unwrap() = config;
    }

    /// 解析指定场景当前应使用的代理。
    pub fn resolve(&self, feature: ProxyFeature) -> ProxyDecision {
        self.config.lock().unwrap().resolve(feature)
    }
}

/// 规范化代理地址：识别已有协议前缀，未带前缀时按 HTTP 代理补全。
///
/// 返回 `None` 表示地址无效（未知协议前缀、缺少主机名或无法解析），空输入同样返回 `None`。
pub fn normalize_proxy_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = match trimmed.split_once("://") {
        Some((scheme, rest)) => {
            let scheme = scheme.trim().to_ascii_lowercase();
            let rest = rest.trim();
            if rest.is_empty() || !SUPPORTED_PROXY_SCHEMES.contains(&scheme.as_str()) {
                return None;
            }
            format!("{scheme}://{rest}")
        }
        // 没有协议前缀：按 HTTP 代理处理。
        None => format!("http://{trimmed}"),
    };

    let url = reqwest::Url::parse(&candidate).ok()?;
    if !url.host_str().is_some_and(|host| !host.is_empty()) {
        return None;
    }
    // 代理地址不应包含路径、查询或片段，避免把 `http:/host` 这类手误输入当成合法地址。
    if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
        return None;
    }

    Some(candidate)
}

/// 解析单个代理字段：空 → `None`；`direct` → 直连；合法地址 → 显式代理；无效 → 告警并忽略。
fn parse_override(raw: &str) -> Option<ProxyDecision> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    if trimmed.eq_ignore_ascii_case(DIRECT_PROXY_KEYWORD) {
        return Some(ProxyDecision::Direct);
    }

    normalize_proxy_url(trimmed).map(|url| ProxyDecision::Explicit { url })
}

/// 字段是否包含一个非空且无法解析的地址（用于一次性日志提示）。
fn is_invalid_override(raw: &str) -> bool {
    let trimmed = raw.trim();
    !trimmed.is_empty()
        && !trimmed.eq_ignore_ascii_case(DIRECT_PROXY_KEYWORD)
        && normalize_proxy_url(trimmed).is_none()
}

/// 前端同步完整代理设置。
#[tauri::command]
pub fn proxy_set_config(config: ProxyConfig, state: tauri::State<'_, Arc<ProxyState>>) {
    state.update(config);
}

/// 返回指定场景当前生效的代理决策。
#[tauri::command]
pub fn proxy_get_effective(
    feature: ProxyFeature,
    state: tauri::State<'_, Arc<ProxyState>>,
) -> ProxyDecision {
    state.resolve(feature)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn explicit(url: &str) -> ProxyDecision {
        ProxyDecision::Explicit {
            url: url.to_string(),
        }
    }

    fn state_with(config: ProxyConfig) -> ProxyState {
        let state = ProxyState::new();
        state.update(config);
        state
    }

    #[test]
    fn normalizes_known_schemes() {
        assert_eq!(
            normalize_proxy_url("socks5://127.0.0.1:1080").as_deref(),
            Some("socks5://127.0.0.1:1080")
        );
        assert_eq!(
            normalize_proxy_url("SOCKS5://127.0.0.1:1080").as_deref(),
            Some("socks5://127.0.0.1:1080")
        );
        assert_eq!(
            normalize_proxy_url("https://127.0.0.1:7890").as_deref(),
            Some("https://127.0.0.1:7890")
        );
        assert_eq!(
            normalize_proxy_url("socks4a://127.0.0.1:1080").as_deref(),
            Some("socks4a://127.0.0.1:1080")
        );
    }

    #[test]
    fn defaults_to_http_when_scheme_is_missing() {
        assert_eq!(
            normalize_proxy_url("127.0.0.1:7890").as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(
            normalize_proxy_url(" 127.0.0.1:7890 ").as_deref(),
            Some("http://127.0.0.1:7890")
        );
        assert_eq!(
            normalize_proxy_url("user:pass@127.0.0.1:7890").as_deref(),
            Some("http://user:pass@127.0.0.1:7890")
        );
        assert_eq!(
            normalize_proxy_url("[::1]:1080").as_deref(),
            Some("http://[::1]:1080")
        );
    }

    #[test]
    fn rejects_invalid_proxy_inputs() {
        assert!(normalize_proxy_url("").is_none());
        assert!(normalize_proxy_url("   ").is_none());
        assert!(normalize_proxy_url("ftp://127.0.0.1:21").is_none());
        assert!(normalize_proxy_url("http://").is_none());
        assert!(normalize_proxy_url("socks5://").is_none());
        assert!(normalize_proxy_url("http://127.0.0.1:99999").is_none());
        assert!(normalize_proxy_url("http:/127.0.0.1:7890").is_none());
        // `direct` 是关键词而非地址，关键词语义由 `parse_override` 优先处理。
        assert_eq!(
            normalize_proxy_url("direct").as_deref(),
            Some("http://direct")
        );
    }

    #[test]
    fn feature_proxy_wins_over_unified() {
        let state = state_with(ProxyConfig {
            unified_proxy: "http://127.0.0.1:1111".to_string(),
            update_proxy: "socks5://127.0.0.1:2222".to_string(),
            ..Default::default()
        });

        assert_eq!(
            state.resolve(ProxyFeature::Update),
            explicit("socks5://127.0.0.1:2222")
        );
    }

    #[test]
    fn unified_proxy_is_used_when_feature_proxy_is_empty() {
        let state = state_with(ProxyConfig {
            unified_proxy: "127.0.0.1:1111".to_string(),
            update_proxy: "  ".to_string(),
            download_proxy: String::new(),
            upload_proxy: String::new(),
            ..Default::default()
        });

        assert_eq!(
            state.resolve(ProxyFeature::Download),
            explicit("http://127.0.0.1:1111")
        );
        assert_eq!(
            state.resolve(ProxyFeature::Upload),
            explicit("http://127.0.0.1:1111")
        );
    }

    #[test]
    fn direct_keyword_forces_direct_connection() {
        let state = state_with(ProxyConfig {
            unified_proxy: "http://127.0.0.1:1111".to_string(),
            download_proxy: "DIRECT".to_string(),
            ..Default::default()
        });

        assert_eq!(state.resolve(ProxyFeature::Download), ProxyDecision::Direct);
        // 其它场景仍沿用统一代理。
        assert_eq!(
            state.resolve(ProxyFeature::Update),
            explicit("http://127.0.0.1:1111")
        );
    }

    #[test]
    fn falls_back_to_system_toggle_when_nothing_configured() {
        let system = state_with(ProxyConfig::default());
        assert_eq!(system.resolve(ProxyFeature::Update), ProxyDecision::System);

        let direct = state_with(ProxyConfig {
            follow_system_proxy: false,
            ..Default::default()
        });
        assert_eq!(direct.resolve(ProxyFeature::Upload), ProxyDecision::Direct);
    }

    #[test]
    fn invalid_values_are_ignored_and_fall_through() {
        let state = state_with(ProxyConfig {
            follow_system_proxy: false,
            unified_proxy: "http://127.0.0.1:1111".to_string(),
            update_proxy: "ftp://127.0.0.1:21".to_string(),
            ..Default::default()
        });

        // 更新代理无效 → 回落统一代理。
        assert_eq!(
            state.resolve(ProxyFeature::Update),
            explicit("http://127.0.0.1:1111")
        );

        let invalid_all = state_with(ProxyConfig {
            follow_system_proxy: false,
            unified_proxy: "not a url".to_string(),
            ..Default::default()
        });
        assert_eq!(
            invalid_all.resolve(ProxyFeature::Download),
            ProxyDecision::Direct
        );
    }

    #[test]
    fn proxy_config_defaults_to_following_system_proxy() {
        let config = ProxyConfig::default();
        assert!(config.follow_system_proxy);
        assert!(config.unified_proxy.is_empty());
    }
}
