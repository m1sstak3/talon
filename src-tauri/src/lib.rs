mod agent;
mod clip;
mod evm;
mod ledger;
mod prices;
mod solana;
mod vault;

use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};
use zeroize::Zeroizing;

// Trust boundary: Rust holds keys, builds and signs. The UI (one window, owner's call 07.10, reviewed with Codex) types
// secrets into a sheet that only opens after the window is excluded from screen capture (verified by readback). The
// bundled frontend is trusted app code: strict CSP, no remote scripts. A send is confirmed by its draft id: Rust
// signs only the message it built itself, once.

#[derive(Default)]
pub(crate) struct Session {
    pub(crate) unlocked: bool,
    pub(crate) accounts: Vec<vault::Account>, // proven by decryption at unlock/save; the file's header is not trusted after that
    epoch: u64, // bumped by send_cancel / lock: a draft prepared across either is dropped
    pending: Option<Zeroizing<String>>, // a freshly generated phrase while the user writes it down
    send: Option<Send>,                 // the one transfer waiting for confirmation
}
struct Send { id: String, account: String, draft: Draft }
enum Draft { Sol(solana::Draft), Evm(evm::Draft) }
pub(crate) struct App { pub(crate) session: Mutex<Session>, pub(crate) agent: Mutex<agent::Hub> }

