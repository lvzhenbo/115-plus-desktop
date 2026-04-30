//! 字幕系统字体发现与匹配。
//!
//! # 背景
//!
//! 前端 ASS 字幕渲染器需要加载用户指定的字体文件。当字幕中引用的字体未
//! 在系统内安装时，渲染会降级到后备字体。本模块的作用是：扫描系统字体目录，
//! 建立"字体族名 → 字体文件路径"的映射，供前端字幕引擎查询使用。
//!
//! # 架构
//!
//! ```text
//! 前端调用 subtitle_get_system_font_config(["微软雅黑","SimHei",…])
//!   └─ resolve_subtitle_system_font_config()  [按平台分发]
//!       ├─ Windows  → 读注册表 Fonts 键 → 解析 .ttf/.ttc 文件名
//!       ├─ macOS    → 扫描 /System/Library/Fonts 等目录 → ttf-parser 读 name 表
//!       └─ Linux    → 扫描 /usr/share/fonts 等目录 → ttf-parser 读 name 表
//!           └─ build_font_config()
//!               ├─ 对每个请求字体名，在候选集中匹配
//!               ├─ 按平台偏好选出默认字体
//!               └─ 返回 SubtitleSystemFontConfig { default_font, fonts, unmatched }
//! ```
//!
//! # 平台差异
//!
//! - **Windows**：通过注册表 `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts`
//!   获取字体名→文件路径映射，避免遍历整个 Fonts 目录。
//! - **macOS / Linux**：使用 `ttf-parser` 读取字体文件 name 表获取 family name，
//!   递归扫描标准字体目录。
//! - 各平台都有**已知字体硬编码列表**作为后备，确保中文字体不会被遗漏。

use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf, sync::LazyLock};

#[cfg(target_os = "windows")]
use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};

// ---- 数据结构 ----

/// 扫描阶段发现的一个字体候选。
///
/// 同一字体文件可能有多个族名（如 "Microsoft YaHei" 和 "微软雅黑"），
/// 记录原始名称和归一化名称以支持大小写/空格不敏感的匹配。
#[derive(Debug)]
struct SystemFontCandidate {
    path: String,
    aliases: Vec<String>,
    normalized_aliases: HashSet<String>,
}

/// 返回给前端的单个字体来源描述。
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSystemFontSource {
    family_names: Vec<String>,
    path: String,
}

/// 字幕系统字体配置 — `subtitle_get_system_font_config` 命令的返回值。
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSystemFontConfig {
    default_font: String,
    fonts: Vec<SubtitleSystemFontSource>,
    /// 在系统中未找到的字体族名，前端可用于降级提示。
    #[serde(default)]
    unmatched_families: Vec<String>,
}

// ---- 字体名归一化 ----

/// 去空格、去引号、去 @ 前缀、全小写。保证不同写法的同一字体名能匹配。
fn normalize_font_name(font_name: &str) -> String {
    font_name
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '\''))
        .trim_start_matches('@')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

// ---- 候选集操作 ----

/// 向字体候选追加别名，自动去重。
fn push_unique_alias(candidate: &mut SystemFontCandidate, alias: &str) {
    let alias = alias
        .trim()
        .trim_matches(|ch| matches!(ch, '"' | '\''))
        .trim_start_matches('@')
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if alias.is_empty() {
        return;
    }

    let normalized = normalize_font_name(&alias);
    if candidate.normalized_aliases.contains(&normalized) {
        return;
    }

    candidate.normalized_aliases.insert(normalized);
    candidate.aliases.push(alias);
}

