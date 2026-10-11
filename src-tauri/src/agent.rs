// Agent access (owner 09.10, designed with Codex — PLAN.md «Agent access + batch requests»). An MCP adapter
// (agent/talon-mcp.mjs) talks to the core over a Windows named pipe. Agents read public data and file transfer
// REQUESTS; nothing here signs. The owner confirms each plan in the wallet window with the password (lib.rs batch_confirm).
// A same-user process can reach the pipe: the secret proves it read the owner's discovery file, not that it is "Claude".

use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use crate::evm;
use crate::solana::{self, Cluster};

pub const MAX_SOURCES: usize = 50;
pub const PER_MIN: usize = 5; // new requests per minute
pub const TTL: Duration = Duration::from_secs(600); // a plan not confirmed in 10 minutes is gone
pub const READS_PER_MIN: usize = 30; // reads that reach a node (balances, history)

// One place for every batch (docs/BATCHES.md, reviewed 11.10): whoever made it — the owner's folder, an agent, an add-on —
// a plan is the exact transactions the owner reads, one per own account. Nothing is rebuilt at confirm: only what goes
// stale is refreshed (Solana: the blockhash; EVM: the nonce is checked, gas and fees stay frozen), a row that moved is
// left out with the reason, one password signs the rest.

/// the planned transactions: (own account id, the draft the owner reads)
#[derive(Clone)]
pub enum Body { Sol(Vec<(String, solana::Draft)>), Evm(Vec<(String, evm::Draft)>) }
/// an immutable plan: the password confirms exactly this; any change = a new request
#[derive(Clone)]
pub struct Plan { pub id: String, pub to: String, pub ask_tail: bool, pub body: Body, pub view: Value, pub at: Instant, pub owner: bool, pub addon: Option<String> } // ask_tail: the owner types the destination's last 4 characters // owner: made by hand (a folder); addon: proposed by that add-on
/// what became of one row after the owner confirmed; `state` follows the core's own trackers (lib.rs `told`)
#[derive(Clone)]
pub struct Done { pub account: String, pub sig: Option<String>, pub error: Option<String>, pub state: &'static str }
/// a signed row, ready to journal and send
pub enum Tx { Sol(solana::Draft, [u8; 64]), Evm(evm::Draft, Vec<u8>) }

#[derive(Default)]
pub struct Hub {
    pub on: bool,
    pub waiting: Option<Plan>,
    pub closed: HashMap<String, Result<Vec<Done>, String>>, // id -> results, or why it never ran (rejected, expired, locked)
    recent: VecDeque<Instant>,
    reads: VecDeque<Instant>,
    #[cfg(windows)]
    pub server: Option<tauri::async_runtime::JoinHandle<()>>,
}

impl Hub {
    /// the waiting plan, dropped (and recorded) once it is past its time
    pub fn live(&mut self) -> Option<&Plan> {
        if self.waiting.as_ref().is_some_and(|p| p.at.elapsed() > TTL) { let p = self.waiting.take().unwrap(); self.close(&p.id, Err("время на подтверждение вышло".into())) }
        self.waiting.as_ref()
    }
    pub fn close(&mut self, id: &str, r: Result<Vec<Done>, String>) {
        // forget old outcomes, never one still on its way (an agent that sees «unknown» might ask again)
        if self.closed.len() > 50 { self.closed.retain(|_, r| r.as_ref().is_ok_and(|rows| rows.iter().any(|d| matches!(d.state, "pending" | "confirmed")))) }
        self.closed.insert(id.into(), r);
    }
    /// a send's state as the core's tracker saw it (status never asks a node)
    pub fn seen(&mut self, sig: &str, state: &'static str) {
        let st = match state { "confirmed" | "finalized" | "unknown" => state, "failed" | "failed-allowance" => "failed", "expired" | "replaced" => "expired", _ => return };
        for d in self.closed.values_mut().filter_map(|r| r.as_mut().ok()).flatten().filter(|d| d.sig.as_deref() == Some(sig)) { d.state = st }
    }
    /// the network refused a send outright: nothing left the account
    pub fn refused(&mut self, sig: &str, why: String) {
        for d in self.closed.values_mut().filter_map(|r| r.as_mut().ok()).flatten().filter(|d| d.sig.as_deref() == Some(sig)) { d.sig = None; d.state = "not-sent"; d.error = Some(why.clone()) }
    }
    /// reads that reach a node: a looping agent must not keep the owner's own node rate-limited
    pub fn admit_read(&mut self, now: Instant) -> Result<(), String> {
        while self.reads.front().is_some_and(|t| now.duration_since(*t) > Duration::from_secs(60)) { self.reads.pop_front(); }
        if self.reads.len() >= READS_PER_MIN { return Err(format!("не больше {READS_PER_MIN} чтений в минуту: подождите")) }
        self.reads.push_back(now);
        Ok(())
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
pub struct Source { pub account: String, pub amount: String } // "1.5" or "max"

/// where a batch goes: a Solana cluster, or an EVM network from the core's table (its own node)
pub enum Net { Sol(Cluster), Evm { chain_id: u64, rpc: &'static str } }
impl Net {
    /// the one network gate for batches, as for single sends: development builds move test money only, the local test
    /// chains only with a throwaway vault; release builds never the local ones (whatever listens there could lie)
    pub fn parse(network: &str) -> Result<Net, String> {
        let n = match Cluster::parse(network) {
            Ok(c) => Net::Sol(c),
            Err(_) => evm::find(network).map(|(chain_id, rpc)| Net::Evm { chain_id, rpc }).ok_or("такой сети кошелёк не знает: mainnet, devnet или EVM-сеть из списка")?,
        };
        let local = matches!(n, Net::Sol(Cluster::Local) | Net::Evm { chain_id: 31337, .. });
        #[cfg(debug_assertions)]
        {
            let test = match &n { Net::Sol(c) => !matches!(c, Cluster::Mainnet), Net::Evm { chain_id, .. } => evm::is_test(*chain_id) };
            if !test { return Err("настоящие сети закрыты в сборке для разработки; переводы — только в установленной программе".into()) }
            if local && std::env::var("CW_VAULT").is_err() { return Err("локальная сеть: только с одноразовым хранилищем (CW_VAULT)".into()) }
        }
        #[cfg(not(debug_assertions))]
        if local { return Err("локальная тестовая сеть только в сборке для разработки".into()) }
        Ok(n)
    }
}

/// sources -> plan: own accounts only, one row per account and per sender address, no Ledger, no self-transfer; every row
/// built, priced and dry-run by the core, «max» written out as the exact amount now; a native row whose fee is not less
/// than its amount is refused. A requester (agent, add-on; `owner` false) also meets the recipient guard (an ordinary
/// wallet, never a contract or a token account) and, on real money, the core's own token list
pub async fn plan(accounts: &[crate::vault::Account], net: &Net, to: &str, token: Option<&str>, sources: &[Source], id: String, owner: bool, addon: Option<String>) -> Result<Plan, String> {
    if sources.is_empty() { return Err("нет ни одного счёта-источника".into()) }
    if sources.len() > MAX_SOURCES { return Err(format!("не больше {MAX_SOURCES} счетов в одном запросе")) }
    let to = to.trim();
    let token = match token.map(str::trim) { Some("") => return Err("token: пусто — уберите его, чтобы переводить монету сети, или укажите токен".into()), t => t };
    let mut seen = HashSet::new();
    let mut own = vec![];
    for s in sources {
        let a = accounts.iter().find(|a| a.id == s.account || a.solana == s.account || a.evm.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(&s.account)))
            .ok_or_else(|| format!("счёт {} не из этого кошелька", s.account))?;
        if !seen.insert(a.id.clone()) { return Err(format!("счёт «{}» указан дважды", a.name)) }
        if a.ledger { return Err(format!("счёт «{}» подписывает Ledger: в пакет он не входит", a.name)) }
        own.push((a, s.amount.trim()));
    }
    let warns = |w: Vec<String>| w.into_iter().collect::<BTreeSet<_>>();
    let (body, mut v) = match *net {
        Net::Sol(c) => {
            if !solana::valid_address(to) { return Err("получатель: это не адрес Solana".into()) }
            if !owner && !solana::plain_wallet(to, c).await? { return Err("получатель — не обычный кошелёк (счёт токена или программы): по запросу туда не отправляем".into()) }
            let price = if token.is_none() { solana::sol_price(c).await? } else { 0 }; // SOL rows: one fixed price, the fee the owner reads
            let (mut rows, mut shown) = (vec![], vec![]);
            for (a, amount) in &own {
                if a.solana == to { return Err(format!("счёт «{}» и есть получатель", a.name)) }
                if rows.iter().any(|(_, d): &(String, solana::Draft)| d.from == a.solana) { return Err(format!("адрес счёта «{}» уже есть в пакете", a.name)) }
                let max = amount.eq_ignore_ascii_case("max");
                let (d, left) = match token {
                    Some(mint) => {
                        let amount = if max {
                            let (raw, dec) = solana::holding(&solana::watch(&a.solana, c).await?, mint).filter(|(r, _)| *r > 0).ok_or_else(|| format!("на счёте «{}» нет этого токена", a.name))?;
                            solana::units(raw, dec as u32)
                        } else { amount.replace(',', ".") };
                        (solana::prepare_token(&a.solana, to, mint, &amount, c).await.map_err(|e| format!("«{}»: {e}", a.name))?, None)
                    }
                    None => {
                        let fee = solana::sol_transfer_fee(price);
                        let bal = solana::balance(&a.solana, c).await?;
                        let lamports = if max { bal.checked_sub(fee).filter(|v| *v > 0).ok_or_else(|| format!("на счёте «{}» не хватает даже на комиссию", a.name))? } else { solana::parse_amount(amount, 9)? };
                        let d = solana::prepare_priced(&a.solana, to, lamports, c, Some(price)).await.map_err(|e| format!("«{}»: {e}", a.name))?;
                        if d.fee != fee { return Err("сеть посчитала другую комиссию: попробуйте ещё раз".into()) }
                        (d, Some(bal.saturating_sub(lamports.saturating_add(fee))))
                    }
                };
                if d.mint.is_none() && d.amount <= d.fee { return Err(format!("«{}»: комиссия не меньше суммы — такой перевод не имеет смысла", a.name)) }
                if !owner && matches!(c, Cluster::Mainnet) && d.mint.is_some() && d.symbol.is_none() { return Err("по запросу — только известные токены (USDC, USDT): этот кошелёк не знает, кто его выпустил".into()) }
                shown.push(json!({ "name": a.name, "from": d.from, "amount": solana::units(d.amount, d.decimals as u32), "fee": solana::units(d.fee + d.rent, 9), "left": left.map(|l| solana::units(l, 9)) }));
                rows.push((a.id.clone(), d));
            }
            let d0 = &rows[0].1;
            let v = json!({
                "network": c.label(), "test": !matches!(c, Cluster::Mainnet), "newRecipient": rows.iter().any(|(_, d)| d.new_recipient),
                "total": solana::units(rows.iter().try_fold(0u64, |s, (_, d)| s.checked_add(d.amount)).ok_or("сумма вышла за пределы")?, d0.decimals as u32),
                "sym": d0.symbol.map(str::to_string).unwrap_or_else(|| token.unwrap_or_default().to_string()), // a name only from the core's list; else the whole mint
                "fees": solana::units(rows.iter().map(|(_, d)| d.fee + d.rent).sum(), 9), "feeSym": "SOL", "feeMax": token.is_some(), // a token row's rent is paid once per new recipient
                "warn": warns(rows.iter().filter_map(|(_, d)| d.warn.map(str::to_string)).collect()), "rows": shown,
            });
            (Body::Sol(rows), v)
        }
        Net::Evm { chain_id, rpc } => {
            if !evm::valid_address(to) { return Err("получатель: это не адрес EVM".into()) }
            if !owner && (to[2..38].bytes().all(|b| b == b'0') || evm::is_contract(rpc, to).await?) { return Err("получатель — контракт или служебный адрес сети: по запросу туда не отправляем".into()) }
            let mut rows: Vec<(String, evm::Draft)> = vec![];
            for (a, amount) in &own {
                let from = a.evm.clone().ok_or_else(|| format!("у счёта «{}» нет адреса Ethereum", a.name))?;
                // two accounts can carry one key: one row per sender ADDRESS, or two rows would take one nonce
                if rows.iter().any(|(_, d)| d.from.eq_ignore_ascii_case(&from)) { return Err(format!("адрес счёта «{}» уже есть в пакете", a.name)) }
                let d = evm::prepare(evm::Ask { rpc, chain_id, from: &from, to, amount, token, name: None, symbol: None }).await.map_err(|e| format!("«{}»: {e}", a.name))?;
                if d.token.is_none() && d.fee_max >= d.amount { return Err(format!("«{}»: комиссия не меньше суммы — такой перевод не имеет смысла", a.name)) }
                rows.push((a.id.clone(), d));
            }
            // ponytail: a token row's fee is paid in the coin and not capped against the amount; the sheet shows the ceiling
            let d0 = &rows[0].1;
            let v = json!({
                "network": d0.network, "test": d0.test, "newRecipient": rows.iter().any(|(_, d)| d.new_recipient),
                "total": evm::units(rows.iter().try_fold(0u128, |s, (_, d)| s.checked_add(d.amount)).ok_or("сумма вышла за пределы")?, d0.decimals as u32), "sym": d0.symbol,
                "fees": evm::units(rows.iter().try_fold(0u128, |s, (_, d)| s.checked_add(d.fee_max)).ok_or("плата сети вышла за пределы")?, 18), "feeSym": d0.native, "feeMax": true,
                "warn": warns(rows.iter().filter_map(|(_, d)| d.warn.clone()).collect()), // the core's own words, never the requester's
                "rows": rows.iter().zip(&own).map(|((_, d), (a, _))| json!({ "name": a.name, "from": d.from, "amount": evm::units(d.amount, d.decimals as u32), "fee": evm::units(d.fee_max, 18) })).collect::<Vec<_>>(),
            });
            (Body::Evm(rows), v)
        }
    };
    // a requester's destination that is not one of the owner's own accounts: the owner types its last 4 characters (from
    // their own source). Not tied to «never used»: a requester could dust its own address first (review 11.10)
    let ask_tail = !owner && !accounts.iter().any(|a| a.solana == to || a.evm.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(to)));
    v["id"] = json!(id); v["to"] = json!(to); v["count"] = json!(own.len()); v["askTail"] = json!(ask_tail);
    v["by"] = json!(if owner { "owner" } else if addon.is_some() { "addon" } else { "agent" }); v["byName"] = json!(addon);
    Ok(Plan { id, to: to.into(), ask_tail, body, view: v, at: Instant::now(), owner, addon })
}

/// the owner confirmed: each row refreshed (Solana: a fresh blockhash; EVM: nonce, balance and dry run re-checked) and
/// left out with the reason if it moved; one decryption signs the rest; each signature checked against its sender
pub async fn sign(body: &Body, vault: &std::path::Path, pass: &str) -> Result<(Vec<(String, Tx)>, Vec<Done>), String> {
    let skip = |a: &str, e: String| Done { account: a.into(), sig: None, error: Some(e), state: "not-sent" };
    let (mut out, mut skipped) = (vec![], vec![]);
    match body {
        Body::Sol(rows) => {
            let mut built = vec![];
            for (a, d) in rows { match solana::refresh(d).await { Ok(f) => built.push((a.clone(), f)), Err(e) => skipped.push(skip(a, e)) } }
            let items: Vec<(&str, &[u8])> = built.iter().map(|(a, d)| (a.as_str(), d.message.as_slice())).collect();
            let sigs = crate::vault::sign_many(vault, pass, &items)?; // opens the vault even with nothing left to sign: a wrong password never spends a plan
            for ((a, d), sig) in built.into_iter().zip(sigs) {
                let pk: [u8; 32] = bs58::decode(&d.from).into_vec().ok().and_then(|v| v.try_into().ok()).ok_or("bad sender")?;
                ed25519_dalek::VerifyingKey::from_bytes(&pk).and_then(|k| k.verify_strict(&d.message, &ed25519_dalek::Signature::from_bytes(&sig)))
                    .map_err(|_| "подпись не сходится с адресом счёта".to_string())?;
                out.push((a, Tx::Sol(d, sig)));
            }
        }
        Body::Evm(rows) => {
            let mut built = vec![];
            for (a, d) in rows { match evm::recheck(d).await { Ok(()) => built.push((a.clone(), d.clone())), Err(e) => skipped.push(skip(a, e)) } }
            let items: Vec<(&str, [u8; 32])> = built.iter().map(|(a, d)| (a.as_str(), d.hash())).collect();
            let sigs = crate::vault::sign_evm_many(vault, pass, &items)?;
            for ((a, d), (sig, parity)) in built.into_iter().zip(sigs) {
                let rs = k256::ecdsa::Signature::from_slice(&sig).map_err(|_| "подпись повреждена".to_string())?;
                let rec = k256::ecdsa::RecoveryId::from_byte(parity).ok_or("подпись повреждена")?;
                let vk = k256::ecdsa::VerifyingKey::recover_from_prehash(&d.hash(), &rs, rec).map_err(|_| "подпись повреждена".to_string())?;
                if crate::vault::evm_address_of(&vk) != d.from { return Err("подпись не сходится с адресом счёта".into()) }
                let raw = d.signed(&sig, parity);
                out.push((a, Tx::Evm(d, raw)));
            }
        }
    }
    Ok((out, skipped))
}

/// what the confirmation shows: plain numbers and addresses, no text from the requester (prompt injection has no voice here)
pub fn show(p: &Plan, open: bool) -> Value {
    let mut v = p.view.clone();
    v["open"] = json!(open);
    v["expires"] = json!(TTL.saturating_sub(p.at.elapsed()).as_secs());
    v
}
/// a requester's plan to an address new on its network: the owner types the address's last 4 characters (a swapped
/// address shows up here). EVM addresses compare without case
pub fn tail_ok(to: &str, tail: Option<&str>) -> bool {
    let t = tail.unwrap_or("").trim();
    t.chars().count() == 4 && if to.starts_with("0x") { to.to_ascii_lowercase().ends_with(&t.to_ascii_lowercase()) } else { to.ends_with(t) }
}

/// the agent's view of a request: waiting, closed with a reason, or each row with the state the core's tracker saw
pub fn status(h: &mut Hub, id: &str) -> Value {
    if let Some(p) = h.live().filter(|p| p.id == id) { return json!({ "id": id, "state": "waiting", "transfers": p.view["count"], "secondsLeft": TTL.saturating_sub(p.at.elapsed()).as_secs() }) }
    match h.closed.get(id) {
        None => json!({ "id": id, "state": "unknown" }),
        Some(Err(why)) => json!({ "id": id, "state": "closed", "reason": why }),
        Some(Ok(rows)) => json!({ "id": id, "state": "confirmed", "transfers": rows.iter().map(|r| json!({ "account": r.account, "sig": r.sig, "state": r.state, "error": r.error })).collect::<Vec<_>>() }),
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
    static ONE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(()); // one agent call at a time, however many pipes
    let _one = ONE.lock().await;
    let st = app.state::<crate::App>();
    if !st.agent.lock().unwrap().on { return Err("доступ агента выключен в настройках кошелька".into()) }
    // the one call that works while locked: an agent learns why it cannot read yet, and whether a request still waits
    if method == "status" {
        let unlocked = st.session.lock().unwrap().unlocked;
        let mut h = st.agent.lock().unwrap();
        // someone else's batch: the agent learns that one waits, never its id (its outcome is not the agent's to read)
        let w = h.live().map(|p| if p.owner || p.addon.is_some() { json!({ "by": if p.owner { "owner" } else { "addon" }, "transfers": p.view["count"] }) } else { json!({ "id": p.id, "by": "agent", "transfers": p.view["count"] }) });
        return Ok(json!({ "unlocked": unlocked, "waiting": w, "version": env!("CARGO_PKG_VERSION"),
            "can": ["read accounts, folders, balances (Solana and EVM), history",
                "request transfers from own accounts to one address: SOL, Solana tokens, the coin or a listed stablecoin of an EVM network from networks (the owner confirms)"],
            "cannot": ["sign", "export keys or words", "send to a contract or a token account", "TON or networks the owner added by hand", "change settings"] }))
    }
    let (accounts, locks) = {
        let s = st.session.lock().unwrap();
        if !s.unlocked { return Err("кошелёк заблокирован: попросите владельца открыть его".into()) }
        (s.accounts.clone(), s.locks) // proven at unlock, never the file's unverified header
    };
    // the local test chain answers unverified: agents reach it only in development builds
    let net = || Cluster::parse(p["network"].as_str().unwrap_or("mainnet")).and_then(|c| if matches!(c, Cluster::Local) && !cfg!(debug_assertions) { Err("локальная тестовая сеть только в сборке для разработки".into()) } else { Ok(c) });
    if matches!(method, "balances" | "evm_balances" | "history") { st.agent.lock().unwrap().admit_read(Instant::now())? }
    match method {
        "accounts" => Ok(json!(accounts.iter().map(|a| json!({ "id": a.id, "name": a.name, "solana": a.solana, "evm": a.evm, "ton": a.ton })).collect::<Vec<_>>())),
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
            let net = Net::parse(p["network"].as_str().ok_or("network: mainnet (настоящие деньги), devnet или EVM-сеть из networks — укажите явно")?)?;
            let to = p["to"].as_str().ok_or("нужен адрес получателя: to")?;
            let sources: Vec<Source> = serde_json::from_value(p["sources"].clone()).map_err(|_| "sources: список { account, amount }".to_string())?;
            let busy = || "уже есть запрос, который ждёт подтверждения владельца".to_string();
            {
                let mut h = st.agent.lock().unwrap();
                if h.live().is_some() { return Err(busy()) }
                h.admit(Instant::now())?;
            }
            let token = match &p["token"] { Value::Null => None, Value::String(t) => Some(t.as_str()), _ => return Err("token: строка (адрес токена или его название) или ничего".into()) };
            let plan = plan(&accounts, &net, to, token, &sources, crate::rand_id(), false, None).await?;
            let v = show(&plan, crate::vault::is_open(&crate::vault_path(app)?));
            {
                let s = st.session.lock().unwrap();
                if !s.unlocked || s.locks != locks { return Err("кошелёк заблокировали, пока запрос собирался".into()) }
                let mut h = st.agent.lock().unwrap();
                if h.live().is_some() { return Err(busy()) }
                h.waiting = Some(plan);
            }
            let _ = app.emit_to("main", "agent-request", v.clone());
            #[cfg(desktop)]
            if let Some(w) = app.get_webview_window("main") { let _ = w.request_user_attention(Some(tauri::UserAttentionType::Informational)); }
            Ok(json!({ "id": v["id"], "state": "waiting", "plan": v, "next": "the owner confirms in the wallet window; poll request_status" }))
        }
        "request_status" => Ok(status(&mut st.agent.lock().unwrap(), p["id"].as_str().unwrap_or(""))),
        // the owner's folders: named groups of account ids (own and watch-only); sources for a request come from here
        // own accounts only: watch-only addresses in a folder are not the agent's business (nor a destination)
        "folders" => Ok(json!(crate::folders::read(&crate::vault_path(app)?.with_file_name("folders.json"))?.into_iter()
            .map(|f| json!({ "id": f.id, "name": f.name, "accounts": f.accounts.into_iter().filter(|id| accounts.iter().any(|a| &a.id == id)).collect::<Vec<_>>() })).collect::<Vec<_>>())),
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
            let n = p["limit"].as_u64().unwrap_or(10).clamp(1, 10) as usize; // each detail is one node call
            let net = p["network"].as_str().unwrap_or("mainnet");
            if let Ok(c) = Cluster::parse(net) {
                if matches!(c, Cluster::Local) && !cfg!(debug_assertions) { return Err("локальная тестовая сеть только в сборке для разработки".into()) }
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
        h.waiting = Some(Plan { id: "a".into(), to: String::new(), ask_tail: false, body: Body::Sol(vec![]), view: json!({}), at: t - TTL - Duration::from_secs(1), owner: false, addon: None });
        assert!(h.live().is_none(), "an old plan expires");
        assert!(matches!(h.closed.get("a"), Some(Err(_))));
        // outcomes come from the core's trackers; a refusal clears the signature
        let row = |s: &str| Done { account: "x".into(), sig: Some(s.into()), error: None, state: "pending" };
        h.close("b", Ok(vec![row("s1"), row("s2"), row("s3")]));
        h.seen("s1", "confirmed"); h.seen("s1", "finalized"); h.seen("s2", "replaced"); h.seen("s3", "sent");
        h.refused("s3", "nope".into());
        let v = status(&mut h, "b");
        let st: Vec<&str> = v["transfers"].as_array().unwrap().iter().map(|r| r["state"].as_str().unwrap()).collect();
        assert_eq!(st, ["finalized", "expired", "not-sent"]);
        assert!(v["transfers"][2]["sig"].is_null());
        assert_eq!(status(&mut h, "a")["state"], "closed");
        // the last 4 characters of a new recipient: exact for Solana, any case for EVM
        assert!(tail_ok("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin", Some(" VFin ")));
        assert!(!tail_ok("9xQeWvG816bUx9EPjHmaT23yvVM2ZWbrrpZb9PusVFin", Some("vfin")));
        assert!(tail_ok("0x52908400098527886E0F7030069857D2E4169EE7", Some("9ee7")));
        assert!(!tail_ok("0x52908400098527886E0F7030069857D2E4169EE7", Some("EE7")) && !tail_ok("0x52908400098527886E0F7030069857D2E4169EE7", None));
        assert_eq!(solana::sol_transfer_fee(1_000), 5001);
        assert_eq!(solana::sol_transfer_fee(1_000_000), 6000);
    }
    // a batch end to end on a local validator: plan (own accounts, "max", fixed price), sign with one decryption,
    // submit, finalize; the destination gets exactly the planned sum and "max" sources end at zero.
    // `CW_TEST_LOCAL=1 cargo test batch_live -- --ignored --nocapture` with solana-test-validator running
    #[test]
    #[ignore]
    fn batch_live() {
        use crate::vault;
        tauri::async_runtime::block_on(async {
            assert!(std::env::var("CW_TEST_LOCAL").is_ok(), "local validator only");
            let c = Cluster::Local;
            let net = Net::Sol(c);
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
            // refusals: foreign account, duplicate, self as recipient, too many, a requester sending to a program's account
            assert!(plan(&accs, &net, &to, None, &[src("nope", "1")], "x".into(), false, None).await.is_err());
            assert!(plan(&accs, &net, &to, None, &[src(&accs[0].id, "1"), src(&accs[0].id, "1")], "x".into(), false, None).await.is_err());
            assert!(plan(&accs, &net, &accs[1].solana, None, &[src(&accs[1].id, "1")], "x".into(), false, None).await.is_err());
            assert!(plan(&accs, &net, &to, None, &vec![src(&accs[0].id, "1"); MAX_SOURCES + 1], "x".into(), false, None).await.is_err());
            assert!(plan(&accs, &net, "SysvarRent111111111111111111111111111111111", None, &[src(&accs[0].id, "0.5")], "x".into(), false, None).await.is_err());
            let pl = plan(&accs, &net, &to, None, &[src(&accs[0].id, "0.5"), src(&accs[1].id, "max"), src(&accs[2].solana, "MAX")], "b1".into(), false, None).await.unwrap();
            let v = show(&pl, false);
            assert_eq!((v["count"].as_u64(), v["newRecipient"].as_bool(), v["by"].as_str()), (Some(3), Some(true), Some("agent")));
            assert_eq!(v["rows"][1]["left"], "0");
            let Body::Sol(rows) = &pl.body else { panic!() };
            let want: u64 = rows.iter().map(|(_, d)| d.amount).sum();
            assert!(sign(&pl.body, &p, "wrong password!!").await.is_err(), "wrong password signs nothing");
            let (signed, skipped) = sign(&pl.body, &p, pass).await.unwrap();
            assert!(skipped.is_empty() && signed.len() == 3);
            let sent: Vec<(solana::Draft, [u8; 64])> = signed.into_iter().map(|(_, t)| match t { Tx::Sol(d, s) => (d, s), Tx::Evm(..) => panic!() }).collect();
            for (d, sig) in &sent { assert!(matches!(solana::submit(d, sig).await, solana::Sent::Ok)) }
            for (d, sig) in &sent { assert!(matches!(solana::track(&solana::tx_id(sig), c, d.last_valid, || {}).await, solana::Outcome::Confirmed)) }
            assert_eq!(solana::balance(&to, c).await.unwrap(), want);
            assert_eq!(solana::balance(&accs[1].solana, c).await.unwrap(), 0);
            assert_eq!(solana::balance(&accs[2].solana, c).await.unwrap(), 0);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }
}
