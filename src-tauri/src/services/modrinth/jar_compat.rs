
use std::cmp::Ordering;
use std::io::{Cursor, Read};
use std::path::Path;

use zip::read::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct McVersion {
    major: u32,
    minor: u32,
    patch: u32,
}

fn parse_mc_version(raw: &str) -> Option<McVersion> {
    let s = raw.trim().trim_start_matches('v');
    if s.is_empty() || !s.chars().next()?.is_ascii_digit() {
        return None;
    }
    let mut parts = s.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts
        .next()
        .map(|p| {
            let digits: String = p.chars().take_while(|c| c.is_ascii_digit()).collect();
            digits.parse().unwrap_or(0)
        })
        .unwrap_or(0);
    Some(McVersion {
        major,
        minor,
        patch,
    })
}

fn cmp_ver(a: McVersion, b: McVersion) -> Ordering {
    a.cmp(&b)
}

pub fn fabric_minecraft_range_matches(range: &str, game_version: &str) -> Option<bool> {
    let game = parse_mc_version(game_version)?;
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return Some(true);
    }

    if let Some(base) = range.strip_suffix(".x").or_else(|| range.strip_suffix(".X")) {
        let base_ver = parse_mc_version(base)?;
        let next_minor = McVersion {
            major: base_ver.major,
            minor: base_ver.minor + 1,
            patch: 0,
        };
        let floor = McVersion {
            major: base_ver.major,
            minor: base_ver.minor,
            patch: 0,
        };
        return Some(cmp_ver(game, floor) != Ordering::Less && cmp_ver(game, next_minor) == Ordering::Less);
    }

    if range
        .chars()
        .all(|c| c.is_ascii_digit() || c == '.' || c == ' ')
        && !range.contains(' ')
    {
        let expected = parse_mc_version(range)?;
        let range_parts = range.split('.').count();
        return Some(match range_parts {
            1 => game.major == expected.major,
            2 => game.major == expected.major && game.minor == expected.minor,
            _ => game == expected,
        });
    }

    let mut ok = true;
    let mut saw_predicate = false;
    for token in range.split_whitespace() {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let (op, ver_str) = if let Some(v) = token.strip_prefix(">=") {
            (">=", v)
        } else if let Some(v) = token.strip_prefix("<=") {
            ("<=", v)
        } else if let Some(v) = token.strip_prefix('>') {
            (">", v)
        } else if let Some(v) = token.strip_prefix('<') {
            ("<", v)
        } else if let Some(v) = token.strip_prefix('=') {
            ("=", v)
        } else if let Some(v) = token.strip_prefix('~') {
            let ver = parse_mc_version(v)?;
            saw_predicate = true;
            let next = McVersion {
                major: ver.major,
                minor: ver.minor + 1,
                patch: 0,
            };
            ok = ok && cmp_ver(game, ver) != Ordering::Less && cmp_ver(game, next) == Ordering::Less;
            continue;
        } else if let Some(v) = token.strip_prefix('^') {
            let ver = parse_mc_version(v)?;
            saw_predicate = true;
            let next = McVersion {
                major: ver.major,
                minor: ver.minor + 1,
                patch: 0,
            };
            ok = ok && cmp_ver(game, ver) != Ordering::Less && cmp_ver(game, next) == Ordering::Less;
            continue;
        } else {
            return None;
        };

        let ver = parse_mc_version(ver_str)?;
        saw_predicate = true;
        ok = ok
            && match op {
                ">=" => cmp_ver(game, ver) != Ordering::Less,
                ">" => cmp_ver(game, ver) == Ordering::Greater,
                "<=" => cmp_ver(game, ver) != Ordering::Greater,
                "<" => cmp_ver(game, ver) == Ordering::Less,
                "=" => game == ver,
                _ => return None,
            };
    }

    if !saw_predicate {
        return None;
    }
    Some(ok)
}