/// 路径存在则返回其字符串形式，否则返回 `None`。
fn existing_font_path(path: PathBuf) -> Option<String> {
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

/// 插入或更新候选集：同路径则追加别名，否则新增条目。
fn upsert_font_candidate(
    candidates: &mut Vec<SystemFontCandidate>,
    candidate_by_path: &mut HashMap<String, usize>,
    path: String,
    aliases: impl IntoIterator<Item = String>,
) {
    let index = if let Some(index) = candidate_by_path.get(&path).copied() {
        index
    } else {
        candidates.push(SystemFontCandidate {
            path: path.clone(),
            aliases: Vec::new(),
            normalized_aliases: HashSet::new(),
        });
        let index = candidates.len() - 1;
        candidate_by_path.insert(path, index);
        index
    };

    for alias in aliases {
        push_unique_alias(&mut candidates[index], &alias);
    }
}

// ---- 字体配置构建 ----

/// 将候选加入最终输出列表，同源文件合并 family_names。
fn add_font_source(
    fonts: &mut Vec<SubtitleSystemFontSource>,
    loaded_sources: &mut HashMap<String, usize>,
    source: &str,
    aliases: &[String],
) {
    let index = if let Some(index) = loaded_sources.get(source).copied() {
        index
    } else {
        fonts.push(SubtitleSystemFontSource {
            family_names: Vec::new(),
            path: source.to_string(),
        });
        let index = fonts.len() - 1;
        loaded_sources.insert(source.to_string(), index);
        index
    };

    for alias in aliases {
        let alias = alias
            .trim()
            .trim_matches(|ch| matches!(ch, '"' | '\''))
            .trim_start_matches('@')
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        if alias.is_empty() {
            continue;
        }

        let normalized = normalize_font_name(&alias);
        if fonts[index]
            .family_names
            .iter()
            .any(|existing| normalize_font_name(existing) == normalized)
        {
            continue;
        }

        fonts[index].family_names.push(alias);
    }
}

/// 按归一化名称在候选集中查找字体。
fn find_font_candidate<'a>(
    candidates: &'a [SystemFontCandidate],
    font_name: &str,
) -> Option<&'a SystemFontCandidate> {
    let normalized = normalize_font_name(font_name);
    if normalized.is_empty() {
        return None;
    }

    candidates
        .iter()
        .find(|c| c.normalized_aliases.contains(&normalized))
}

/// 从候选集中选出默认字体：优先平台推荐 → 其次请求列表第一项。
fn pick_default_candidate<'a>(
    candidates: &'a [SystemFontCandidate],
    requested_font_families: &[String],
    default_preferences: &[&str],
) -> Option<(&'a SystemFontCandidate, String)> {
    for font_name in default_preferences {
        if let Some(candidate) = find_font_candidate(candidates, font_name) {
            return Some((candidate, (*font_name).to_string()));
        }
    }

    for font_name in requested_font_families {
        if let Some(candidate) = find_font_candidate(candidates, font_name) {
            let default_font = candidate
                .aliases
                .first()
                .cloned()
                .unwrap_or_else(|| font_name.clone());
            return Some((candidate, default_font));
        }
    }

    None
}

/// 构建最终返回前端的字体配置。
///
/// 流程：遍历请求字体 → 匹配候选 → 合并 family_names → 选默认字体 → 补齐 sans-serif 别名。
fn build_font_config(
    candidates: &[SystemFontCandidate],
    requested_font_families: &[String],
    default_preferences: &[&str],
    fallback_default_font: &str,
) -> SubtitleSystemFontConfig {
    let mut fonts = Vec::new();
    let mut loaded_sources = HashMap::new();
    let mut unmatched_families: Vec<String> = Vec::new();

    for requested_font in requested_font_families {
        let Some(candidate) = find_font_candidate(candidates, requested_font) else {
            unmatched_families.push(requested_font.clone());
            continue;
        };

        let mut aliases = candidate.aliases.clone();
        let alias_normalized = normalize_font_name(requested_font);
        if !candidate.normalized_aliases.contains(&alias_normalized) {
            aliases.push(requested_font.clone());
        }
        add_font_source(&mut fonts, &mut loaded_sources, &candidate.path, &aliases);
    }

    let mut default_font = fallback_default_font.to_string();
    if let Some((candidate, resolved_default_font)) =
        pick_default_candidate(candidates, requested_font_families, default_preferences)
    {
        default_font = resolved_default_font;
        let mut aliases = candidate.aliases.clone();
        let default_normalized = normalize_font_name(&default_font);
        if !candidate.normalized_aliases.contains(&default_normalized) {
            aliases.push(default_font.clone());
        }
        push_unique_alias_inline(&mut aliases, "sans-serif");
        add_font_source(&mut fonts, &mut loaded_sources, &candidate.path, &aliases);
    }

    SubtitleSystemFontConfig {
        default_font,
        fonts,
        unmatched_families,
    }
}

fn push_unique_alias_inline(aliases: &mut Vec<String>, alias: &str) {
    let alias = alias.trim().to_string();
    if alias.is_empty() {
        return;
    }
    let normalized = normalize_font_name(&alias);
    if aliases
        .iter()
        .any(|existing| normalize_font_name(existing) == normalized)
    {
        return;
    }
    aliases.push(alias);
}

// ---------------------------------------------------------------------------
// 非 Windows 平台：ttf-parser 读取 name 表
// ---------------------------------------------------------------------------

