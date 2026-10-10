//! Watch-only Solana reads over JSON-RPC: native balance and SPL / Token-2022 holdings of any address.
//! No keys here. Amounts stay integer base units (as strings) all the way to the UI.

use serde::Serialize;
use base64::Engine;
use serde_json::{Value, json};

const TOKEN: &str = "TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA";
const TOKEN_2022: &str = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb";

#[derive(Clone, Copy)]
pub enum Cluster {
    Mainnet,
    Devnet,
    Local,
}

impl Cluster {
    pub fn parse(s: &str) -> Result<Self, String> {
        match s {
            "mainnet" => Ok(Self::Mainnet),
            "devnet" => Ok(Self::Devnet),
            "local" => Ok(Self::Local),
            _ => Err(format!("unknown cluster: {s}")),
        }
    }
    pub fn name(self) -> &'static str {
        match self { Self::Mainnet => "mainnet", Self::Devnet => "devnet", Self::Local => "local" }
    }
    pub fn label(self) -> &'static str {
        match self { Self::Mainnet => "Solana Mainnet", Self::Devnet => "Solana Devnet", Self::Local => "Solana Local" }
    }
    fn rpc(self) -> String {
        if let Some(u) = self.slot().and_then(|i| OWN.read().unwrap()[i].clone()) { return u }
        match self {
            Self::Mainnet => "https://api.mainnet-beta.solana.com",
            Self::Devnet => "https://api.devnet.solana.com",
            Self::Local => "http://127.0.0.1:8899",
        }.into()
    }
    fn slot(self) -> Option<usize> { match self { Self::Mainnet => Some(0), Self::Devnet => Some(1), Self::Local => None } }
    // network guard: the RPC must prove which chain it serves before we trust a single number from it
    fn genesis(self) -> Option<&'static str> {
        match self {
            Self::Mainnet => Some("5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d"),
            Self::Devnet => Some("EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"),
            Self::Local => None, // ponytail: a local validator mints a fresh genesis each run
        }
    }
}

#[derive(Serialize)]
pub struct Holding {
    mint: String,
    amount: String, // base units
    decimals: u8,
    token2022: bool,
    name: Option<String>,   // from the chain (Metaplex record or Token-2022 metadata): anyone can write anything here
    symbol: Option<String>,
}

// ---------------------------------------------------------------- token names, read from the chain (no token list service)
const METAPLEX: &str = "metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s";
/// a borsh string at `at`: (text, next offset); printable only, trimmed, capped. None if the bytes do not fit.
fn bstr(d: &[u8], at: usize, cap: usize) -> Option<(String, usize)> {
    let len = u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?) as usize;
    if len > 256 { return None }
    let raw = d.get(at + 4..at + 4 + len)?;
    let s: String = String::from_utf8_lossy(raw).chars().filter(|c| !c.is_control()).collect::<String>().trim().chars().take(cap).collect();
    Some((s, at + 4 + len))
}
/// Metaplex metadata account: key(1) update_authority(32) mint(32) name symbol ...
fn metaplex_names(d: &[u8]) -> Option<(String, String)> {
    if d.first() != Some(&4) { return None } // Key::MetadataV1
    let (name, at) = bstr(d, 65, 32)?;
    let (symbol, _) = bstr(d, at, 12)?;
    Some((name, symbol))
}
/// Token-2022 mint with the TokenMetadata extension (type 19) stored in the mint itself
fn t22_names(d: &[u8]) -> Option<(String, String)> {
    if d.len() <= 166 || d[165] != 1 { return None } // account type: mint
    let mut at = 166;
    while at + 4 <= d.len() {
        let (ty, len) = (u16::from_le_bytes([d[at], d[at + 1]]), u16::from_le_bytes([d[at + 2], d[at + 3]]) as usize);
        let v = d.get(at + 4..at + 4 + len)?;
        if ty == 19 { let (name, n) = bstr(v, 64, 32)?; let (symbol, _) = bstr(v, n, 12)?; return Some((name, symbol)) }
        if ty == 0 { return None }
        at += 4 + len;
    }
    None
}
fn metadata_pda(mint: &str) -> Option<String> {
    let prog: pubkey3::Pubkey = METAPLEX.parse().ok()?;
    let m: pubkey3::Pubkey = mint.parse().ok()?;
    Some(pubkey3::Pubkey::find_program_address(&[b"metadata", prog.as_ref(), m.as_ref()], &prog).0.to_string())
}

#[derive(Serialize)]
pub struct Watch {
    lamports: String,
    tokens: Vec<Holding>,
    partial: bool, // Token-2022 holdings could not be read from this RPC
}

fn b64(v: &Value) -> Option<Vec<u8>> {
    base64::engine::general_purpose::STANDARD.decode(v.get(0)?.as_str()?).ok()
}

pub fn valid_address(s: &str) -> bool {
    matches!(bs58::decode(s).into_vec(), Ok(v) if v.len() == 32)
}

// ---------------------------------------------------------------- own node per network
// The owner's node (Helius, QuickNode…) usually carries an API key in its URL: kept in the core, never echoed back
// whole, never in errors. Local stays fixed (dev builds only).
static OWN: std::sync::RwLock<[Option<String>; 2]> = std::sync::RwLock::new([None, None]);

/// what the UI may show of a node URL: the host only (the key lives in the path or query)
pub fn host(url: &str) -> Option<String> { reqwest::Url::parse(url).ok()?.host_str().map(str::to_string) }
pub fn own_host(c: Cluster) -> Option<String> { OWN.read().unwrap()[c.slot()?].as_deref().and_then(host) }
/// trusted input (our own file, already checked when saved): just install it
pub fn set_own(c: Cluster, url: Option<String>) { if let Some(i) = c.slot() { OWN.write().unwrap()[i] = url } }
/// a new URL must be https and prove, by genesis hash, that it serves the selected network
pub async fn check_own(c: Cluster, url: &str) -> Result<(), String> {
    let want = c.genesis().ok_or("для этой сети свой узел не задаётся")?;
    let u = reqwest::Url::parse(url.trim()).map_err(|_| "это не адрес узла".to_string())?;
    if u.scheme() != "https" || u.host_str().is_none() { return Err("нужен адрес, который начинается с https://".into()) }
    let got = post(&client()?, u.as_str(), "getGenesisHash", json!([])).await.map_err(|(_, m)| m)?;
    if got.as_str() != Some(want) { return Err("этот узел обслуживает другую сеть".into()) }
    Ok(())
}

async fn rpc(http: &reqwest::Client, c: Cluster, method: &str, params: Value) -> Result<Value, String> {
    rpc_coded(http, c, method, params).await.map_err(|(_, m)| m)
}
/// the JSON-RPC error code, when the node gave one, travels as a number next to the message
async fn rpc_coded(http: &reqwest::Client, c: Cluster, method: &str, params: Value) -> Result<Value, (Option<i64>, String)> {
    post(http, &c.rpc(), method, params).await
}
async fn post(http: &reqwest::Client, url: &str, method: &str, params: Value) -> Result<Value, (Option<i64>, String)> {
    let body = json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params });
    let resp = http.post(url).json(&body).send().await.map_err(|e| (None, net(&e)))?;
    match resp.status().as_u16() {
        200 => {}
        429 => return Err((None, "the network is rate-limiting requests, try again in a minute".into())),
        s => return Err((None, format!("RPC answered with HTTP {s}"))),
    }
    let r: Value = resp.json().await.map_err(|_| (None, "RPC answered with something that is not JSON".to_string()))?;
    if let Some(e) = r.get("error") {
        let code = e.get("code").and_then(Value::as_i64);
        return Err((code, format!("RPC error in {method}: {}", e.get("message").and_then(Value::as_str).unwrap_or("unknown"))));
    }
    r.get("result").cloned().ok_or_else(|| (None, "RPC answer has no result".into()))
}

// no URLs or addresses in user-facing errors: they end up in screenshots
fn net(e: &reqwest::Error) -> String {
    if e.is_timeout() { "network timeout".into() } else if e.is_connect() { "no connection to the network".into() } else { "network request failed".into() }
}

/// one token's balance in a Watch: (base units, decimals); None when the address holds none of it
pub fn holding(w: &Watch, mint: &str) -> Option<(u64, u8)> { w.tokens.iter().find(|h| h.mint == mint).and_then(|h| h.amount.parse().ok().map(|a| (a, h.decimals))) }
pub async fn watch(address: &str, c: Cluster) -> Result<Watch, String> {
    if !valid_address(address) {
        return Err("not a Solana address".into());
    }
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
    if let Some(want) = c.genesis() {
        let got = rpc(&http, c, "getGenesisHash", json!([])).await?;
        if got.as_str() != Some(want) {
            return Err("this RPC serves a different network than selected".into());
        }
    }
    let bal = rpc(&http, c, "getBalance", json!([address, { "commitment": "confirmed" }])).await?;
    let lamports = bal.get("value").and_then(Value::as_u64).ok_or("bad balance answer")?.to_string();

    // public RPC refuses jsonParsed here (2026), so read the raw layouts: token account = mint[0..32] owner[32..64]
    // amount u64 LE [64..72]; mint account decimals u8 at [44]. Same prefix for Token-2022 (extensions come after).
    // ponytail: hand-read three fields for watch-only; signing paths use the official spl crates (PLAN.md)
    let mut raw: Vec<(String, u64, bool)> = Vec::new();
    let mut partial = false;
    for program in [TOKEN, TOKEN_2022] {
        let r = rpc(&http, c, "getTokenAccountsByOwner",
            json!([address, { "programId": program }, { "encoding": "base64", "commitment": "confirmed" }])).await;
        // a node without a Token-2022 owner index answers INVALID_PARAMS: say so instead of failing everything
        let r = match r { Ok(r) => r, Err(_) if program == TOKEN_2022 => { partial = true; continue } Err(e) => return Err(e) };
        for acc in r.get("value").and_then(Value::as_array).into_iter().flatten() {
            let Some(d) = b64(&acc["account"]["data"]) else { continue };
            if d.len() < 72 { continue }
            let amount = u64::from_le_bytes(d[64..72].try_into().unwrap());
            if amount == 0 { continue }
            // one owner may hold several accounts of the same mint: sum them into one holding
            let mint = bs58::encode(&d[0..32]).into_string();
            match raw.iter_mut().find(|(m, ..)| *m == mint) {
                Some(h) => h.1 = h.1.saturating_add(amount),
                None => raw.push((mint, amount, program == TOKEN_2022)),
            }
        }
    }
    let mut mints: Vec<&str> = raw.iter().map(|(m, ..)| m.as_str()).collect();
    mints.sort_unstable();
    mints.dedup();
    let mut decimals = std::collections::HashMap::new();
    let mut names: std::collections::HashMap<String, (String, String)> = std::collections::HashMap::new();
    for chunk in mints.chunks(100) {
        let r = rpc(&http, c, "getMultipleAccounts", json!([chunk, { "encoding": "base64" }])).await?;
        for (m, acc) in chunk.iter().zip(r.get("value").and_then(Value::as_array).into_iter().flatten()) {
            let Some(d) = b64(&acc["data"]).filter(|d| d.len() > 44) else { continue };
            decimals.insert(*m, d[44]);
            if let Some(n) = t22_names(&d) { names.insert(m.to_string(), n); }
        }
    }
    // the rest: Metaplex records, one batch per 100; a node that refuses only costs the names, never the balances
    let pdas: Vec<(String, String)> = mints.iter().filter(|m| !names.contains_key(**m)).filter_map(|m| Some((m.to_string(), metadata_pda(m)?))).collect();
    for chunk in pdas.chunks(100) {
        let keys: Vec<&str> = chunk.iter().map(|(_, p)| p.as_str()).collect();
        let Ok(r) = rpc(&http, c, "getMultipleAccounts", json!([keys, { "encoding": "base64" }])).await else { break };
        for ((m, _), acc) in chunk.iter().zip(r.get("value").and_then(Value::as_array).into_iter().flatten()) {
            if let Some(n) = b64(&acc["data"]).and_then(|d| metaplex_names(&d)) { names.insert(m.clone(), n); }
        }
    }
    let tokens = raw.iter().filter_map(|(m, amount, t22)| {
        let (name, symbol) = names.remove(m).map_or((None, None), |(n, s)| ((!n.is_empty()).then_some(n), (!s.is_empty()).then_some(s)));
        Some(Holding { mint: m.clone(), amount: amount.to_string(), decimals: *decimals.get(m.as_str())?, token2022: *t22, name, symbol })
    }).collect();
    Ok(Watch { lamports, tokens, partial })
}

// ---------------------------------------------------------------- send SOL
fn client() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().map_err(|e| e.to_string())
}
async fn guard(http: &reqwest::Client, c: Cluster) -> Result<(), String> {
    if let Some(want) = c.genesis() {
        if rpc(http, c, "getGenesisHash", json!([])).await?.as_str() != Some(want) { return Err("this RPC serves a different network than selected".into()) }
    }
    Ok(())
}
// reads only (history entries): a node URL that proved its network this run is not asked again, so a history page is
// 25 calls, not 50. Every send still checks afresh (guard).
static PROVEN: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
async fn guard_read(http: &reqwest::Client, c: Cluster) -> Result<(), String> {
    let url = c.rpc();
    if PROVEN.lock().unwrap().contains(&url) { return Ok(()) }
    guard(http, c).await?;
    PROVEN.lock().unwrap().push(url);
    Ok(())
}
fn value_u64(v: &Value) -> Result<u64, String> { v.get("value").and_then(Value::as_u64).ok_or_else(|| "RPC answer has no number".into()) }