pub fn maven_minecraft_range_matches(range: &str, game_version: &str) -> Option<bool> {
    let game = parse_mc_version(game_version)?;
    let range = range.trim();
    if range.is_empty() || range == "*" {
        return Some(true);
    }

    let chars: Vec<char> = range.chars().collect();
    if chars.len() < 3 {
        return None;
    }
    let start_incl = match chars[0] {
        '[' => true,
        '(' => false,
        _ => return None,
    };
    let end_incl = match chars[chars.len() - 1] {
        ']' => true,
        ')' => false,
        _ => return None,
    };
    let inner: String = chars[1..chars.len() - 1].iter().collect();
    let (left_raw, right_raw) = match inner.split_once(',') {
        Some(pair) => pair,
        None => {
            let only = parse_mc_version(inner.trim())?;
            return Some(game == only);
        }
    };

    if !left_raw.trim().is_empty() {
        let left = parse_mc_version(left_raw)?;
        let left_ok = if start_incl {
            cmp_ver(game, left) != Ordering::Less
        } else {
            cmp_ver(game, left) == Ordering::Greater
        };
        if !left_ok {
            return Some(false);
        }
    }
    if !right_raw.trim().is_empty() {
        let right = parse_mc_version(right_raw)?;
        let right_ok = if end_incl {
            cmp_ver(game, right) != Ordering::Greater
        } else {
            cmp_ver(game, right) == Ordering::Less
        };
        if !right_ok {
            return Some(false);
        }
    }
    Some(true)
}

fn read_zip_entry(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    let cursor = Cursor::new(bytes);
    let mut archive = ZipArchive::new(cursor).ok()?;
    let mut file = archive.by_name(name).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    if buf.is_empty() {
        None
    } else {
        Some(buf)
    }
}

fn fabric_mod_title_and_mc_range(json_bytes: &[u8]) -> (Option<String>, Option<String>) {
    let Ok(json) = serde_json::from_slice::<serde_json::Value>(json_bytes) else {
        return (None, None);
    };
    let title = json
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let range = json
        .pointer("/depends/minecraft")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            json.get("depends")
                .and_then(|d| d.as_object())
                .and_then(|obj| {
                    obj.iter().find_map(|(k, v)| {
                        if k.eq_ignore_ascii_case("minecraft") {
                            v.as_str().map(|s| s.trim().to_string())
                        } else {
                            None
                        }
                    })
                })
                .filter(|s| !s.is_empty())
        });
    (title, range)
}

fn mods_toml_title_and_mc_range(text: &str) -> (Option<String>, Option<String>) {
    let mut title: Option<String> = None;
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t
            .strip_prefix("displayName")
            .or_else(|| t.strip_prefix("displayName "))
        {
            let rest = rest.trim().trim_start_matches('=').trim();
            let val = rest.trim_matches('"').trim_matches('\'').trim();
            if !val.is_empty() && title.is_none() {
                title = Some(val.to_string());
            }
        }
    }

    let lower = text.to_ascii_lowercase();
    let mut range: Option<String> = None;
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find("modid") {
        let abs = search_from + rel;
        let after = &text[abs..];
        let line_end = after.find('\n').unwrap_or(after.len());
        let line = &after[..line_end];
        let is_minecraft = line.to_ascii_lowercase().contains("minecraft");
        if is_minecraft {
            let window = &text[abs..((abs + 400).min(text.len()))];
            for wline in window.lines().take(12) {
                let wl = wline.trim();
                if let Some(rest) = wl
                    .strip_prefix("versionRange")
                    .or_else(|| wl.strip_prefix("versionRange "))
                {
                    let rest = rest.trim().trim_start_matches('=').trim();
                    let val = rest.trim_matches('"').trim_matches('\'').trim();
                    if !val.is_empty() {
                        range = Some(val.to_string());
                        break;
                    }
                }
            }
            if range.is_some() {
                break;
            }
        }
        search_from = abs + 5;
    }

    (title, range)
}

