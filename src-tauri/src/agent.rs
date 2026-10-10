// Agent access (owner 09.10, designed with Codex — PLAN.md «Agent access + batch requests»). An MCP adapter
// (agent/talon-mcp.mjs) talks to the core over a Windows named pipe. Agents read public data and file transfer
// REQUESTS; nothing here signs. The owner confirms each plan in the wallet window with the password (lib.rs batch_confirm).
// A same-user process can reach the pipe: the secret proves it read the owner's discovery file, not that it is "Claude".

use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

use crate::solana::{self, Cluster};

pub const MAX_SOURCES: usize = 50;
pub const PER_MIN: usize = 5; // new requests per minute
pub const TTL: Duration = Duration::from_secs(600); // a plan not confirmed in 10 minutes is gone

/// one source of a batch: an own account, the exact lamports it sends and the fee it pays at the plan's price
#[derive(Clone)]
pub struct Row { pub account: String, pub name: String, pub from: String, pub balance: u64, pub amount: u64, pub fee: u64 }
/// an immutable plan: the password confirms exactly this (destination, rows, price); any change = a new request
#[derive(Clone)]
pub struct Plan { pub id: String, pub cluster: Cluster, pub to: String, pub new_recipient: bool, pub price: u64, pub rows: Vec<Row>, pub at: Instant, pub owner: bool, pub addon: Option<String> } // owner: made by hand (a folder); addon: proposed by that add-on
/// what became of one row after the owner confirmed
#[derive(Clone)]
pub struct Done { pub account: String, pub sig: Option<String>, pub error: Option<String> }

#[derive(Default)]
pub struct Hub {
    pub on: bool,
    pub waiting: Option<Plan>,
    pub closed: HashMap<String, (Cluster, Result<Vec<Done>, String>)>, // id -> results, or why it never ran (rejected, expired, locked)
    recent: VecDeque<Instant>,
    #[cfg(windows)]
    pub server: Option<tauri::async_runtime::JoinHandle<()>>,
}

impl Hub {
    /// the waiting plan, dropped (and recorded) once it is past its time
    pub fn live(&mut self) -> Option<&Plan> {
        if self.waiting.as_ref().is_some_and(|p| p.at.elapsed() > TTL) { let p = self.waiting.take().unwrap(); self.close(&p.id, p.cluster, Err("время на подтверждение вышло".into())) }
        self.waiting.as_ref()
    }
    pub fn close(&mut self, id: &str, c: Cluster, r: Result<Vec<Done>, String>) {
        if self.closed.len() > 50 { self.closed.clear() } // ponytail: forget old outcomes wholesale; agents poll within minutes
        self.closed.insert(id.into(), (c, r));
    }
    /// sliding one-minute window
    pub fn admit(&mut self, now: Instant) -> Result<(), String> {
        while self.recent.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60)) { self.recent.pop_front(); }
        if self.recent.len() >= PER_MIN { return Err(format!("не больше {PER_MIN} запросов в минуту")) }
        self.recent.push_back(now);
        Ok(())
    }
}

#[derive(Deserialize, Clone)]
pub struct Source { pub account: String, pub amount: String } // "1.5" SOL or "max"