pub(crate) fn vault_path(app: &AppHandle) -> Result<PathBuf, String> {
    // dev/test builds may point at a throwaway vault; release builds always use the app data folder
    #[cfg(debug_assertions)]
    if let Ok(p) = std::env::var("CW_VAULT") { return Ok(PathBuf::from(p)) }
    Ok(app.path().app_data_dir().map_err(|_| "no app data folder".to_string())?.join("vault.json"))
}
/// tell the window what changed; it re-reads public data itself
fn changed(app: &AppHandle, accounts: &[vault::Account], unlocked: bool) {
    let _ = app.emit_to("main", "vault-changed", serde_json::json!({ "accounts": accounts, "unlocked": unlocked }));
}
// a send whose outcome is not known yet, kept on disk until the network decides (survives a restart)
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct Pending {
    sig: String, cluster: String, last_valid: u64, // Solana; cluster "evm" marks an EVM send
    #[serde(default)] chain: u64, #[serde(default)] rpc: String, #[serde(default)] from: String, #[serde(default)] nonce: u64,
}
static PENDING: Mutex<()> = Mutex::new(());
fn journal(app: &AppHandle) -> Result<PathBuf, String> { Ok(vault_path(app)?.with_file_name("pending.json")) }
/// only "no journal yet" is empty; a broken journal is an error, never silently forgotten (second security review 07.10)
fn pending_read(app: &AppHandle) -> Result<Vec<Pending>, String> {
    match std::fs::read_to_string(journal(app)?) {
        Ok(t) => serde_json::from_str(&t).map_err(|_| "журнал переводов повреждён".to_string()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(_) => Err("журнал переводов не читается".into()),
    }
}
fn pending_edit(app: &AppHandle, f: impl FnOnce(&mut Vec<Pending>)) -> Result<(), String> {
    let _g = PENDING.lock().unwrap();
    let mut list = pending_read(app)?;
    f(&mut list);
    if let Some(dir) = journal(app)?.parent() { let _ = std::fs::create_dir_all(dir); }
    vault::durable(&journal(app)?, &serde_json::to_string(&list).map_err(|e| e.to_string())?)
}
/// follow one send to its outcome; only a final answer removes it from the pending list
fn follow(app: AppHandle, p: Pending) {
    if p.cluster == "evm" { return follow_evm(app, p) }
    tauri::async_runtime::spawn(async move {
        let Ok(c) = solana::Cluster::parse(&p.cluster) else { return };
        let (a2, s2) = (app.clone(), p.sig.clone()); // early, provisional "confirmed"; the journal waits for finalized
        let state = match solana::track(&p.sig, c, p.last_valid, move || { let _ = a2.emit_to("main", "send-status", serde_json::json!({ "sig": s2, "state": "confirmed" })); }).await {
            solana::Outcome::Confirmed => "finalized", solana::Outcome::Failed => "failed",
            solana::Outcome::Expired => "expired", solana::Outcome::Unknown => "unknown",
        };
        if state != "unknown" { let _ = pending_edit(&app, |l| l.retain(|x| x.sig != p.sig)); }
        let _ = app.emit_to("main", "send-status", serde_json::json!({ "sig": p.sig, "state": state }));
    });
}

fn follow_evm(app: AppHandle, p: Pending) {
    tauri::async_runtime::spawn(async move {
        let (a2, s2) = (app.clone(), p.sig.clone());
        let state = match evm::track(&p.rpc, &p.sig, &p.from, p.nonce, move || { let _ = a2.emit_to("main", "send-status", serde_json::json!({ "sig": s2, "state": "confirmed" })); }).await {
            evm::Outcome::Confirmed => "finalized", evm::Outcome::Failed => "failed", evm::Outcome::Replaced => "replaced",
            evm::Outcome::Included => "confirmed", evm::Outcome::Unknown => "unknown",
        };
        if !matches!(state, "unknown" | "confirmed") { let _ = pending_edit(&app, |l| l.retain(|x| x.sig != p.sig)); } // not final yet: the journal keeps it for the next start
        let _ = app.emit_to("main", "send-status", serde_json::json!({ "sig": p.sig, "state": state }));
    });
}

// ---------------------------------------------------------------- own node per network
// ponytail: plain file next to the vault; the URL's key reads the chain, it cannot move money. Encrypt if that changes.
const OWN_NETS: [&str; 2] = ["mainnet", "devnet"];
fn own_path(app: &AppHandle) -> Result<PathBuf, String> { Ok(vault_path(app)?.with_file_name("rpc.json")) }
fn own_load(app: &AppHandle) {
    let Ok(t) = own_path(app).and_then(|p| std::fs::read_to_string(p).map_err(|_| String::new())) else { return };
    let Ok(m) = serde_json::from_str::<std::collections::HashMap<String, String>>(&t) else { return };
    for n in OWN_NETS { if let (Ok(c), Some(u)) = (solana::Cluster::parse(n), m.get(n)) { solana::set_own(c, Some(u.clone())) } }
}
/// which networks use the owner's node: host only, never the full URL
#[tauri::command]
fn rpc_own() -> serde_json::Value {
    serde_json::Value::Object(OWN_NETS.iter().filter_map(|n| Some((n.to_string(), solana::own_host(solana::Cluster::parse(n).ok()?)?.into()))).collect())
}
/// set (checked against the network first) or clear (empty url) the owner's node for one network
#[tauri::command]
async fn rpc_set(app: AppHandle, cluster: String, url: Zeroizing<String>) -> Result<serde_json::Value, String> {
    let c = solana::Cluster::parse(&cluster)?;
    if !OWN_NETS.contains(&c.name()) { return Err("для этой сети свой узел не задаётся".into()) }
    let url = url.trim();
    if !url.is_empty() { solana::check_own(c, url).await? }
    let path = own_path(&app)?;
    let mut m: std::collections::HashMap<String, String> = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if url.is_empty() { m.remove(c.name()); } else { m.insert(c.name().into(), url.into()); }
    if let Some(dir) = path.parent() { let _ = std::fs::create_dir_all(dir); }
    vault::durable(&path, &serde_json::to_string(&m).map_err(|e| e.to_string())?).map_err(|_| "не удалось сохранить настройку".to_string())?;
    solana::set_own(c, (!url.is_empty()).then(|| url.to_string()));
    Ok(rpc_own())
}

pub(crate) fn rand_id() -> String { let mut b = [0u8; 8]; getrandom::getrandom(&mut b).expect("OS randomness"); b.iter().map(|x| format!("{x:02x}")).collect() }

// ---------------------------------------------------------------- vault
#[tauri::command]
async fn vault_state(app: AppHandle, state: tauri::State<'_, App>) -> Result<serde_json::Value, String> {
    let path = vault_path(&app)?;
    let accounts = vault::accounts(&path)?;
    // a vault without a password opens by itself (still decrypted once: proves the file is whole)
    if accounts.is_empty() { state.session.lock().unwrap().unlocked = false; } // no vault: nothing is open
    let open = vault::is_open(&path);
    if open && !state.session.lock().unwrap().unlocked {
        let accs = vault::unlock(&path, "")?;
        let mut s = state.session.lock().unwrap(); s.unlocked = true; s.accounts = accs;
    }
    // unlocked: the decrypted, verified list; locked: the header's list, public and unproven (shown dimmed, not for receiving)
    let s = state.session.lock().unwrap();
    let accounts = if s.unlocked { s.accounts.clone() } else { accounts };
    Ok(serde_json::json!({ "accounts": accounts, "unlocked": s.unlocked, "open": open }))
}

/// screen-capture exclusion for the whole window while secrets can be on it. On: the UI shows nothing secret until
/// this returns Ok (readback proves the flag took). Off: also drops an unsaved phrase and an unconfirmed transfer.
#[tauri::command]
fn panel_secret(app: AppHandle, state: tauri::State<App>, on: bool) -> Result<(), String> {
    if !on { let mut s = state.session.lock().unwrap(); s.pending = None; s.send = None; }
    let w = app.get_webview_window("main").ok_or("no main window")?;
    // dev only: shots of the sheet for design review need the window capturable
    #[cfg(debug_assertions)]
    if on && std::env::var("CW_ALLOW_CAPTURE").is_ok() { return Ok(()) }
    // Android: the whole app carries FLAG_SECURE from its first frame (MainActivity.kt), so nothing to switch here
    #[cfg(desktop)]
    w.set_content_protected(on).map_err(|e| e.to_string())?;
    #[cfg(mobile)]
    let _ = w;
    if on { protected(&app)? }
    Ok(())
}
/// the main window is excluded from screen capture right now (readback, not the flag we last set)
fn protected(app: &AppHandle) -> Result<(), String> {
    #[cfg(debug_assertions)]
    if std::env::var("CW_ALLOW_CAPTURE").is_ok() { return Ok(()) }
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{GetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE};
        let w = app.get_webview_window("main").ok_or("no main window")?;
        let hwnd = w.hwnd().map_err(|e| e.to_string())?;
        let mut got = 0u32;
        let ok = unsafe { GetWindowDisplayAffinity(hwnd.0 as _, &mut got) } != 0;
        if !ok || got != WDA_EXCLUDEFROMCAPTURE { return Err("не удалось скрыть окно от снимков экрана".into()) }
    }
    #[cfg(not(windows))]
    let _ = app;
    Ok(())
}

/// set, change or remove (empty `new`) the vault password
#[tauri::command]
async fn vault_password(app: AppHandle, old: Zeroizing<String>, new: Zeroizing<String>) -> Result<(), String> {
    vault::repass(&vault_path(&app)?, &old, &new)
}

#[tauri::command]
fn vault_lock(state: tauri::State<App>) -> Result<(), String> {
    { let mut h = state.agent.lock().unwrap(); if let Some(p) = h.waiting.take() { h.close(&p.id, p.cluster, Err("кошелёк заблокирован".into())) } }
    let mut s = state.session.lock().unwrap();
    s.unlocked = false;
    s.accounts.clear();
    s.pending = None;
    s.send = None;
    s.epoch += 1;
    Ok(())
}

/// async: Argon2 takes a moment and must not freeze the window
#[tauri::command]
async fn vault_unlock(app: AppHandle, state: tauri::State<'_, App>, pass: Zeroizing<String>) -> Result<(), String> {
    let accs = vault::unlock(&vault_path(&app)?, &pass)?;
    { let mut s = state.session.lock().unwrap(); s.unlocked = true; s.accounts = accs.clone(); }
    changed(&app, &accs, true);
    Ok(())
}

