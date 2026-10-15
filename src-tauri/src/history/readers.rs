use crate::agents::{AgentKind, Permission, TurnSettings};
use chrono::{DateTime, NaiveDateTime};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use std::{
    collections::HashMap,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Clone, Debug)]
pub struct Source {
    pub agent: AgentKind,
    pub native_id: String,
    pub path: PathBuf,
    pub stamp: String,
    pub title: Option<String>,
    pub cwd: Option<String>,
    pub updated_at: i64,
    pub model: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Message {
    pub role: String,
    pub text: String,
    pub created_at: i64,
}
#[derive(Clone, Debug)]
pub struct Session {
    pub source: Source,
    pub title: String,
    pub cwd: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub messages: Vec<Message>,
    pub settings: TurnSettings,
}
pub fn stamp(path: &Path) -> String {
    [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.display())),
    ]
    .iter()
    .map(|p| {
        p.metadata()
            .ok()
            .map(|m| {
                format!(
                    "{}:{}",
                    m.modified()
                        .ok()
                        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                        .map(|t| t.as_nanos())
                        .unwrap_or(0),
                    m.len()
                )
            })
            .unwrap_or_else(|| "-".into())
    })
    .collect::<Vec<_>>()
    .join("|")
}
fn modified(path: &Path) -> i64 {
    path.metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
pub fn timestamp(value: &Value) -> Option<i64> {
    if let Some(v) = value.as_f64() {
        return Some(if v > 1e14 {
            (v / 1000.0) as i64
        } else if v > 1e11 {
            v as i64
        } else {
            (v * 1000.0) as i64
        });
    }
    let text = value.as_str()?.trim();
    let iso = if text.contains('T') {
        text.into()
    } else {
        text.replacen(' ', "T", 1)
    };
    DateTime::parse_from_rfc3339(&iso)
        .ok()
        .map(|t| t.timestamp_millis())
        .or_else(|| {
            NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S%.f")
                .ok()
                .map(|t| t.and_utc().timestamp_millis())
        })
}
fn jsonl(path: &Path, mut visit: impl FnMut(Value)) -> Result<(), String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    for line in BufReader::new(file).lines() {
        if let Ok(line) = line
            && let Ok(value) = serde_json::from_str(&line)
        {
            visit(value);
        }
    }
    Ok(())
}
fn blocks(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.into();
    }
    value
        .as_array()
        .map(|blocks| {
            blocks
                .iter()
                .filter_map(|block| {
                    if let Some(text) = block.as_str() {
                        Some(text)
                    } else if matches!(
                        block["type"].as_str(),
                        Some("text" | "input_text" | "output_text")
                    ) {
                        block["text"].as_str()
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}
pub(super) fn noise(text: &str) -> bool {
    let text = text.trim_start();
    text.is_empty()
        || [
            "# AGENTS.md instructions",
            "<system-reminder",
            "<command-name>",
            "<command-message>",
            "<local-command",
            "<user-prompt-submit-hook",
            "<environment_context",
            "<user_instructions",
            "Caveat: The messages below",
            "This session is being continued from a previous",
        ]
        .iter()
        .any(|prefix| text.starts_with(prefix))
}
fn clean_title(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(120)
        .collect()
}
fn open_ro(path: &Path) -> rusqlite::Result<Connection> {
    let c = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    c.busy_timeout(std::time::Duration::from_secs(2))?;
    Ok(c)
}
fn paths(root: &Path, depth: usize, suffix: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if depth > 0 && path.is_dir() {
            paths(&path, depth - 1, suffix, out);
        } else if depth == 0
            && path.is_file()
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(suffix))
        {
            out.push(path);
        }
    }
}
fn configured_root(
    home: &Path,
    vars: &HashMap<String, String>,
    key: &str,
    default: &str,
) -> PathBuf {
    vars.get(key)
        .filter(|path| !path.trim().is_empty())
        .map(|path| {
            if path == "~" {
                home.to_path_buf()
            } else if let Some(rest) = path.strip_prefix("~/") {
                home.join(rest)
            } else {
                PathBuf::from(path)
            }
        })
        .unwrap_or_else(|| home.join(default))
}

pub fn discover(home: &Path, vars: &HashMap<String, String>, agents: &[AgentKind]) -> Vec<Source> {
    let mut out = Vec::new();
    let mut files = Vec::new();
    if agents.contains(&AgentKind::Claude) {
        paths(
            &configured_root(home, vars, "CLAUDE_CONFIG_DIR", ".claude").join("projects"),
            1,
            ".jsonl",
            &mut files,
        );
        for path in files.drain(..) {
            out.push(Source {
                agent: AgentKind::Claude,
                native_id: path.file_stem().unwrap().to_string_lossy().into_owned(),
                stamp: stamp(&path),
                updated_at: modified(&path),
                path,
                title: None,
                cwd: None,
                model: None,
            });
        }
    }
    if agents.contains(&AgentKind::Codex) {
        let index = configured_root(home, vars, "CODEX_HOME", ".codex").join("session_index.jsonl");
        let mut titles = HashMap::new();
        let _ = jsonl(&index, |r| {
            if let (Some(id), Some(title)) = (r["id"].as_str(), r["thread_name"].as_str()) {
                titles.insert(id.to_owned(), title.to_owned());
            }
        });
        paths(
            &configured_root(home, vars, "CODEX_HOME", ".codex").join("sessions"),
            3,
            ".jsonl",
            &mut files,
        );
        for path in files.drain(..) {
            let stem = path.file_stem().unwrap().to_string_lossy();
            if !stem.starts_with("rollout-") {
                continue;
            }
            let native_id = stem
                .chars()
                .rev()
                .take(36)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>();
            out.push(Source {
                agent: AgentKind::Codex,
                title: titles.get(&native_id).cloned(),
                native_id: native_id.clone(),
                cwd: None,
                model: None,
                stamp: format!(
                    "{}|{}",
                    stamp(&path),
                    titles
                        .get(&native_id)
                        .map(String::as_str)
                        .unwrap_or_default()
                ),
                updated_at: modified(&path),
                path,
            });
        }
    }
    if agents.contains(&AgentKind::Muse) {
        let index = configured_root(home, vars, "XDG_DATA_HOME", ".local/share")
            .join("muse/session-index.db");
        if let Ok(c)=open_ro(&index) && let Ok(mut q)=c.prepare("SELECT session_id,session_log_path,workspace_root,COALESCE(effective_long_title,title),first_user_prompt,COALESCE(msp_last_activity_at_us,updated_at_us),model_id FROM sessions WHERE msp_parent_session_id IS NULL AND prompt_count>0")
        && let Ok(rows)=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,Option<String>>(4)?,r.get::<_,i64>(5)?,r.get::<_,Option<String>>(6)?))) {
            for (id,path,cwd,title,prompt,updated,model) in rows.flatten() {
                let path=PathBuf::from(path);let title=title.filter(|t|!t.trim().is_empty() && t.to_lowercase()!="new session").or(prompt);
                out.push(Source{agent:AgentKind::Muse,native_id:id,stamp:format!("{}|{}",stamp(&path),stamp(&index)),path,title,cwd,updated_at:updated/1000,model});
            }
        }
    }
    if agents.contains(&AgentKind::Agy) {
        let index = home.join(".gemini/antigravity-cli/conversation_summaries.db");
        if let Ok(c)=open_ro(&index) && let Ok(mut q)=c.prepare("SELECT conversation_id,title,preview,workspace_uris,last_modified_time FROM conversation_summaries") && let Ok(rows)=q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?,r.get::<_,Option<String>>(2)?,r.get::<_,Option<String>>(3)?,r.get::<_,Option<String>>(4)?))) {
        for (id,title,preview,uris,time) in rows.flatten() {
            let path=home.join(".gemini/antigravity-cli/conversations").join(format!("{id}.db"));if !path.is_file(){continue;}
            let cwd=uris.and_then(|text|serde_json::from_str::<Vec<String>>(&text).ok()).and_then(|uris|uris.into_iter().next()).and_then(|uri|url::Url::parse(&uri).ok()).and_then(|uri|uri.to_file_path().ok()).map(|p|p.to_string_lossy().into_owned());
            out.push(Source{agent:AgentKind::Agy,native_id:id,stamp:format!("{}|{}",stamp(&path),stamp(&index)),path,title:title.filter(|t|!t.trim().is_empty()).or(preview),cwd,updated_at:time.and_then(|t|timestamp(&Value::String(t))).unwrap_or(0),model:None});
        }
    }
    }
    for source in &mut out {
        source.stamp = format!("history-v1|{}", source.stamp);
    }
    out
}
fn muse_records(record: &Value, messages: &mut Vec<Message>, time: i64) {
    let time = timestamp(&record["recorded_at"]).unwrap_or(time);
    let payload = &record["payload"];
    if payload["kind"] == "run" {
        let event = &payload["event"];
        let (role, key) = match event["kind"].as_str() {
            Some("started") => ("user", "prompt"),
            Some("user_message_committed") => ("user", "text"),
            Some("assistant_message_committed") => ("assistant", "text"),
            _ => ("", ""),
        };
        if let Some(text) = event[key].as_str()
            && !noise(text)
        {
            messages.push(Message {
                role: role.into(),
                text: text.into(),
                created_at: time,
            });
        }
    }
    if let Some(children) = record["children"].as_array() {
        for child in children {
            if let Some(raw) = child["record_json"].as_str()
                && let Ok(record) = serde_json::from_str::<Value>(raw)
            {
                muse_records(&record, messages, time);
            }
        }
    }
}
fn varint(bytes: &[u8], pos: &mut usize) -> Option<u64> {
    let mut value = 0;
    for shift in (0..=63).step_by(7) {
        let byte = *bytes.get(*pos)?;
        *pos += 1;
        value |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Some(value);
        }
    }
    None
}
fn pb_strings(bytes: &[u8], depth: usize, out: &mut Vec<String>) {
    let mut pos = 0;
    while pos < bytes.len() {
        let Some(key) = varint(bytes, &mut pos) else {
            break;
        };
        match key & 7 {
            0 => {
                if varint(bytes, &mut pos).is_none() {
                    break;
                }
            }
            1 => pos += 8,
            5 => pos += 4,
            2 => {
                let Some(len) = varint(bytes, &mut pos).and_then(|n| usize::try_from(n).ok())
                else {
                    break;
                };
                let Some(end) = pos.checked_add(len).filter(|end| *end <= bytes.len()) else {
                    break;
                };
                let sub = &bytes[pos..end];
                pos = end;
                if let Ok(text) = std::str::from_utf8(sub)
                    && text.chars().count() >= 8
                    && text
                        .chars()
                        .filter(|c| !c.is_control() || ['\n', '\t', '\r'].contains(c))
                        .count()
                        * 10
                        > text.chars().count() * 9
                {
                    out.push(text.into());
                } else if depth < 12 {
                    pb_strings(sub, depth + 1, out);
                }
            }
            _ => break,
        }
    }
}
fn pb_field<'a>(bytes: &'a [u8], path: &[u64]) -> Option<&'a [u8]> {
    let mut pos = 0;
    while pos < bytes.len() {
        let key = varint(bytes, &mut pos)?;
        match key & 7 {
            0 => {
                varint(bytes, &mut pos)?;
            }
            1 => {
                pos = pos.checked_add(8)?;
            }
            5 => {
                pos = pos.checked_add(4)?;
            }
            2 => {
                let len = usize::try_from(varint(bytes, &mut pos)?).ok()?;
                let end = pos.checked_add(len)?;
                let sub = bytes.get(pos..end)?;
                pos = end;
                if key >> 3 == path[0] {
                    return if path.len() == 1 {
                        Some(sub)
                    } else {
                        pb_field(sub, &path[1..])
                    };
                }
            }
            _ => return None,
        }
    }
    None
}
fn agy_text(bytes: &[u8], kind: i64) -> String {
    if let Some(bytes) = pb_field(bytes, if kind == 14 { &[19, 2] } else { &[20, 1] })
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        return text.to_owned();
    }
    let mut strings = Vec::new();
    pb_strings(bytes, 0, &mut strings);
    strings
        .into_iter()
        .map(|text| text.trim().to_owned())
        .filter(|text| {
            !text.starts_with("sessionID")
                && !text.starts_with("bot-")
                && !(text.len() >= 16 && text.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
                && !((text.starts_with('{') && text.ends_with('}'))
                    || (text.starts_with('[') && text.ends_with(']')))
        })
        .max_by_key(String::len)
        .unwrap_or_default()
}
pub fn parse(source: &Source) -> Result<Session, String> {
    let mut source = source.clone();
    let mut messages = Vec::new();
    let mut settings = TurnSettings {
        agent: source.agent,
        model: source.model.clone(),
        ..TurnSettings::default()
    };
    let mut cwd = source.cwd.clone().unwrap_or_default();
    let mut started = 0;
    let mut updated = source.updated_at;
    match source.agent {
        AgentKind::Claude | AgentKind::Codex => jsonl(&source.path, |record| {
            let time = timestamp(&record["timestamp"]).unwrap_or(0);
            if time > 0 {
                if started == 0 || time < started {
                    started = time;
                }
                updated = updated.max(time);
            }
            if source.agent == AgentKind::Claude {
                if cwd.is_empty() {
                    cwd = record["cwd"].as_str().unwrap_or_default().into();
                }
                if let Some(mode) = record["permissionMode"].as_str() {
                    settings.permission = match mode {
                        "bypassPermissions" => Permission::Yolo,
                        "plan" => Permission::Plan,
                        _ => Permission::Ask,
                    };
                }
                if let Some(model) = record["message"]["model"].as_str() {
                    settings.model = Some(model.into());
                }
                if !matches!(record["type"].as_str(), Some("user" | "assistant"))
                    || record["isMeta"] == true
                    || record["isSidechain"] == true
                {
                    return;
                }
                let text = blocks(&record["message"]["content"]);
                if !noise(&text) {
                    messages.push(Message {
                        role: record["type"].as_str().unwrap().into(),
                        text,
                        created_at: time,
                    });
                }
            } else {
                let payload = &record["payload"];
                if record["type"] == "session_meta" {
                    cwd = payload["cwd"].as_str().unwrap_or(&cwd).into();
                    if let Some(id) = payload["session_id"]
                        .as_str()
                        .or_else(|| payload["id"].as_str())
                    {
                        source.native_id = id.into();
                    }
                } else if record["type"] == "turn_context" {
                    settings.model = payload["model"].as_str().map(str::to_owned);
                    settings.effort = payload["effort"].as_str().map(str::to_owned);
                    settings.permission = if payload["approval_policy"] == "never" {
                        Permission::Yolo
                    } else {
                        Permission::Ask
                    };
                } else if record["type"] == "response_item"
                    && payload["type"] == "message"
                    && matches!(payload["role"].as_str(), Some("user" | "assistant"))
                {
                    let text = blocks(&payload["content"]);
                    if !noise(&text) {
                        messages.push(Message {
                            role: payload["role"].as_str().unwrap().into(),
                            text,
                            created_at: time,
                        });
                    }
                }
            }
        })?,
        AgentKind::Muse => jsonl(&source.path, |record| {
            muse_records(&record, &mut messages, updated)
        })?,
        AgentKind::Agy => {
            let c = open_ro(&source.path).map_err(|e| e.to_string())?;
            let mut q=c.prepare("SELECT step_type,step_payload FROM steps WHERE step_type IN (14,15) ORDER BY idx").map_err(|e|e.to_string())?;
            let rows = q
                .query_map([], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, Option<Vec<u8>>>(1)?))
                })
                .map_err(|e| e.to_string())?;
            for (kind, blob) in rows.flatten() {
                let text = agy_text(blob.as_deref().unwrap_or_default(), kind);
                if !noise(&text) {
                    messages.push(Message {
                        role: if kind == 14 { "user" } else { "assistant" }.into(),
                        text,
                        created_at: updated,
                    });
                }
            }
        }
    }
    if messages.is_empty() {
        return Err("No conversation text was found in this native session.".into());
    }
    if cwd.is_empty() {
        return Err("This native session has no project folder.".into());
    }
    if started == 0 {
        started = messages
            .iter()
            .filter_map(|m| (m.created_at > 0).then_some(m.created_at))
            .min()
            .unwrap_or(updated);
    }
    updated = updated.max(messages.iter().map(|m| m.created_at).max().unwrap_or(0));
    let title = source
        .title
        .as_deref()
        .map(clean_title)
        .filter(|t| !noise(t))
        .unwrap_or_else(|| {
            messages
                .iter()
                .find(|m| m.role == "user")
                .map(|m| clean_title(&m.text))
                .unwrap_or_else(|| "Terminal conversation".into())
        });
    Ok(Session {
        source,
        title,
        cwd,
        created_at: started,
        updated_at: updated,
        messages,
        settings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn field(key: u64, value: &[u8]) -> Vec<u8> {
        fn v(mut value: u64, out: &mut Vec<u8>) {
            while value >= 128 {
                out.push((value as u8 & 127) | 128);
                value >>= 7;
            }
            out.push(value as u8);
        }
        let mut out = Vec::new();
        v(key << 3 | 2, &mut out);
        v(value.len() as u64, &mut out);
        out.extend_from_slice(value);
        out
    }
    #[test]
    fn agy_message_fields_ignore_longer_metadata_and_keep_short_replies() {
        let mut bytes = field(
            5,
            &field(12, b"Metadata longer than the short assistant response"),
        );
        bytes.extend(field(20, &field(1, b"Hi")));
        assert_eq!(agy_text(&bytes, 15), "Hi");
        let mut bytes = field(
            5,
            &field(20, b"Metadata longer than the short user message"),
        );
        bytes.extend(field(19, &field(2, b"Yes")));
        assert_eq!(agy_text(&bytes, 14), "Yes");
    }
    #[test]
    fn numeric_and_timezone_timestamps_use_milliseconds() {
        assert_eq!(
            timestamp(&serde_json::json!(1767323045000000_i64)),
            Some(1767323045000)
        );
        assert_eq!(
            timestamp(&serde_json::json!("2026-01-02 03:04:05+00:00")),
            Some(1767323045000)
        );
    }
}
