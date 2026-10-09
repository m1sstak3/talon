//! EVM networks: balances (native coin + the known tokens of each network) and sends of the native coin or an ERC-20,
//! built, priced, dry-run and signed in the core (EIP-1559, type 2). The RPC must first prove its chain id; the chain
//! id is inside the signed bytes, so a lying node cannot move the transfer to another network.

use serde_json::{Value, json};
use sha3::{Digest, Keccak256};

// ---------------------------------------------------------------- networks the core knows
/// how a network charges for posting data to Ethereum on top of gas (estimateGas covers it on Arbitrum, Linea, ...)
#[derive(Clone, Copy, PartialEq)]
enum L1 { None, Op, Scroll }
struct Net { id: u64, name: &'static str, native: &'static str, test: bool, l1: L1, explorer: &'static str, tokens: &'static [(&'static str, &'static str, u8)] }
// tokens checked on chain 09.10 (symbol + decimals through each network's public RPC). Only these are read and sent:
// an unknown contract can say any name, so it never shows up as «USDC» here.
const NETS: &[Net] = &[
    Net { id: 1, name: "Ethereum", native: "ETH", test: false, l1: L1::None, explorer: "https://etherscan.io", tokens: &[("USDC", "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48", 6), ("USDT", "0xdAC17F958D2ee523a2206206994597C13D831ec7", 6), ("DAI", "0x6B175474E89094C44Da98b954EedeAC495271d0F", 18)] },
    Net { id: 8453, name: "Base", native: "ETH", test: false, l1: L1::Op, explorer: "https://basescan.org", tokens: &[("USDC", "0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913", 6)] },
    Net { id: 42161, name: "Arbitrum One", native: "ETH", test: false, l1: L1::None, explorer: "https://arbiscan.io", tokens: &[("USDC", "0xaf88d065e77c8cC2239327C5EDb3A432268e5831", 6), ("USDT", "0xFd086bC7CD5C481DCC9C85ebE478A1C0b69FCbb9", 6)] },
    Net { id: 10, name: "OP Mainnet", native: "ETH", test: false, l1: L1::Op, explorer: "https://optimistic.etherscan.io", tokens: &[("USDC", "0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85", 6), ("USDT", "0x94b008aA00579c1307B0EF2c499aD98a8ce58e58", 6)] },
    Net { id: 4663, name: "Robinhood Chain", native: "ETH", test: false, l1: L1::None, explorer: "", tokens: &[] },
    Net { id: 137, name: "Polygon", native: "POL", test: false, l1: L1::None, explorer: "https://polygonscan.com", tokens: &[("USDC", "0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359", 6), ("USDT", "0xc2132D05D31c914a87C6611C10748AEb04B58e8F", 6)] },
    Net { id: 56, name: "BNB Chain", native: "BNB", test: false, l1: L1::None, explorer: "https://bscscan.com", tokens: &[("USDT", "0x55d398326f99059fF775485246999027B3197955", 18), ("USDC", "0x8AC76a51cc950d9822D68b83fE1Ad97B32Cd580d", 18)] },
    Net { id: 43114, name: "Avalanche", native: "AVAX", test: false, l1: L1::None, explorer: "https://snowtrace.io", tokens: &[("USDC", "0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E", 6), ("USDT", "0x9702230A8Ea53601f5cD2dc00fDBc13d4dF4A8c7", 6)] },
    Net { id: 59144, name: "Linea", native: "ETH", test: false, l1: L1::None, explorer: "https://lineascan.build", tokens: &[("USDC", "0x176211869cA2b568f2A7D4EE941E073a821EE1ff", 6)] },
    Net { id: 130, name: "Unichain", native: "ETH", test: false, l1: L1::Op, explorer: "https://uniscan.xyz", tokens: &[("USDC", "0x078D782b760474a361dDA0AF3839290b0EF57AD6", 6)] },
    Net { id: 146, name: "Sonic", native: "S", test: false, l1: L1::None, explorer: "https://sonicscan.org", tokens: &[("USDC", "0x29219dd400f2Bf60E5a23d13Be72B486D4038894", 6)] },
    Net { id: 100, name: "Gnosis", native: "xDAI", test: false, l1: L1::None, explorer: "https://gnosisscan.io", tokens: &[("USDC.e", "0x2a22f9c3b484c3629090FeED35F17Ff8F88f76F0", 6)] },
    Net { id: 534352, name: "Scroll", native: "ETH", test: false, l1: L1::Scroll, explorer: "https://scrollscan.com", tokens: &[("USDC", "0x06eFdBFf2a14a7c8E15944D1F4A48F9F95F663A4", 6)] },
    Net { id: 57073, name: "Ink", native: "ETH", test: false, l1: L1::Op, explorer: "https://explorer.inkonchain.com", tokens: &[] },
    Net { id: 480, name: "World Chain", native: "ETH", test: false, l1: L1::Op, explorer: "https://worldscan.org", tokens: &[] },
    Net { id: 80094, name: "Berachain", native: "BERA", test: false, l1: L1::None, explorer: "https://berascan.com", tokens: &[] },
    Net { id: 143, name: "Monad", native: "MON", test: false, l1: L1::None, explorer: "https://monadscan.com", tokens: &[] },
    Net { id: 999, name: "HyperEVM", native: "HYPE", test: false, l1: L1::None, explorer: "https://hyperevmscan.io", tokens: &[] },
    Net { id: 11155111, name: "Sepolia", native: "ETH", test: true, l1: L1::None, explorer: "https://sepolia.etherscan.io", tokens: &[("USDC", "0x1c7D4B196Cb0C7B01d743Fbc6116a902379C7238", 6)] },
    Net { id: 84532, name: "Base Sepolia", native: "ETH", test: true, l1: L1::Op, explorer: "https://sepolia.basescan.org", tokens: &[("USDC", "0x036CbD53842c5426634e7929541eC2318f3dCF7e", 6)] },
    Net { id: 31337, name: "EVM Local", native: "ETH", test: true, l1: L1::None, explorer: "", tokens: &[] }, // anvil (tests, dev builds)
];
fn net(id: u64) -> Option<&'static Net> { NETS.iter().find(|n| n.id == id) }
pub fn explorer(id: u64) -> Option<&'static str> { net(id).map(|n| n.explorer).filter(|e| !e.is_empty()) }

