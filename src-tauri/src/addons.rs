//! Add-ons («Дополнения»): one HTML file with its manifest inside, installed from a file the owner picks.
//! The copy in the app folder runs (hash checked on every load), in a sandboxed frame on its own origin with a CSP
//! from the manifest. It talks to the wallet only through the app's message bridge; the permissions it was given at
//! install gate every call. Never: keys of existing accounts, words, signing. Model and risks: PLAN «Add-ons».
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tauri::AppHandle;

/// what an add-on may ask for; each is shown to the owner by name before install
pub const PERMS: [&str; 5] = ["accounts", "balances", "history", "keys", "transfer"];
const TAG: &str = r#"<script type="application/json" id="talon-addon">"#;
const MAX_HTML: usize = 2 << 20;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)] pub description: String,
    #[serde(default)] pub permissions: Vec<String>,
    #[serde(default)] pub hosts: Vec<String>, // https origins the add-on may reach (its own server, an RPC)
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Installed { #[serde(flatten)] pub m: Manifest, pub sha256: String }

fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }
fn sha(html: &str) -> String { hex(&Sha256::digest(html.as_bytes())) }

/// the manifest from the file itself, checked
pub fn parse(html: &str) -> Result<Manifest, String> {
    if html.len() > MAX_HTML { return Err("дополнение больше 2 МБ".into()) }
    let start = html.find(TAG).ok_or("это не дополнение Talon: нет описания внутри файла")? + TAG.len();
    let end = html[start..].find("</script>").ok_or("описание дополнения не закрыто")? + start;
    let m: Manifest = serde_json::from_str(&html[start..end]).map_err(|e| format!("описание дополнения не читается: {e}"))?;
    let id_ok = (2..=32).contains(&m.id.len()) && m.id.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-') && !m.id.starts_with('-');
    if !id_ok { return Err("id дополнения: 2–32 знака, латиница в нижнем регистре, цифры, дефис".into()) }
    let name_len = m.name.trim().chars().count();
    if name_len == 0 || name_len > 24 { return Err("имя дополнения: 1–24 знака".into()) }
    if m.version.is_empty() || m.version.len() > 16 || m.description.chars().count() > 200 { return Err("версия или описание дополнения слишком длинные".into()) }
    if let Some(p) = m.permissions.iter().find(|p| !PERMS.contains(&p.as_str())) { return Err(format!("неизвестное разрешение «{p}»")) }
    if m.hosts.len() > 8 { return Err("не больше 8 адресов в сети".into()) }
    for h in &m.hosts {
        let rest = h.strip_prefix("https://").ok_or_else(|| format!("адрес «{h}»: только https://"))?;
        let host = rest.split(':').next().unwrap_or("");
        let port_ok = rest.split_once(':').is_none_or(|(_, p)| !p.is_empty() && p.len() <= 5 && p.bytes().all(|b| b.is_ascii_digit()));
        if host.is_empty() || !host.contains('.') || !host.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'.' || b == b'-') || !port_ok {
            return Err(format!("адрес «{h}»: только https://имя.домен, без пути"));
        }
    }
    Ok(m)
}

/// the frame's rules: inline code of the file only, network only to the manifest's hosts
pub fn csp(m: &Manifest) -> String {
    let connect = if m.hosts.is_empty() { "'none'".to_string() } else { m.hosts.join(" ") };
    format!("default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; font-src data:; connect-src {connect}; form-action 'none'; frame-src 'none'; worker-src 'none'; base-uri 'none'; webrtc 'block'")
}
// WebRTC reaches any STUN/TURN host past connect-src, and Chromium ignores `webrtc 'block'`: the page's first script
// takes the constructors away for good (no other realm to fetch them from: frames, workers and popups are closed)
const LOCK: &str = "<script>for(const k of['RTCPeerConnection','webkitRTCPeerConnection','RTCDataChannel','RTCRtpSender','RTCRtpReceiver','RTCIceTransport'])try{Object.defineProperty(window,k,{value:undefined,writable:false,configurable:false})}catch{}</script>";
fn locked(html: &str) -> String {
    let at = if html.get(..9).is_some_and(|d| d.eq_ignore_ascii_case("<!doctype")) { html.find('>').map_or(0, |i| i + 1) } else { 0 };
    format!("{}{LOCK}{}", &html[..at], &html[at..])
}
// the one add-on whose tab is open: only its page is served, so a frame cannot navigate itself into another add-on's
// page and use the first one's rights through the same window (Codex 10.10)
static ACTIVE: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());
pub fn activate(id: &str) { *ACTIVE.lock().unwrap() = id.to_string() }