/// a new phrase for the user to write down; kept in memory until saved or the sheet closes
#[tauri::command]
fn secure_new_phrase(state: tauri::State<App>) -> Result<String, String> {
    let p = vault::new_phrase();
    let out = p.to_string();
    state.session.lock().unwrap().pending = Some(p);
    Ok(out) // ponytail: the copy in the IPC reply cannot be wiped; it lives only while the capture-protected sheet is open
}

/// create the vault or add a wallet. kind: "new" uses the pending phrase; "phrase" / "key" use `secret`.
/// Empty `pass` on a new vault = no password (owner's risk).
#[tauri::command]
async fn secure_save_wallet(app: AppHandle, state: tauri::State<'_, App>, kind: String, secret: Option<Zeroizing<String>>, name: String, pass: Zeroizing<String>) -> Result<(), String> {
    let path = vault_path(&app)?;
    let pending = if kind == "new" { Some(state.session.lock().unwrap().pending.clone().ok_or("фраза не создана: начните заново")?) } else { None };
    let secret = secret.unwrap_or_default();
    let src = match kind.as_str() {
        "new" => vault::Source::Phrase(pending.as_deref().unwrap()),
        "phrase" => vault::Source::Phrase(&secret),
        "key" => vault::Source::SolanaKey(&secret),
        _ => return Err("unknown kind".into()),
    };
    let name = if name.trim().is_empty() { "Кошелёк".to_string() } else { name };
    let accs = if path.exists() { vault::add_wallet(&path, src, &name, &pass)? } else { vault::create(&path, src, &name, &pass, vault::KDF_DEFAULT)? };
    let mut s = state.session.lock().unwrap();
    s.pending = None; // only after a successful save: a wrong password must not lose the phrase being written down
    s.unlocked = true;
    s.accounts = accs.clone();
    drop(s);
    changed(&app, &accs, true);
    Ok(())
}

// ---------------------------------------------------------------- Ledger (t-018): the device keeps the key, signs on its own screen
/// the first five Solana addresses of the connected Ledger (m/44'/501'/i'), to pick one
#[tauri::command]
async fn ledger_accounts() -> Result<Vec<String>, String> {
    tauri::async_runtime::spawn_blocking(|| (0..5u32).map(|i| ledger::pubkey(i).map(|k| bs58::encode(k).into_string())).collect())
        .await.map_err(|_| "Ledger: внутренняя ошибка".to_string())?
}
/// add (or start the vault with) a Ledger account: the address is read from the device again here, never taken from the page
#[tauri::command]
async fn secure_add_ledger(app: AppHandle, state: tauri::State<'_, App>, index: u32, name: String, pass: Zeroizing<String>) -> Result<(), String> {
    if index > 99 { return Err("такого номера счёта нет".into()) }
    let path = vault_path(&app)?;
    let pk = tauri::async_runtime::spawn_blocking(move || ledger::pubkey(index)).await.map_err(|_| "Ledger: внутренняя ошибка".to_string())??;
    let pk = bs58::encode(pk).into_string();
    let name = if name.trim().is_empty() { "Ledger".to_string() } else { name };
    let src = vault::Source::Ledger { pubkey: &pk, index };
    let accs = if path.exists() { vault::add_wallet(&path, src, &name, &pass)? } else { vault::create(&path, src, &name, &pass, vault::KDF_DEFAULT)? };
    let mut s = state.session.lock().unwrap();
    s.unlocked = true;
    s.accounts = accs.clone();
    drop(s);
    changed(&app, &accs, true);
    Ok(())
}

#[tauri::command]
async fn secure_add_account(app: AppHandle, state: tauri::State<'_, App>, wallet: String, name: String, pass: Zeroizing<String>) -> Result<(), String> {
    let accs = vault::add_account(&vault_path(&app)?, &wallet, if name.trim().is_empty() { "Счёт" } else { &name }, &pass)?;
    state.session.lock().unwrap().accounts = accs.clone();
    changed(&app, &accs, true);
    Ok(())
}

/// a secret from the sheet (words, a key) to the clipboard: only while the window is out of screenshots; kept out of
/// clipboard history and wiped after a while
#[tauri::command]
fn clip_secret(app: AppHandle, text: Zeroizing<String>) -> Result<u64, String> {
    protected(&app)?;
    #[cfg(windows)]
    let hwnd = app.get_webview_window("main").ok_or("no main window")?.hwnd().map_err(|e| e.to_string())?.0 as isize;
    #[cfg(not(windows))]
    let hwnd = 0;
    clip::put(hwnd, &text)?;
    Ok(clip::CLEAR_S)
}

/// an Ethereum key for an account imported from a Solana key (it has no EVM address of its own)
#[tauri::command]
async fn secure_add_evm(app: AppHandle, state: tauri::State<'_, App>, account: String, key: Zeroizing<String>, pass: Zeroizing<String>) -> Result<(), String> {
    protected(&app)?; // the key is typed into the sheet: only while the window is out of screenshots
    let accs = vault::add_evm_key(&vault_path(&app)?, &account, &key, &pass)?;
    state.session.lock().unwrap().accounts = accs.clone();
    changed(&app, &accs, true);
    Ok(())
}