// ---------------------------------------------------------------- JSON-RPC
fn http() -> Result<reqwest::Client, String> {
    reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().map_err(|e| e.to_string())
}
/// node errors in plain Russian; a revert keeps the contract's own reason (it is what the user can act on)
async fn rpc(http: &reqwest::Client, url: &str, method: &str, params: Value) -> Result<Value, String> {
    let r = http.post(url).json(&json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params })).send().await
        .map_err(|e| if e.is_timeout() { "сеть не ответила вовремя".to_string() } else { "нет связи с сетью".to_string() })?;
    match r.status().as_u16() {
        200 => {}
        429 => return Err("сеть просит подождать (слишком много запросов), попробуйте через минуту".into()),
        s => return Err(format!("узел сети ответил ошибкой {s}")),
    }
    let v: Value = r.json().await.map_err(|_| "узел сети ответил не тем".to_string())?;
    if let Some(e) = v.get("error") { return Err(rpc_error(e.get("message").and_then(Value::as_str).unwrap_or(""))) }
    v.get("result").cloned().ok_or_else(|| "узел сети ответил без результата".into())
}
fn rpc_error(m: &str) -> String {
    let l = m.to_lowercase();
    if l.contains("insufficient funds") { "не хватает средств на сумму и комиссию".into() }
    else if l.contains("nonce too low") || l.contains("already known") || l.contains("known transaction") { "такой перевод уже в сети".into() }
    else if l.contains("underpriced") || l.contains("fee cap") { "комиссия ниже нужной сети, подготовьте перевод заново".into() }
    else if l.contains("execution reverted") { format!("контракт отказал: {}", m.trim_start_matches("execution reverted").trim_start_matches(':').trim()).trim_end_matches(": ").to_string() }
    else if m.is_empty() { "сеть ответила ошибкой".into() }
    else { format!("сеть ответила: {}", m.chars().take(140).collect::<String>()) }
}
fn hex_u128(v: &Value) -> Result<u128, String> {
    let s = v.as_str().ok_or("bad hex")?.trim_start_matches("0x");
    let s = s.trim_start_matches('0');
    if s.len() > 32 { return Err("число больше, чем умеет кошелёк".into()) }
    u128::from_str_radix(if s.is_empty() { "0" } else { s }, 16).map_err(|_| "bad hex".into())
}
fn word_u128(hex: &str) -> Result<u128, String> { hex_u128(&Value::String(hex.into())) } // eth_call answers: one 32-byte word
fn qty(v: u128) -> String { format!("0x{v:x}") }

async fn chain_ok(http: &reqwest::Client, url: &str, chain_id: u64) -> Result<(), String> {
    if !url.starts_with("https://") && !(cfg!(debug_assertions) && url.starts_with("http://127.0.0.1")) { return Err("адрес узла сети должен начинаться с https://".into()) }
    if hex_u128(&rpc(http, url, "eth_chainId", json!([])).await?)? != chain_id as u128 { return Err("этот узел обслуживает другую сеть, не ту, что выбрана".into()) }
    Ok(())
}

// ---------------------------------------------------------------- addresses and amounts
pub fn valid_address(s: &str) -> bool { s.len() == 42 && s.starts_with("0x") && s[2..].chars().all(|c| c.is_ascii_hexdigit()) }
fn addr_bytes(s: &str) -> Result<[u8; 20], String> {
    if !valid_address(s) { return Err("это не адрес сети Ethereum".into()) }
    let mut a = [0u8; 20];
    for (i, b) in a.iter_mut().enumerate() { *b = u8::from_str_radix(&s[2 + 2 * i..4 + 2 * i], 16).map_err(|_| "bad address")? }
    // mixed case = EIP-55 checksum: a typo in it is caught here, before money moves
    let body = &s[2..];
    if body.chars().any(|c| c.is_ascii_lowercase()) && body.chars().any(|c| c.is_ascii_uppercase()) && checksum(&a) != s {
        return Err("в адресе опечатка: контрольные заглавные буквы не сходятся".into())
    }
    Ok(a)
}
pub fn checksum(a: &[u8; 20]) -> String {
    let lower: String = a.iter().map(|b| format!("{b:02x}")).collect();
    let h = Keccak256::digest(lower.as_bytes());
    let mut s = String::from("0x");
    for (i, c) in lower.chars().enumerate() {
        let nib = (h[i / 2] >> if i % 2 == 0 { 4 } else { 0 }) & 0xf;
        s.push(if c.is_ascii_digit() || nib < 8 { c } else { c.to_ascii_uppercase() });
    }
    s
}
// ponytail: u128 base units, enough for every listed token (ETH-like 18 decimals: 3.4e20 coins); a token beyond that is refused
pub fn parse_units(s: &str, decimals: u32) -> Result<u128, String> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect::<String>().replace(',', ".");
    let (i, f) = s.split_once('.').unwrap_or((&s, ""));
    if (i.is_empty() && f.is_empty()) || !i.chars().chain(f.chars()).all(|c| c.is_ascii_digit()) { return Err("сумма: только цифры и одна точка".into()) }
    if f.len() > decimals as usize { return Err(format!("сумма: не больше {decimals} знаков после точки")) }
    let digits = format!("{i}{f:0<w$}", w = decimals as usize);
    let digits = digits.trim_start_matches('0');
    if digits.is_empty() { return Err("сумма должна быть больше нуля".into()) }
    digits.parse::<u128>().map_err(|_| "сумма слишком большая".into())
}
pub fn units(v: u128, decimals: u32) -> String {
    let s = format!("{v:0>w$}", w = decimals as usize + 1);
    let (i, f) = s.split_at(s.len() - decimals as usize);
    let f = f.trim_end_matches('0');
    if f.is_empty() { i.to_string() } else { format!("{i}.{f}") }
}
fn word(b: &[u8]) -> [u8; 32] { let mut w = [0u8; 32]; w[32 - b.len()..].copy_from_slice(b); w }
fn erc20_transfer(to: &[u8; 20], amount: u128) -> Vec<u8> {
    let mut d = vec![0xa9, 0x05, 0x9c, 0xbb]; // transfer(address,uint256)
    d.extend_from_slice(&word(to)); d.extend_from_slice(&word(&amount.to_be_bytes())); d
}
fn hex0x(b: &[u8]) -> String { format!("0x{}", b.iter().map(|x| format!("{x:02x}")).collect::<String>()) }

// ---------------------------------------------------------------- balances
#[derive(serde::Serialize)]
pub struct TokenBal { contract: String, symbol: String, amount: String, decimals: u8 }
#[derive(serde::Serialize)]
pub struct Watch { wei: String, tokens: Vec<TokenBal>, native: String }
async fn token_balance(http: &reqwest::Client, url: &str, token: &str, owner: &[u8; 20], at: &str) -> Result<u128, String> {
    let mut d = vec![0x70, 0xa0, 0x82, 0x31]; d.extend_from_slice(&word(owner)); // balanceOf(address)
    word_u128(rpc(http, url, "eth_call", json!([{ "to": token, "data": hex0x(&d) }, at])).await?.as_str().ok_or("bad answer")?)
}
/// the native coin and every known token of the network with a non-zero balance
pub async fn watch(url: &str, chain_id: u64, address: &str) -> Result<Watch, String> {
    let a = addr_bytes(address)?;
    let http = http()?;
    chain_ok(&http, url, chain_id).await?;
    let wei = hex_u128(&rpc(&http, url, "eth_getBalance", json!([address, "latest"])).await?)?;
    let mut tokens = vec![];
    for (sym, c, dec) in net(chain_id).map(|n| n.tokens).unwrap_or(&[]) {
        // one token failing to read never hides the coin: it is just absent this round
        if let Ok(v) = token_balance(&http, url, c, &a, "latest").await { if v > 0 { tokens.push(TokenBal { contract: c.to_string(), symbol: sym.to_string(), amount: v.to_string(), decimals: *dec }) } }
    }
    Ok(Watch { wei: wei.to_string(), tokens, native: net(chain_id).map(|n| n.native).unwrap_or("").into() })
}