/// "1,5" / "0.000000001" -> base units; exact integer math, no floats
pub fn parse_amount(s: &str, decimals: u32) -> Result<u64, String> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect::<String>().replace(',', ".");
    let (i, f) = s.split_once('.').unwrap_or((&s, ""));
    if (i.is_empty() && f.is_empty()) || !i.chars().chain(f.chars()).all(|c| c.is_ascii_digit()) { return Err("сумма: только цифры и одна точка".into()) }
    if f.len() > decimals as usize { return Err(format!("сумма: не больше {decimals} знаков после точки")) }
    let frac = format!("{f:0<w$}", w = decimals as usize);
    let v = format!("{i}{frac}").trim_start_matches('0').parse::<u64>().or_else(|e| if e.kind() == &std::num::IntErrorKind::Empty { Ok(0) } else { Err(()) });
    match v { Ok(0) => Err("сумма должна быть больше нуля".into()), Ok(v) => Ok(v), Err(_) => Err("сумма слишком большая".into()) }
}
/// base units -> "1.5" (exact)
pub fn units(v: u64, decimals: u32) -> String {
    let s = format!("{v:0>w$}", w = decimals as usize + 1);
    let (i, f) = s.split_at(s.len() - decimals as usize);
    let f = f.trim_end_matches('0');
    if f.is_empty() { i.to_string() } else { format!("{i}.{f}") }
}

/// one transfer (SOL or a token), built and priced by the core. `message` is exactly what gets signed and sent.
pub struct Draft {
    pub from: String,
    pub to: String,
    pub amount: u64, // base units of the asset
    pub decimals: u8,
    pub mint: Option<String>, // None = SOL
    pub symbol: Option<&'static str>,
    pub fee: u64,  // network fee, lamports
    pub rent: u64, // lamports for the recipient's new token account (0 if none)
    pub new_recipient: bool,
    pub warn: Option<&'static str>, // shown on the confirmation (e.g. a token whose issuer can take it back)
    pub swap: Option<SwapView>,     // a swap: what comes back, as the dry run saw it
    pub cluster: Cluster,
    pub message: Vec<u8>,
    pub last_valid: u64,
}