/// sources -> plan: own accounts only, no duplicates, no self-transfer, each row priced and dry-run by the core
pub async fn plan(accounts: &[crate::vault::Account], c: Cluster, to: &str, sources: &[Source], id: String, owner: bool, addon: Option<String>) -> Result<Plan, String> {
    if sources.is_empty() { return Err("нет ни одного счёта-источника".into()) }
    if sources.len() > MAX_SOURCES { return Err(format!("не больше {MAX_SOURCES} счетов в одном запросе")) }
    if !solana::valid_address(to) { return Err("получатель: это не адрес Solana".into()) }
    let mut seen = std::collections::HashSet::new();
    let price = solana::sol_price(c).await?;
    let fee = solana::sol_transfer_fee(price);
    let mut rows = Vec::with_capacity(sources.len());
    let mut new_recipient = false;
    for s in sources {
        let a = accounts.iter().find(|a| a.id == s.account || a.solana == s.account).ok_or_else(|| format!("счёт {} не из этого кошелька", s.account))?;
        if !seen.insert(a.id.clone()) { return Err(format!("счёт «{}» указан дважды", a.name)) }
        if a.ledger { return Err(format!("счёт «{}» подписывает Ledger: в пакет он не входит", a.name)) }
        if a.solana == to { return Err(format!("счёт «{}» и есть получатель", a.name)) }
        let bal = solana::balance(&a.solana, c).await?;
        let amount = if s.amount.trim().eq_ignore_ascii_case("max") {
            bal.checked_sub(fee).filter(|v| *v > 0).ok_or_else(|| format!("на счёте «{}» не хватает даже на комиссию", a.name))?
        } else { solana::parse_amount(&s.amount, 9)? };
        // the core's own checks: recipient kind, rent rules, balance, dry run; at the plan's fixed price
        let d = solana::prepare_priced(&a.solana, to, amount, c, Some(price)).await.map_err(|e| format!("«{}»: {e}", a.name))?;
        if d.fee != fee { return Err("сеть посчитала другую комиссию: попробуйте ещё раз".into()) }
        new_recipient |= d.new_recipient;
        rows.push(Row { account: a.id.clone(), name: a.name.clone(), from: a.solana.clone(), balance: bal, amount, fee });
    }
    Ok(Plan { id, cluster: c, to: to.into(), new_recipient, price, rows, at: Instant::now(), owner, addon })
}

/// the owner confirmed: each row rebuilt now (fresh blockhash) at the plan's amounts and price, signed with one
/// decryption, each signature checked against its sender. A row whose numbers moved is left out with the reason.
pub async fn sign_plan(plan: &Plan, vault: &std::path::Path, pass: &str) -> Result<(Vec<(Row, solana::Draft, [u8; 64])>, Vec<Done>), String> {
    let mut built = Vec::new();
    let mut skipped = Vec::new();
    for r in &plan.rows {
        match solana::prepare_priced(&r.from, &plan.to, r.amount, plan.cluster, Some(plan.price)).await {
            Ok(d) if d.fee == r.fee => built.push((r.clone(), d)),
            Ok(_) => skipped.push(Done { account: r.account.clone(), sig: None, error: Some("комиссия изменилась".into()) }),
            Err(e) => skipped.push(Done { account: r.account.clone(), sig: None, error: Some(e) }),
        }
    }
    let items: Vec<(&str, &[u8])> = built.iter().map(|(r, d)| (r.account.as_str(), d.message.as_slice())).collect();
    let sigs = crate::vault::sign_many(vault, pass, &items)?;
    let mut out = Vec::new();
    for ((r, d), sig) in built.into_iter().zip(sigs) {
        let pk: [u8; 32] = bs58::decode(&d.from).into_vec().ok().and_then(|v| v.try_into().ok()).ok_or("bad sender")?;
        ed25519_dalek::VerifyingKey::from_bytes(&pk).and_then(|k| k.verify_strict(&d.message, &ed25519_dalek::Signature::from_bytes(&sig)))
            .map_err(|_| format!("подпись не сходится с адресом счёта «{}»", r.name))?;
        out.push((r, d, sig));
    }
    Ok((out, skipped))
}

/// what the confirmation shows: plain numbers and addresses, no text from the agent (prompt injection has no voice here)
pub fn view(p: &Plan, open: bool) -> Value {
    let u = |v: u64| solana::units(v, 9);
    let total: u64 = p.rows.iter().map(|r| r.amount).sum();
    let fees: u64 = p.rows.iter().map(|r| r.fee).sum();
    json!({
        "id": p.id, "by": if p.owner { "owner" } else if p.addon.is_some() { "addon" } else { "agent" }, "byName": p.addon, "network": p.cluster.label(), "test": !matches!(p.cluster, Cluster::Mainnet), "to": p.to, "newRecipient": p.new_recipient,
        "open": open, "count": p.rows.len(), "total": u(total), "fees": u(fees), "expires": (TTL.saturating_sub(p.at.elapsed())).as_secs(),
        "rows": p.rows.iter().map(|r| json!({ "name": r.name, "from": r.from, "balance": u(r.balance), "amount": u(r.amount), "fee": u(r.fee), "left": u(r.balance - r.amount - r.fee) })).collect::<Vec<_>>(),
    })
}