pub fn jar_supports_game_version(
    jar_path: &Path,
    game_version: &str,
) -> (Option<bool>, Option<String>) {
    let bytes = match std::fs::read(jar_path) {
        Ok(b) => b,
        Err(_) => return (None, None),
    };

    if let Some(json_bytes) = read_zip_entry(&bytes, "fabric.mod.json") {
        let (title, range) = fabric_mod_title_and_mc_range(&json_bytes);
        if let Some(range) = range {
            if let Some(ok) = fabric_minecraft_range_matches(&range, game_version) {
                return (Some(ok), title);
            }
        }
    }

    for toml_name in ["META-INF/mods.toml", "META-INF/neoforge.mods.toml"] {
        if let Some(toml_bytes) = read_zip_entry(&bytes, toml_name) {
            let Ok(text) = String::from_utf8(toml_bytes) else {
                continue;
            };
            let (title, range) = mods_toml_title_and_mc_range(&text);
            if let Some(range) = range {
                if let Some(ok) = maven_minecraft_range_matches(&range, game_version) {
                    return (Some(ok), title);
                }
            }
        }
    }

    (None, None)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JarModRelation {
    pub mod_id: String,
    pub optional: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JarModInfo {
    pub filename: String,
    pub enabled: bool,
    pub mod_id: Option<String>,
    pub title: Option<String>,
    pub version: Option<String>,
    pub loader: Option<String>,
    pub provides: Vec<String>,
    pub depends: Vec<JarModRelation>,
    pub breaks: Vec<JarModRelation>,
}

fn is_platform_mod_id(id: &str) -> bool {
    matches!(
        id.to_ascii_lowercase().as_str(),
        "minecraft"
            | "java"
            | "fabricloader"
            | "fabric-loader"
            | "quilt_loader"
            | "quilt-loader"
            | "forge"
            | "neoforge"
            | "mcp"
            | "fabric"
            | "quilt"
    )
}

fn push_unique(out: &mut Vec<String>, value: String) {
    if value.is_empty() {
        return;
    }
    if !out.iter().any(|x| x.eq_ignore_ascii_case(&value)) {
        out.push(value);
    }
}

fn fabric_json_relations(
    obj: Option<&serde_json::Map<String, serde_json::Value>>,
    optional_default: bool,
) -> Vec<JarModRelation> {
    let Some(obj) = obj else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, _val) in obj {
        let id = key.trim();
        if id.is_empty() || is_platform_mod_id(id) {
            continue;
        }
        out.push(JarModRelation {
            mod_id: id.to_string(),
            optional: optional_default,
        });
    }
    out
}

fn parse_fabric_like_mod_json(json_bytes: &[u8], loader_hint: &str) -> Option<JarModInfo> {
    let json: serde_json::Value = serde_json::from_slice(json_bytes).ok()?;
    let mod_id = json
        .get("id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let title = json
        .get("name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let version = json
        .get("version")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let mut provides = Vec::new();
    if let Some(arr) = json.get("provides").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(s) = item.as_str() {
                push_unique(&mut provides, s.trim().to_string());
            }
        }
    }

    let depends_obj = json.get("depends").and_then(|v| v.as_object());
    let recommends_obj = json.get("recommends").and_then(|v| v.as_object());
    let suggests_obj = json.get("suggests").and_then(|v| v.as_object());
    let mut depends = fabric_json_relations(depends_obj, false);
    depends.extend(fabric_json_relations(recommends_obj, true));
    depends.extend(fabric_json_relations(suggests_obj, true));

    let breaks_obj = json.get("breaks").and_then(|v| v.as_object());
    let conflicts_obj = json.get("conflicts").and_then(|v| v.as_object());
    let mut breaks = fabric_json_relations(breaks_obj, false);
    breaks.extend(fabric_json_relations(conflicts_obj, true));

    Some(JarModInfo {
        filename: String::new(),
        enabled: true,
        mod_id,
        title,
        version,
        loader: Some(loader_hint.to_string()),
        provides,
        depends,
        breaks,
    })
}

fn unquote_toml_value(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .trim_matches('\'')
        .trim()
        .to_string()
}

fn parse_quilt_mod_json(json_bytes: &[u8]) -> Option<JarModInfo> {
    let json: serde_json::Value = serde_json::from_slice(json_bytes).ok()?;
    if let Some(loader) = json.get("quilt_loader") {
        let mod_id = loader
            .get("id")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let title = loader
            .pointer("/metadata/name")
            .and_then(|v| v.as_str())
            .or_else(|| json.get("name").and_then(|v| v.as_str()))
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let version = loader
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);

        let mut provides = Vec::new();
        if let Some(arr) = loader.get("provides").and_then(|v| v.as_array()) {
            for item in arr {
                let id = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .or_else(|| item.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                if let Some(id) = id {
                    push_unique(&mut provides, id.to_string());
                }
            }
        }

        let mut depends = Vec::new();
        let mut breaks = Vec::new();
        if let Some(arr) = loader.get("depends").and_then(|v| v.as_array()) {
            for item in arr {
                let id = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let Some(id) = id else { continue };
                if is_platform_mod_id(id) {
                    continue;
                }
                let optional = item
                    .get("optional")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                depends.push(JarModRelation {
                    mod_id: id.to_string(),
                    optional,
                });
            }
        }
        if let Some(arr) = loader.get("breaks").and_then(|v| v.as_array()) {
            for item in arr {
                let id = item
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let Some(id) = id else { continue };
                if is_platform_mod_id(id) {
                    continue;
                }
                breaks.push(JarModRelation {
                    mod_id: id.to_string(),
                    optional: false,
                });
            }
        }

        return Some(JarModInfo {
            filename: String::new(),
            enabled: true,
            mod_id,
            title,
            version,
            loader: Some("quilt".into()),
            provides,
            depends,
            breaks,
        });
    }

    parse_fabric_like_mod_json(json_bytes, "quilt")
}

fn parse_mods_toml_info(text: &str, loader_hint: &str) -> Option<JarModInfo> {
    let mut mod_id: Option<String> = None;
    let mut title: Option<String> = None;
    let mut version: Option<String> = None;
    let mut in_mods = false;

    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_mods = t == "[[mods]]" || t.starts_with("[[mods.");
            continue;
        }
        if !in_mods {
            continue;
        }
        if let Some(rest) = t.strip_prefix("modId").or_else(|| t.strip_prefix("modId ")) {
            let val = unquote_toml_value(rest.trim().trim_start_matches('='));
            if !val.is_empty() && mod_id.is_none() {
                mod_id = Some(val);
            }
        } else if let Some(rest) = t
            .strip_prefix("displayName")
            .or_else(|| t.strip_prefix("displayName "))
        {
            let val = unquote_toml_value(rest.trim().trim_start_matches('='));
            if !val.is_empty() && title.is_none() {
                title = Some(val);
            }
        } else if let Some(rest) = t.strip_prefix("version").or_else(|| t.strip_prefix("version "))
        {
            if rest.starts_with('=') || rest.starts_with(' ') || rest.is_empty() {
                let val = unquote_toml_value(rest.trim().trim_start_matches('='));
                if !val.is_empty() && !val.starts_with('$') && version.is_none() {
                    version = Some(val);
                }
            }
        }
    }

    let mut depends = Vec::new();
    let mut breaks = Vec::new();
    let lower = text.to_ascii_lowercase();
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find("[[dependencies.") {
        let abs = search_from + rel;
        let window_end = (abs + 500).min(text.len());
        let window = &text[abs..window_end];
        let mut dep_id: Option<String> = None;
        let mut mandatory = true;
        let mut side_type = String::from("required");
        for wline in window.lines().skip(1).take(20) {
            let wl = wline.trim();
            if wl.starts_with('[') {
                break;
            }
            if let Some(rest) = wl.strip_prefix("modId").or_else(|| wl.strip_prefix("modId ")) {
                let val = unquote_toml_value(rest.trim().trim_start_matches('='));
                if !val.is_empty() {
                    dep_id = Some(val);
                }
            } else if let Some(rest) = wl
                .strip_prefix("mandatory")
                .or_else(|| wl.strip_prefix("mandatory "))
            {
                let val = unquote_toml_value(rest.trim().trim_start_matches('='));
                mandatory = !matches!(val.to_ascii_lowercase().as_str(), "false" | "0");
            } else if let Some(rest) = wl
                .strip_prefix("type")
                .or_else(|| wl.strip_prefix("type "))
            {
                let val = unquote_toml_value(rest.trim().trim_start_matches('='));
                if !val.is_empty() {
                    side_type = val;
                }
            }
        }
        if let Some(id) = dep_id {
            if !is_platform_mod_id(&id) {
                let kind = side_type.to_ascii_lowercase();
                if kind == "incompatible" || kind == "discouraged" {
                    breaks.push(JarModRelation {
                        mod_id: id,
                        optional: kind == "discouraged",
                    });
                } else {
                    depends.push(JarModRelation {
                        mod_id: id,
                        optional: !mandatory || kind == "optional",
                    });
                }
            }
        }
        search_from = abs + 16;
    }

    if mod_id.is_none() && title.is_none() {
        return None;
    }

    Some(JarModInfo {
        filename: String::new(),
        enabled: true,
        mod_id,
        title,
        version,
        loader: Some(loader_hint.to_string()),
        provides: Vec::new(),
        depends,
        breaks,
    })
}