/// 从字体文件读取所有 family name（支持 .ttc 集合）。
#[cfg(not(target_os = "windows"))]
fn read_font_family_names(path: &str) -> Vec<String> {
    let Ok(data) = std::fs::read(path) else {
        return Vec::new();
    };

    // Try as a single face first
    if let Ok(face) = ttf_parser::Face::parse(&data, 0) {
        let names = collect_face_family_names(&face);
        if !names.is_empty() {
            return names;
        }
    }

    // Try as a TrueType Collection (.ttc)
    let count = ttf_parser::fonts_in_collection(&data).unwrap_or(0);
    let mut all_names = Vec::new();
    let mut seen = HashSet::new();

    for index in 0..count {
        if let Ok(face) = ttf_parser::Face::parse(&data, index) {
            for name in collect_face_family_names(&face) {
                let key = name.to_lowercase();
                if seen.insert(key) {
                    all_names.push(name);
                }
            }
        }
    }

    all_names
}

#[cfg(not(target_os = "windows"))]
fn collect_face_family_names(face: &ttf_parser::Face) -> Vec<String> {
    // Name IDs: 1=Font Family, 16=Typographic Family (preferred)
    let preferred_ids = [16, 1];
    let mut names = Vec::new();
    let mut seen = HashSet::new();

    for &name_id in &preferred_ids {
        if let Some(name) = find_best_name(face, name_id) {
            if !name.is_empty() && seen.insert(name.to_lowercase()) {
                names.push(name);
            }
        }
    }

    names
}

/// 从 name 表中按语言优先级查找最佳名称（英文 > Macintosh > 其他）。
#[cfg(not(target_os = "windows"))]
fn find_best_name(face: &ttf_parser::Face, name_id: u16) -> Option<String> {
    for record in face.names() {
        if record.name_id != name_id {
            continue;
        }

        let lang_score = record_lang_score(record.platform_id, record.language_id);
        if lang_score == 0 {
            continue;
        }

        if let Some(text) = record.to_string() {
            let text = text.trim().to_string();
            if !text.is_empty() {
                return Some(text);
            }
        }
    }

    None
}

/// 语言评分：英文(US)最高优先 → 其他 Windows 语言 → Macintosh → 未知。
#[cfg(not(target_os = "windows"))]
fn record_lang_score(platform_id: ttf_parser::PlatformId, language_id: u16) -> u32 {
    match platform_id {
        ttf_parser::PlatformId::Windows => {
            if language_id == 0x0409 {
                3 // English (United States) - highest priority
            } else {
                2 // Windows other language
            }
        }
        ttf_parser::PlatformId::Macintosh => 1,
        _ => 0, // Unknown/unsupported
    }
}

// ---------------------------------------------------------------------------
// 非 Windows 平台：目录扫描
// ---------------------------------------------------------------------------

/// 判断文件名是否为支持的字体格式（.ttf / .ttc / .otf）。
#[cfg(not(target_os = "windows"))]
fn is_font_extension(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".ttf") || lower.ends_with(".ttc") || lower.ends_with(".otf")
}

/// 递归扫描目录，返回所有字体文件路径及其 family name。
#[cfg(not(target_os = "windows"))]
fn scan_font_directory(dir: &Path) -> Vec<(String, Vec<String>)> {
    let mut results = Vec::new();
    scan_dir_recursive(dir, &mut results);
    results
}