// confirm screen names only these (mainnet mints); every other token shows its mint address
const KNOWN: &[(&str, &str)] = &[("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "USDC"), ("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB", "USDT")];
const SYSTEM: &str = "11111111111111111111111111111111";

struct Raw { owner: String, exec: bool, data: Vec<u8> }
async fn account(http: &reqwest::Client, c: Cluster, addr: &str) -> Result<Option<Raw>, String> {
    let r = rpc(http, c, "getAccountInfo", json!([addr, { "encoding": "base64", "commitment": "confirmed" }])).await?;
    let Some(v) = r.get("value").filter(|v| !v.is_null()) else { return Ok(None) };
    Ok(Some(Raw { owner: v["owner"].as_str().unwrap_or("").into(), exec: v["executable"].as_bool() == Some(true), data: b64(&v["data"]).ok_or("bad account answer")? }))
}
async fn rent_for(http: &reqwest::Client, c: Cluster, len: usize) -> Result<u64, String> {
    rpc(http, c, "getMinimumBalanceForRentExemption", json!([len])).await?.as_u64().ok_or_else(|| "bad rent answer".into())
}
fn addr(s: &str) -> Result<solana_address::Address, String> { s.parse().map_err(|_| "это не адрес Solana".to_string()) }
fn p3(a: &solana_address::Address) -> pubkey3::Pubkey { pubkey3::Pubkey::new_from_array(a.to_bytes()) }

pub async fn prepare(from: &str, to: &str, lamports: u64, c: Cluster) -> Result<Draft, String> { prepare_priced(from, to, lamports, c, None).await }
/// a SOL transfer at a fixed priority price (batch plans: the fee the owner approved is the fee that gets paid)
pub async fn prepare_priced(from: &str, to: &str, lamports: u64, c: Cluster, price: Option<u64>) -> Result<Draft, String> {
    if !valid_address(to) { return Err("это не адрес Solana".into()) }
    if from == to { return Err("это адрес этого же счёта".into()) }
    let (fa, ta) = (addr(from)?, addr(to)?);
    let http = client()?;
    guard(&http, c).await?;
    // recipient: a program address is not a wallet — SOL sent there is usually gone for good
    let acc = account(&http, c, to).await?;
    if acc.as_ref().is_some_and(|a| a.exec) { return Err("получатель — программа, а не кошелёк".into()) }
    let new_recipient = acc.is_none();
    let rent0 = rent_for(&http, c, 0).await?;
    if new_recipient && lamports < rent0 {
        return Err(format!("адрес получателя ещё пустой: первым переводом сеть требует не меньше {} SOL", units(rent0, 9)));
    }
    let ix = solana_system_interface::instruction::transfer(&fa, &ta, lamports);
    let mut d = Draft { from: from.into(), to: to.into(), amount: lamports, decimals: 9, mint: None, symbol: Some("SOL"), fee: 0, rent: 0, new_recipient, warn: None, cluster: c, message: vec![], last_valid: 0, swap: None };
    seal(&http, &mut d, &fa, &[ix], lamports, rent0, price).await?;
    Ok(d)
}

/// a plain SPL or Token-2022 transfer: transferChecked (mint and decimals checked on chain), the recipient's
/// associated token account created idempotently when missing. Token-2022 mints pass an extension allowlist.
// ponytail: tokens spread over several accounts of one owner are not merged; the send needs one account that covers it
pub async fn prepare_token(from: &str, to: &str, mint: &str, amount: &str, c: Cluster) -> Result<Draft, String> {
    use solana_program_pack::Pack;
    use spl_token_2022_interface::{extension::{BaseStateWithExtensions, ExtensionType, StateWithExtensions}, state::{Account as Tok, AccountState, Mint}};
    if !valid_address(to) { return Err("это не адрес Solana".into()) }
    if from == to { return Err("это адрес этого же счёта".into()) }
    let (fa, ta, ma) = (addr(from)?, addr(to)?, addr(mint)?);
    let http = client()?;
    guard(&http, c).await?;

    let m = account(&http, c, mint).await?.ok_or("токена нет в этой сети")?;
    if m.owner != TOKEN && m.owner != TOKEN_2022 { return Err("это не токен".into()) }
    let program = addr(&m.owner)?;
    let mm = StateWithExtensions::<Mint>::unpack(&m.data).map_err(|_| "токен не читается")?;
    if !mm.base.is_initialized { return Err("токен не читается".into()) }
    let exts = if m.owner == TOKEN_2022 { mm.get_extension_types().map_err(|_| "токен не читается")? } else { vec![] };
    let mut frozen_new = false;
    for e in &exts {
        use ExtensionType as E;
        match e {
            E::MintCloseAuthority | E::DefaultAccountState | E::PermanentDelegate | E::MetadataPointer | E::TokenMetadata
            | E::GroupPointer | E::TokenGroup | E::GroupMemberPointer | E::TokenGroupMember | E::Pausable | E::PermissionedBurn => {}
            E::NonTransferable => return Err("этот токен нельзя переводить".into()),
            E::TransferFeeConfig => return Err("у токена своя комиссия за перевод — такие пока не отправляем".into()),
            E::TransferHook => return Err("токен запускает чужую программу при переводе — такие пока не отправляем".into()),
            other => return Err(format!("у токена особое свойство ({other:?}) — такие пока не отправляем")),
        }
    }
    // states that make a transfer fail on chain (fee and rent lost): refuse before signing (mainnet gate 07.10)
    {
        use spl_token_2022_interface::extension::{default_account_state::DefaultAccountState, pausable::PausableConfig};
        if mm.get_extension::<PausableConfig>().is_ok_and(|p| bool::from(p.paused)) { return Err("выпуск токена сейчас на паузе: переводы остановлены".into()) }
        frozen_new = mm.get_extension::<DefaultAccountState>().is_ok_and(|d| d.state == AccountState::Frozen as u8);
    }
    let decimals = mm.base.decimals;
    let amount = parse_amount(amount, decimals as u32)?;
    let ata = |owner: &solana_address::Address| solana_address::Address::new_from_array(
        spl_associated_token_account_interface::address::get_associated_token_address_with_program_id(&p3(owner), &p3(&ma), &p3(&program)).to_bytes());

    // source: the sender's own initialized account of this mint that covers the amount; the associated one first
    let mine = ata(&fa);
    let r = rpc(&http, c, "getTokenAccountsByOwner", json!([from, { "mint": mint }, { "encoding": "base64", "commitment": "confirmed" }])).await?;
    let (mut src, mut total) = (None, 0u64);
    for a in r.get("value").and_then(Value::as_array).into_iter().flatten() {
        let (Some(key), Some(d)) = (a["pubkey"].as_str().and_then(|k| k.parse::<solana_address::Address>().ok()), b64(&a["account"]["data"])) else { continue };
        if a["account"]["owner"].as_str() != Some(m.owner.as_str()) { continue }
        let Ok(t) = StateWithExtensions::<Tok>::unpack(&d) else { continue };
        if t.base.owner != fa || t.base.mint != ma || t.base.state != AccountState::Initialized { continue }
        total = total.saturating_add(t.base.amount);
        if t.base.amount >= amount && (src.is_none() || key == mine) { src = Some(key) }
    }
    let src = src.ok_or_else(|| if total >= amount { "токены лежат на нескольких счетах: такой перевод пока не собираем".to_string() }
        else { format!("не хватает: доступно {}", units(total, decimals as u32)) })?;

    // destination: a wallet gets its associated account (created if missing); a pasted token account of this mint is used as is
    let (dest, create) = match account(&http, c, to).await? {
        Some(a) if a.exec => return Err("получатель — программа, а не кошелёк".into()),
        Some(a) if a.owner == m.owner => {
            let t = StateWithExtensions::<Tok>::unpack(&a.data).map_err(|_| "это адрес токена, а не кошелька")?;
            if t.base.mint != ma { return Err("это счёт другого токена".into()) }
            receivable(&t)?;
            (ta, false)
        }
        Some(a) if a.owner != SYSTEM => return Err("адрес принадлежит программе, а не кошельку".into()),
        _ => {
            let d = ata(&ta);
            match account(&http, c, &d.to_string()).await? {
                None => (d, true),
                Some(x) => {
                    let t = StateWithExtensions::<Tok>::unpack(&x.data).ok().filter(|t| x.owner == m.owner && t.base.mint == ma && t.base.owner == ta)
                        .ok_or("счёт токена у получателя не сходится")?;
                    receivable(&t)?;
                    (d, false)
                }
            }
        }
    };
    if dest == src { return Err("это адрес этого же счёта".into()) }
    if create && frozen_new { return Err("у этого токена новые счета создаются замороженными: перевод новому получателю не пройдёт".into()) }
    let rent = if create {
        let len = if m.owner == TOKEN_2022 {
            spl_token_2022_interface::extension::account_len::try_calculate_account_len_from_mint_data(&m.data, &[ExtensionType::ImmutableOwner]).map_err(|_| "токен не читается")?
        } else { Tok::LEN };
        rent_for(&http, c, len).await?
    } else { 0 };

    let mut ixs = vec![];
    if create {
        ixs.push(spl_associated_token_account_interface::instruction::create_associated_token_account_idempotent(&p3(&fa), &p3(&ta), &p3(&ma), &p3(&program)));
    }
    ixs.push(spl_token_2022_interface::instruction::transfer_checked(&program, &src, &ma, &dest, &fa, &[], amount, decimals).map_err(|_| "перевод не собирается")?);
    let symbol = KNOWN.iter().find(|(k, _)| *k == mint).map(|(_, s)| *s);
    // permanent delegate: transfers work, but the issuer can move or burn this token from any account (security review 07.10)
    let warn = exts.contains(&ExtensionType::PermanentDelegate).then_some("У этого токена есть постоянный распорядитель: выпустивший его может забрать или сжечь токен с любого счёта, в том числе у получателя.");
    let mut d = Draft { from: from.into(), to: to.into(), amount, decimals, mint: Some(mint.into()), symbol, fee: 0, rent, new_recipient: create, warn, cluster: c, message: vec![], last_valid: 0, swap: None };
    let rent0 = rent_for(&http, c, 0).await?;
    seal(&http, &mut d, &fa, &ixs, rent, rent0, None).await?;
    Ok(d)
}

/// a destination token account that would make the transfer fail on chain
fn receivable(t: &spl_token_2022_interface::extension::StateWithExtensions<spl_token_2022_interface::state::Account>) -> Result<(), String> {
    use spl_token_2022_interface::{extension::{BaseStateWithExtensions, memo_transfer::MemoTransfer}, state::AccountState};
    if t.base.state == AccountState::Frozen { return Err("счёт получателя для этого токена заморожен".into()) }
    if t.get_extension::<MemoTransfer>().is_ok_and(|m| bool::from(m.require_incoming_transfer_memos)) {
        return Err("получатель требует заметку к каждому переводу: такие пока не отправляем".into())
    }
    Ok(())
}

/// blockhash, exact message, network fee, and the SOL balance check (`sol_out` = lamports leaving besides the fee)
async fn seal(http: &reqwest::Client, d: &mut Draft, payer: &solana_address::Address, ixs: &[solana_instruction::Instruction], sol_out: u64, rent0: u64, price: Option<u64>) -> Result<(), String> {
    let c = d.cluster;
    // priority (owner opened mainnet 08.10): a compute-unit cap sized to the transfer and a price from recent blocks,
    // so a busy network does not drop it. Cost ceiling: 80k CU x 1 lamport = 0.00008 SOL; the fee below includes it.
    let token = ixs.iter().any(|ix| ix.program_id != solana_system_interface::program::ID);
    let cu: u32 = if token { 40_000 * ixs.len().max(2) as u32 } else { 1_000 }; // 80k for a token send; more for several account creations
    let price = match price { Some(p) => p, None => priority_price(http, c).await };
    let budget = |data: Vec<u8>| solana_instruction::Instruction::new_with_bytes(COMPUTE_BUDGET.parse().expect("program id"), &data, vec![]);
    let all: Vec<solana_instruction::Instruction> = [budget([&[2u8][..], &cu.to_le_bytes()].concat()), budget([&[3u8][..], &price.to_le_bytes()].concat())]
        .into_iter().chain(ixs.iter().cloned()).collect();
    let bh = rpc(http, c, "getLatestBlockhash", json!([{ "commitment": "confirmed" }])).await?;
    let hash: solana_hash::Hash = bh["value"]["blockhash"].as_str().and_then(|s| s.parse().ok()).ok_or("bad blockhash answer")?;
    d.last_valid = bh["value"]["lastValidBlockHeight"].as_u64().ok_or("bad blockhash answer")?;
    d.message = solana_message::Message::new_with_blockhash(&all, Some(payer), &hash).serialize();
    let b64m = base64::engine::general_purpose::STANDARD.encode(&d.message);
    d.fee = value_u64(&rpc(http, c, "getFeeForMessage", json!([b64m, { "commitment": "confirmed" }])).await?)?;
    let bal = value_u64(&rpc(http, c, "getBalance", json!([d.from, { "commitment": "confirmed" }])).await?)?;
    let need = sol_out.checked_add(d.fee).ok_or("сумма слишком большая")?;
    if bal < need { return Err(format!("не хватает SOL: с комиссией нужно {}, на счёте {}", units(need, 9), units(bal, 9))) }
    let left = bal - need;
    if left != 0 && left < rent0 {
        return Err(format!("на счёте должно остаться 0 или не меньше {} SOL; можно отправить максимум {} SOL", units(rent0, 9), units(bal.saturating_sub(d.fee + rent0), 9)));
    }
    // dry run (Codex 08.10): the network executes this exact message once, unsigned. A transfer that would fail stops
    // here, before any password; a node that cannot simulate does not block the send, the confirmation says so.
    let wire = [&[1u8][..], &[0u8; 64][..], &d.message[..]].concat();
    match rpc(http, c, "simulateTransaction", json!([base64::engine::general_purpose::STANDARD.encode(wire), { "encoding": "base64", "sigVerify": false, "commitment": "confirmed" }])).await {
        Ok(r) if r["value"]["err"].is_null() => {}
        Ok(r) => return Err(format!("сеть отклонила бы этот перевод: {}", sim_reason(&r["value"]))),
        Err(_) => if d.warn.is_none() { d.warn = Some("Сеть не смогла проверить перевод заранее: он может не пройти.") },
    }
    Ok(())
}
pub(crate) const COMPUTE_BUDGET: &str = "ComputeBudget111111111111111111111111111111";
/// the priority price now (micro-lamports per CU), for plans that fix it up front
pub async fn sol_price(c: Cluster) -> Result<u64, String> { let http = client()?; guard(&http, c).await?; Ok(priority_price(&http, c).await) }
/// the network fee of one plain SOL transfer at `price`: base 5000 + ceil(1000 CU x price / 1e6) (the cap `seal` sets)
pub fn sol_transfer_fee(price: u64) -> u64 { 5000 + (1000 * price).div_ceil(1_000_000) }
/// confirmed SOL balance of an address, lamports
pub async fn balance(address: &str, c: Cluster) -> Result<u64, String> {
    if !valid_address(address) { return Err("это не адрес Solana".into()) }
    let http = client()?; guard(&http, c).await?;
    value_u64(&rpc(&http, c, "getBalance", json!([address, { "commitment": "confirmed" }])).await?)
}
/// the network's view of a signature right now: "pending" | "confirmed" | "finalized" | "failed" | "unknown"
pub async fn sig_state(sig: &str, c: Cluster) -> &'static str { sig_states(&[sig.to_string()], c).await[0] }
/// the same for up to 256 signatures in ONE call (an agent polling a 50-row batch must not burn the node's limit)
pub async fn sig_states(sigs: &[String], c: Cluster) -> Vec<&'static str> {
    let unknown = || vec!["unknown"; sigs.len()];
    if sigs.is_empty() || sigs.len() > 256 { return unknown() }
    let Ok(http) = client() else { return unknown() };
    let Ok(r) = rpc(&http, c, "getSignatureStatuses", json!([sigs, { "searchTransactionHistory": true }])).await else { return unknown() };
    let Some(a) = r.get("value").and_then(Value::as_array).filter(|a| a.len() == sigs.len()) else { return unknown() };
    a.iter().map(|one| match status(&json!({ "value": [one] })) {
        Some(Some(s)) if !s.ok => "failed",
        Some(Some(s)) if s.finalized => "finalized",
        Some(Some(s)) if s.confirmed => "confirmed",
        Some(_) => "pending",
        None => "unknown",
    }).collect()
}

/// micro-lamports per compute unit: the 75th percentile of recent non-zero priority fees, 1 000 .. 1 000 000
async fn priority_price(http: &reqwest::Client, c: Cluster) -> u64 {
    let mut fees: Vec<u64> = match rpc(http, c, "getRecentPrioritizationFees", json!([])).await {
        Ok(v) => v.as_array().into_iter().flatten().filter_map(|x| x["prioritizationFee"].as_u64()).filter(|f| *f > 0).collect(),
        Err(_) => vec![],
    };
    fees.sort_unstable();
    fees.get(fees.len() * 3 / 4).copied().unwrap_or(0).clamp(1_000, 1_000_000)
}
/// the most telling line of a failed dry run: the program's own error log if any, else the error object
fn sim_reason(v: &Value) -> String {
    let logs = v["logs"].as_array().into_iter().flatten().filter_map(Value::as_str);
    let line = logs.filter(|l| l.contains("Error") || l.contains("failed") || l.contains("insufficient")).last().map(str::to_string);
    let s = line.unwrap_or_else(|| v["err"].to_string());
    s.trim_start_matches("Program log: ").chars().take(160).collect()
}

/// the transaction id is the fee payer's signature: known before anything leaves the machine
pub fn tx_id(sig: &[u8; 64]) -> String { bs58::encode(sig).into_string() }

pub enum Sent { Ok, Unknown, Rejected(String) }
/// wire format for one signer: shortvec(1) + signature + message. A lost answer is NOT a failure: the network may have
/// taken it (security review 07.10), so the caller keeps tracking the locally known id instead of letting a retry pay twice.
pub async fn submit(d: &Draft, sig: &[u8; 64]) -> Sent {
    let wire = [&[1u8][..], sig, &d.message].concat();
    let Ok(http) = client() else { return Sent::Rejected("нет сетевого клиента".into()) };
    let b = base64::engine::general_purpose::STANDARD.encode(wire);
    match rpc_coded(&http, d.cluster, "sendTransaction", json!([b, { "encoding": "base64", "preflightCommitment": "confirmed" }])).await {
        Ok(r) if r.as_str() == Some(tx_id(sig).as_str()) => Sent::Ok,
        Ok(_) => Sent::Unknown, // a node answering with some other signature is not to be believed either way
        Err((code, e)) => classify(code, e),
    }
}

/// only explicit pre-broadcast refusals count as "not sent": preflight failure, bad signature, invalid params, wrong
/// signature count. Anything else (internal, unhealthy, no code) may have gone out (security reviews 07.10)
fn classify(code: Option<i64>, e: String) -> Sent {
    if matches!(code, Some(-32002 | -32003 | -32602 | -32013)) { Sent::Rejected(e) } else { Sent::Unknown }
}

/// one status entry, every field the node must send present and typed; anything less is "ask again"
struct Status { finalized: bool, confirmed: bool, ok: bool }
fn status(r: &Value) -> Option<Option<Status>> {
    let a = r.get("value")?.as_array()?;
    if a.len() != 1 { return None }
    let s = &a[0];
    if s.is_null() { return Some(None) } // the node has not seen it
    let o = s.as_object()?;
    o.get("slot")?.as_u64()?;
    let conf = o.get("confirmationStatus")?.as_str()?;
    let err = o.get("err")?; // must be present: null = success, anything else = failed
    if !o.contains_key("confirmations") { return None }
    Some(Some(Status { finalized: conf == "finalized", confirmed: conf == "confirmed" || conf == "finalized", ok: err.is_null() }))
}

pub enum Outcome { Confirmed, Failed, Expired, Unknown }

/// wait for the network's FINAL verdict (finalized commitment). `confirmed` is reported early through `on_confirmed`
/// but stays provisional: a fork can still undo it. No re-send: after blockhash expiry the signature can never land.
pub async fn track(sig: &str, c: Cluster, last_valid: u64, on_confirmed: impl Fn()) -> Outcome {
    let Ok(http) = client() else { return Outcome::Unknown };
    let mut told = false;
    for _ in 0..150 {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        if let Ok(r) = rpc(&http, c, "getSignatureStatuses", json!([[sig]])).await {
            let Some(st) = status(&r) else { continue }; // malformed: ask again
            if let Some(s) = st {
                if s.finalized { return if s.ok { Outcome::Confirmed } else { Outcome::Failed } }
                if s.confirmed && s.ok && !told { told = true; on_confirmed() }
                continue;
            }
            // not seen: past the FINALIZED last valid height it never will be; a successful full-history look decides,
            // a failed look proves nothing (security review 07.10)
            if let Ok(h) = rpc(&http, c, "getBlockHeight", json!([{ "commitment": "finalized" }])).await {
                if h.as_u64().is_some_and(|h| h > last_valid) {
                    let Ok(r) = rpc(&http, c, "getSignatureStatuses", json!([[sig], { "searchTransactionHistory": true }])).await else { return Outcome::Unknown };
                    match status(&r) {
                        None => return Outcome::Unknown,
                        Some(None) => return Outcome::Expired,
                        Some(Some(s)) if s.finalized => return if s.ok { Outcome::Confirmed } else { Outcome::Failed },
                        Some(Some(_)) => {} // seen, not final yet: keep watching
                    }
                }
            }
        }
    }
    Outcome::Unknown
}
#[derive(Serialize)]
pub struct Sig { pub sig: String, time: Option<i64>, ok: bool }

/// last signatures that touched an address (newest first). Amounts need the full transaction: later.
pub async fn history(address: &str, c: Cluster) -> Result<Vec<Sig>, String> {
    if !valid_address(address) { return Err("not a Solana address".into()) }
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
    if let Some(want) = c.genesis() {
        if rpc(&http, c, "getGenesisHash", json!([])).await?.as_str() != Some(want) { return Err("this RPC serves a different network than selected".into()) }
    }
    let r = rpc(&http, c, "getSignaturesForAddress", json!([address, { "limit": 25, "commitment": "confirmed" }])).await?;
    Ok(r.as_array().into_iter().flatten().filter_map(|x| Some(Sig {
        sig: x["signature"].as_str()?.to_string(), time: x["blockTime"].as_i64(), ok: x["err"].is_null(),
    })).collect())
}

// ---------------------------------------------------------------- swap through Jupiter, assembled and checked here
// Jupiter (keyless lite API) only proposes: a quote and the instructions of a route. The core keeps the trust boundary:
// every instruction is checked against an allowlist (no signer but us, no approvals, wraps only into our own wSOL
// account), compute budget is ours, lookup tables are read from the chain, the exact v0 message is dry-run with the
// accounts that matter, and the confirmation shows what the dry run saw arrive, never Jupiter's word alone.
use solana_address::Address;
const JUPITER: &str = "JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4";
const JUP_API: &str = "https://lite-api.jup.ag/swap/v1";
const WSOL_MINT: &str = "So11111111111111111111111111111111111111112";
const ATA_PROGRAM: &str = "ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL";
const ALT_PROGRAM: &str = "AddressLookupTab1e1111111111111111111111111";
/// the wallet's share of a swap (owner 09.10: 0.2%), paid to FEE_OWNER's token account of the input or output mint.
/// Only mints whose fee account already exists take the fee: «Подготовить» in Settings creates them once (prepare_fee_setup)
pub const FEE_BPS: u16 = 20;
/// the owner's «Комиссия» account (made 09.10 by the wallet's own code in his vault; the agent saw only this address)
pub const FEE_OWNER: Option<&str> = Some("9C8S2aM3bgTApyiZoSLhmcdDzhYiFdvbTLZdLbY4StBJ");
/// the token accounts the fee wallet receives into: wSOL, USDC, USDT (classic SPL)
const FEE_MINTS: [&str; 3] = [WSOL_MINT, "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB"];

/// fee accounts still missing (ATA addresses), mainnet
pub async fn fee_missing(c: Cluster) -> Result<Vec<(String, String)>, String> {
    let owner = FEE_OWNER.ok_or("кошелёк комиссии не задан")?;
    let http = client()?;
    guard(&http, c).await?;
    let mut out = Vec::new();
    for m in FEE_MINTS {
        let a = ata(owner, m, TOKEN).ok_or("bad address")?;
        if account(&http, c, &a).await?.is_none() { out.push((m.to_string(), a)) }
    }
    Ok(out)
}
/// one transaction from `from` creating the fee wallet's missing token accounts (idempotent; rent paid by `from`)
pub async fn prepare_fee_setup(from: &str, c: Cluster) -> Result<Draft, String> {
    let owner = FEE_OWNER.ok_or("кошелёк комиссии не задан")?;
    let missing = fee_missing(c).await?;
    if missing.is_empty() { return Err("счета для комиссии уже созданы".into()) }
    let http = client()?;
    let (fa, oa, tp) = (addr(from)?, addr(owner)?, addr(TOKEN)?);
    let ixs: Vec<solana_instruction::Instruction> = missing.iter().map(|(m, _)| Ok(spl_associated_token_account_interface::instruction::create_associated_token_account_idempotent(&p3(&fa), &p3(&oa), &p3(&addr(m)?), &p3(&tp))))
        .collect::<Result<_, String>>()?;
    let rent = rent_for(&http, c, 165).await? * missing.len() as u64;
    let rent0 = rent_for(&http, c, 0).await?;
    let mut d = Draft { from: from.into(), to: owner.into(), amount: rent, decimals: 9, mint: None, symbol: Some("SOL"), fee: 0, rent: 0, new_recipient: false,
        warn: Some("Создаёт у счёта «Комиссия» счета для приёма wSOL, USDC и USDT: туда будет приходить доля от обменов. SOL уходит на их хранение в сети."),
        cluster: c, message: vec![], last_valid: 0, swap: None };
    seal(&http, &mut d, &fa, &ixs, rent, rent0, None).await?;
    Ok(d)
}

// ---------------------------------------------------------------- permissions: who may spend our tokens
/// a token account of ours with a delegate (an «approve»): that address may move up to `amount` without asking
#[derive(Serialize)]
pub struct Grant { account: String, mint: String, delegate: String, amount: String, decimals: u8 }
/// every token account of `owner` that has a delegate. Layout (SPL Token and Token-2022 share it): mint 0..32,
/// owner 32..64, amount 64..72, delegate COption 72..108, state 108, is_native 109..121, delegated_amount 121..129
pub async fn grants(owner: &str, c: Cluster) -> Result<Vec<Grant>, String> {
    if !valid_address(owner) { return Err("это не адрес Solana".into()) }
    let http = client()?;
    guard_read(&http, c).await?;
    let mut out = vec![];
    for program in [TOKEN, TOKEN_2022] {
        // no silent skip (Codex 09.10): a list missing the Token-2022 half would read as «nobody may spend»
        let r = rpc(&http, c, "getTokenAccountsByOwner", json!([owner, { "programId": program }, { "encoding": "base64", "commitment": "confirmed" }])).await?;
        for acc in r.get("value").and_then(Value::as_array).into_iter().flatten() {
            let Some(d) = b64(&acc["account"]["data"]).filter(|d| d.len() >= 129) else { continue };
            if d[72..76] != [1, 0, 0, 0] || bs58::encode(&d[32..64]).into_string() != owner { continue }
            let amount = u64::from_le_bytes(d[121..129].try_into().unwrap());
            out.push(Grant { account: acc["pubkey"].as_str().unwrap_or("").into(), mint: bs58::encode(&d[0..32]).into_string(), delegate: bs58::encode(&d[76..108]).into_string(), amount: amount.to_string(), decimals: 0 });
        }
    }
    let mints: Vec<&str> = out.iter().map(|g| g.mint.as_str()).collect();
    let mut dec = std::collections::HashMap::new();
    for chunk in mints.chunks(100) {
        let r = rpc(&http, c, "getMultipleAccounts", json!([chunk, { "encoding": "base64" }])).await?;
        for (m, a) in chunk.iter().zip(r.get("value").and_then(Value::as_array).into_iter().flatten()) { if let Some(d) = b64(&a["data"]).filter(|d| d.len() > 44) { dec.insert(m.to_string(), d[44]); } }
    }
    for g in &mut out { g.decimals = *dec.get(&g.mint).unwrap_or(&0) }
    Ok(out)
}
/// one Revoke for one of our token accounts, built from the chain's own record of it (owner, program, delegate)
pub async fn prepare_revoke(owner: &str, token_account: &str, c: Cluster) -> Result<Draft, String> {
    let http = client()?;
    guard(&http, c).await?;
    let raw = account(&http, c, token_account).await?.ok_or("такого счёта токена нет")?;
    if raw.owner != TOKEN && raw.owner != TOKEN_2022 { return Err("это не счёт токена".into()) }
    let d = &raw.data;
    if d.len() < 129 || bs58::encode(&d[32..64]).into_string() != owner { return Err("этот счёт токена принадлежит не вам".into()) }
    if d[72..76] != [1, 0, 0, 0] { return Err("у этого счёта уже нет разрешения на траты".into()) }
    let (oa, ta, prog) = (addr(owner)?, addr(token_account)?, addr(&raw.owner)?);
    let mint = bs58::encode(&d[0..32]).into_string();
    let ix = spl_token_2022_interface::instruction::revoke(&prog, &ta, &oa, &[]).map_err(|_| "не удалось собрать отзыв".to_string())?;
    let rent0 = rent_for(&http, c, 0).await?;
    let mut dr = Draft { from: owner.into(), to: bs58::encode(&d[76..108]).into_string(), amount: 0, decimals: 0, mint: Some(mint.clone()),
        symbol: KNOWN.iter().find(|k| k.0 == mint).map(|k| k.1), fee: 0, rent: 0, new_recipient: false, warn: None, swap: None, cluster: c, message: vec![], last_valid: 0 };
    seal(&http, &mut dr, &oa, &[ix], 0, rent0, None).await?;
    Ok(dr)
}

#[derive(Serialize, Clone, Debug)]
pub struct SwapView { pub out_mint: String, pub out_amount: u64, pub min_out: u64, pub out_decimals: u8, pub route: String, pub fee_bps: u16 }

#[derive(Serialize)]
pub struct Quote { in_amount: String, out_amount: String, min_out: String, out_decimals: u8, impact: String, route: String, fee_bps: u16 }

fn mint_or_sol(m: &str) -> &str { if m == "SOL" { WSOL_MINT } else { m } }
async fn get_json(http: &reqwest::Client, url: &str) -> Result<Value, String> {
    let r = http.get(url).send().await.map_err(|e| net(&e))?;
    let ok = r.status().is_success();
    let v: Value = r.json().await.map_err(|_| "сервис обмена ответил не тем".to_string())?;
    if !ok { return Err(format!("сервис обмена: {}", v["error"].as_str().unwrap_or("отказ"))) }
    Ok(v)
}
async fn mint_decimals(http: &reqwest::Client, c: Cluster, mint: &str) -> Result<(u8, String), String> {
    if mint == "SOL" || mint == WSOL_MINT { return Ok((9, TOKEN.into())) }
    let r = rpc(http, c, "getAccountInfo", json!([mint, { "encoding": "base64" }])).await?;
    let d = b64(&r["value"]["data"]).filter(|d| d.len() > 44).ok_or("такого токена нет")?;
    Ok((d[44], r["value"]["owner"].as_str().unwrap_or(TOKEN).to_string()))
}
fn ata(owner: &str, mint: &str, program: &str) -> Option<String> {
    let (o, m, p): (pubkey3::Pubkey, pubkey3::Pubkey, pubkey3::Pubkey) = (owner.parse().ok()?, mint.parse().ok()?, program.parse().ok()?);
    Some(spl_associated_token_account_interface::address::get_associated_token_address_with_program_id(&o, &m, &p).to_string())
}
/// the fee account Jupiter pays our share into: FEE_OWNER's existing classic-token account of the input, else output mint
async fn fee_account(http: &reqwest::Client, c: Cluster, input: &str, output: &str) -> Option<String> {
    let owner = FEE_OWNER?;
    for m in [input, output] {
        let a = ata(owner, m, TOKEN)?;
        let r = rpc(http, c, "getAccountInfo", json!([a, { "encoding": "base64" }])).await.ok()?;
        if r["value"]["owner"].as_str() == Some(TOKEN) { return Some(a) }
    }
    None
}

async fn quote_raw(http: &reqwest::Client, input: &str, output: &str, amount: u64, fee: bool) -> Result<Value, String> {
    let fee = if fee { format!("&platformFeeBps={FEE_BPS}") } else { String::new() };
    get_json(http, &format!("{JUP_API}/quote?inputMint={}&outputMint={}&amount={amount}&slippageBps=50&restrictIntermediateTokens=true{fee}", mint_or_sol(input), mint_or_sol(output))).await
}
fn route_of(q: &Value) -> String {
    let labels: Vec<&str> = q["routePlan"].as_array().into_iter().flatten().filter_map(|r| r["swapInfo"]["label"].as_str()).collect();
    labels.join(" → ").chars().take(80).collect()
}

/// a live quote for the swap screen; nothing is built or kept
// ---------------------------------------------------------------- tokens to swap into (owner 10.10: «as many as possible, like Phantom»)
/// one token of Jupiter's index, trimmed to what the picker shows. `verified` = Jupiter's own verification; anything else
/// is shown with its mint and a warning, never as the name it gave itself alone
#[derive(Serialize)]
pub struct TokenInfo { id: String, symbol: String, name: String, icon: Option<String>, decimals: u8, verified: bool, price: Option<f64>, liquidity: Option<f64> }
fn token_info(v: &Value) -> Option<TokenInfo> {
    let id = v["id"].as_str().filter(|m| valid_address(m))?.to_string();
    let clip = |s: &str, n: usize| s.chars().filter(|c| !c.is_control()).take(n).collect::<String>();
    Some(TokenInfo {
        symbol: clip(v["symbol"].as_str()?, 12), name: clip(v["name"].as_str().unwrap_or(""), 40),
        icon: v["icon"].as_str().filter(|u| u.starts_with("https://")).map(str::to_string),
        decimals: v["decimals"].as_u64().filter(|d| *d <= 18)? as u8,
        verified: v["isVerified"].as_bool() == Some(true) || v["tags"].as_array().is_some_and(|t| t.iter().any(|x| x == "verified")),
        price: v["usdPrice"].as_f64().filter(|p| p.is_finite() && *p > 0.0), liquidity: v["liquidity"].as_f64(),
        id,
    })
}
/// the tokens people trade most today (empty query) or a search by name, ticker or mint. What leaves: the query text.
/// Thin pools (< $10k) are dropped unless the query is that exact mint: a fake can be pushed to any price there
pub async fn tokens(query: &str) -> Result<Vec<TokenInfo>, String> {
    let q = query.trim();
    let http = client()?;
    let url = if q.is_empty() { "https://lite-api.jup.ag/tokens/v2/toporganicscore/24h?limit=100".to_string() }
        else { format!("https://lite-api.jup.ag/tokens/v2/search?query={}", q.chars().filter(|c| c.is_ascii_alphanumeric() || " .-_$".contains(*c)).take(60).collect::<String>().replace(' ', "%20")) };
    let v = get_json(&http, &url).await?;
    Ok(v.as_array().into_iter().flatten().filter_map(token_info)
        .filter(|t| t.id == q || t.liquidity.unwrap_or(0.0) >= 10_000.0).collect())
}
/// a token's picture for the picker, fetched by the core (the page may load nothing from the web): https only, an image,
/// at most 96 KB; kept in memory for the session
pub async fn token_icon(url: &str) -> Result<String, String> {
    use base64::Engine as _;
    static CACHE: std::sync::Mutex<Option<std::collections::HashMap<String, String>>> = std::sync::Mutex::new(None);
    if let Some(d) = CACHE.lock().unwrap().as_ref().and_then(|m| m.get(url).cloned()) { return Ok(d) }
    if !url.starts_with("https://") || url.len() > 400 { return Err("not an icon url".into()) }
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(8)).build().map_err(|e| e.to_string())?;
    let r = http.get(url).send().await.map_err(|_| "icon".to_string())?;
    let ct = r.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
    if !ct.starts_with("image/") || ct.contains("svg") { return Err("not an image".into()) } // no SVG: it can carry script
    let b = r.bytes().await.map_err(|_| "icon".to_string())?;
    if b.len() > 96 * 1024 { return Err("icon too big".into()) }
    let d = format!("data:{};base64,{}", ct.split(';').next().unwrap_or("image/png"), base64::engine::general_purpose::STANDARD.encode(&b));
    CACHE.lock().unwrap().get_or_insert_with(Default::default).insert(url.to_string(), d.clone());
    Ok(d)
}