/// the agent's view of a request: waiting, closed with a reason, or each row with its live network state
pub async fn status(hub: &std::sync::Mutex<Hub>, id: &str) -> Value {
    let (waiting, closed) = {
        let mut h = hub.lock().unwrap();
        let w = h.live().filter(|p| p.id == id).map(|p| (p.rows.len(), p.at.elapsed().as_secs()));
        (w, h.closed.get(id).cloned())
    };
    if let Some((n, age)) = waiting { return json!({ "id": id, "state": "waiting", "transfers": n, "secondsLeft": TTL.as_secs().saturating_sub(age) }) }
    match closed {
        None => json!({ "id": id, "state": "unknown" }),
        Some((_, Err(why))) => json!({ "id": id, "state": "closed", "reason": why }),
        Some((c, Ok(rows))) => {
            let mut out = Vec::new();
            for r in rows {
                let st = match &r.sig { Some(s) => solana::sig_state(s, c).await, None => "not-sent" };
                out.push(json!({ "account": r.account, "sig": r.sig, "state": st, "error": r.error }));
            }
            json!({ "id": id, "state": "confirmed", "transfers": out })
        }
    }
}

// ---------------------------------------------------------------- the pipe (Windows)
// Discovery file next to the vault: pipe name + a fresh secret per launch (owner-only by the AppData ACL). The pipe takes
// local clients only; Windows' default pipe DACL gives other users read access at most, and every call needs the secret.

/// open the pipe and serve until aborted
#[cfg(windows)]
pub fn start(app: tauri::AppHandle, file: std::path::PathBuf) -> tauri::async_runtime::JoinHandle<()> {
    tauri::async_runtime::spawn(async move {
        use tokio::net::windows::named_pipe::ServerOptions;
        let name = format!(r"\\.\pipe\talon-{}", crate::rand_id());
        let secret = (0..4).map(|_| crate::rand_id()).collect::<String>(); // 32 random bytes
        let mut server = match ServerOptions::new().first_pipe_instance(true).reject_remote_clients(true).create(&name) { Ok(s) => s, Err(e) => { eprintln!("agent pipe: {e}"); return } };
        if let Err(e) = crate::vault::durable(&file, &json!({ "pipe": name, "secret": secret, "pid": std::process::id() }).to_string()) { eprintln!("agent file: {e}"); return }
        loop {
            if server.connect().await.is_err() { continue }
            let Ok(next) = ServerOptions::new().reject_remote_clients(true).create(&name) else { return };
            let conn = std::mem::replace(&mut server, next);
            let (app, secret) = (app.clone(), secret.clone());
            tauri::async_runtime::spawn(async move { serve(app, conn, secret).await });
        }
    })
}

#[cfg(windows)]
async fn serve(app: tauri::AppHandle, conn: tokio::net::windows::named_pipe::NamedPipeServer, secret: String) {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let (r, mut w) = tokio::io::split(conn);
    let mut lines = BufReader::new(r).lines(); // ponytail: line length unbounded; the caller already proved the secret file
    while let Ok(Some(line)) = lines.next_line().await {
        let v: Value = serde_json::from_str(&line).unwrap_or(Value::Null);
        let given = v["secret"].as_str().unwrap_or("");
        let ok = given.len() == secret.len() && given.bytes().zip(secret.bytes()).fold(0u8, |a, (x, y)| a | (x ^ y)) == 0;
        let out = if ok { call(&app, v["method"].as_str().unwrap_or(""), &v["params"]).await } else { Err("неверный ключ доступа".into()) };
        let resp = match out { Ok(r) => json!({ "id": v["id"], "result": r }), Err(e) => json!({ "id": v["id"], "error": e }) };
        if w.write_all(format!("{resp}\n").as_bytes()).await.is_err() { break }
    }
}