#[cfg(not(target_os = "windows"))]
fn scan_dir_recursive(dir: &Path, results: &mut Vec<(String, Vec<String>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            scan_dir_recursive(&path, results);
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if is_font_extension(name) {
                if let Some(path_str) = existing_font_path(path) {
                    let family_names = read_font_family_names(&path_str);
                    if !family_names.is_empty() {
                        results.push((path_str, family_names));
                    }
                }
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with('~') {
        if let Some(home) = dirs_fallback() {
            return home.join(path.trim_start_matches("~/"));
        }
    }
    PathBuf::from(path)
}

#[cfg(not(target_os = "windows"))]
fn dirs_fallback() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// Windows：注册表扫描 + 已知字体后备
// ---------------------------------------------------------------------------

/// 去除注册表字体值中的 "(TrueType)" 等后缀。
#[cfg(target_os = "windows")]
fn strip_registry_font_suffix(font_name: &str) -> &str {
    match font_name.rsplit_once(" (") {
        Some((name, suffix)) if suffix.ends_with(')') => name,
        _ => font_name,
    }
}

#[cfg(target_os = "windows")]
fn registry_font_aliases(font_name: &str) -> Vec<String> {
    let stripped = strip_registry_font_suffix(font_name);
    let mut aliases = vec![stripped.to_string()];

    for part in stripped.split(['&', ',']) {
        let part = part.trim();
        if !part.is_empty() {
            aliases.push(part.to_string());
        }
    }

    aliases
}

#[cfg(target_os = "windows")]
fn add_windows_known_candidates(
    candidates: &mut Vec<SystemFontCandidate>,
    candidate_by_path: &mut HashMap<String, usize>,
    fonts_dir: &Path,
) {
    let known_candidates: [(&str, &[&str]); 4] = [
        ("msyh.ttc", &["Microsoft YaHei", "微软雅黑"]),
        ("msyhbd.ttc", &["Microsoft YaHei Bold", "微软雅黑 Bold"]),
        ("simsun.ttc", &["SimSun", "宋体"]),
        ("simhei.ttf", &["SimHei", "黑体"]),
    ];

    for (file_name, aliases) in known_candidates {
        let Some(path) = existing_font_path(fonts_dir.join(file_name)) else {
            continue;
        };

        upsert_font_candidate(
            candidates,
            candidate_by_path,
            path,
            aliases.iter().map(|alias| (*alias).to_string()),
        );
    }
}

#[cfg(target_os = "windows")]
fn build_windows_font_candidates() -> Vec<SystemFontCandidate> {
    let windows_dir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"));
    let fonts_dir = windows_dir.join("Fonts");

    let mut candidates = Vec::new();
    let mut candidate_by_path = HashMap::new();

    if let Ok(fonts_key) = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion\\Fonts")
    {
        for item in fonts_key.enum_values().flatten() {
            let font_name = item.0;
            let Ok(font_value) = fonts_key.get_value::<String, _>(&font_name) else {
                continue;
            };

            let resolved_path = {
                let font_path = PathBuf::from(&font_value);
                if font_path.is_absolute() {
                    existing_font_path(font_path)
                } else {
                    existing_font_path(fonts_dir.join(&font_value))
                }
            };
            let Some(path) = resolved_path else {
                continue;
            };

            upsert_font_candidate(
                &mut candidates,
                &mut candidate_by_path,
                path,
                registry_font_aliases(&font_name),
            );
        }
    }

    add_windows_known_candidates(&mut candidates, &mut candidate_by_path, &fonts_dir);
    candidates
}

#[cfg(target_os = "windows")]
static WINDOWS_FONT_CANDIDATES: LazyLock<Vec<SystemFontCandidate>> =
    LazyLock::new(build_windows_font_candidates);

#[cfg(target_os = "windows")]
fn resolve_subtitle_system_font_config(font_families: &[String]) -> SubtitleSystemFontConfig {
    build_font_config(
        &WINDOWS_FONT_CANDIDATES,
        font_families,
        &["Microsoft YaHei", "SimSun", "SimHei"],
        "Microsoft YaHei",
    )
}

// ---------------------------------------------------------------------------
// macOS：目录扫描 + CoreText 字体后备
// ---------------------------------------------------------------------------

#[cfg(target_os = "macos")]
fn build_macos_font_candidates() -> Vec<SystemFontCandidate> {
    let font_dirs = [
        PathBuf::from("/System/Library/Fonts"),
        PathBuf::from("/Library/Fonts"),
        expand_tilde("~/Library/Fonts"),
    ];

    let mut candidates = Vec::new();
    let mut candidate_by_path = HashMap::new();

    // Also check known font paths as a safety net
    let known_aliases: [(&str, &[&str]); 3] = [
        ("PingFang SC", &["PingFang SC", "苹方"]),
        ("Hiragino Sans GB", &["Hiragino Sans GB"]),
        ("Songti SC", &["Songti SC"]),
    ];

    for font_dir in &font_dirs {
        for (path_str, family_names) in scan_font_directory(font_dir) {
            upsert_font_candidate(
                &mut candidates,
                &mut candidate_by_path,
                path_str,
                family_names,
            );
        }
    }

    // Supplement known aliases for fonts that may not have been found
    for (font_name, aliases) in known_aliases {
        if find_font_candidate(&candidates, font_name).is_some() {
            // Already found, aliases are already present from name table
            continue;
        }
        // Try hardcoded paths as fallback
        for (file, _) in MACOS_FALLBACK_PATHS {
            if let Some(existing) = existing_font_path(PathBuf::from(file)) {
                upsert_font_candidate(
                    &mut candidates,
                    &mut candidate_by_path,
                    existing,
                    aliases.iter().map(|a| (*a).to_string()),
                );
                break;
            }
        }
    }

    candidates
}

#[cfg(target_os = "macos")]
static MACOS_FALLBACK_PATHS: [(&str, &[&str]); 3] = [
    (
        "/System/Library/Fonts/PingFang.ttc",
        &["PingFang SC", "苹方"],
    ),
    (
        "/System/Library/Fonts/Hiragino Sans GB.ttc",
        &["Hiragino Sans GB"],
    ),
    (
        "/System/Library/Fonts/Supplemental/Songti.ttc",
        &["Songti SC"],
    ),
];

#[cfg(target_os = "macos")]
static MACOS_FONT_CANDIDATES: LazyLock<Vec<SystemFontCandidate>> =
    LazyLock::new(build_macos_font_candidates);

#[cfg(target_os = "macos")]
fn resolve_subtitle_system_font_config(font_families: &[String]) -> SubtitleSystemFontConfig {
    build_font_config(
        &MACOS_FONT_CANDIDATES,
        font_families,
        &["PingFang SC", "Hiragino Sans GB", "Songti SC"],
        "PingFang SC",
    )
}

// ---------------------------------------------------------------------------
// Linux：目录扫描 + 已知字体后备
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn build_linux_font_candidates() -> Vec<SystemFontCandidate> {
    let font_dirs = [
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
        expand_tilde("~/.local/share/fonts"),
        expand_tilde("~/.fonts"),
    ];

    let mut candidates = Vec::new();
    let mut candidate_by_path = HashMap::new();

    for font_dir in &font_dirs {
        for (path_str, family_names) in scan_font_directory(font_dir) {
            upsert_font_candidate(
                &mut candidates,
                &mut candidate_by_path,
                path_str,
                family_names,
            );
        }
    }

    // Supplement known aliases as safety net
    let known_aliases: [(&str, &[&str]); 4] = [
        ("Noto Sans CJK SC", &["Noto Sans CJK SC", "Noto Sans SC"]),
        (
            "Noto Sans CJK SC Bold",
            &["Noto Sans CJK SC Bold", "Noto Sans SC Bold"],
        ),
        ("WenQuanYi Zen Hei", &["WenQuanYi Zen Hei"]),
        ("WenQuanYi Micro Hei", &["WenQuanYi Micro Hei"]),
    ];

    for (font_name, aliases) in known_aliases {
        if find_font_candidate(&candidates, font_name).is_some() {
            continue;
        }
        // Try known paths
        for (file, _) in LINUX_FALLBACK_PATHS {
            if let Some(existing) = existing_font_path(PathBuf::from(file)) {
                upsert_font_candidate(
                    &mut candidates,
                    &mut candidate_by_path,
                    existing,
                    aliases.iter().map(|a| (*a).to_string()),
                );
                break;
            }
        }
    }

    candidates
}

#[cfg(target_os = "linux")]
static LINUX_FALLBACK_PATHS: [(&str, &[&str]); 3] = [
    (
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        &["Noto Sans CJK SC", "Noto Sans SC"],
    ),
    (
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
        &["Noto Sans CJK SC Bold", "Noto Sans SC Bold"],
    ),
    (
        "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
        &["WenQuanYi Zen Hei"],
    ),
];

#[cfg(target_os = "linux")]
static LINUX_FONT_CANDIDATES: LazyLock<Vec<SystemFontCandidate>> =
    LazyLock::new(build_linux_font_candidates);

#[cfg(target_os = "linux")]
fn resolve_subtitle_system_font_config(font_families: &[String]) -> SubtitleSystemFontConfig {
    build_font_config(
        &LINUX_FONT_CANDIDATES,
        font_families,
        &[
            "Noto Sans CJK SC",
            "WenQuanYi Zen Hei",
            "WenQuanYi Micro Hei",
        ],
        "Noto Sans CJK SC",
    )
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn resolve_subtitle_system_font_config(_font_families: &[String]) -> SubtitleSystemFontConfig {
    SubtitleSystemFontConfig::default()
}

// ---- Tauri command ----

/// 前端调用的入口：传入期望的字体族名列表，返回系统字体配置。
///
/// 移动端/iOS/Android 直接返回空配置（暂不支持系统字体扫描）。
#[tauri::command]
pub fn subtitle_get_system_font_config(font_families: Vec<String>) -> SubtitleSystemFontConfig {
    resolve_subtitle_system_font_config(&font_families)
}