pub async fn swap_quote(input: &str, output: &str, amount: &str, c: Cluster) -> Result<Quote, String> {
    if !matches!(c, Cluster::Mainnet) { return Err("обмен работает только в основной сети".into()) }
    if input == output { return Err("выберите другой токен".into()) }
    let http = client()?;
    let (din, _) = mint_decimals(&http, c, input).await?;
    let (dout, _) = mint_decimals(&http, c, output).await?;
    let amt = parse_amount(amount, din as u32)?;
    let fee = fee_account(&http, c, mint_or_sol(input), mint_or_sol(output)).await.is_some();
    let q = quote_raw(&http, input, output, amt, fee).await?;
    Ok(Quote {
        in_amount: units(amt, din as u32), out_amount: units(q["outAmount"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0), dout as u32),
        min_out: units(q["otherAmountThreshold"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0), dout as u32), out_decimals: dout,
        impact: q["priceImpactPct"].as_str().unwrap_or("0").chars().take(8).collect(), route: route_of(&q), fee_bps: if fee { FEE_BPS } else { 0 },
    })
}

fn ix_of(v: &Value) -> Result<solana_instruction::Instruction, String> {
    let bad = || "сервис обмена прислал странный шаг".to_string();
    let program: Address = v["programId"].as_str().and_then(|s| s.parse().ok()).ok_or_else(bad)?;
    let accounts = v["accounts"].as_array().ok_or_else(bad)?.iter().map(|a| Ok(solana_instruction::AccountMeta {
        pubkey: a["pubkey"].as_str().and_then(|s| s.parse().ok()).ok_or_else(bad)?,
        is_signer: a["isSigner"].as_bool().ok_or_else(bad)?, is_writable: a["isWritable"].as_bool().ok_or_else(bad)?,
    })).collect::<Result<Vec<_>, String>>()?;
    let data = base64::engine::general_purpose::STANDARD.decode(v["data"].as_str().ok_or_else(bad)?).map_err(|_| bad())?;
    Ok(solana_instruction::Instruction { program_id: program, accounts, data })
}