/// export (owner 08.10: «ключик отсюда забрать»): one account's phrase and private keys, password asked again,
/// handed out only while the window is excluded from capture
#[tauri::command]
async fn secure_export(app: AppHandle, account: String, pass: Zeroizing<String>) -> Result<serde_json::Value, String> {
    // elsewhere the capture exclusion cannot be proven (Linux has none): keys are never shown there
    #[cfg(not(any(windows, target_os = "android")))]
    return Err("экспорт ключей пока только в Windows и Android".into());
    protected(&app)?;
    let e = vault::export(&vault_path(&app)?, &pass, &account)?;
    // ponytail: the JSON copy in the IPC reply cannot be wiped, as with secure_new_phrase; it lives while the sheet is open
    Ok(serde_json::json!({ "phrase": e.phrase.as_deref().map(|s| s.as_str()), "solana": e.solana.as_str(), "evm": e.evm.as_deref().map(|s| s.as_str()) }))
}

// ---------------------------------------------------------------- networks
#[tauri::command]
async fn sol_watch(address: String, cluster: String) -> Result<solana::Watch, String> {
    solana::watch(address.trim(), solana::Cluster::parse(&cluster)?).await
}
#[tauri::command]
async fn sol_history(address: String, cluster: String) -> Result<Vec<solana::Sig>, String> {
    solana::history(address.trim(), solana::Cluster::parse(&cluster)?).await
}
/// USD per unit, mainnet assets only; absent = unknown (see prices.rs for what leaves the machine)
#[tauri::command]
async fn usd_prices(mints: Vec<String>, coins: Vec<String>) -> Result<std::collections::HashMap<String, f64>, String> {
    if mints.len() > 500 || coins.len() > 20 { return Err("слишком длинный список".into()) }
    prices::usd(mints, coins).await
}
#[tauri::command]
async fn sol_tx(sig: String, address: String, cluster: String) -> Result<solana::Tx, String> {
    solana::tx(sig.trim(), address.trim(), solana::Cluster::parse(&cluster)?).await
}
/// open an address page of the network's explorer (EVM history lives there for now)
#[tauri::command]
fn open_address(address: String, cluster: String) -> Result<(), String> {
    let a = address.trim();
    let url = match cluster.strip_prefix("evm:") {
        Some(id) if evm::valid_address(a) => format!("{}/address/{a}", evm::explorer(id.parse().map_err(|_| "нет такой сети")?).ok_or("у этой сети нет обозревателя")?),
        Some(_) => return Err("это не адрес сети Ethereum".into()),
        None => { solana::Cluster::parse(&cluster)?; if bs58::decode(a).into_vec().map_or(true, |v| v.len() != 32) { return Err("это не адрес Solana".into()) } format!("https://solscan.io/account/{a}") }
    };
    launch(&url)
}
fn launch(url: &str) -> Result<(), String> {
    // ponytail: stdlib launcher (Windows); the opener plugin when Android/macOS builds arrive
    #[cfg(windows)]
    { std::process::Command::new("rundll32").args(["url.dll,FileProtocolHandler", url]).spawn().map(|_| ()).map_err(|_| "не удалось открыть браузер".to_string()) }
    #[cfg(not(windows))]
    { let _ = url; Err("открытие ссылок пока только в Windows".into()) }
}
/// open a transaction in the system browser; the URL is built here from a signature, never taken from the page
#[tauri::command]
fn open_tx(sig: String, cluster: String) -> Result<(), String> {
    // EVM: cluster "evm:<chain id>", a 0x + 64 hex hash, the explorer from the core's own table
    let url = match cluster.strip_prefix("evm:") {
        Some(id) => {
            let h = sig.trim();
            if h.len() != 66 || !h.starts_with("0x") || !h[2..].chars().all(|c| c.is_ascii_hexdigit()) { return Err("это не номер перевода".into()) }
            format!("{}/tx/{h}", evm::explorer(id.parse().map_err(|_| "нет такой сети")?).ok_or("у этой сети нет обозревателя")?)
        }
        None => solana::explorer(sig.trim(), solana::Cluster::parse(&cluster)?)?,
    };
    launch(&url)
}
#[tauri::command]
async fn evm_watch(rpc: String, chain_id: u64, address: String) -> Result<evm::Watch, String> {
    evm::watch(rpc.trim(), chain_id, address.trim()).await
}
/// the latest operations from the network's explorer (EVM nodes cannot list an address's transactions)
#[tauri::command]
async fn evm_history(chain_id: u64, address: String) -> Result<Vec<evm::HTx>, String> { evm::history(chain_id, address.trim()).await }
/// «Всё» for the native coin: the balance minus the most the fee can be
#[tauri::command]
async fn evm_max(rpc: String, chain_id: u64, address: String, to: Option<String>) -> Result<String, String> {
    evm::max_native(rpc.trim(), chain_id, address.trim(), to.as_deref()).await
}
/// like sol_prepare_send for an EVM network: the core builds, prices and dry-runs; the answer is the confirmation's data
#[tauri::command]
async fn evm_prepare_send(app: AppHandle, state: tauri::State<'_, App>, account: String, to: String, amount: String, chain_id: u64, rpc: String,
    token: Option<String>, name: Option<String>, symbol: Option<String>) -> Result<serde_json::Value, String> {
    // dev builds sign test networks only (no agent running the app from source signs real money; same rule as Solana)
    #[cfg(debug_assertions)]
    if !matches!(chain_id, 11155111 | 84532 | 31337) { return Err("настоящие сети закрыты в сборке для разработки; переводы — только в установленной программе".into()) }
    #[cfg(debug_assertions)]
    if chain_id == 31337 && std::env::var("CW_VAULT").is_err() { return Err("локальная сеть: только с одноразовым хранилищем (CW_VAULT)".into()) }
    #[cfg(not(debug_assertions))]
    if chain_id == 31337 { return Err("локальная тестовая сеть доступна только в сборке для разработки".into()) }
    let path = vault_path(&app)?;
    let (a, epoch) = {
        let s = state.session.lock().unwrap();
        if !s.unlocked { return Err("кошелёк заблокирован".into()) }
        (s.accounts.iter().find(|a| a.id == account).cloned().ok_or("нет такого счёта")?, s.epoch)
    };
    let from = a.evm.clone().ok_or("у этого счёта нет адреса Ethereum")?;
    let d = evm::prepare(evm::Ask { rpc: rpc.trim(), chain_id, from: &from, to: to.trim(), amount: &amount, token: token.as_deref(), name: name.as_deref(), symbol: symbol.as_deref() }).await?;
    let id = rand_id();
    let out = serde_json::json!({
        "id": id, "open": vault::is_open(&path) || a.ledger, "ledger": a.ledger, "account": a.name, "from": d.from, "to": d.to, "network": d.network,
        "test": d.test, "newRecipient": d.new_recipient, "warn": d.warn,
        "amount": evm::units(d.amount, d.decimals as u32), "symbol": d.symbol, "mint": d.token, "native": d.native,
        "fee": evm::units(d.fee_max, 18), "rent": "0", "feeMax": true,
        "solTotal": evm::units(d.fee_max + if d.token.is_none() { d.amount } else { 0 }, 18),
    });
    let mut s = state.session.lock().unwrap();
    if !s.unlocked || s.epoch != epoch { return Err("перевод отменён".into()) }
    s.send = Some(Send { id, account: a.id, draft: Draft::Evm(d) });
    Ok(out)
}

