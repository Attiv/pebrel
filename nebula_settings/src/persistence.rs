//! Settings file line-level reads and rewrites; typed defaults remain with RawSettings.

use crate::{settings_dir, settings_path};

/// 把若干键值写回 `nebula_settings.txt`：已有键**原地替换**（保留行序、
/// 未知键与原样格式），缺失键追加到尾部。全量读-改-写；与旧壳的全量覆盖
/// 写并存时后写者胜——与旧壳多窗口的既有语义一致。
pub fn persist_keys(updates: &[(&str, String)]) -> std::io::Result<()> {
    let path = settings_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error),
    };
    let updated = apply_updates(&text, updates);
    std::fs::create_dir_all(settings_dir())?;
    std::fs::write(&path, updated)
}

/// `nebula_settings.txt` 里的 `keybind=combo:Action` 行（行序即优先级，
/// 后写者胜）。两壳共读这一份；`RawSettings` 的哈希视图按 key 去重，表达
/// 不了多行 keybind，必须走行级解析。
pub fn keybind_pairs() -> Vec<(String, String)> {
    std::fs::read_to_string(settings_path())
        .map(|text| keybind_pairs_from_text(&text))
        .unwrap_or_default()
}

/// [`keybind_pairs`] 的纯文本解析，单测锁定。
pub fn keybind_pairs_from_text(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else { continue };
        if !key.trim().eq_ignore_ascii_case("keybind") {
            continue;
        }
        let Some((combo, action)) = value.split_once(':') else { continue };
        let (combo, action) = (combo.trim(), action.trim());
        if combo.is_empty() || action.is_empty() {
            continue;
        }
        pairs.push((combo.to_owned(), action.to_owned()));
    }
    pairs
}

/// 整表替换全部 `keybind=` 行（删除旧行、按给定顺序追加到尾部），其余行
/// 原样保留（注释、未知键、行序）。键位编辑器的提交路径——逐行增删的
/// 语义由调用方（keymap.rs）先算好整表再落盘。
pub fn persist_keybinds(pairs: &[(String, String)]) -> std::io::Result<()> {
    let path = settings_path();
    let text = std::fs::read_to_string(&path).unwrap_or_default();
    let updated = apply_keybinds(&text, pairs);
    std::fs::create_dir_all(settings_dir())?;
    std::fs::write(&path, updated)
}

/// [`persist_keybinds`] 的纯文本变换，单测锁定。
pub fn apply_keybinds(text: &str, pairs: &[(String, String)]) -> String {
    let mut out = String::with_capacity(text.len() + 64);
    for line in text.lines() {
        let is_keybind =
            line.split_once('=').is_some_and(|(key, _)| key.trim().eq_ignore_ascii_case("keybind"));
        if !is_keybind {
            out.push_str(line);
            out.push('\n');
        }
    }
    for (combo, action) in pairs {
        out.push_str("keybind=");
        out.push_str(combo);
        out.push(':');
        out.push_str(action);
        out.push('\n');
    }
    out
}

/// [`persist_keys`] 的纯文本变换，单测锁定行为。
pub fn apply_updates(text: &str, updates: &[(&str, String)]) -> String {
    let mut pending: Vec<(usize, &(&str, String))> = updates.iter().enumerate().collect();
    let mut out = String::with_capacity(text.len() + 64);

    for line in text.lines() {
        let key = line.split_once('=').map(|(key, _)| key.trim().to_ascii_lowercase());
        let hit = key
            .as_deref()
            .and_then(|key| pending.iter().position(|(_, (k, _))| k.eq_ignore_ascii_case(key)));
        match hit {
            Some(ix) => {
                let (_, (key, value)) = pending.remove(ix);
                out.push_str(key);
                out.push('=');
                out.push_str(value);
            },
            None => out.push_str(line),
        }
        out.push('\n');
    }

    // 追加缺失键，保持调用方给出的次序。
    pending.sort_by_key(|(order, _)| *order);
    for (_, (key, value)) in pending {
        out.push_str(key);
        out.push('=');
        out.push_str(value);
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apply_updates_replaces_in_place_and_appends_missing() {
        let text = "language=system\ntheme=Nebula\nshell=powershell\n";
        let updated =
            apply_updates(text, &[("theme", "SilverLight".into()), ("cursor_blink", "1".into())]);
        assert_eq!(
            updated,
            "language=system\ntheme=SilverLight\nshell=powershell\ncursor_blink=1\n"
        );
    }

    #[test]
    fn keybind_pairs_parse_and_rewrite() {
        let text = "theme=Nebula\nkeybind=ctrl+x:Copy\n# comment\nkeybind=alt+f1:SplitRight\n";
        assert_eq!(
            keybind_pairs_from_text(text),
            vec![
                ("ctrl+x".to_owned(), "Copy".to_owned()),
                ("alt+f1".to_owned(), "SplitRight".to_owned()),
            ]
        );
        let rewritten = apply_keybinds(text, &[("ctrl+y".to_owned(), "Paste".to_owned())]);
        assert_eq!(rewritten, "theme=Nebula\n# comment\nkeybind=ctrl+y:Paste\n");
    }

    #[test]
    fn apply_updates_preserves_unknown_lines_and_matches_case_insensitively() {
        let text = "# comment survives\nTHEME=CoalDark\nkeybind=ctrl+x=Copy\n";
        let updated = apply_updates(text, &[("theme", "MossDark".into())]);
        // 键名按调用方写法输出，注释与奇形行原样保留。
        assert_eq!(updated, "# comment survives\ntheme=MossDark\nkeybind=ctrl+x=Copy\n");
    }

    #[test]
    fn apply_updates_on_empty_file_appends_all() {
        let updated =
            apply_updates("", &[("theme", "Nebula".into()), ("font_size", "12.5".into())]);
        assert_eq!(updated, "theme=Nebula\nfont_size=12.5\n");
    }
}