/// the allowlist. Returns the checked instructions in order (Jupiter's compute budget dropped: ours goes first).
pub(crate) fn vet(r: &Value, user: &str) -> Result<Vec<solana_instruction::Instruction>, String> {
    if !r["tokenLedgerInstruction"].is_null() { return Err("сервис обмена предложил необычный маршрут; отказ".into()) }
    let u: Address = user.parse().map_err(|_| "bad user")?;
    let wsol = ata(user, WSOL_MINT, TOKEN).and_then(|a| a.parse::<Address>().ok()).ok_or("bad wsol")?;
    let list = |k: &str| r[k].as_array().cloned().unwrap_or_default();
    let mut ixs = Vec::new();
    for v in list("setupInstructions").iter().chain(std::iter::once(&r["swapInstruction"])).chain(r["cleanupInstruction"].as_object().map(|_| &r["cleanupInstruction"])).chain(list("otherInstructions").iter()) {
        let ix = ix_of(v)?;
        if ix.accounts.iter().any(|a| a.is_signer && a.pubkey != u) { return Err("шаг обмена просит чужую подпись; отказ".into()) }
        let p = ix.program_id.to_string();
        let ok = match p.as_str() {
            JUPITER => true, // the route itself: its effect is what the dry run measures
            ATA_PROGRAM => ix.accounts.get(2).is_some_and(|a| a.pubkey == u) && ix.accounts.first().is_some_and(|a| a.pubkey == u), // our own account, paid by us
            TOKEN | TOKEN_2022 => match ix.data.first() {
                Some(17) => true, // SyncNative: wrapped SOL counts its lamports
                Some(9) => ix.accounts.get(1).is_some_and(|a| a.pubkey == u) && ix.accounts.get(2).is_some_and(|a| a.pubkey == u), // close, rent back to us
                _ => false, // no approve, no set-authority, no transfer outside the route
            },
            "11111111111111111111111111111111" => ix.data.first_chunk::<4>() == Some(&2u32.to_le_bytes()) && ix.accounts.first().is_some_and(|a| a.pubkey == u) && ix.accounts.get(1).is_some_and(|a| a.pubkey == wsol), // wrap: into our wSOL only
            _ => false,
        };
        if !ok { return Err(format!("шаг обмена вне разрешённого списка ({}); отказ", &p[..8.min(p.len())])) }
        ixs.push(ix);
    }
    if ixs.iter().filter(|ix| ix.program_id.to_string() == JUPITER).count() != 1 { return Err("в обмене должен быть ровно один шаг маршрута".into()) }
    Ok(ixs)
}