// ---------------------------------------------------------------- the transaction
fn rlp_bytes(b: &[u8]) -> Vec<u8> {
    if b.len() == 1 && b[0] < 0x80 { return b.to_vec() }
    let mut o = rlp_len(b.len(), 0x80); o.extend_from_slice(b); o
}
fn rlp_len(n: usize, base: u8) -> Vec<u8> {
    if n < 56 { return vec![base + n as u8] }
    let be: Vec<u8> = n.to_be_bytes().into_iter().skip_while(|x| *x == 0).collect();
    let mut o = vec![base + 55 + be.len() as u8]; o.extend(be); o
}
fn rlp_uint(v: u128) -> Vec<u8> { let be: Vec<u8> = v.to_be_bytes().into_iter().skip_while(|x| *x == 0).collect(); rlp_bytes(&be) }
fn rlp_list(items: &[Vec<u8>]) -> Vec<u8> { let body: Vec<u8> = items.concat(); let mut o = rlp_len(body.len(), 0xc0); o.extend(body); o }

/// one transfer, built and priced by the core; `payload()` is exactly what gets signed
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
pub struct Draft {
    pub chain_id: u64, pub rpc: String, pub nonce: u64, prio: u128, max_fee: u128, gas: u64,
    call_to: [u8; 20], value: u128, data: Vec<u8>,
    pub from: String, pub to: String, pub amount: u128, pub decimals: u8, pub symbol: String, pub token: Option<String>,
    pub native: String, pub network: String, pub test: bool, pub custom: bool, pub new_recipient: bool, pub warn: Option<String>,
    pub fee_max: u128, // gas x max fee + the L1 part with its margin: the most this can cost
}
impl Draft {
    fn fields(&self) -> Vec<Vec<u8>> {
        vec![rlp_uint(self.chain_id as u128), rlp_uint(self.nonce as u128), rlp_uint(self.prio), rlp_uint(self.max_fee), rlp_uint(self.gas as u128),
             rlp_bytes(&self.call_to), rlp_uint(self.value), rlp_bytes(&self.data), rlp_list(&[])]
    }
    /// EIP-1559 signing payload: 0x02 || rlp(fields)
    pub fn payload(&self) -> Vec<u8> { let mut p = vec![2u8]; p.extend(rlp_list(&self.fields())); p }
    pub fn hash(&self) -> [u8; 32] { Keccak256::digest(self.payload()).into() }
    /// the signed transaction (raw bytes for eth_sendRawTransaction)
    pub fn signed(&self, sig: &[u8; 64], parity: u8) -> Vec<u8> {
        let mut f = self.fields();
        let trim = |b: &[u8]| -> Vec<u8> { b.iter().copied().skip_while(|x| *x == 0).collect() };
        f.push(rlp_uint(parity as u128)); f.push(rlp_bytes(&trim(&sig[..32]))); f.push(rlp_bytes(&trim(&sig[32..])));
        let mut p = vec![2u8]; p.extend(rlp_list(&f)); p
    }
}
pub fn tx_hash(raw: &[u8]) -> String { hex0x(&Keccak256::digest(raw)) }

/// fees: base of the latest block x2 (two full blocks of growth) + a tip no lower than the recent median
async fn fees(http: &reqwest::Client, url: &str) -> Result<(u128, u128), String> {
    let blk = rpc(http, url, "eth_getBlockByNumber", json!(["latest", false])).await?;
    let base = blk.get("baseFeePerGas").ok_or("эта сеть без EIP-1559: пока не поддерживается")?;
    let base = hex_u128(base)?;
    let mut tip = rpc(http, url, "eth_maxPriorityFeePerGas", json!([])).await.ok().and_then(|v| hex_u128(&v).ok()).unwrap_or(0);
    if let Ok(h) = rpc(http, url, "eth_feeHistory", json!(["0x5", "latest", [50]])).await {
        let mut r: Vec<u128> = h["reward"].as_array().into_iter().flatten().filter_map(|x| hex_u128(&x[0]).ok()).collect();
        r.sort_unstable();
        if let Some(m) = r.get(r.len() / 2) { tip = tip.max(*m) }
    }
    let tip = tip.max(1_000_000); // 0.001 gwei: some nodes answer 0, a zero tip may wait forever
    Ok((tip, base * 2 + tip))
}
/// what posting this transaction to Ethereum costs on a rollup that charges it apart from gas (OP stack, Scroll)
async fn l1_fee(http: &reqwest::Client, url: &str, l1: L1, unsigned: &[u8]) -> Result<u128, String> {
    let oracle = match l1 { L1::None => return Ok(0), L1::Op => "0x420000000000000000000000000000000000000F", L1::Scroll => "0x5300000000000000000000000000000000000002" };
    // getL1Fee(bytes) with the transaction plus room for its 65-byte signature, so the guess errs high
    let mut tx = unsigned.to_vec(); tx.extend_from_slice(&[0xff; 68]);
    let mut d = vec![0x49, 0x94, 0x8e, 0x0e];
    d.extend_from_slice(&word(&[0x20])); d.extend_from_slice(&word(&(tx.len() as u64).to_be_bytes()));
    d.extend_from_slice(&tx); d.resize(d.len() + (32 - tx.len() % 32) % 32, 0);
    word_u128(rpc(http, url, "eth_call", json!([{ "to": oracle, "data": hex0x(&d) }, "latest"])).await?.as_str().ok_or("bad answer")?)
}

pub struct Ask<'a> { pub rpc: &'a str, pub chain_id: u64, pub from: &'a str, pub to: &'a str, pub amount: &'a str, pub token: Option<&'a str>, pub name: Option<&'a str>, pub symbol: Option<&'a str> }

