use serde::Serialize;
use std::collections::HashSet;
use std::path::Path;
use std::{collections::HashMap, path::PathBuf, sync::LazyLock};

#[cfg(target_os = "windows")]
use winreg::{RegKey, enums::HKEY_LOCAL_MACHINE};

#[derive(Debug)]
struct SystemFontCandidate {
    path: String,
    aliases: Vec<String>,
    normalized_aliases: HashSet<String>,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSystemFontSource {
    family_names: Vec<String>,
    path: String,
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SubtitleSystemFontConfig {
    default_font: String,
    fonts: Vec<SubtitleSystemFontSource>,
    #[serde(default)]
    unmatched_families: Vec<String>,
}

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

fn existing_font_path(path: PathBuf) -> Option<String> {
    path.is_file().then(|| path.to_string_lossy().into_owned())
}

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
// Font file name table parsing (non-Windows platforms)
// ---------------------------------------------------------------------------

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
// Directory scanning helpers (non-Windows platforms)
// ---------------------------------------------------------------------------

#[cfg(not(target_os = "windows"))]
fn is_font_extension(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".ttf") || lower.ends_with(".ttc") || lower.ends_with(".otf")
}

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
// Windows font discovery (registry + known candidates)
// ---------------------------------------------------------------------------

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
// macOS font discovery (directory scan + CoreText alternatives)
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
    ("/System/Library/Fonts/PingFang.ttc", &["PingFang SC", "苹方"]),
    ("/System/Library/Fonts/Hiragino Sans GB.ttc", &["Hiragino Sans GB"]),
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
// Linux font discovery (directory scan)
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
        &["Noto Sans CJK SC", "WenQuanYi Zen Hei", "WenQuanYi Micro Hei"],
        "Noto Sans CJK SC",
    )
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn resolve_subtitle_system_font_config(_font_families: &[String]) -> SubtitleSystemFontConfig {
    SubtitleSystemFontConfig::default()
}

#[tauri::command]
pub fn subtitle_get_system_font_config(font_families: Vec<String>) -> SubtitleSystemFontConfig {
    resolve_subtitle_system_font_config(&font_families)
}