// ---------------------------------------------------------------- send
/// build and price a transfer in the core and keep it; the answer is the confirmation screen's data and the draft id.
/// The UI names account, recipient and amount; it never sees or touches the message that gets signed.
#[tauri::command]
async fn sol_prepare_send(app: AppHandle, state: tauri::State<'_, App>, account: String, to: String, amount: String, cluster: String, mint: Option<String>) -> Result<serde_json::Value, String> {
    let c = solana::Cluster::parse(&cluster)?;
    // mainnet: opened by the owner 08.10 for the installed (release) build. Dev builds keep refusing it, so no agent
    // running the app from source can ever sign real money.
    #[cfg(debug_assertions)]
    if matches!(c, solana::Cluster::Mainnet) { return Err("основная сеть закрыта в сборке для разработки; переводы настоящих денег — только в установленной программе".into()) }
    // a local "validator" is whatever listens on 127.0.0.1:8899: it could hand out a mainnet blockhash and get a signature
    // that is valid on mainnet (security review 07.10). Dev builds with a throwaway vault only.
    #[cfg(not(debug_assertions))]
    if matches!(c, solana::Cluster::Local) { return Err("локальная тестовая сеть доступна только в сборке для разработки".into()) }
    // dev builds: the local network only with a throwaway vault (CW_VAULT), never the real one
    #[cfg(debug_assertions)]
    if matches!(c, solana::Cluster::Local) && std::env::var("CW_VAULT").is_err() { return Err("локальная сеть: только с одноразовым хранилищем (CW_VAULT)".into()) }
    let path = vault_path(&app)?;
    let (a, epoch) = {
        let s = state.session.lock().unwrap();
        if !s.unlocked { return Err("кошелёк заблокирован".into()) }
        (s.accounts.iter().find(|a| a.id == account).cloned().ok_or("нет такого счёта")?, s.epoch)
    };
    let d = match mint {
        None if to == "fee-setup" => solana::prepare_fee_setup(&a.solana, c).await?, // the fee wallet's receiving accounts (Settings)
        None => solana::prepare(&a.solana, to.trim(), solana::parse_amount(&amount, 9)?, c).await?,
        Some(m) => solana::prepare_token(&a.solana, to.trim(), m.trim(), &amount, c).await?,
    };
    let network = match c { solana::Cluster::Mainnet => "Solana Mainnet", solana::Cluster::Devnet => "Solana Devnet", solana::Cluster::Local => "Solana Local" };
    let id = rand_id();
    let out = serde_json::json!({
        "id": id, "open": vault::is_open(&path) || a.ledger, "ledger": a.ledger, "account": a.name, "from": d.from, "to": d.to, "network": network,
        "test": !matches!(c, solana::Cluster::Mainnet), "newRecipient": d.new_recipient, "warn": d.warn,
        "amount": solana::units(d.amount, d.decimals as u32), "symbol": d.symbol, "mint": d.mint,
        "fee": solana::units(d.fee, 9), "rent": solana::units(d.rent, 9),
        // SOL leaving the account: the amount itself for SOL, only fee + new token account for tokens
        "solTotal": solana::units(if d.mint.is_none() { d.amount + d.fee } else { d.fee + d.rent }, 9),
    });
    let mut s = state.session.lock().unwrap();
    if !s.unlocked || s.epoch != epoch { return Err("перевод отменён".into()) }
    s.send = Some(Send { id, account: a.id, draft: Draft::Sol(d) }); // replaces any older draft
    Ok(out)
}