/// build, price and dry-run one transfer. `amount` = "max" sends the whole native balance minus the most the fee can be
pub async fn prepare(a: Ask<'_>) -> Result<Draft, String> {
    let known = net(a.chain_id);
    let l1 = known.map(|n| n.l1).unwrap_or(L1::None);
    let from = addr_bytes(a.from)?;
    let to = addr_bytes(a.to.trim())?;
    if to == from { return Err("это адрес этого же счёта".into()) }
    if to == [0u8; 20] { return Err("нулевой адрес: всё отправленное туда сгорает".into()) }
    let token = match a.token {
        None => None,
        Some(t) => Some(known.and_then(|n| n.tokens.iter().find(|x| x.1.eq_ignore_ascii_case(t))).ok_or("этот токен кошелёк пока не отправляет")?),
    };
    if let Some((_, c, _)) = token { if addr_bytes(c)? == to { return Err("это адрес самого токена: монеты туда пропадут. Нужен адрес получателя".into()) } }
    let native = known.map(|n| n.native.to_string()).or_else(|| a.symbol.map(|s| s.trim().chars().take(10).collect())).filter(|s| !s.is_empty()).ok_or("у сети нет названия монеты")?;
    let http = http()?;
    chain_ok(&http, a.rpc, a.chain_id).await?;
    let (bal, nonce, code, to_nonce, to_bal) = (
        // «pending»: a send still waiting in the pool has already taken its coins and its nonce (Codex 09.10)
        hex_u128(&rpc(&http, a.rpc, "eth_getBalance", json!([a.from, "pending"])).await?)?,
        hex_u128(&rpc(&http, a.rpc, "eth_getTransactionCount", json!([a.from, "pending"])).await?)? as u64,
        rpc(&http, a.rpc, "eth_getCode", json!([checksum(&to), "latest"])).await?.as_str().unwrap_or("0x").to_string(),
        hex_u128(&rpc(&http, a.rpc, "eth_getTransactionCount", json!([checksum(&to), "latest"])).await?)?,
        hex_u128(&rpc(&http, a.rpc, "eth_getBalance", json!([checksum(&to), "latest"])).await?)?,
    );
    let is_contract = code.len() > 2;
    let (prio, max_fee) = fees(&http, a.rpc).await?;
    let (decimals, symbol) = match token { Some((s, _, d)) => (*d, s.to_string()), None => (18u8, native.clone()) };
    let max = a.amount.trim().eq_ignore_ascii_case("max");
    let amount = if max { 1 } else { parse_units(a.amount, decimals as u32)? }; // max: priced with a stand-in, set below
    let (call_to, value, data) = match token { Some((_, c, _)) => (addr_bytes(c)?, 0, erc20_transfer(&to, amount)), None => (to, amount, vec![]) };
    let tok_bal = match token { Some((_, c, _)) => token_balance(&http, a.rpc, c, &from, "pending").await?, None => 0 };
    if token.is_some() && !max && amount > tok_bal { return Err(format!("на счёте только {} {symbol}", units(tok_bal, decimals as u32))) }
    let mut d = Draft {
        chain_id: a.chain_id, rpc: a.rpc.to_string(), nonce, prio, max_fee, gas: 0, call_to, value: if max { 0 } else { value }, data,
        from: checksum(&from), to: checksum(&to), amount, decimals, symbol, token: token.map(|t| t.1.to_string()),
        native: native.clone(), network: known.map(|n| n.name.to_string()).unwrap_or_else(|| a.name.unwrap_or("Своя сеть").trim().chars().take(40).collect()),
        test: known.is_some_and(|n| n.test), custom: known.is_none(), new_recipient: !is_contract && to_nonce == 0 && to_bal == 0, warn: None, fee_max: 0,
    };
    // the dry run prices the exact call; a refusal (frozen funds, paused token, a contract that rejects coins) stops here.
    // «Всё» is priced twice: first with nothing moving, then with the real amount and calldata (Codex 09.10: gas and the
    // L1 part may depend on both), so the fee that is signed belongs to exactly what is sent
    price(&http, a.rpc, &mut d, is_contract, l1).await?;
    if max {
        for _ in 0..2 {
            if token.is_some() { d.amount = tok_bal; d.data = erc20_transfer(&to, tok_bal) }
            else { d.amount = bal.checked_sub(d.fee_max).filter(|v| *v > 0).ok_or("на счёте не хватает даже на комиссию")?; d.value = d.amount }
            if d.amount == 0 { return Err(format!("на счёте нет {}", d.symbol)) }
            price(&http, a.rpc, &mut d, is_contract, l1).await?;
        }
        if token.is_none() { d.amount = bal.checked_sub(d.fee_max).filter(|v| *v > 0).ok_or("на счёте не хватает даже на комиссию")?; d.value = d.amount }
    }
    let need = d.fee_max + d.value;
    if bal < need {
        return Err(if token.is_some() { format!("не хватает {native} на комиссию: нужно до {}", units(d.fee_max, 18)) }
            else { format!("не хватает на сумму и комиссию: можно отправить до {} {native}", units(bal.saturating_sub(d.fee_max), 18)) })
    }
    if is_contract && token.is_none() { d.warn = Some("Получатель — контракт, а не обычный счёт. Отправляйте, только если знаете, что он принимает монеты.".into()) }
    if d.custom { d.warn = Some(format!("Сеть добавлена вами (номер сети {}). Название и монету сети кошелёк проверить не может.", a.chain_id)) }
    Ok(d)
}

/// gas from the node's dry run of the draft as it stands (+20% room, plain 21000 sends exact) and the fee ceiling:
/// gas x max fee + the L1 part with 50% room (it moves with Ethereum's own fee; an estimate, the node refuses a send it no
/// longer covers, nothing is lost)
async fn price(http: &reqwest::Client, url: &str, d: &mut Draft, is_contract: bool, l1: L1) -> Result<(), String> {
    let call = json!({ "from": d.from, "to": hex0x(&d.call_to), "value": qty(d.value), "data": hex0x(&d.data) });
    let est = hex_u128(&rpc(http, url, "eth_estimateGas", json!([call])).await?)? as u64;
    d.gas = if est == 21_000 && d.data.is_empty() && !is_contract { est } else { est + est / 5 };
    let l1 = l1_fee(http, url, l1, &d.payload()).await?;
    d.fee_max = d.gas as u128 * d.max_fee + l1 + l1 / 2;
    Ok(())
}

/// the most of the native coin that can leave right now, priced for the real recipient when it is known
pub async fn max_native(rpc_url: &str, chain_id: u64, from: &str, to: Option<&str>) -> Result<String, String> {
    let to = to.map(str::trim).filter(|t| valid_address(t)).unwrap_or("0x000000000000000000000000000000000000dEaD"); // no recipient yet: an ordinary address
    let d = prepare(Ask { rpc: rpc_url, chain_id, from, to, amount: "max", token: None, name: None, symbol: Some("?") }).await?;
    Ok(units(d.amount, 18))
}