fn guess_loader_from_filename(filename: &str) -> Option<String> {
    let f = filename.to_ascii_lowercase();
    if f.contains("neoforge") {
        Some("neoforge".into())
    } else if f.contains("forge") {
        Some("forge".into())
    } else if f.contains("fabric") {
        Some("fabric".into())
    } else if f.contains("quilt") {
        Some("quilt".into())
    } else {
        None
    }
}

pub fn read_jar_mod_info(jar_path: &Path, filename: &str, enabled: bool) -> JarModInfo {
    let empty = JarModInfo {
        filename: filename.to_string(),
        enabled,
        mod_id: None,
        title: None,
        version: None,
        loader: guess_loader_from_filename(filename),
        provides: Vec::new(),
        depends: Vec::new(),
        breaks: Vec::new(),
    };

    let bytes = match std::fs::read(jar_path) {
        Ok(b) => b,
        Err(_) => return empty,
    };

    if let Some(json_bytes) = read_zip_entry(&bytes, "fabric.mod.json") {
        if let Some(mut info) = parse_fabric_like_mod_json(&json_bytes, "fabric") {
            info.filename = filename.to_string();
            info.enabled = enabled;
            if info.loader.is_none() {
                info.loader = guess_loader_from_filename(filename);
            }
            return info;
        }
    }

    if let Some(json_bytes) = read_zip_entry(&bytes, "quilt.mod.json") {
        if let Some(mut info) = parse_quilt_mod_json(&json_bytes) {
            info.filename = filename.to_string();
            info.enabled = enabled;
            return info;
        }
    }

    for (toml_name, loader) in [
        ("META-INF/neoforge.mods.toml", "neoforge"),
        ("META-INF/mods.toml", "forge"),
    ] {
        if let Some(toml_bytes) = read_zip_entry(&bytes, toml_name) {
            let Ok(text) = String::from_utf8(toml_bytes) else {
                continue;
            };
            if let Some(mut info) = parse_mods_toml_info(&text, loader) {
                info.filename = filename.to_string();
                info.enabled = enabled;
                if info.loader.is_none() {
                    info.loader = guess_loader_from_filename(filename);
                }
                return info;
            }
        }
    }

    empty
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fabric_range_below_1_21() {
        assert_eq!(
            fabric_minecraft_range_matches(">=1.20.1 <1.21", "1.20.1"),
            Some(true)
        );
        assert_eq!(
            fabric_minecraft_range_matches(">=1.20.1 <1.21", "1.21.9"),
            Some(false)
        );
    }

    #[test]
    fn maven_range_below_1_21() {
        assert_eq!(
            maven_minecraft_range_matches("[1.20.1,1.21)", "1.20.1"),
            Some(true)
        );
        assert_eq!(
            maven_minecraft_range_matches("[1.20.1,1.21)", "1.21.9"),
            Some(false)
        );
    }
}