// ---------------------------------------------------------------- permissions (who may spend our tokens) and their revoke
#[tauri::command]
async fn sol_grants(address: String, cluster: String) -> Result<Vec<solana::Grant>, String> { solana::grants(address.trim(), solana::Cluster::parse(&cluster)?).await }
#[tauri::command]
async fn evm_allowances(rpc: String, chain_id: u64, address: String) -> Result<Vec<evm::Allowance>, String> { evm::allowances(rpc.trim(), chain_id, address.trim()).await }
/// a revoke is one more draft for the same confirmation (send_confirm signs it): Solana token account, or EVM token + spender
#[tauri::command]
async fn prepare_revoke(app: AppHandle, state: tauri::State<'_, App>, account: String, cluster: Option<String>, token_account: Option<String>,
    chain_id: Option<u64>, rpc: Option<String>, token: Option<String>, spender: Option<String>) -> Result<serde_json::Value, String> {
    let path = vault_path(&app)?;
    let (a, epoch) = {
        let s = state.session.lock().unwrap();
        if !s.unlocked { return Err("кошелёк заблокирован".into()) }
        (s.accounts.iter().find(|a| a.id == account).cloned().ok_or("нет такого счёта")?, s.epoch)
    };
    let id = rand_id();
    let (draft, out) = match (cluster, chain_id) {
        (Some(cl), None) => {
            let c = solana::Cluster::parse(&cl)?;
            #[cfg(debug_assertions)]
            if matches!(c, solana::Cluster::Mainnet) { return Err("основная сеть закрыта в сборке для разработки".into()) }
            let d = solana::prepare_revoke(&a.solana, token_account.as_deref().ok_or("нет счёта токена")?.trim(), c).await?;
            let out = serde_json::json!({ "id": id, "open": vault::is_open(&path) || a.ledger, "ledger": a.ledger, "account": a.name, "from": d.from, "to": d.to, "network": c.label(),
                "test": !matches!(c, solana::Cluster::Mainnet), "newRecipient": false, "warn": d.warn, "amount": "0", "symbol": d.symbol, "mint": d.mint,
                "fee": solana::units(d.fee, 9), "rent": "0", "solTotal": solana::units(d.fee, 9), "revoke": true });
            (Draft::Sol(d), out)
        }
        (None, Some(cid)) => {
            #[cfg(debug_assertions)]
            if !matches!(cid, 11155111 | 84532 | 31337) { return Err("настоящие сети закрыты в сборке для разработки".into()) }
            let from = a.evm.clone().ok_or("у этого счёта нет адреса Ethereum")?;
            let d = evm::prepare_revoke(rpc.as_deref().ok_or("нет адреса узла")?.trim(), cid, &from, token.as_deref().ok_or("нет токена")?.trim(), spender.as_deref().ok_or("нет адреса")?.trim()).await?;
            let out = serde_json::json!({ "id": id, "open": vault::is_open(&path) || a.ledger, "ledger": a.ledger, "account": a.name, "from": d.from, "to": d.to, "network": d.network,
                "test": d.test, "newRecipient": false, "warn": d.warn, "amount": "0", "symbol": d.symbol, "mint": d.token, "native": d.native,
                "fee": evm::units(d.fee_max, 18), "rent": "0", "feeMax": true, "solTotal": evm::units(d.fee_max, 18), "revoke": true });
            (Draft::Evm(d), out)
        }
        _ => return Err("непонятно, в какой сети отзывать".into()),
    };
    let mut s = state.session.lock().unwrap();
    if !s.unlocked || s.epoch != epoch { return Err("отзыв отменён".into()) }
    s.send = Some(Send { id, account: a.id, draft });
    Ok(out)
}

/// the swap fee wallet: its address and how many of its receiving accounts are still missing (mainnet)
#[tauri::command]
async fn fee_state() -> Result<serde_json::Value, String> {
    let missing = solana::fee_missing(solana::Cluster::Mainnet).await?;
    Ok(serde_json::json!({ "owner": solana::FEE_OWNER, "bps": solana::FEE_BPS, "missing": missing.len() }))
}

/// tokens to swap into: today's most traded (empty query) or a search
#[tauri::command]
async fn sol_tokens(query: String) -> Result<Vec<solana::TokenInfo>, String> { solana::tokens(&query).await }
#[tauri::command]
async fn token_icon(url: String) -> Result<String, String> { solana::token_icon(&url).await }
/// live quote for the swap screen (nothing kept)
#[tauri::command]
async fn sol_swap_quote(input: String, output: String, amount: String, cluster: String) -> Result<solana::Quote, String> {
    solana::swap_quote(input.trim(), output.trim(), &amount, solana::Cluster::parse(&cluster)?).await
}
/// a swap built, checked and dry-run by the core; kept as the one draft the confirmation may sign (like a send)
#[tauri::command]
async fn sol_prepare_swap(app: AppHandle, state: tauri::State<'_, App>, account: String, input: String, output: String, amount: String, cluster: String) -> Result<serde_json::Value, String> {
    let c = solana::Cluster::parse(&cluster)?;
    #[cfg(debug_assertions)]
    if matches!(c, solana::Cluster::Mainnet) { return Err("основная сеть закрыта в сборке для разработки; обмен — только в установленной программе".into()) }
    let path = vault_path(&app)?;
    let (a, epoch) = {
        let s = state.session.lock().unwrap();
        if !s.unlocked { return Err("кошелёк заблокирован".into()) }
        (s.accounts.iter().find(|a| a.id == account).cloned().ok_or("нет такого счёта")?, s.epoch)
    };
    let d = solana::prepare_swap(&a.solana, input.trim(), output.trim(), &amount, c).await?;
    let sw = d.swap.clone().ok_or("bad swap")?;
    let id = rand_id();
    let out = serde_json::json!({
        "id": id, "open": vault::is_open(&path) || a.ledger, "ledger": a.ledger, "account": a.name, "from": d.from, "to": d.to, "network": "Solana Mainnet",
        "test": false, "newRecipient": false, "warn": d.warn,
        "amount": solana::units(d.amount, d.decimals as u32), "symbol": d.symbol, "mint": d.mint,
        "fee": solana::units(d.fee, 9), "rent": "0", "solTotal": solana::units(if d.mint.is_none() { d.amount + d.fee } else { d.fee }, 9),
        "swap": { "outMint": sw.out_mint, "out": solana::units(sw.out_amount, sw.out_decimals as u32), "min": solana::units(sw.min_out, sw.out_decimals as u32), "route": sw.route, "feeBps": sw.fee_bps },
    });
    let mut s = state.session.lock().unwrap();
    if !s.unlocked || s.epoch != epoch { return Err("обмен отменён".into()) }
    s.send = Some(Send { id, account: a.id, draft: Draft::Sol(d) });
    Ok(out)
}

/// the confirmation is being closed: drop the draft now, before any animation (no confirm can win the race after this)
#[tauri::command]
fn send_cancel(state: tauri::State<App>) { let mut s = state.session.lock().unwrap(); s.send = None; s.epoch += 1; }