// ---------------------------------------------------------------- permissions: who may spend our tokens
// ponytail: the spenders people approve most (routers, aggregators, marketplaces), the same addresses on most networks.
// An allowance to any other contract is not seen here; finding every one means reading the whole Approval log of the
// chain (an indexer). The screen says so.
const SPENDERS: &[(&str, &str)] = &[
    ("Permit2 (Uniswap)", "0x000000000022D473030F116dDEE9F6B43aC78BA3"),
    ("Uniswap Universal Router", "0x66a9893cC07D91D95644AEDD05D03f95e1dBA8Af"),
    ("Uniswap Universal Router (старый)", "0x3fC91A3afd70395Cd496C647d5a6CC9D4B2b7FAD"),
    ("Uniswap SwapRouter02", "0x68b3465833fb72A70ecDF485E0e4C7bD8665Fc45"),
    ("Uniswap V3 Router", "0xE592427A0AEce92De3Edee1F18E0157C05861564"),
    ("Uniswap V2 Router", "0x7a250d5630B4cF539739dF2C5dAcb4c659F2488D"),
    ("1inch v6", "0x111111125421cA6dc452d289314280a0f8842A65"),
    ("1inch v5", "0x1111111254EEB25477B68fb85Ed929f73A960582"),
    ("0x Exchange Proxy", "0xDef1C0ded9bec7F1a1670819833240f027b25EfF"),
    ("LI.FI", "0x1231DEB6f5749EF6cE6943a275A1D3E7486F4EaE"),
    ("ParaSwap v6", "0x6A000F20005980200259B80c5102003040001068"),
    ("ParaSwap v5", "0x216B4B4Ba9F3e719726886d34a177484278Bfcae"), // its TokenTransferProxy holds the approvals, not Augustus (Codex 09.10)
    ("CoW Protocol", "0xC92E8bdf79f0507f65a392b0ab4667716BFE0110"),
    ("PancakeSwap Router", "0x10ED43C718714eb63d5aA57B78B54704E256024E"),
    ("PancakeSwap Smart Router", "0x13f4EA83D0bd40E75C8222255bc855a974568Dd4"),
    ("OpenSea Seaport", "0x00000000000000ADc04C56Bf30aC9d3c0aAF14dC"),
    ("OpenSea Conduit", "0x1E0049783F008A0085193E00003D00cd54003c71"),
    ("KyberSwap", "0x6131B5fae19EA4f9D964eAc0408E4408b66337b5"),
    ("Odos", "0xCf5540fFFCdC3d510B18bFcA6d2b9987b0772559"),
];
#[derive(serde::Serialize)]
pub struct Allowance { token: String, symbol: String, decimals: u8, spender: String, name: String, amount: String, unlimited: bool }
/// one JSON-RPC batch; a node that refuses batches gets the calls one by one
async fn rpc_many(http: &reqwest::Client, url: &str, calls: &[Value]) -> Vec<Option<Value>> {
    let body: Vec<Value> = calls.iter().enumerate().map(|(i, p)| json!({ "jsonrpc": "2.0", "id": i, "method": "eth_call", "params": p })).collect();
    if let Ok(r) = http.post(url).json(&body).send().await {
        if let Ok(Value::Array(a)) = r.json::<Value>().await {
            let mut out = vec![None; calls.len()];
            for v in a { if let (Some(i), Some(res)) = (v["id"].as_u64(), v.get("result")) { if let Some(o) = out.get_mut(i as usize) { *o = Some(res.clone()) } } }
            if out.iter().any(Option::is_some) { return out }
        }
    }
    let mut out = vec![];
    for p in calls { out.push(rpc(http, url, "eth_call", p.clone()).await.ok()) }
    out
}
/// every non-zero allowance of our known tokens to the known spenders
pub async fn allowances(url: &str, chain_id: u64, owner: &str) -> Result<Vec<Allowance>, String> {
    let o = addr_bytes(owner)?;
    let http = http()?;
    chain_ok(&http, url, chain_id).await?;
    let tokens = net(chain_id).map(|n| n.tokens).unwrap_or(&[]);
    let pairs: Vec<(&(&str, &str, u8), &(&str, &str))> = tokens.iter().flat_map(|t| SPENDERS.iter().map(move |sp| (t, sp))).collect();
    let calls: Vec<Value> = pairs.iter().map(|((_, c, _), (_, sp))| {
        let mut d = vec![0xdd, 0x62, 0xed, 0x3e]; d.extend_from_slice(&word(&o)); d.extend_from_slice(&word(&addr_bytes(sp).unwrap())); // allowance(owner,spender)
        json!([{ "to": c, "data": hex0x(&d) }, "latest"])
    }).collect();
    let mut out = vec![];
    for (((sym, c, dec), (name, sp)), r) in pairs.iter().zip(rpc_many(&http, url, &calls).await) {
        let Some(h) = r.as_ref().and_then(Value::as_str) else { continue };
        let raw = h.trim_start_matches("0x").trim_start_matches('0');
        if raw.is_empty() { continue }
        // ponytail: above u128 reads as «без предела»: more base units than any listed token has in existence (max uint is the usual case)
        let unlimited = raw.len() > 32;
        let amount = if unlimited { String::new() } else { word_u128(h).map(|v| units(v, *dec as u32)).unwrap_or_default() };
        out.push(Allowance { token: c.to_string(), symbol: sym.to_string(), decimals: *dec, spender: sp.to_string(), name: name.to_string(), amount, unlimited });
    }
    Ok(out)
}
/// approve(spender, 0) on one of our known tokens: the allowance goes back to nothing
pub async fn prepare_revoke(url: &str, chain_id: u64, from: &str, token: &str, spender: &str) -> Result<Draft, String> {
    let n = net(chain_id).ok_or("нет такой сети")?;
    let (sym, c, dec) = n.tokens.iter().find(|t| t.1.eq_ignore_ascii_case(token)).ok_or("этот токен кошелёк не знает")?;
    let (fa, sp) = (addr_bytes(from)?, addr_bytes(spender)?);
    let http = http()?;
    chain_ok(&http, url, chain_id).await?;
    let bal = hex_u128(&rpc(&http, url, "eth_getBalance", json!([from, "pending"])).await?)?;
    let nonce = hex_u128(&rpc(&http, url, "eth_getTransactionCount", json!([from, "pending"])).await?)? as u64;
    let (prio, max_fee) = fees(&http, url).await?;
    let mut data = vec![0x09, 0x5e, 0xa7, 0xb3]; data.extend_from_slice(&word(&sp)); data.extend_from_slice(&[0u8; 32]); // approve(spender, 0)
    let mut d = Draft {
        chain_id, rpc: url.to_string(), nonce, prio, max_fee, gas: 0, call_to: addr_bytes(c)?, value: 0, data,
        from: checksum(&fa), to: checksum(&sp), amount: 0, decimals: *dec, symbol: sym.to_string(), token: Some(c.to_string()),
        native: n.native.into(), network: n.name.into(), test: n.test, custom: false, new_recipient: false, warn: None, fee_max: 0,
    };
    price(&http, url, &mut d, true, n.l1).await?;
    if bal < d.fee_max { return Err(format!("не хватает {} на комиссию: нужно до {}", n.native, units(d.fee_max, 18))) }
    Ok(d)
}