fn dir(app: &AppHandle) -> Result<PathBuf, String> { Ok(crate::vault_path(app)?.with_file_name("addons")) }
pub fn list(app: &AppHandle) -> Result<Vec<Installed>, String> {
    match std::fs::read_to_string(dir(app)?.join("index.json")) {
        Ok(t) => serde_json::from_str(&t).map_err(|_| "список дополнений повреждён".to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(_) => Err("список дополнений не читается".into()),
    }
}
// content-addressed: an update writes the new file first, then the index points at it; a failed step leaves the old pair
fn file(app: &AppHandle, a: &Installed) -> Result<PathBuf, String> { Ok(dir(app)?.join(format!("{}-{}.html", a.m.id, &a.sha256[..16]))) }
fn save(app: &AppHandle, l: &[Installed]) -> Result<(), String> {
    crate::vault::durable(&dir(app)?.join("index.json"), &serde_json::to_string_pretty(l).map_err(|e| e.to_string())?)
}
static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

// Android WebView hands its JS bridge (Tauri's IPC) to every frame, so a frame there could call the core as the
// wallet. Add-ons stay desktop-only until the frame is isolated (WebView2 delivers IPC from the top frame only: tested).
pub const SUPPORTED: bool = cfg!(desktop);

/// install or update (same id): the file is copied in; the copy is what runs
pub fn install(app: &AppHandle, html: &str) -> Result<Vec<Installed>, String> {
    if !SUPPORTED { return Err("дополнения пока только на компьютере".into()) }
    let m = parse(html)?;
    let _w = WRITE.lock().unwrap();
    let d = dir(app)?;
    std::fs::create_dir_all(&d).map_err(|_| "не удалось создать папку дополнений".to_string())?;
    let new = Installed { m, sha256: sha(html) };
    crate::vault::durable(&file(app, &new)?, html)?;
    let mut l = list(app)?;
    let old: Vec<Installed> = l.iter().filter(|x| x.m.id == new.m.id && x.sha256 != new.sha256).cloned().collect();
    l.retain(|x| x.m.id != new.m.id);
    l.push(new);
    save(app, &l)?;
    for o in old { let _ = std::fs::remove_file(file(app, &o)?); }
    Ok(l)
}
pub fn remove(app: &AppHandle, id: &str) -> Result<Vec<Installed>, String> {
    let _w = WRITE.lock().unwrap();
    let mut l = list(app)?;
    let gone: Vec<Installed> = l.iter().filter(|x| x.m.id == id).cloned().collect();
    l.retain(|x| x.m.id != id);
    save(app, &l)?; // rights go first, then the file
    for g in gone { let _ = std::fs::remove_file(file(app, &g)?); }
    Ok(l)
}
pub fn granted(app: &AppHandle, id: &str, perm: &str) -> Result<(), String> {
    let l = list(app)?;
    let a = l.iter().find(|x| x.m.id == id).ok_or("дополнение не установлено")?;
    if a.m.permissions.iter().any(|p| p == perm) { Ok(()) } else { Err(format!("у дополнения «{}» нет этого разрешения", a.m.name)) }
}

/// the installed copy, only if it is still the file that was installed
pub fn page(app: &AppHandle, id: &str) -> Option<(String, String)> {
    if !SUPPORTED || *ACTIVE.lock().unwrap() != id { return None }
    let a = list(app).ok()?.into_iter().find(|x| x.m.id == id)?;
    let html = std::fs::read_to_string(file(app, &a).ok()?).ok()?;
    (sha(&html) == a.sha256).then(|| (locked(&html), csp(&a.m)))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn file(m: &str) -> String { format!("<!doctype html><title>x</title>{TAG}{m}</script><p>hi</p>") }

    #[test]
    fn manifest_checks() {
        let ok = parse(&file(r#"{"id":"extract","name":"Extract","version":"1.0.0","permissions":["accounts","keys"],"hosts":["https://api.example.com"]}"#)).unwrap();
        assert_eq!((ok.id.as_str(), ok.permissions.len()), ("extract", 2));
        assert!(parse(&file(r#"{"id":"sweep","name":"Сбор","version":"1.0.0","permissions":["history","transfer"]}"#)).is_ok(), "v2 permissions");
        assert_eq!(csp(&ok).split("connect-src ").nth(1).unwrap().split(';').next(), Some("https://api.example.com"));
        assert!(csp(&Manifest { hosts: vec![], ..ok.clone() }).contains("connect-src 'none'"));
        assert!(parse("<p>no manifest</p>").is_err());
        for bad in [
            r#"{"id":"Extract","name":"x","version":"1"}"#,          // upper case id
            r#"{"id":"../x","name":"x","version":"1"}"#,             // path in id
            r#"{"id":"x1","name":"","version":"1"}"#,                 // empty name
            r#"{"id":"x1","name":"x","version":"1","permissions":["sign"]}"#,
            r#"{"id":"x1","name":"x","version":"1","hosts":["http://a.com"]}"#,
            r#"{"id":"x1","name":"x","version":"1","hosts":["https://a.com/path"]}"#,
            r#"{"id":"x1","name":"x","version":"1","hosts":["https://*.com"]}"#,
            r#"{"id":"x1","name":"x","version":"1","hosts":["https://a.com 'unsafe-eval'"]}"#,
        ] { assert!(parse(&file(bad)).is_err(), "{bad}") }
        assert!(parse(&format!("{}{}", file(r#"{"id":"x1","name":"x","version":"1"}"#), "a".repeat(MAX_HTML))).is_err());
        assert!(parse(&file(r#"{"id":"x1","name":"x","version":"1","hosts":["https://rpc.ankr.com:8443"]}"#)).is_ok());
        // the WebRTC lock lands after the doctype (standards mode kept) or first
        assert!(locked("<!DOCTYPE html><p>").starts_with(&format!("<!DOCTYPE html>{LOCK}")));
        assert!(locked("<p>x").starts_with(LOCK));
    }
}