/// everything the confirmation needs, built and dry-run; the caller keeps the Draft for one signature
pub async fn prepare_swap(user: &str, input: &str, output: &str, amount: &str, c: Cluster) -> Result<Draft, String> {
    if !matches!(c, Cluster::Mainnet) { return Err("обмен работает только в основной сети".into()) }
    if !valid_address(user) || input == output { return Err("выберите другой токен".into()) }
    let http = client()?;
    guard(&http, c).await?;
    let (din, pin) = mint_decimals(&http, c, input).await?;
    let (dout, pout) = mint_decimals(&http, c, output).await?;
    let amt = parse_amount(amount, din as u32)?;
    let fee_acc = fee_account(&http, c, mint_or_sol(input), mint_or_sol(output)).await;
    let q = quote_raw(&http, input, output, amt, fee_acc.is_some()).await?;
    let min_out: u64 = q["otherAmountThreshold"].as_str().and_then(|s| s.parse().ok()).ok_or("сервис обмена не назвал минимум")?;
    let mut body = json!({ "quoteResponse": q, "userPublicKey": user, "wrapAndUnwrapSol": true, "dynamicComputeUnitLimit": true });
    if let Some(f) = &fee_acc { body["feeAccount"] = json!(f) }
    let resp = http.post(format!("{JUP_API}/swap-instructions")).json(&body).send().await.map_err(|e| net(&e))?;
    let r: Value = resp.json().await.map_err(|_| "сервис обмена ответил не тем".to_string())?;
    if let Some(e) = r["error"].as_str() { return Err(format!("сервис обмена: {}", e.chars().take(140).collect::<String>())) }
    let route = vet(&r, user)?;
    // lookup tables: read from the chain, owned by the lookup-table program, addresses after the 56-byte header
    let tables: Vec<&str> = r["addressLookupTableAddresses"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
    let mut alts = Vec::new();
    if !tables.is_empty() {
        let t = rpc(&http, c, "getMultipleAccounts", json!([tables, { "encoding": "base64" }])).await?;
        for (k, acc) in tables.iter().zip(t["value"].as_array().into_iter().flatten()) {
            if acc["owner"].as_str() != Some(ALT_PROGRAM) { return Err("таблица адресов обмена не та; отказ".into()) }
            let d = b64(&acc["data"]).filter(|d| d.len() >= 56 && (d.len() - 56) % 32 == 0).ok_or("таблица адресов обмена повреждена")?;
            alts.push(solana_message::AddressLookupTableAccount {
                key: k.parse().map_err(|_| "bad table")?,
                addresses: d[56..].chunks(32).map(|x| Address::new_from_array(x.try_into().unwrap())).collect(),
            });
        }
    }
    // our compute budget: Jupiter's measured limit (capped) and our price
    let cu = r["computeUnitLimit"].as_u64().unwrap_or(400_000).clamp(100_000, 1_400_000) as u32;
    let price = priority_price(&http, c).await;
    let budget = |data: Vec<u8>| solana_instruction::Instruction::new_with_bytes(COMPUTE_BUDGET.parse().expect("program id"), &data, vec![]);
    let all: Vec<_> = [budget([&[2u8][..], &cu.to_le_bytes()].concat()), budget([&[3u8][..], &price.to_le_bytes()].concat())].into_iter().chain(route).collect();
    let bh = rpc(&http, c, "getLatestBlockhash", json!([{ "commitment": "confirmed" }])).await?;
    let hash: solana_hash::Hash = bh["value"]["blockhash"].as_str().and_then(|s| s.parse().ok()).ok_or("bad blockhash answer")?;
    let payer: Address = user.parse().map_err(|_| "bad user")?;
    let message = solana_message::v0::Message::try_compile(&payer, &all, &alts, hash).map_err(|_| "обмен не собрался в одну транзакцию".to_string())?.serialize();
    if message.len() + 65 > 1232 { return Err("маршрут обмена слишком длинный для одной транзакции".into()) }
    let fee = value_u64(&rpc(&http, c, "getFeeForMessage", json!([base64::engine::general_purpose::STANDARD.encode(&message), { "commitment": "confirmed" }])).await?)?;
    // dry run with the accounts that matter: our SOL and our token accounts of both sides, before and after
    let side = |m: &str, prog: &str| if m == "SOL" || m == WSOL_MINT { None } else { ata(user, m, prog) };
    let (ain, aout) = (side(input, &pin), side(output, &pout));
    let watch: Vec<String> = [Some(user.to_string()), ain.clone(), aout.clone()].into_iter().flatten().collect();
    let pre = rpc(&http, c, "getMultipleAccounts", json!([watch, { "encoding": "base64", "commitment": "confirmed" }])).await?;
    let wire = [&[1u8][..], &[0u8; 64][..], &message[..]].concat();
    let sim = rpc(&http, c, "simulateTransaction", json!([base64::engine::general_purpose::STANDARD.encode(wire),
        { "encoding": "base64", "sigVerify": false, "commitment": "confirmed", "accounts": { "addresses": watch, "encoding": "base64" } }])).await?;
    let v = &sim["value"];
    if !v["err"].is_null() { return Err(format!("сеть отклонила бы этот обмен: {}", sim_reason(v))) }
    let lamports = |x: &Value| x["lamports"].as_u64().unwrap_or(0);
    let tokens = |x: &Value| b64(&x["data"]).filter(|d| d.len() >= 72).map(|d| u64::from_le_bytes(d[64..72].try_into().unwrap())).unwrap_or(0);
    let (pre, post) = (pre["value"].as_array().cloned().unwrap_or_default(), v["accounts"].as_array().cloned().unwrap_or_default());
    if post.len() != watch.len() { return Err("сеть не показала итог обмена; отказ".into()) }
    let at = |l: &[Value], i: usize| l.get(i).cloned().unwrap_or(Value::Null);
    let out_idx = if aout.is_some() { watch.len() - 1 } else { 0 };
    let got = if aout.is_some() { tokens(&at(&post, out_idx)).saturating_sub(tokens(&at(&pre, out_idx))) }
              else { (lamports(&at(&post, 0)) + fee).saturating_sub(lamports(&at(&pre, 0))) }; // SOL out: lamports arrive, fee was taken
    if got < min_out { return Err(format!("по проверке придёт меньше обещанного минимума ({} вместо {}); отказ", units(got, dout as u32), units(min_out, dout as u32))) }
    let bal = lamports(&at(&pre, 0));
    if bal < fee + if input == "SOL" { amt } else { 0 } { return Err("не хватает SOL на обмен и комиссию".into()) }
    let mut d = Draft { from: user.into(), to: format!("Jupiter · {}", route_of(&q)), amount: amt, decimals: din, mint: (input != "SOL").then(|| input.into()),
        symbol: None, fee, rent: 0, new_recipient: false, warn: None, cluster: c, message, last_valid: bh["value"]["lastValidBlockHeight"].as_u64().ok_or("bad blockhash answer")?,
        swap: Some(SwapView { out_mint: output.into(), out_amount: got, min_out, out_decimals: dout, route: route_of(&q), fee_bps: if fee_acc.is_some() { FEE_BPS } else { 0 } }) };
    if input == "SOL" { d.symbol = Some("SOL") }
    Ok(d)
}

// ---------------------------------------------------------------- one history entry, read from the chain itself
#[derive(Serialize, Debug, PartialEq)]
pub struct Move { mint: Option<String>, amount: String, decimals: u8, incoming: bool } // mint None = SOL; amount = base units
#[derive(Serialize, Debug)]
pub struct Tx { sig: String, time: Option<i64>, ok: bool, fee: Option<String>, moves: Vec<Move>, peer: Option<String> }

/// what one transaction did to `address`: balance changes from the node's own pre/post balances (no instruction
/// parsing, so swaps and unknown programs read right too), the fee if we paid it, and the counterparty when exactly
/// one other account changed by the opposite amount
pub fn summarize(sig: &str, r: &Value, address: &str) -> Result<Tx, String> {
    let meta = r.get("meta").filter(|m| !m.is_null()).ok_or("transaction has no status yet")?;
    let msg = &r["transaction"]["message"];
    let keys: Vec<&str> = msg["accountKeys"].as_array().into_iter().flatten()
        .chain(meta["loadedAddresses"]["writable"].as_array().into_iter().flatten())
        .chain(meta["loadedAddresses"]["readonly"].as_array().into_iter().flatten())
        .filter_map(Value::as_str).collect();
    let n = |v: &Value, i: usize| v.get(i).and_then(Value::as_u64).map(i128::from);
    let (pre, post) = (&meta["preBalances"], &meta["postBalances"]);
    let fee = meta["fee"].as_u64().unwrap_or(0) as i128;
    // SOL change of account i, the fee taken back out for the payer (index 0): the fee is shown on its own
    let sol = |i: usize| Some(n(post, i)? - n(pre, i)? + if i == 0 { fee } else { 0 });
    let me = keys.iter().position(|k| *k == address);
    let mut moves = Vec::new();
    let mut peer = None;
    if let Some(i) = me {
        if let Some(d) = sol(i).filter(|d| *d != 0) {
            moves.push(Move { mint: None, amount: d.unsigned_abs().to_string(), decimals: 9, incoming: d > 0 });
            let others: Vec<usize> = (0..keys.len()).filter(|&j| j != i && sol(j) == Some(-d)).collect();
            if let [j] = others[..] { peer = Some(keys[j].to_string()) }
        }
    }
    // tokens: per (owner, mint), post minus pre over every token account of that owner
    let tok = |side: &Value| -> Vec<(String, String, i128, u8)> {
        side.as_array().into_iter().flatten().filter_map(|b| Some((
            b["owner"].as_str()?.to_string(), b["mint"].as_str()?.to_string(),
            b["uiTokenAmount"]["amount"].as_str()?.parse::<i128>().ok()?, b["uiTokenAmount"]["decimals"].as_u64()? as u8,
        ))).collect()
    };
    let (tpre, tpost) = (tok(&meta["preTokenBalances"]), tok(&meta["postTokenBalances"]));
    let mut deltas: Vec<(String, String, i128, u8)> = Vec::new();
    for (o, m, a, d) in tpost.into_iter().chain(tpre.into_iter().map(|(o, m, a, d)| (o, m, -a, d))) {
        match deltas.iter_mut().find(|x| x.0 == o && x.1 == m) { Some(x) => x.2 += a, None => deltas.push((o, m, a, d)) }
    }
    for (o, m, a, d) in deltas.iter().filter(|x| x.0 == address && x.2 != 0) {
        moves.push(Move { mint: Some(m.clone()), amount: a.unsigned_abs().to_string(), decimals: *d, incoming: *a > 0 });
        let others: Vec<&String> = deltas.iter().filter(|x| x.0 != *o && x.1 == *m && x.2 == -a).map(|x| &x.0).collect();
        if let [p] = others[..] { peer = Some(p.clone()) }
    }
    let paid = me == Some(0);
    Ok(Tx { sig: sig.into(), time: r["blockTime"].as_i64(), ok: meta["err"].is_null(), fee: paid.then(|| fee.to_string()), moves, peer })
}

pub async fn tx(sig: &str, address: &str, c: Cluster) -> Result<Tx, String> {
    if !valid_address(address) || bs58::decode(sig).into_vec().map_or(true, |v| v.len() != 64) { return Err("bad signature or address".into()) }
    let http = client()?;
    guard_read(&http, c).await?;
    let r = rpc(&http, c, "getTransaction", json!([sig, { "encoding": "json", "commitment": "confirmed", "maxSupportedTransactionVersion": 0 }])).await?;
    if r.is_null() { return Err("the network does not know this transaction yet".into()) }
    summarize(sig, &r, address)
}

/// solscan page of a transaction; the core builds the URL, the UI never hands one in
pub fn explorer(sig: &str, c: Cluster) -> Result<String, String> {
    if bs58::decode(sig).into_vec().map_or(true, |v| v.len() != 64) { return Err("bad signature".into()) }
    match c { Cluster::Mainnet => Ok(format!("https://solscan.io/tx/{sig}")), Cluster::Devnet => Ok(format!("https://solscan.io/tx/{sig}?cluster=devnet")), Cluster::Local => Err("no explorer for a local validator".into()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn program_ids() {
        assert_eq!(TOKEN, spl_token_2022_interface::inline_spl_token::ID.to_string());
        assert_eq!(TOKEN_2022, spl_token_2022_interface::ID.to_string());
    }

    #[test]
    fn addresses() {
        assert!(valid_address("11111111111111111111111111111111"));
        assert!(valid_address(TOKEN));
        assert!(!valid_address("0xcbe50f29436d87a1cbe50f29436d87a1cbe50f29"));
        assert!(!valid_address("abc"));
        assert!(Cluster::parse("testnet").is_err());
    }

    #[test]
    fn swap_allowlist() {
        let user = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU";
        let other = "HAgk14JpMQLgt6rVgv7cBQFJWFto5Dqxi472uT3DKpqk";
        let wsol = ata(user, WSOL_MINT, TOKEN).unwrap();
        let acc = |k: &str, s: bool, w: bool| json!({ "pubkey": k, "isSigner": s, "isWritable": w });
        let ix = |p: &str, accs: Vec<Value>, data: Vec<u8>| json!({ "programId": p, "accounts": accs, "data": base64::engine::general_purpose::STANDARD.encode(data) });
        let route = ix(JUPITER, vec![acc(user, true, true), acc(other, false, true)], vec![1, 2, 3]);
        let wrap = ix("11111111111111111111111111111111", vec![acc(user, true, true), acc(&wsol, false, true)], [&2u32.to_le_bytes()[..], &5u64.to_le_bytes()].concat());
        let ok = json!({ "tokenLedgerInstruction": null,
            "setupInstructions": [ix(ATA_PROGRAM, vec![acc(user, true, true), acc(&wsol, false, true), acc(user, false, false)], vec![1]), wrap, ix(TOKEN, vec![acc(&wsol, false, true)], vec![17])],
            "swapInstruction": route, "cleanupInstruction": ix(TOKEN, vec![acc(&wsol, false, true), acc(user, false, true), acc(user, true, false)], vec![9]), "otherInstructions": [] });
        assert_eq!(vet(&ok, user).unwrap().len(), 5);
        let with = |k: &str, v: Value| { let mut x = ok.clone(); x[k] = v; x };
        // SOL sent anywhere but our own wSOL account
        let steal = ix("11111111111111111111111111111111", vec![acc(user, true, true), acc(other, false, true)], [&2u32.to_le_bytes()[..], &5u64.to_le_bytes()].concat());
        assert!(vet(&with("setupInstructions", json!([steal])), user).is_err());
        // a token approval (delegate) slipped into setup
        assert!(vet(&with("setupInstructions", json!([ix(TOKEN, vec![acc(&wsol, false, true), acc(other, false, false), acc(user, true, false)], vec![4, 1, 0, 0, 0, 0, 0, 0, 0])])), user).is_err());
        // somebody else's signature, an unknown program, two routes, a token ledger
        assert!(vet(&with("swapInstruction", ix(JUPITER, vec![acc(other, true, true)], vec![1])), user).is_err());
        assert!(vet(&with("otherInstructions", json!([ix("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr", vec![], vec![])])), user).is_err());
        assert!(vet(&with("otherInstructions", json!([ok["swapInstruction"].clone()])), user).is_err());
        assert!(vet(&with("tokenLedgerInstruction", json!({})), user).is_err());
        // closing our wSOL with the rent going elsewhere
        assert!(vet(&with("cleanupInstruction", ix(TOKEN, vec![acc(&wsol, false, true), acc(other, false, true), acc(user, true, false)], vec![9])), user).is_err());
    }

    #[test]
    fn dry_run_reason() {
        let v = json!({ "err": { "InstructionError": [0, { "Custom": 1 }] }, "logs": ["Program Tokenkeg invoke [1]", "Program log: Error: insufficient funds", "Program Tokenkeg failed: custom program error: 0x1"] });
        assert_eq!(sim_reason(&v), "Program Tokenkeg failed: custom program error: 0x1");
        assert_eq!(sim_reason(&json!({ "err": "AccountNotFound", "logs": [] })), "\"AccountNotFound\"");
    }

    #[test]
    fn token_names() {
        let s = |t: &str| [(t.len() as u32).to_le_bytes().to_vec(), t.as_bytes().to_vec()].concat();
        let mut mp = vec![4u8]; mp.extend([0u8; 64]); mp.extend(s("Bonk\0\0\0")); mp.extend(s("BONK\n")); mp.extend(s("https://x"));
        assert_eq!(metaplex_names(&mp), Some(("Bonk".into(), "BONK".into())), "padding and control chars dropped");
        mp[0] = 1;
        assert_eq!(metaplex_names(&mp), None, "not a metadata account");
        let mut huge = vec![4u8]; huge.extend([0u8; 64]); huge.extend(u32::MAX.to_le_bytes());
        assert_eq!(metaplex_names(&huge), None, "length out of bounds");
        // Token-2022 mint: 165 base bytes, account type 1, TLV: an unrelated extension, then TokenMetadata (19)
        let meta = [vec![0u8; 64], s("My Token"), s("MYT"), s("")].concat();
        let mut mint = vec![0u8; 165]; mint.push(1);
        mint.extend(3u16.to_le_bytes()); mint.extend(2u16.to_le_bytes()); mint.extend([9, 9]);
        mint.extend(19u16.to_le_bytes()); mint.extend((meta.len() as u16).to_le_bytes()); mint.extend(&meta);
        assert_eq!(t22_names(&mint), Some(("My Token".into(), "MYT".into())));
        assert_eq!(t22_names(&mint[..170]), None, "truncated");
        // the well-known Metaplex PDA of the USDC mint
        assert_eq!(metadata_pda("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v").as_deref(), Some("5x38Kp4hvdomTCnCrAny4UtMUt5rQBdB6px2K1Ui45Wq"));
    }

    #[test]
    fn history_entries() {
        let tb = |i: u64, owner: &str, mint: &str, amount: &str| json!({ "accountIndex": i, "owner": owner, "mint": mint, "uiTokenAmount": { "amount": amount, "decimals": 6 } });
        // we (index 0) send 1 SOL to B, fee 5000
        let send = json!({ "blockTime": 7, "transaction": { "message": { "accountKeys": ["ME", "B", "11111111111111111111111111111111"] } },
            "meta": { "err": null, "fee": 5000, "preBalances": [3_000_005_000u64, 0, 1], "postBalances": [2_000_000_000u64, 1_000_000_000u64, 1], "preTokenBalances": [], "postTokenBalances": [] } });
        let t = summarize("s", &send, "ME").unwrap();
        assert_eq!(t.moves, vec![Move { mint: None, amount: "1000000000".into(), decimals: 9, incoming: false }]);
        assert_eq!((t.peer.as_deref(), t.fee.as_deref(), t.ok), (Some("B"), Some("5000"), true));
        // the same seen from B: incoming, peer = ME, no fee of ours
        let t = summarize("s", &send, "B").unwrap();
        assert!(t.moves[0].incoming && t.peer.as_deref() == Some("ME") && t.fee.is_none());
        // token send that also creates the recipient's account: the token peer is the owner, rent is a SOL move
        let tok = json!({ "transaction": { "message": { "accountKeys": ["ME", "ATA_B", "ATA_ME"] } },
            "meta": { "err": null, "fee": 5000, "preBalances": [10_000_000u64, 0, 2_039_280], "postBalances": [7_960_720u64, 2_039_280, 2_039_280],
              "preTokenBalances": [tb(2, "ME", "USDC", "5000000")], "postTokenBalances": [tb(2, "ME", "USDC", "3000000"), tb(1, "B", "USDC", "2000000")] } });
        let t = summarize("s", &tok, "ME").unwrap();
        assert_eq!(t.moves[1], Move { mint: Some("USDC".into()), amount: "2000000".into(), decimals: 6, incoming: false });
        assert_eq!(t.peer.as_deref(), Some("B"));
        // a swap: one token out, another in, nobody single to name; a failed one still reports ok = false
        let swap = json!({ "transaction": { "message": { "accountKeys": ["ME", "POOL"] } },
            "meta": { "err": { "InstructionError": [0, "Custom"] }, "fee": 5000, "preBalances": [10_000, 0], "postBalances": [5_000, 0],
              "preTokenBalances": [tb(1, "ME", "A", "10"), tb(1, "POOL", "B", "50")], "postTokenBalances": [tb(1, "ME", "A", "0"), tb(1, "ME", "B", "20"), tb(1, "POOL", "B", "30"), tb(1, "POOL", "A", "10")] } });
        let t = summarize("s", &swap, "ME").unwrap();
        assert_eq!((t.moves.len(), t.ok), (2, false));
        assert!(summarize("s", &json!({ "meta": null }), "ME").is_err());
        assert!(explorer("s", Cluster::Mainnet).is_err());
    }

    #[test]
    fn own_node() {
        assert_eq!(host("https://mainnet.helius-rpc.com/?api-key=SECRET").as_deref(), Some("mainnet.helius-rpc.com"));
        assert_eq!(host("https://u:SECRET@node.example/v2/SECRET").as_deref(), Some("node.example"));
        let check = |c, u| tauri::async_runtime::block_on(check_own(c, u));
        assert!(check(Cluster::Mainnet, "http://api.mainnet-beta.solana.com").is_err(), "plain http refused");
        assert!(check(Cluster::Local, "https://x.example").is_err(), "local stays fixed");
        assert!(check(Cluster::Mainnet, "not a url").is_err());
    }

    #[test]
    fn rpc_outcomes() {
        assert!(matches!(classify(Some(-32002), "x".into()), Sent::Rejected(_)));
        assert!(matches!(classify(Some(-32603), "[-32002] in text".into()), Sent::Unknown), "text never decides");
        assert!(matches!(classify(None, "timeout".into()), Sent::Unknown));
        let ok = json!({ "value": [{ "slot": 5, "confirmations": null, "err": null, "confirmationStatus": "finalized" }] });
        assert!(matches!(status(&ok), Some(Some(Status { finalized: true, ok: true, .. }))));
        let no_err = json!({ "value": [{ "slot": 5, "confirmations": null, "confirmationStatus": "finalized" }] });
        assert!(status(&no_err).is_none(), "missing err is not success");
        assert!(matches!(status(&json!({ "value": [null] })), Some(None)));
        assert!(status(&json!({ "value": [] })).is_none());
        assert!(status(&json!({})).is_none());
    }

    #[test]
    fn amounts() {
        assert_eq!(parse_amount("1", 9), Ok(1_000_000_000));
        assert_eq!(parse_amount(" 1,5 ", 9), Ok(1_500_000_000));
        assert_eq!(parse_amount(".000000001", 9), Ok(1));
        assert_eq!(parse_amount("0010.25", 9), Ok(10_250_000_000));
        for bad in ["", "0", "0.0", "1.0000000001", "1e3", "-1", "1.2.3", "99999999999999999999"] { assert!(parse_amount(bad, 9).is_err(), "{bad}") }
        assert_eq!(units(1_500_000_000, 9), "1.5");
        assert_eq!(units(1, 9), "0.000000001");
        assert_eq!(units(0, 9), "0");
        assert_eq!(units(5000, 9), "0.000005");
        for v in [1u64, 890_880, 1_000_000_000, u64::MAX] { assert_eq!(parse_amount(&units(v, 9), 9), Ok(v)) }
    }

    #[test]
    fn transfer_message_layout() {
        // legacy message: header, [from, to, system], blockhash, one System::Transfer(2) with u64 LE lamports
        let (f, t) = ("HAgk14JpMQLgt6rVgv7cBQFJWFto5Dqxi472uT3DKpqk", "BLeUXTx9thHGT7VJUtF9vHEmfMDgW1nnKZ9UVer2CoLX");
        let (fa, ta): (solana_address::Address, solana_address::Address) = (f.parse().unwrap(), t.parse().unwrap());
        let h = solana_hash::Hash::new_from_array([7; 32]);
        let m = solana_message::Message::new_with_blockhash(&[solana_system_interface::instruction::transfer(&fa, &ta, 42)], Some(&fa), &h).serialize();
        assert_eq!(&m[..4], &[1, 0, 1, 3]);
        assert_eq!(&m[4..36], &bs58::decode(f).into_vec().unwrap()[..]);
        assert_eq!(&m[36..68], &bs58::decode(t).into_vec().unwrap()[..]);
        assert_eq!(&m[68..100], &[0; 32]);
        assert_eq!(&m[100..132], &[7; 32]);
        assert_eq!(&m[132..], &[1, 2, 2, 0, 1, 12, 2, 0, 0, 0, 42, 0, 0, 0, 0, 0, 0, 0]);
    }
}

#[cfg(test)]
/// legacy message: (signers, account keys, instructions as (program, accounts, data)) — for asserting what gets signed
pub(crate) fn decode(m: &[u8]) -> (u8, Vec<[u8; 32]>, Vec<([u8; 32], Vec<[u8; 32]>, Vec<u8>)>) {
    fn sv(m: &[u8], i: &mut usize) -> usize { let (mut v, mut s) = (0usize, 0); loop { let b = m[*i]; *i += 1; v |= ((b & 0x7f) as usize) << s; if b & 0x80 == 0 { return v } s += 7 } }
    let mut i = 3;
    let n = sv(m, &mut i);
    let keys: Vec<[u8; 32]> = (0..n).map(|k| m[i + k * 32..i + k * 32 + 32].try_into().unwrap()).collect();
    i += n * 32 + 32; // keys, blockhash
    let ni = sv(m, &mut i);
    let ixs = (0..ni).map(|_| {
        let prog = keys[m[i] as usize]; i += 1;
        let na = sv(m, &mut i); let accs = m[i..i + na].iter().map(|&x| keys[x as usize]).collect(); i += na;
        let nd = sv(m, &mut i); let data = m[i..i + nd].to_vec(); i += nd;
        (prog, accs, data)
    }).collect();
    assert_eq!(i, m.len(), "no trailing bytes");
    (m[0], keys, ixs)
}

#[cfg(test)]
mod live {
    // read-only call to public mainnet: `cargo test -- --ignored`
    // the fee wallet setup is built and dry-run on mainnet with a funded public payer (nothing is signed):
    // `cargo test fee_setup_dry -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn fee_setup_dry() {
        let c = super::Cluster::Mainnet;
        let missing = tauri::async_runtime::block_on(super::fee_missing(c)).expect("read");
        println!("missing {}", missing.len());
        if missing.is_empty() { return }
        let d = tauri::async_runtime::block_on(super::prepare_fee_setup("9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM", c)).expect("dry run passes");
        assert_eq!(d.to, super::FEE_OWNER.unwrap());
        assert!(d.amount > 0 && d.fee > 5000, "rent and fee priced");
        println!("rent {} fee {} warn {:?}", d.amount, d.fee, d.warn);
    }

    #[test]
    #[ignore]
    fn mainnet_read() {
        let rt = tauri::async_runtime::block_on(super::watch("JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN", super::Cluster::Mainnet));
        let w = rt.expect("mainnet read");
        assert!(w.lamports.parse::<u64>().is_ok());
        assert!(w.tokens.iter().all(|t| t.decimals <= 18));
        let mut m: Vec<&str> = w.tokens.iter().map(|t| t.mint.as_str()).collect();
        m.sort_unstable();
        assert!(m.windows(2).all(|p| p[0] != p[1]), "one holding per mint");
        println!("{} tokens, {} named", w.tokens.len(), w.tokens.iter().filter(|t| t.symbol.is_some()).count());
        for t in w.tokens.iter().take(8) { println!("  {} {:?} {:?}", &t.mint[..6], t.symbol, t.name) }
    }

    // history entries of a busy public address read without errors: `cargo test history_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn history_live() {
        let a = "HAgk14JpMQLgt6rVgv7cBQFJWFto5Dqxi472uT3DKpqk"; // the public «abandon … about» test address
        let list = tauri::async_runtime::block_on(super::history(a, super::Cluster::Mainnet)).expect("history");
        for s in list.iter().take(6) {
            let t = tauri::async_runtime::block_on(super::tx(&s.sig, a, super::Cluster::Mainnet)).expect("tx");
            println!("{} ok={} fee={:?} peer={:?} moves={:?}", &s.sig[..8], t.ok, t.fee, t.peer, t.moves);
        }
    }

    // a real mainnet swap, built and dry-run only (never signed): `cargo test swap_live -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn swap_live() {
        let a = "9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM"; // a public exchange wallet holding SOL (dry run only)
        let q = tauri::async_runtime::block_on(super::swap_quote("SOL", "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "0,01", super::Cluster::Mainnet)).expect("quote");
        println!("quote {} -> {} (min {}) via {}", q.in_amount, q.out_amount, q.min_out, q.route);
        let d = tauri::async_runtime::block_on(super::prepare_swap(a, "SOL", "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v", "0,01", super::Cluster::Mainnet)).expect("prepare");
        let s = d.swap.unwrap();
        println!("dry run: {} USDC arrives (min {}), fee {} lamports, message {} bytes", super::units(s.out_amount, 6), super::units(s.min_out, 6), d.fee, d.message.len());
        assert!(s.out_amount >= s.min_out && d.message[0] == 0x80);
    }

    // a node must serve the network it is set for: `cargo test own_node_live -- --ignored`
    #[test]
    #[ignore]
    fn own_node_live() {
        use super::{check_own, Cluster::*};
        let check = |c, u| tauri::async_runtime::block_on(check_own(c, u));
        check(Mainnet, "https://api.mainnet-beta.solana.com").expect("right network");
        assert_eq!(check(Mainnet, "https://api.devnet.solana.com").unwrap_err(), "этот узел обслуживает другую сеть");
        assert!(!check(Mainnet, "https://nonexistent.invalid/?api-key=SECRET").unwrap_err().contains("SECRET"), "no URL in errors");
    }

    // devnet end to end with a throwaway vault: airdrop -> prepare -> sign -> send -> confirmed.
    // `cargo test devnet_send -- --ignored --nocapture`; devnet faucet dry -> CW_TEST_LOCAL=1 against solana-test-validator
    #[test]
    #[ignore]
    fn devnet_send() {
        use super::*;
        use crate::vault;
        tauri::async_runtime::block_on(async {
            let dir = std::env::temp_dir().join(format!("cw-devnet-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let p = dir.join("vault.json");
            let pass = "devnet throwaway pass";
            let accs = vault::create(&p, vault::Source::Phrase(&vault::new_phrase()), "t", pass, vault::Kdf { m: 64, t: 1, p: 1 }).unwrap();
            let from = accs[0].solana.clone();
            let to = vault::sol_address(&vault::solana_key(&vault::new_phrase(), 0));
            println!("from {from}");
            let http = client().unwrap();
            let c = if std::env::var("CW_TEST_LOCAL").is_ok() { Cluster::Local } else { Cluster::Devnet };
            rpc(&http, c, "requestAirdrop", json!([from, 100_000_000])).await.expect("airdrop");
            for _ in 0..30 {
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                if value_u64(&rpc(&http, c, "getBalance", json!([from, { "commitment": "confirmed" }])).await.unwrap()).unwrap() > 0 { break }
            }
            // below rent for a fresh recipient: refused before signing
            assert!(prepare(&from, &to, 1000, c).await.err().unwrap().contains("пустой"));
            let d = prepare(&from, &to, 1_000_000, c).await.unwrap();
            assert!(d.new_recipient && d.fee > 5000 && d.fee < 5000 + 1_000 && d.rent == 0, "base fee + priority under the cap");
            // the signed bytes, decoded in full, say exactly what the confirmation shows: one signer (the payer), the two
            // compute-budget settings, one System transfer from -> to of the drafted amount, nothing else
            let key = |a: &str| -> [u8; 32] { bs58::decode(a).into_vec().unwrap().try_into().unwrap() };
            let (signers, keys, ixs) = super::decode(&d.message);
            assert_eq!((signers, keys[0]), (1, key(&from)));
            assert_eq!(ixs.len(), 3);
            assert_eq!((ixs[0].0, ixs[1].0), (key(COMPUTE_BUDGET), key(COMPUTE_BUDGET)));
            assert_eq!(ixs[0].2, [&[2u8][..], &1_000u32.to_le_bytes()].concat());
            assert_eq!((ixs[1].2[0], ixs[1].2.len()), (3, 9));
            assert_eq!(ixs[2].0, [0u8; 32]);
            assert_eq!(ixs[2].1, vec![key(&from), key(&to)]);
            assert_eq!(ixs[2].2, [&2u32.to_le_bytes()[..], &1_000_000u64.to_le_bytes()].concat());
            let sig = vault::sign_solana(&p, pass, &accs[0].id, &d.message).unwrap();
            assert!(matches!(submit(&d, &sig).await, Sent::Ok));
            let id = tx_id(&sig);
            println!("sig {id}");
            assert!(matches!(track(&id, c, d.last_valid, || {}).await, Outcome::Confirmed));
            assert_eq!(value_u64(&rpc(&http, c, "getBalance", json!([to, { "commitment": "confirmed" }])).await.unwrap()).unwrap(), 1_000_000);
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    // tokens end to end on a local validator: plain SPL and Token-2022 sent (recipient account created on the way),
    // a Token-2022 mint with a transfer fee refused before signing.
    // `CW_TEST_LOCAL=1 cargo test token_send -- --ignored --nocapture` against solana-test-validator
    #[test]
    #[ignore]
    fn token_send() {
        use super::*;
        use crate::vault;
        use ed25519_dalek::{Signer, SigningKey};
        use solana_address::Address;
        use solana_program_pack::Pack;
        use spl_token_2022_interface::{extension::ExtensionType, instruction as ti, state::Mint};
        let c = if std::env::var("CW_TEST_LOCAL").is_ok() { Cluster::Local } else { Cluster::Devnet };
        tauri::async_runtime::block_on(async {
            let http = client().unwrap();
            let key = |k: &SigningKey| Address::new_from_array(k.verifying_key().to_bytes());
            let fresh = || { let mut s = [0u8; 32]; getrandom::getrandom(&mut s).unwrap(); SigningKey::from_bytes(&s) };
            // multi-signer helper for setup only: signatures in the order of the message's signer keys
            let run = |signers: Vec<SigningKey>, ixs: Vec<solana_instruction::Instruction>| { let http = http.clone(); async move {
                let bh = rpc(&http, c, "getLatestBlockhash", json!([{ "commitment": "confirmed" }])).await.unwrap();
                let hash: solana_hash::Hash = bh["value"]["blockhash"].as_str().unwrap().parse().unwrap();
                let msg = solana_message::Message::new_with_blockhash(&ixs, Some(&key(&signers[0])), &hash);
                let bytes = msg.serialize();
                let mut wire = vec![msg.header.num_required_signatures];
                for k in &msg.account_keys[..msg.header.num_required_signatures as usize] {
                    wire.extend(signers.iter().find(|s| key(s) == *k).unwrap().sign(&bytes).to_bytes());
                }
                wire.extend(&bytes);
                let sig = rpc(&http, c, "sendTransaction", json!([base64::engine::general_purpose::STANDARD.encode(wire), { "encoding": "base64", "preflightCommitment": "confirmed" }])).await.unwrap();
                assert!(matches!(track(sig.as_str().unwrap(), c, u64::MAX, || {}).await, Outcome::Confirmed));
            }};

            let dir = std::env::temp_dir().join(format!("cw-tokens-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let p = dir.join("vault.json");
            let pass = "token throwaway pass";
            let accs = vault::create(&p, vault::Source::Phrase(&vault::new_phrase()), "t", pass, vault::Kdf { m: 64, t: 1, p: 1 }).unwrap();
            let from = accs[0].solana.clone();
            let fa: Address = from.parse().unwrap();
            let boss = fresh();
            for a in [from.clone(), key(&boss).to_string()] { rpc(&http, c, "requestAirdrop", json!([a, 2_000_000_000])).await.expect("airdrop"); }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let rent = |n: usize| { let http = http.clone(); async move { rent_for(&http, c, n).await.unwrap() } };

            // one mint per case: (program, extensions, should send)
            for (program, fee_ext) in [(TOKEN, false), (TOKEN_2022, false), (TOKEN_2022, true)] {
                let prog: Address = program.parse().unwrap();
                let mint = fresh();
                let ma = key(&mint);
                let space = if fee_ext { ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::TransferFeeConfig]).unwrap() } else { Mint::LEN };
                let mut ixs = vec![solana_system_interface::instruction::create_account(&key(&boss), &ma, rent(space).await, space as u64, &prog)];
                if fee_ext { ixs.push(spl_token_2022_interface::extension::transfer_fee::instruction::initialize_transfer_fee_config(&prog, &ma, None, None, 100, 1_000_000).unwrap()) }
                ixs.push(ti::initialize_mint2(&prog, &ma, &key(&boss), None, 6).unwrap());
                ixs.push(spl_associated_token_account_interface::instruction::create_associated_token_account_idempotent(&p3(&key(&boss)), &p3(&fa), &p3(&ma), &p3(&prog)));
                let src = Address::new_from_array(spl_associated_token_account_interface::address::get_associated_token_address_with_program_id(&p3(&fa), &p3(&ma), &p3(&prog)).to_bytes());
                ixs.push(ti::mint_to(&prog, &ma, &src, &key(&boss), &[], 10_000_000).unwrap());
                run(vec![fresh_copy(&boss), mint], ixs).await;

                let to = vault::sol_address(&vault::solana_key(&vault::new_phrase(), 0));
                let r = prepare_token(&from, &to, &ma.to_string(), "1,5", c).await;
                if fee_ext { assert!(r.err().unwrap().contains("комиссия")); continue }
                let d = r.unwrap();
                assert!(d.new_recipient && d.rent > 0 && d.amount == 1_500_000 && d.decimals == 6);
                // decoded in full: create the recipient's ATA (payer from, owner to, this mint and program), then one
                // TransferChecked from the sender's ATA to that ATA, authority from, the drafted amount and decimals
                let (signers, keys, ixs) = super::decode(&d.message);
                let ataf = |o: &Address| Address::new_from_array(spl_associated_token_account_interface::address::get_associated_token_address_with_program_id(&p3(o), &p3(&ma), &p3(&prog)).to_bytes());
                let ta: Address = to.parse().unwrap();
                assert_eq!((signers, keys[0]), (1, fa.to_bytes()));
                assert_eq!(ixs.len(), 4);
                let cb: Address = COMPUTE_BUDGET.parse().unwrap();
                assert_eq!((ixs[0].0, ixs[1].0, &ixs[0].2), (cb.to_bytes(), cb.to_bytes(), &[&[2u8][..], &80_000u32.to_le_bytes()].concat()));
                assert_eq!(ixs[2].0, spl_associated_token_account_interface::program::id().to_bytes());
                assert_eq!(ixs[2].1, vec![fa.to_bytes(), ataf(&ta).to_bytes(), ta.to_bytes(), ma.to_bytes(), [0u8; 32], prog.to_bytes()]);
                assert_eq!(ixs[2].2, vec![1]);
                assert_eq!(ixs[3].0, prog.to_bytes());
                assert_eq!(ixs[3].1, vec![ataf(&fa).to_bytes(), ma.to_bytes(), ataf(&ta).to_bytes(), fa.to_bytes()]);
                assert_eq!(ixs[3].2, [&[12u8][..], &1_500_000u64.to_le_bytes(), &[6u8]].concat());
                assert!(prepare_token(&from, &to, &ma.to_string(), "11", c).await.err().unwrap().contains("не хватает"));
                let sig = vault::sign_solana(&p, pass, &accs[0].id, &d.message).unwrap();
                assert!(matches!(submit(&d, &sig).await, Sent::Ok));
                assert!(matches!(track(&tx_id(&sig), c, d.last_valid, || {}).await, Outcome::Confirmed), "{program}");
                let mut w = watch(&to, c).await.unwrap();
                for _ in 0..10 { if !w.tokens.is_empty() { break } tokio::time::sleep(std::time::Duration::from_secs(1)).await; w = watch(&to, c).await.unwrap(); }
                assert_eq!(w.tokens.len(), 1);
                assert_eq!(w.tokens[0].amount, "1500000");
                // second send to the same wallet: its token account exists now, nothing to create
                let d2 = prepare_token(&from, &to, &ma.to_string(), "0.5", c).await.unwrap();
                assert!(!d2.new_recipient && d2.rent == 0);
                println!("{program}: ok, rent {}", d.rent);
            }
            let _ = std::fs::remove_dir_all(&dir);
        });
        fn fresh_copy(k: &ed25519_dalek::SigningKey) -> ed25519_dalek::SigningKey { ed25519_dalek::SigningKey::from_bytes(&k.to_bytes()) }
    }

    // permissions on a local validator: a delegate approved on our token account shows up, the wallet's revoke removes it.
    // `CW_TEST_LOCAL=1 cargo test grants_live -- --ignored` with solana-test-validator running
    #[test]
    #[ignore]
    fn grants_live() {
        use super::*;
        use crate::vault;
        use ed25519_dalek::{Signer, SigningKey};
        use solana_address::Address;
        use solana_program_pack::Pack;
        use spl_token_2022_interface::{instruction as ti, state::Mint};
        let c = if std::env::var("CW_TEST_LOCAL").is_ok() { Cluster::Local } else { Cluster::Devnet };
        tauri::async_runtime::block_on(async {
            let http = client().unwrap();
            let key = |k: &SigningKey| Address::new_from_array(k.verifying_key().to_bytes());
            let fresh = || { let mut s = [0u8; 32]; getrandom::getrandom(&mut s).unwrap(); SigningKey::from_bytes(&s) };
            let run = |signers: Vec<SigningKey>, ixs: Vec<solana_instruction::Instruction>| { let http = http.clone(); async move {
                let bh = rpc(&http, c, "getLatestBlockhash", json!([{ "commitment": "confirmed" }])).await.unwrap();
                let hash: solana_hash::Hash = bh["value"]["blockhash"].as_str().unwrap().parse().unwrap();
                let msg = solana_message::Message::new_with_blockhash(&ixs, Some(&key(&signers[0])), &hash);
                let bytes = msg.serialize();
                let mut wire = vec![msg.header.num_required_signatures];
                for k in &msg.account_keys[..msg.header.num_required_signatures as usize] { wire.extend(signers.iter().find(|s| key(s) == *k).unwrap().sign(&bytes).to_bytes()); }
                wire.extend(&bytes);
                let sig = rpc(&http, c, "sendTransaction", json!([base64::engine::general_purpose::STANDARD.encode(wire), { "encoding": "base64", "preflightCommitment": "confirmed" }])).await.unwrap();
                assert!(matches!(track(sig.as_str().unwrap(), c, u64::MAX, || {}).await, Outcome::Confirmed));
            }};
            let dir = std::env::temp_dir().join(format!("cw-grants-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let p = dir.join("vault.json");
            let pass = "grants throwaway pass";
            let phrase = vault::new_phrase();
            let accs = vault::create(&p, vault::Source::Phrase(&phrase), "t", pass, vault::Kdf { m: 64, t: 1, p: 1 }).unwrap();
            let me = SigningKey::from_bytes(&vault::solana_key(&phrase, 0));
            let (boss, spender) = (fresh(), fresh());
            for a in [accs[0].solana.clone(), key(&boss).to_string()] { rpc(&http, c, "requestAirdrop", json!([a, 2_000_000_000])).await.expect("airdrop"); }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            for program in [TOKEN, TOKEN_2022] {
                let prog: Address = program.parse().unwrap();
                let mint = fresh();
                let ma = key(&mint);
                let fa = key(&me);
                let src = Address::new_from_array(spl_associated_token_account_interface::address::get_associated_token_address_with_program_id(&p3(&fa), &p3(&ma), &p3(&prog)).to_bytes());
                let r = rent_for(&http, c, Mint::LEN).await.unwrap();
                run(vec![SigningKey::from_bytes(&boss.to_bytes()), SigningKey::from_bytes(&mint.to_bytes())], vec![
                    solana_system_interface::instruction::create_account(&key(&boss), &ma, r, Mint::LEN as u64, &prog),
                    ti::initialize_mint2(&prog, &ma, &key(&boss), None, 6).unwrap(),
                    spl_associated_token_account_interface::instruction::create_associated_token_account_idempotent(&p3(&key(&boss)), &p3(&fa), &p3(&ma), &p3(&prog)),
                    ti::mint_to(&prog, &ma, &src, &key(&boss), &[], 10_000_000).unwrap(),
                ]).await;
                run(vec![SigningKey::from_bytes(&me.to_bytes())], vec![ti::approve(&prog, &src, &key(&spender), &fa, &[], 2_500_000).unwrap()]).await;
                let g = grants(&accs[0].solana, c).await.unwrap();
                let g: Vec<_> = g.iter().filter(|g| g.mint == ma.to_string()).collect();
                assert_eq!(g.len(), 1, "{program}");
                assert_eq!((g[0].delegate.clone(), g[0].amount.as_str(), g[0].decimals, g[0].account.clone()), (key(&spender).to_string(), "2500000", 6, src.to_string()));
                let d = prepare_revoke(&accs[0].solana, &src.to_string(), c).await.unwrap();
                assert_eq!(d.to, key(&spender).to_string());
                let sig = vault::sign_solana(&p, pass, &accs[0].id, &d.message).unwrap();
                assert!(matches!(submit(&d, &sig).await, Sent::Ok));
                assert!(matches!(track(&tx_id(&sig), c, d.last_valid, || {}).await, Outcome::Confirmed));
                assert!(grants(&accs[0].solana, c).await.unwrap().iter().all(|g| g.mint != ma.to_string()), "{program}: revoked");
                assert!(prepare_revoke(&accs[0].solana, &src.to_string(), c).await.err().unwrap().contains("уже нет"));
                assert!(prepare_revoke(&key(&boss).to_string(), &src.to_string(), c).await.err().unwrap().contains("не вам"));
            }
            let _ = std::fs::remove_dir_all(&dir);
        });
    }

    // token guards on a local validator: paused mint, frozen-by-default mint, frozen destination, memo-required
    // destination — each refused before signing. `CW_TEST_LOCAL=1 cargo test token_guards -- --ignored`
    #[test]
    #[ignore]
    fn token_guards() {
        use super::*;
        use ed25519_dalek::{Signer, SigningKey};
        use solana_address::Address;
        use solana_program_pack::Pack;
        use spl_token_2022_interface::{extension::{self, ExtensionType}, instruction as ti, state::{AccountState, Mint}};
        let c = if std::env::var("CW_TEST_LOCAL").is_ok() { Cluster::Local } else { Cluster::Devnet };
        tauri::async_runtime::block_on(async {
            let http = client().unwrap();
            let key = |k: &SigningKey| Address::new_from_array(k.verifying_key().to_bytes());
            let fresh = || { let mut s = [0u8; 32]; getrandom::getrandom(&mut s).unwrap(); SigningKey::from_bytes(&s) };
            let copy = |k: &SigningKey| SigningKey::from_bytes(&k.to_bytes());
            let run = |signers: Vec<SigningKey>, ixs: Vec<solana_instruction::Instruction>| { let http = http.clone(); async move {
                let bh = rpc(&http, c, "getLatestBlockhash", json!([{ "commitment": "confirmed" }])).await.unwrap();
                let hash: solana_hash::Hash = bh["value"]["blockhash"].as_str().unwrap().parse().unwrap();
                let msg = solana_message::Message::new_with_blockhash(&ixs, Some(&key(&signers[0])), &hash);
                let bytes = msg.serialize();
                let mut wire = vec![msg.header.num_required_signatures];
                for k in &msg.account_keys[..msg.header.num_required_signatures as usize] {
                    wire.extend(signers.iter().find(|s| key(s) == *k).unwrap().sign(&bytes).to_bytes());
                }
                wire.extend(&bytes);
                let sig = rpc(&http, c, "sendTransaction", json!([base64::engine::general_purpose::STANDARD.encode(wire), { "encoding": "base64", "preflightCommitment": "confirmed" }])).await.unwrap();
                assert!(matches!(track(sig.as_str().unwrap(), c, u64::MAX, || {}).await, Outcome::Confirmed));
            }};
            let boss = fresh(); let from = fresh(); let recv = fresh();
            for k in [&boss, &from, &recv] { rpc(&http, c, "requestAirdrop", json!([key(k).to_string(), 2_000_000_000])).await.expect("airdrop"); }
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let fa = key(&from);
            let ata = |owner: &Address, mint: &Address, prog: &Address| Address::new_from_array(
                spl_associated_token_account_interface::address::get_associated_token_address_with_program_id(&p3(owner), &p3(mint), &p3(prog)).to_bytes());
            let mk_ata = |payer: &SigningKey, owner: &Address, mint: &Address, prog: &Address| spl_associated_token_account_interface::instruction::create_associated_token_account_idempotent(&p3(&key(payer)), &p3(owner), &p3(mint), &p3(prog));
            let rent = |n: usize| { let http = http.clone(); async move { rent_for(&http, c, n).await.unwrap() } };
            let t22: Address = TOKEN_2022.parse().unwrap(); let t1: Address = TOKEN.parse().unwrap();

            // 1. paused mint
            let mint = fresh(); let ma = key(&mint);
            let space = ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::Pausable]).unwrap();
            run(vec![copy(&boss), copy(&mint)], vec![
                solana_system_interface::instruction::create_account(&key(&boss), &ma, rent(space).await, space as u64, &t22),
                extension::pausable::instruction::initialize(&t22, &ma, &key(&boss)).unwrap(),
                ti::initialize_mint2(&t22, &ma, &key(&boss), None, 6).unwrap(),
                mk_ata(&boss, &fa, &ma, &t22),
                ti::mint_to(&t22, &ma, &ata(&fa, &ma, &t22), &key(&boss), &[], 5_000_000).unwrap(),
                extension::pausable::instruction::pause(&t22, &ma, &key(&boss), &[]).unwrap(),
            ]).await;
            assert!(prepare_token(&fa.to_string(), &key(&recv).to_string(), &ma.to_string(), "1", c).await.err().unwrap().contains("паузе"));

            // 2. new accounts frozen by default (sender thawed by the issuer, fresh recipient would be frozen)
            let mint = fresh(); let ma = key(&mint);
            let space = ExtensionType::try_calculate_account_len::<Mint>(&[ExtensionType::DefaultAccountState]).unwrap();
            run(vec![copy(&boss), copy(&mint)], vec![
                solana_system_interface::instruction::create_account(&key(&boss), &ma, rent(space).await, space as u64, &t22),
                extension::default_account_state::instruction::initialize_default_account_state(&t22, &ma, &AccountState::Frozen).unwrap(),
                ti::initialize_mint2(&t22, &ma, &key(&boss), Some(&key(&boss)), 6).unwrap(),
                mk_ata(&boss, &fa, &ma, &t22),
                ti::thaw_account(&t22, &ata(&fa, &ma, &t22), &ma, &key(&boss), &[]).unwrap(),
                ti::mint_to(&t22, &ma, &ata(&fa, &ma, &t22), &key(&boss), &[], 5_000_000).unwrap(),
            ]).await;
            assert!(prepare_token(&fa.to_string(), &key(&recv).to_string(), &ma.to_string(), "1", c).await.err().unwrap().contains("замороженными"));

            // 3. frozen destination (plain SPL)
            let mint = fresh(); let ma = key(&mint);
            run(vec![copy(&boss), copy(&mint)], vec![
                solana_system_interface::instruction::create_account(&key(&boss), &ma, rent(Mint::LEN).await, Mint::LEN as u64, &t1),
                ti::initialize_mint2(&t1, &ma, &key(&boss), Some(&key(&boss)), 6).unwrap(),
                mk_ata(&boss, &fa, &ma, &t1),
                mk_ata(&boss, &key(&recv), &ma, &t1),
                ti::mint_to(&t1, &ma, &ata(&fa, &ma, &t1), &key(&boss), &[], 5_000_000).unwrap(),
                ti::freeze_account(&t1, &ata(&key(&recv), &ma, &t1), &ma, &key(&boss), &[]).unwrap(),
            ]).await;
            assert!(prepare_token(&fa.to_string(), &key(&recv).to_string(), &ma.to_string(), "1", c).await.err().unwrap().contains("заморожен"));

            // 4. destination requires a memo on every incoming transfer
            let mint = fresh(); let ma = key(&mint);
            let ra = ata(&key(&recv), &ma, &t22);
            run(vec![copy(&boss), copy(&mint), copy(&recv)], vec![
                solana_system_interface::instruction::create_account(&key(&boss), &ma, rent(Mint::LEN).await, Mint::LEN as u64, &t22),
                ti::initialize_mint2(&t22, &ma, &key(&boss), None, 6).unwrap(),
                mk_ata(&boss, &fa, &ma, &t22),
                mk_ata(&boss, &key(&recv), &ma, &t22),
                ti::reallocate(&t22, &ra, &key(&boss), &key(&recv), &[], &[ExtensionType::MemoTransfer]).unwrap(),
                extension::memo_transfer::instruction::enable_required_transfer_memos(&t22, &ra, &key(&recv), &[]).unwrap(),
                ti::mint_to(&t22, &ma, &ata(&fa, &ma, &t22), &key(&boss), &[], 5_000_000).unwrap(),
            ]).await;
            assert!(prepare_token(&fa.to_string(), &key(&recv).to_string(), &ma.to_string(), "1", c).await.err().unwrap().contains("заметку"));
            println!("token guards: paused, frozen-by-default, frozen destination, memo-required refused");
        });
    }
}