/// the methods; anything that signs or exports is simply not here
pub async fn call(app: &tauri::AppHandle, method: &str, p: &Value) -> Result<Value, String> {
    use tauri::{Emitter, Manager};
    let st = app.state::<crate::App>();
    if !st.agent.lock().unwrap().on { return Err("доступ агента выключен в настройках кошелька".into()) }
    // the one call that works while locked: an agent learns why it cannot read yet, and whether a request still waits
    if method == "status" {
        let unlocked = st.session.lock().unwrap().unlocked;
        let mut h = st.agent.lock().unwrap();
        let w = h.live().map(|p| json!({ "id": p.id, "by": if p.owner { "owner" } else if p.addon.is_some() { "addon" } else { "agent" }, "transfers": p.rows.len() }));
        return Ok(json!({ "unlocked": unlocked, "waiting": w, "version": env!("CARGO_PKG_VERSION"),
            "can": ["read accounts, folders, balances (Solana and EVM), history", "request SOL transfers from own accounts to one address (the owner confirms)"],
            "cannot": ["sign", "export keys or words", "send tokens or EVM coins yet", "change settings"] }))
    }
    let accounts = {
        let s = st.session.lock().unwrap();
        if !s.unlocked { return Err("кошелёк заблокирован: попросите владельца открыть его".into()) }
        s.accounts.clone() // proven at unlock, never the file's unverified header
    };
    let net = || Cluster::parse(p["network"].as_str().unwrap_or("mainnet"));
    match method {
        "accounts" => Ok(json!(accounts.iter().map(|a| json!({ "id": a.id, "name": a.name, "solana": a.solana, "evm": a.evm })).collect::<Vec<_>>())),
        "balances" => {
            let c = net()?;
            let want = p["account"].as_str();
            let mut out = Vec::new();
            for a in accounts.iter().filter(|a| want.is_none_or(|w| w == a.id || w == a.solana)) {
                let w = solana::watch(&a.solana, c).await?;
                out.push(json!({ "account": a.id, "name": a.name, "address": a.solana, "watch": w }));
            }
            Ok(json!({ "network": c.name(), "accounts": out, "note": "lamports and token amounts are integers in base units" }))
        }
        "request_transfer" => {
            let c = net()?;
            #[cfg(debug_assertions)]
            if matches!(c, Cluster::Mainnet) { return Err("основная сеть закрыта в сборке для разработки".into()) }
            #[cfg(debug_assertions)]
            if matches!(c, Cluster::Local) && std::env::var("CW_VAULT").is_err() { return Err("локальная сеть: только с одноразовым хранилищем".into()) }
            #[cfg(not(debug_assertions))]
            if matches!(c, Cluster::Local) { return Err("локальная тестовая сеть только в сборке для разработки".into()) }
            let to = p["to"].as_str().ok_or("нужен адрес получателя: to")?.trim().to_string();
            let sources: Vec<Source> = serde_json::from_value(p["sources"].clone()).map_err(|_| "sources: список { account, amount }".to_string())?;
            {
                let mut h = st.agent.lock().unwrap();
                if h.live().is_some() { return Err("уже есть запрос, который ждёт подтверждения владельца".into()) }
                h.admit(Instant::now())?;
            }
            let plan = plan(&accounts, c, &to, &sources, crate::rand_id(), false, None).await?;
            let open = crate::vault::is_open(&crate::vault_path(app)?);
            let v = view(&plan, open);
            {
                let mut h = st.agent.lock().unwrap();
                if h.live().is_some() { return Err("уже есть запрос, который ждёт подтверждения владельца".into()) }
                h.waiting = Some(plan);
            }
            let _ = app.emit_to("main", "agent-request", v.clone());
            #[cfg(desktop)]
            if let Some(w) = app.get_webview_window("main") { let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational)); }
            Ok(json!({ "id": v["id"], "state": "waiting", "plan": v, "next": "the owner confirms in the wallet window; poll request_status" }))
        }
        "request_status" => Ok(status(&st.agent, p["id"].as_str().unwrap_or("")).await),
        // the owner's folders: named groups of account ids (own and watch-only); sources for a request come from here
        "folders" => Ok(json!(crate::folders::read(&crate::vault_path(app)?.with_file_name("folders.json"))?)),
        "networks" => Ok(json!({ "solana": if cfg!(debug_assertions) { json!(["mainnet", "devnet", "local"]) } else { json!(["mainnet", "devnet"]) }, "evm": crate::evm::networks() })),
        "evm_balances" => {
            let q = p["network"].as_str().ok_or("network: chain id or name of an EVM network (see networks)")?;
            let (id, url) = crate::evm::find(q).ok_or("такой EVM-сети кошелёк не знает (см. networks)")?;
            let want = p["account"].as_str();
            let mut out = Vec::new();
            for a in accounts.iter().filter(|a| a.evm.is_some() && want.is_none_or(|w| w == a.id || Some(w) == a.evm.as_deref())) {
                let addr = a.evm.clone().unwrap_or_default();
                out.push(match crate::evm::watch(url, id, &addr).await {
                    Ok(w) => json!({ "account": a.id, "name": a.name, "address": addr, "watch": w }),
                    Err(e) => json!({ "account": a.id, "name": a.name, "address": addr, "error": e }), // one node failing never hides the rest
                });
            }
            Ok(json!({ "chainId": id, "accounts": out, "note": "wei and token amounts are integers in base units; watch.native names the coin" }))
        }
        "history" => {
            let who = p["account"].as_str().ok_or("account: id or address of an own account")?;
            let a = accounts.iter().find(|a| a.id == who || a.solana == who || a.evm.as_deref() == Some(who)).ok_or("такого счёта в кошельке нет")?;
            let n = p["limit"].as_u64().unwrap_or(10).clamp(1, 20) as usize;
            let net = p["network"].as_str().unwrap_or("mainnet");
            if let Ok(c) = Cluster::parse(net) {
                let mut out = Vec::new();
                for s in solana::history(&a.solana, c).await?.into_iter().take(n) {
                    out.push(match solana::tx(&s.sig, &a.solana, c).await { Ok(t) => json!(t), Err(_) => json!(s) }); // details when the node gives them
                }
                return Ok(json!({ "network": c.name(), "address": a.solana, "transactions": out, "note": "amounts in base units; incoming true = received" }))
            }
            let (id, _) = crate::evm::find(net).ok_or("network: mainnet, devnet or an EVM network (see networks)")?;
            let addr = a.evm.clone().ok_or("у этого счёта нет EVM-адреса")?;
            let mut list = crate::evm::history(id, &addr).await?;
            list.truncate(n);
            Ok(json!({ "chainId": id, "address": addr, "transactions": list }))
        }
        _ => Err(format!("нет такого метода: {method}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limits() {
        let mut h = Hub::default();
        let t = Instant::now();
        for _ in 0..PER_MIN { h.admit(t).unwrap() }
        assert!(h.admit(t).is_err(), "sixth request in a minute refused");
        assert!(h.admit(t + Duration::from_secs(61)).is_ok(), "window slides");
        h.waiting = Some(Plan { id: "a".into(), cluster: Cluster::Local, to: String::new(), new_recipient: false, price: 0, rows: vec![], at: t - TTL - Duration::from_secs(1), owner: false, addon: None });
        assert!(h.live().is_none(), "an old plan expires");
        assert!(matches!(h.closed.get("a"), Some((_, Err(_)))));
        assert_eq!(solana::sol_transfer_fee(1_000), 5001);
        assert_eq!(solana::sol_transfer_fee(1_000_000), 6000);
    }
    // a batch end to end on a local validator: plan (own accounts, "max", fixed price), sign_plan with one decryption,
    // submit, finalize; the destination gets exactly the planned sum and "max" sources end at zero.
    // `CW_TEST_LOCAL=1 cargo test batch_live -- --ignored --nocapture` with solana-test-validator running
    #[test]
    #[ignore]
    fn batch_live() {
        use crate::vault;
        tauri::async_runtime::block_on(async {
            assert!(std::env::var("CW_TEST_LOCAL").is_ok(), "local validator only");
            let c = Cluster::Local;
            let dir = std::env::temp_dir().join(format!("cw-batch-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let p = dir.join("vault.json");
            let pass = "batch throwaway pass";
            let k = vault::Kdf { m: 64, t: 1, p: 1 };
            let accs = vault::create(&p, vault::Source::Phrase(&vault::new_phrase()), "a", pass, k).unwrap();
            let w = accs[0].wallet.clone();
            vault::add_account(&p, &w, "b", pass).unwrap();
            let accs = vault::add_account(&p, &w, "c", pass).unwrap();
            let http = reqwest::Client::new();
            let rpc = |m: &'static str, params: Value| { let http = http.clone(); async move {
                http.post("http://127.0.0.1:8899").json(&json!({ "jsonrpc": "2.0", "id": 1, "method": m, "params": params })).send().await.unwrap().json::<Value>().await.unwrap()
            } };
            for a in &accs { rpc("requestAirdrop", json!([a.solana, 2_000_000_000u64])).await; }
            for a in &accs { while solana::balance(&a.solana, c).await.unwrap() == 0 { tokio::time::sleep(Duration::from_millis(500)).await } }
            let to = vault::sol_address(&vault::solana_key(&vault::new_phrase(), 0));
            let src = |a: &str, v: &str| Source { account: a.into(), amount: v.into() };
            // refusals: foreign account, duplicate, self as recipient, too many
            assert!(plan(&accs, c, &to, &[src("nope", "1")], "x".into(), false, None).await.is_err());
            assert!(plan(&accs, c, &to, &[src(&accs[0].id, "1"), src(&accs[0].id, "1")], "x".into(), false, None).await.is_err());
            assert!(plan(&accs, c, &accs[1].solana, &[src(&accs[1].id, "1")], "x".into(), false, None).await.is_err());
            assert!(plan(&accs, c, &to, &vec![src(&accs[0].id, "1"); MAX_SOURCES + 1], "x".into(), false, None).await.is_err());
            let pl = plan(&accs, c, &to, &[src(&accs[0].id, "0.5"), src(&accs[1].id, "max"), src(&accs[2].solana, "MAX")], "b1".into(), false, None).await.unwrap();
            let v = view(&pl, false);
            assert_eq!((v["count"].as_u64(), v["newRecipient"].as_bool()), (Some(3), Some(true)));
            assert_eq!(v["rows"][1]["left"], "0");
            let want: u64 = pl.rows.iter().map(|r| r.amount).sum();
            assert!(sign_plan(&pl, &p, "wrong password!!").await.is_err(), "wrong password signs nothing");
            let (signed, skipped) = sign_plan(&pl, &p, pass).await.unwrap();
            assert!(skipped.is_empty() && signed.len() == 3);
            for (_, d, sig) in &signed { assert!(matches!(solana::submit(d, sig).await, solana::Sent::Ok)) }
            for (_, d, sig) in &signed { assert!(matches!(solana::track(&solana::tx_id(sig), c, d.last_valid, || {}).await, solana::Outcome::Confirmed)) }
            assert_eq!(solana::balance(&to, c).await.unwrap(), want);
            assert_eq!(solana::balance(&accs[1].solana, c).await.unwrap(), 0);
            assert_eq!(solana::balance(&accs[2].solana, c).await.unwrap(), 0);
            assert_eq!(solana::sig_state(&solana::tx_id(&signed[0].2), c).await, "finalized");
            let _ = std::fs::remove_dir_all(&dir);
        });
    }
}