// ---------------------------------------------------------------- history (an explorer's index: a node has no «transactions of an address»)
// What leaves the machine: the address and the network, to a public Blockscout instance (Routescan for Avalanche). Base,
// Arbitrum, Polygon and BNB put their explorers behind a bot wall: no history there, the screen links the explorer.
fn scout(id: u64) -> Option<String> {
    let h = match id {
        1 => "eth.blockscout.com", 10 => "optimism.blockscout.com", 100 => "gnosis.blockscout.com", 534352 => "scroll.blockscout.com",
        57073 => "explorer.inkonchain.com", 130 => "unichain.blockscout.com", 480 => "worldchain-mainnet.explorer.alchemy.com",
        11155111 => "eth-sepolia.blockscout.com", 84532 => "base-sepolia.blockscout.com",
        43114 => return Some("https://api.routescan.io/v2/network/mainnet/evm/43114/etherscan/api".into()),
        _ => return None,
    };
    Some(format!("https://{h}/api/v2"))
}
pub fn has_history(id: u64) -> bool { scout(id).is_some() }
#[derive(serde::Serialize, Debug)]
pub struct HMove { mint: Option<String>, amount: String, decimals: u8, incoming: bool, sym: String }
#[derive(serde::Serialize, Debug)]
pub struct HTx { sig: String, time: Option<i64>, ok: bool, fee: Option<String>, moves: Vec<HMove>, peer: Option<String> }
/// "2025-10-09T12:34:56.000000Z" -> unix seconds (UTC only, as explorers write it)
fn iso_secs(s: &str) -> Option<i64> {
    let (y, mo, d) = (s.get(0..4)?.parse::<i64>().ok()?, s.get(5..7)?.parse::<i64>().ok()?, s.get(8..10)?.parse::<i64>().ok()?);
    let (h, mi, se) = (s.get(11..13)?.parse::<i64>().ok()?, s.get(14..16)?.parse::<i64>().ok()?, s.get(17..19)?.parse::<i64>().ok()?);
    let (y2, m2) = if mo <= 2 { (y - 1, mo + 9) } else { (y, mo - 3) }; // days from civil (Howard Hinnant)
    let era = y2.div_euclid(400); let yoe = y2 - era * 400; let doy = (153 * m2 + 2) / 5 + d - 1; let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some((era * 146097 + doe - 719468) * 86400 + h * 3600 + mi * 60 + se)
}
/// the latest operations of `address`: coin moves and token transfers per transaction, newest first. Tokens outside the
/// network's known list show their contract, never the name they gave themselves (spam airdrops borrow names)
pub async fn history(chain_id: u64, address: &str) -> Result<Vec<HTx>, String> {
    let me = checksum(&addr_bytes(address)?).to_lowercase();
    let base = scout(chain_id).ok_or("для этой сети история пока не собирается")?;
    let http = http()?;
    let get = |url: String| { let h = http.clone(); async move {
        let r = h.get(url).header("accept", "application/json").send().await.map_err(|_| "обозреватель сети не ответил".to_string())?;
        match r.status().as_u16() { 200 => {}, 429 => return Err("сеть просит подождать (слишком много запросов), попробуйте через минуту".to_string()), s => return Err(format!("обозреватель сети ответил ошибкой {s}")) }
        r.json::<Value>().await.map_err(|_| "обозреватель сети ответил не тем".to_string())
    } };
    let known = |c: &str| net(chain_id).and_then(|n| n.tokens.iter().find(|t| t.1.eq_ignore_ascii_case(c))).map(|t| t.0.to_string());
    let lc = |v: &Value| v.as_str().unwrap_or("").to_lowercase();
    let native = net(chain_id).map(|n| n.native).unwrap_or("ETH").to_string();
    let mut out: Vec<HTx> = vec![];
    let mut add = |hash: String, time: Option<i64>, ok: bool, fee: Option<String>, mv: Option<HMove>, peer: Option<String>| {
        let i = match out.iter().position(|t| t.sig == hash) { Some(i) => i, None => { out.push(HTx { sig: hash, time, ok, fee: None, moves: vec![], peer: None }); out.len() - 1 } };
        let t = &mut out[i];
        if fee.is_some() { t.fee = fee }
        if t.peer.is_none() { t.peer = peer }
        t.ok &= ok;
        if let Some(m) = mv { t.moves.push(m) }
    };
    let short = |c: &str| if c.len() > 10 { format!("{}…{}", &c[..6], &c[c.len() - 4..]) } else { c.to_string() };
    if base.contains("routescan") {
        let txs = get(format!("{base}?module=account&action=txlist&address={address}&page=1&offset=50&sort=desc")).await?;
        for x in txs["result"].as_array().into_iter().flatten() {
            let (from, to) = (lc(&x["from"]), lc(&x["to"]));
            let value: u128 = x["value"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0);
            let ours = from == me;
            let fee = ours.then(|| x["gasUsed"].as_str().and_then(|g| g.parse::<u128>().ok()).zip(x["gasPrice"].as_str().and_then(|g| g.parse::<u128>().ok())).map(|(a, b)| (a * b).to_string())).flatten();
            let mv = (value > 0 && from != to).then(|| HMove { mint: None, amount: value.to_string(), decimals: 18, incoming: !ours, sym: native.clone() });
            add(x["hash"].as_str().unwrap_or("").into(), x["timeStamp"].as_str().and_then(|t| t.parse().ok()), x["isError"].as_str() != Some("1"), fee, mv, Some(if ours { to } else { from }));
        }
        let tt = get(format!("{base}?module=account&action=tokentx&address={address}&page=1&offset=50&sort=desc")).await?;
        for x in tt["result"].as_array().into_iter().flatten() {
            let (from, to, c) = (lc(&x["from"]), lc(&x["to"]), x["contractAddress"].as_str().unwrap_or("").to_string());
            if from == to { continue } // to ourselves: the balance did not change
            let mv = HMove { mint: Some(c.clone()), amount: x["value"].as_str().unwrap_or("0").into(), decimals: x["tokenDecimal"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0), incoming: to == me, sym: known(&c).unwrap_or_else(|| short(&c)) };
            add(x["hash"].as_str().unwrap_or("").into(), x["timeStamp"].as_str().and_then(|t| t.parse().ok()), true, None, Some(mv), Some(if to == me { from } else { to }));
        }
    } else {
        let txs = get(format!("{base}/addresses/{address}/transactions")).await?;
        for x in txs["items"].as_array().into_iter().flatten() {
            let (from, to) = (lc(&x["from"]["hash"]), lc(&x["to"]["hash"]));
            let value: u128 = x["value"].as_str().and_then(|v| v.parse().ok()).unwrap_or(0);
            let ours = from == me;
            let fee = if ours { x["fee"]["value"].as_str().map(str::to_string) } else { None };
            let mv = (value > 0 && from != to).then(|| HMove { mint: None, amount: value.to_string(), decimals: 18, incoming: !ours, sym: native.clone() });
            add(x["hash"].as_str().unwrap_or("").into(), x["timestamp"].as_str().and_then(iso_secs), x["status"].as_str() != Some("error"), fee, mv, Some(if ours { to } else { from }));
        }
        let tt = get(format!("{base}/addresses/{address}/token-transfers?type=ERC-20")).await?;
        for x in tt["items"].as_array().into_iter().flatten() {
            let (from, to) = (lc(&x["from"]["hash"]), lc(&x["to"]["hash"]));
            if from == to { continue } // to ourselves: the balance did not change
            let c = x["token"]["address_hash"].as_str().or(x["token"]["address"].as_str()).unwrap_or("").to_string();
            let mv = HMove { mint: Some(c.clone()), amount: x["total"]["value"].as_str().unwrap_or("0").into(), decimals: x["total"]["decimals"].as_str().and_then(|d| d.parse().ok()).unwrap_or(0), incoming: to == me, sym: known(&c).unwrap_or_else(|| short(&c)) };
            add(x["transaction_hash"].as_str().or(x["tx_hash"].as_str()).unwrap_or("").into(), x["timestamp"].as_str().and_then(iso_secs), true, None, Some(mv), Some(if to == me { from } else { to }));
        }
    }
    out.retain(|t| t.sig.len() == 66);
    out.sort_by(|a, b| b.time.cmp(&a.time));
    out.truncate(50);
    Ok(out)
}