/// sends whose outcome is still unknown (after a crash or a lost reply): shown so the owner never pays twice
#[tauri::command]
fn pending_list(app: AppHandle) -> Result<Vec<Pending>, String> { pending_read(&app) }

/// password -> sign the exact drafted message -> send. One-shot and bound to the draft id the user looked at.
#[tauri::command]
async fn send_confirm(app: AppHandle, state: tauri::State<'_, App>, id: String, pass: Zeroizing<String>) -> Result<String, String> {
    let gone = || "перевод уже отправлен или отменён".to_string();
    let (account, message, evm_hash) = {
        let s = state.session.lock().unwrap();
        if !s.unlocked { return Err("кошелёк заблокирован".into()) }
        let p = s.send.as_ref().filter(|p| p.id == id).ok_or_else(gone)?;
        match &p.draft { Draft::Sol(d) => (p.account.clone(), d.message.clone(), None), Draft::Evm(d) => (p.account.clone(), vec![], Some(d.hash())) }
    };
    if let Some(hash) = evm_hash { return evm_confirm(app, state, id, pass, account, hash).await }
    let on_ledger = state.session.lock().unwrap().accounts.iter().find(|a| a.id == account).filter(|a| a.ledger).map(|a| a.index);
    let sig = match on_ledger {
        // the device shows the transfer and the owner approves it there; the signature is checked against the address below
        Some(index) => { let m = message.clone(); tauri::async_runtime::spawn_blocking(move || ledger::sign(index, &m)).await.map_err(|_| "Ledger: внутренняя ошибка".to_string())?? }
        None => vault::sign_solana(&vault_path(&app)?, &pass, &account, &message)?, // wrong password keeps the draft
    };
    let d = {
        let mut s = state.session.lock().unwrap();
        match s.send.take() { Some(Send { id: i, draft: Draft::Sol(d), .. }) if i == id => d, other => { s.send = other; return Err(gone()) } }
    };
    // the key must be the fee payer the user saw: verify before anything leaves the machine
    let pk: [u8; 32] = bs58::decode(&d.from).into_vec().ok().and_then(|v| v.try_into().ok()).ok_or("bad sender")?;
    ed25519_dalek::VerifyingKey::from_bytes(&pk).and_then(|k| k.verify_strict(&message, &ed25519_dalek::Signature::from_bytes(&sig)))
        .map_err(|_| "подпись не сходится с адресом счёта".to_string())?;
    let sig_id = solana::tx_id(&sig);
    let pending = Pending { sig: sig_id.clone(), cluster: d.cluster.name().into(), last_valid: d.last_valid, chain: 0, rpc: String::new(), from: String::new(), nonce: 0 };
    // on disk before a single byte goes out; no journal, no broadcast (second security review 07.10)
    pending_edit(&app, |l| l.push(pending.clone())).map_err(|e| format!("перевод не отправлен: {e}"))?;
    let first = match solana::submit(&d, &sig).await {
        solana::Sent::Ok => "sent",
        solana::Sent::Unknown => "sent-unknown", // the answer was lost: it may be on its way, never re-send blindly
        solana::Sent::Rejected(e) => { let _ = pending_edit(&app, |l| l.retain(|x| x.sig != sig_id)); return Err(format!("сеть не приняла перевод: {e}")) }
    };
    let _ = app.emit_to("main", "send-status", serde_json::json!({ "sig": sig_id, "state": first }));
    follow(app.clone(), pending);
    Ok(sig_id)
}

/// the EVM half of send_confirm: sign the drafted hash, recover the signer, journal, broadcast, follow
async fn evm_confirm(app: AppHandle, state: tauri::State<'_, App>, id: String, pass: Zeroizing<String>, account: String, hash: [u8; 32]) -> Result<String, String> {
    let gone = || "перевод уже отправлен или отменён".to_string();
    let (sig, parity) = vault::sign_evm(&vault_path(&app)?, &pass, &account, &hash)?; // wrong password keeps the draft
    let d = {
        let mut s = state.session.lock().unwrap();
        match s.send.take() { Some(Send { id: i, draft: Draft::Evm(d), .. }) if i == id && d.hash() == hash => d, other => { s.send = other; return Err(gone()) } }
    };
    // the key must be the sender the user saw: recover it from the signature before anything leaves the machine
    let rs = k256::ecdsa::Signature::from_slice(&sig).map_err(|_| "подпись повреждена".to_string())?;
    let rec = k256::ecdsa::RecoveryId::from_byte(parity).ok_or("подпись повреждена")?;
    let vk = k256::ecdsa::VerifyingKey::recover_from_prehash(&hash, &rs, rec).map_err(|_| "подпись повреждена".to_string())?;
    if vault::evm_address_of(&vk) != d.from { return Err("подпись не сходится с адресом счёта".into()) }
    let raw = d.signed(&sig, parity);
    let tx = evm::tx_hash(&raw);
    let pending = Pending { sig: tx.clone(), cluster: "evm".into(), last_valid: 0, chain: d.chain_id, rpc: d.rpc.clone(), from: d.from.clone(), nonce: d.nonce };
    pending_edit(&app, |l| l.push(pending.clone())).map_err(|e| format!("перевод не отправлен: {e}"))?;
    let first = match evm::submit(&d.rpc, &raw).await {
        evm::Sent::Ok => "sent",
        evm::Sent::Unknown => "sent-unknown",
        evm::Sent::Rejected(e) => { let _ = pending_edit(&app, |l| l.retain(|x| x.sig != tx)); return Err(format!("сеть не приняла перевод: {e}")) }
    };
    let _ = app.emit_to("main", "send-status", serde_json::json!({ "sig": tx, "state": first }));
    follow(app.clone(), pending);
    Ok(tx)
}