// ---------------------------------------------------------------- send and follow
pub enum Sent { Ok, Unknown, Rejected(String) }
pub async fn submit(rpc_url: &str, raw: &[u8]) -> Sent {
    let Ok(http) = http() else { return Sent::Unknown };
    match rpc(&http, rpc_url, "eth_sendRawTransaction", json!([hex0x(raw)])).await {
        Ok(_) => Sent::Ok,
        Err(e) if e == "такой перевод уже в сети" => Sent::Ok, // a lost answer re-asked: the node already has it
        Err(e) if e.starts_with("нет связи") || e.starts_with("сеть не ответила") => Sent::Unknown,
        Err(e) => Sent::Rejected(e),
    }
}
pub enum Outcome { Confirmed, Failed, Replaced, Included, Unknown }
/// the newest block that cannot be undone: the node's «finalized», or 64 blocks deep where it has none; None = no answer
async fn final_block(http: &reqwest::Client, url: &str) -> Option<u128> {
    match rpc(http, url, "eth_getBlockByNumber", json!(["finalized", false])).await {
        Ok(b) => hex_u128(&b["number"]).ok(),
        Err(e) if e.starts_with("нет связи") || e.starts_with("сеть не ответила") || e.starts_with("сеть просит") => None,
        Err(_) => hex_u128(&rpc(http, url, "eth_blockNumber", json!([])).await.ok()?).ok().map(|n| n.saturating_sub(64)),
    }
}
/// follow one send to a final answer (Codex 09.10: only finality ends it, success and failure alike).
/// `included` fires when a receipt first appears. Included = in a block, not final yet within the hour (the journal keeps
/// it, the next start goes on). No receipt in 10 min: replaced only when a FINALIZED nonce passed ours (another send with
/// the same key won); otherwise unknown, never «expired»: some node may still hold it.
pub async fn track(rpc_url: &str, hash: &str, from: &str, nonce: u64, included: impl FnOnce()) -> Outcome {
    let Ok(http) = http() else { return Outcome::Unknown };
    let mut included = Some(included);
    let local = rpc_url.starts_with("http://127.0.0.1"); // a dev chain (anvil; a fork's «finalized» is its fork point): the receipt is final
    let (t0, mut seen) = (std::time::Instant::now(), false);
    while t0.elapsed() < std::time::Duration::from_secs(if seen { 3600 } else { 600 }) {
        if let Ok(r) = rpc(&http, rpc_url, "eth_getTransactionReceipt", json!([hash])).await {
            if !r.is_null() {
                seen = true;
                if let Some(f) = included.take() { f() }
                // the receipt is read afresh every round: a reorg that moves or drops it is followed, not remembered
                let at = hex_u128(&r["blockNumber"]).unwrap_or(u128::MAX);
                if local || final_block(&http, rpc_url).await.is_some_and(|f| f >= at) {
                    return if r["status"].as_str() == Some("0x1") { Outcome::Confirmed } else { Outcome::Failed }
                }
                tokio::time::sleep(std::time::Duration::from_secs(12)).await;
                continue;
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
    if seen { return Outcome::Included }
    let used = rpc(&http, rpc_url, "eth_getTransactionCount", json!([from, "finalized"])).await.ok().and_then(|v| hex_u128(&v).ok());
    let known = rpc(&http, rpc_url, "eth_getTransactionByHash", json!([hash])).await.ok().map(|v| !v.is_null());
    match (used, known) {
        (Some(n), Some(false)) if n > nonce as u128 => Outcome::Replaced,
        _ => Outcome::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checks() {
        assert!(valid_address("0x9858EfFD232B4033E47d90003D41EC34EcaEda94"));
        assert!(!valid_address("0x9858"));
        assert!(addr_bytes("0x9858EfFD232B4033E47d90003D41EC34EcaEda94").is_ok());
        assert!(addr_bytes("0x9858effd232b4033e47d90003d41ec34ecaeda94").is_ok()); // all lower: no checksum to check
        assert!(addr_bytes("0x9858EfFD232B4033E47d90003D41EC34EcaEdA94").is_err()); // one letter's case flipped
        assert_eq!(hex_u128(&json!("0x1bc16d674ec80000")).unwrap(), 2_000_000_000_000_000_000);
        assert_eq!(parse_units("1.5", 18).unwrap(), 1_500_000_000_000_000_000);
        assert_eq!(parse_units("0,000001", 6).unwrap(), 1);
        assert!(parse_units("0.0000001", 6).is_err());
        assert!(parse_units("0", 6).is_err());
        assert_eq!(units(1_500_000, 6), "1.5");
        assert_eq!(rlp_uint(0), vec![0x80]);
        assert_eq!(rlp_uint(15), vec![0x0f]);
        assert_eq!(rlp_uint(1024), vec![0x82, 0x04, 0x00]);
        assert_eq!(rlp_list(&[]), vec![0xc0]);
        assert_eq!(rlp_bytes(&[0xaa; 56])[..2], [0xb8, 56]);
        assert!(rpc_error("execution reverted: ERC20: transfer amount exceeds balance").contains("exceeds balance"));
        assert_eq!(iso_secs("2026-10-09T12:34:56.000000Z"), Some(1791549296));
        assert_eq!(iso_secs("1970-01-01T00:00:00Z"), Some(0));
    }
    /// the bytes match an independent encoder: `cast mktx` (foundry) for the same fields and key
    #[test]
    fn signed_bytes_match_cast() {
        let d = Draft {
            chain_id: 1, rpc: String::new(), nonce: 7, prio: 1_500_000_000, max_fee: 30_000_000_000, gas: 21_000,
            call_to: addr_bytes("0x70997970C51812dc3A010C7d01b50e0d17dc79C8").unwrap(), value: 1_000_000_000_000_000_000, data: vec![],
            from: String::new(), to: String::new(), amount: 0, decimals: 18, symbol: String::new(), token: None, native: String::new(),
            network: String::new(), test: false, custom: false, new_recipient: false, warn: None, fee_max: 0,
        };
        // anvil's first dev key (public test key)
        let key = k256::ecdsa::SigningKey::from_slice(&[0xac, 0x09, 0x74, 0xbe, 0xc3, 0x9a, 0x17, 0xe3, 0x6b, 0xa4, 0xa6, 0xb4, 0xd2, 0x38, 0xff, 0x94, 0x4b, 0xac, 0xb4, 0x78, 0xcb, 0xed, 0x5e, 0xfc, 0xae, 0x78, 0x4d, 0x7b, 0xf4, 0xf2, 0xff, 0x80]).unwrap();
        let (sig, rec) = key.sign_prehash_recoverable(&d.hash()).unwrap();
        let raw = d.signed(&sig.to_bytes().into(), rec.to_byte());
        assert_eq!(hex0x(&raw), include_str!("../tests/evm_cast_tx.txt").trim());
    }
}

/// live: a local fork of Base Sepolia (OP stack, so the L1 fee path runs) with funds dealt to anvil's first dev key.
/// `anvil --fork-url https://sepolia.base.org --port 8546` then `cargo test evm_live -- --ignored`
#[cfg(test)]
mod live {
    use super::*;
    const URL: &str = "http://127.0.0.1:8546";
    const KEY: [u8; 32] = [0xac, 0x09, 0x74, 0xbe, 0xc3, 0x9a, 0x17, 0xe3, 0x6b, 0xa4, 0xa6, 0xb4, 0xd2, 0x38, 0xff, 0x94, 0x4b, 0xac, 0xb4, 0x78, 0xcb, 0xed, 0x5e, 0xfc, 0xae, 0x78, 0x4d, 0x7b, 0xf4, 0xf2, 0xff, 0x80];
    const FROM: &str = "0xf39Fd6e51aad88F6F4ce6aB8827279cffFb92266";
    const TO: &str = "0x70997970C51812dc3A010C7d01b50e0d17dc79C8";
    const USDC: &str = "0x036CbD53842c5426634e7929541eC2318f3dCF7e";
    async fn send(d: &Draft) -> Outcome {
        let key = k256::ecdsa::SigningKey::from_slice(&KEY).unwrap();
        let (sig, rec) = key.sign_prehash_recoverable(&d.hash()).unwrap();
        let raw = d.signed(&sig.to_bytes().into(), rec.to_byte());
        assert!(matches!(submit(URL, &raw).await, Sent::Ok));
        track(URL, &tx_hash(&raw), FROM, d.nonce, || {}).await
    }
    async fn bal(a: &str) -> u128 { hex_u128(&rpc(&http().unwrap(), URL, "eth_getBalance", json!([a, "latest"])).await.unwrap()).unwrap() }
    #[test]
    #[ignore]
    fn evm_live() { tauri::async_runtime::block_on(run()) }
    async fn run() {
        let h = http().unwrap();
        rpc(&h, URL, "anvil_setBalance", json!([FROM, qty(10_000_000_000_000_000_000)])).await.unwrap();
        rpc(&h, URL, "anvil_dealERC20", json!([FROM, USDC, qty(100_000_000)])).await.unwrap();
        let ask = |amount: &'static str, token: Option<&'static str>| Ask { rpc: URL, chain_id: 84532, from: FROM, to: TO, amount, token, name: None, symbol: None };
        // native
        let (b0, r0) = (bal(FROM).await, bal(TO).await);
        let d = prepare(ask("0.5", None)).await.unwrap();
        assert_eq!(d.network, "Base Sepolia"); assert!(d.test && d.fee_max > 0);
        assert!(matches!(send(&d).await, Outcome::Confirmed));
        assert_eq!(bal(TO).await - r0, 500_000_000_000_000_000);
        let spent = b0 - bal(FROM).await;
        assert!(spent > 500_000_000_000_000_000 && spent <= 500_000_000_000_000_000 + d.fee_max, "spent {spent} max {}", d.fee_max);
        // token
        let t0 = token_balance(&h, URL, USDC, &addr_bytes(TO).unwrap(), "latest").await.unwrap();
        let d = prepare(ask("12.5", Some(USDC))).await.unwrap();
        assert_eq!((d.symbol.as_str(), d.decimals), ("USDC", 6));
        assert!(matches!(send(&d).await, Outcome::Confirmed));
        assert_eq!(token_balance(&h, URL, USDC, &addr_bytes(TO).unwrap(), "latest").await.unwrap() - t0, 12_500_000);
        assert!(prepare(ask("1000", Some(USDC))).await.unwrap_err().contains("только"));
        assert!(prepare(Ask { to: USDC, ..ask("1", Some(USDC)) }).await.unwrap_err().contains("самого токена"));
        // «Всё»: the account ends with less than the fee ceiling, nothing is refused
        let d = prepare(ask("max", None)).await.unwrap();
        assert!(matches!(send(&d).await, Outcome::Confirmed));
        assert!(bal(FROM).await < d.fee_max);
        assert!(prepare(ask("0.1", None)).await.unwrap_err().contains("не хватает"));

        // permissions: approve Permit2 for 7 USDC, see it, revoke it from the wallet, see nothing
        rpc(&h, URL, "anvil_setBalance", json!([FROM, qty(1_000_000_000_000_000_000)])).await.unwrap();
        let permit2 = "0x000000000022D473030F116dDEE9F6B43aC78BA3";
        let mut d = prepare_revoke(URL, 84532, FROM, USDC, permit2).await.unwrap();
        d.data.truncate(36); d.data.extend_from_slice(&word(&7_000_000u128.to_be_bytes())); // the same call with 7 USDC: an approve to undo
        assert!(matches!(send(&d).await, Outcome::Confirmed));
        let a = allowances(URL, 84532, FROM).await.unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!((a[0].spender.as_str(), a[0].amount.as_str(), a[0].unlimited), (permit2, "7", false));
        let d = prepare_revoke(URL, 84532, FROM, USDC, permit2).await.unwrap();
        assert_eq!((d.to.as_str(), d.amount, d.value), (permit2, 0, 0));
        assert!(matches!(send(&d).await, Outcome::Confirmed));
        assert!(allowances(URL, 84532, FROM).await.unwrap().is_empty());
        assert!(prepare_revoke(URL, 84532, FROM, "0x0000000000000000000000000000000000000001", permit2).await.is_err(), "unknown token refused");
    }
}

/// live, read-only: a busy public address on Ethereum (Blockscout) and Avalanche (Routescan). `cargo test evm_history_live -- --ignored --nocapture`
#[cfg(test)]
mod history_live {
    #[test]
    #[ignore]
    fn evm_history_live() {
        for id in [1u64, 43114] {
            let h = tauri::async_runtime::block_on(super::history(id, "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045")).unwrap();
            assert!(!h.is_empty(), "{id}");
            assert!(h.windows(2).all(|w| w[0].time >= w[1].time), "newest first");
            assert!(h.iter().all(|t| t.sig.len() == 66 && t.time.is_some()));
            println!("{id}: {} entries, first {:?}", h.len(), h[0]);
        }
        assert!(tauri::async_runtime::block_on(super::history(8453, "0xd8dA6BF26964aF9D7eEd9e03E53415D37aA96045")).unwrap_err().contains("не собирается"));
    }
}