// ---------------------------------------------------------------- agent access and batch requests (agent.rs)
/// switch the agent pipe on or off; the choice survives restarts (a flag file next to the vault)
fn agent_switch(app: &AppHandle, on: bool) -> Result<(), String> {
    let v = vault_path(app)?;
    let (flag, file) = (v.with_file_name("agent-on"), v.with_file_name("agent.json"));
    let st = app.state::<App>();
    let mut h = st.agent.lock().unwrap();
    #[cfg(windows)]
    if let Some(t) = h.server.take() { t.abort() }
    let _ = std::fs::remove_file(&file); // a stale discovery file must never point at an old pipe
    h.on = on;
    if on {
        if let Some(dir) = flag.parent() { let _ = std::fs::create_dir_all(dir); }
        std::fs::write(&flag, b"").map_err(|_| "не удалось сохранить настройку".to_string())?;
        #[cfg(windows)]
        { h.server = Some(agent::start(app.clone(), file)); }
    } else {
        let _ = std::fs::remove_file(&flag);
        if let Some(p) = h.waiting.take() { h.close(&p.id, p.cluster, Err("доступ агента выключен".into())) }
    }
    Ok(())
}
#[tauri::command]
fn agent_state(app: AppHandle, state: tauri::State<App>) -> Result<serde_json::Value, String> {
    let open = vault::is_open(&vault_path(&app)?);
    let mut h = state.agent.lock().unwrap();
    let on = h.on;
    Ok(serde_json::json!({ "on": on, "waiting": h.live().map(|p| agent::view(p, open)) }))
}
#[tauri::command]
fn agent_access(app: AppHandle, on: bool) -> Result<bool, String> { agent_switch(&app, on)?; Ok(on) }
#[tauri::command]
fn batch_reject(state: tauri::State<App>, id: String) {
    let mut h = state.agent.lock().unwrap();
    if h.waiting.as_ref().is_some_and(|p| p.id == id) { let p = h.waiting.take().unwrap(); h.close(&id, p.cluster, Err("владелец отклонил запрос".into())) }
}
/// the owner confirmed a plan: rebuild each transfer at the plan's price, sign all with one decryption, journal all, send.
/// A row whose numbers moved since the plan (balance, fee) is not sent: the rest goes, the remainder needs a new request.
#[tauri::command]
async fn batch_confirm(app: AppHandle, state: tauri::State<'_, App>, id: String, pass: Zeroizing<String>) -> Result<serde_json::Value, String> {
    let gone = || "запрос уже выполнен, отклонён или устарел".to_string();
    if !state.session.lock().unwrap().unlocked { return Err("кошелёк заблокирован".into()) }
    let plan = { let mut h = state.agent.lock().unwrap(); h.live().filter(|p| p.id == id).cloned().ok_or_else(gone)? };
    let (signed, mut done) = agent::sign_plan(&plan, &vault_path(&app)?, &pass).await?; // wrong password: the plan stays
    // one-shot: only the caller that takes the plan out of the hub may send
    { let mut h = state.agent.lock().unwrap(); if !h.waiting.as_ref().is_some_and(|p| p.id == id) { return Err(gone()) } h.waiting = None; }
    let pend: Vec<Pending> = signed.iter().map(|(_, d, sig)| Pending { sig: solana::tx_id(sig), cluster: d.cluster.name().into(), last_valid: d.last_valid, chain: 0, rpc: String::new(), from: String::new(), nonce: 0 }).collect();
    // all on disk before a single byte goes out
    pending_edit(&app, |l| l.extend(pend.iter().cloned())).map_err(|e| format!("переводы не отправлены: {e}"))?;
    let mut sent = 0;
    for ((r, d, sig), p) in signed.iter().zip(pend) {
        match solana::submit(d, sig).await {
            solana::Sent::Rejected(e) => {
                let _ = pending_edit(&app, |l| l.retain(|x| x.sig != p.sig));
                done.push(agent::Done { account: r.account.clone(), sig: None, error: Some(format!("сеть не приняла: {e}")) });
            }
            _ => { sent += 1; done.push(agent::Done { account: r.account.clone(), sig: Some(p.sig.clone()), error: None }); follow(app.clone(), p) }
        }
    }
    let total = plan.rows.len();
    state.agent.lock().unwrap().close(&id, plan.cluster, Ok(done));
    Ok(serde_json::json!({ "sent": sent, "total": total }))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let b = tauri::Builder::default();
    #[cfg(desktop)]
    let b = b.plugin(tauri_plugin_single_instance::init(|app, _, _| {
        if let Some(w) = app.get_webview_window("main") { let _ = w.unminimize(); let _ = w.set_focus(); }
    }));
    b
        .manage(App { session: Mutex::new(Session::default()), agent: Mutex::new(agent::Hub::default()) })
        .setup(|app| {
            // sends from a previous run whose outcome was never seen: follow them again
            let h = app.handle().clone();
            own_load(&h); // before any read: pending sends below already talk to the network
            if let Ok(list) = pending_read(&h) { for p in list { follow(h.clone(), p) } } // read only; errors surface via pending_list
            if vault_path(&h).is_ok_and(|v| v.with_file_name("agent-on").exists()) { let _ = agent_switch(&h, true); }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            vault_state, vault_lock, vault_unlock, vault_password, panel_secret,
            secure_new_phrase, secure_save_wallet, secure_add_account, secure_add_evm, secure_export, clip_secret, ledger_accounts, secure_add_ledger,
            agent_state, agent_access, batch_reject, batch_confirm, fee_state,
            sol_watch, sol_history, sol_tx, open_tx, open_address, usd_prices, sol_swap_quote, sol_prepare_swap, sol_tokens, token_icon, evm_watch, evm_history, evm_max, evm_prepare_send, sol_grants, evm_allowances, prepare_revoke, sol_prepare_send, send_confirm, send_cancel, pending_list, rpc_own, rpc_set,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Talon");
}

