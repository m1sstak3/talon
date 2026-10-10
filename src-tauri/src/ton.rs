//! TON, read-only for now (owner 11.10: «тон можно добавить»). The address the same 12 words give in Trust Wallet:
//! SLIP-10 ed25519 m/44'/607'/index' (vault::ton_key), wallet contract v4R2 (wallet_id 698983191), shown non-bounceable
//! «UQ…» as Trust Wallet, Ledger and Tonkeeper show it. Spec and vectors checked three ways on 11.10 (@ton/ton, tonweb, a
//! sha256-only script; on-chain data of the test phrase's wallets): scratchpad ton-spec.md. Balances: TON Center v3,
//! keyless (1 request a second: calls are spaced), the native coin and USD₮ (Tether's jetton, 6 decimals).
use base64::Engine;
use serde_json::Value;
use sha2::{Digest, Sha256};

/// wallet v4R2 code cell: representation hash and depth (from the official BOC; a constant, so the address is sha256 only)
const V4R2_CODE: [u8; 32] = hex32("feb5ff6820e2ff0d9483e7e0d62c817d846789fb4ae580c878866d959dabd5c0");
const V4R2_DEPTH: u16 = 7;
const V4R2_WALLET_ID: u32 = 698_983_191;
pub const USDT_MASTER: &str = "EQCxE6mUtQJKFnGfaROTKOt1lZbDiiX1kCixRv7Nw2Id_sDs";

const fn hex32(s: &str) -> [u8; 32] {
    let b = s.as_bytes();
    let mut out = [0u8; 32];
    let mut i = 0;
    while i < 32 {
        out[i] = nib(b[2 * i]) << 4 | nib(b[2 * i + 1]);
        i += 1;
    }
    out
}
const fn nib(c: u8) -> u8 { if c <= b'9' { c - b'0' } else { c - b'a' + 10 } }

/// the hash of an ordinary cell without refs: d1 = 0, d2 = floor(bits/8) + ceil(bits/8), data with the completion tag
fn leaf(bits: &[u8], nbits: usize) -> [u8; 32] {
    let mut data = bits[..nbits.div_ceil(8)].to_vec();
    if nbits % 8 != 0 { let last = data.len() - 1; data[last] |= 0x80 >> (nbits % 8); }
    let mut h = Sha256::new();
    h.update([0u8, (nbits / 8 + nbits.div_ceil(8)) as u8]);
    h.update(&data);
    h.finalize().into()
}

/// the v4R2 wallet's address hash for a public key: StateInit {code, data} with data = seqno 0 · wallet_id · key · no plugins
pub fn v4r2(public: &[u8; 32]) -> [u8; 32] {
    let mut bits = vec![0u8; 41]; // 321 bits: 32 + 32 + 256 + 1 (empty plugins dict = 0)
    bits[4..8].copy_from_slice(&V4R2_WALLET_ID.to_be_bytes());
    bits[8..40].copy_from_slice(public);
    let data = leaf(&bits, 321);
    let mut h = Sha256::new();
    h.update([0x02, 0x01, 0x34]); // 2 refs; 5 bits «00110»: no split_depth, no special, code, data, no library
    h.update(V4R2_DEPTH.to_be_bytes());
    h.update(0u16.to_be_bytes()); // the data cell has no refs
    h.update(V4R2_CODE);
    h.update(data);
    h.finalize().into()
}

/// CRC-16/XMODEM over the first 34 bytes of a user-friendly address
fn crc16(b: &[u8]) -> u16 {
    let mut crc: u16 = 0;
    for &x in b { crc ^= (x as u16) << 8; for _ in 0..8 { crc = if crc & 0x8000 != 0 { (crc << 1) ^ 0x1021 } else { crc << 1 } } }
    crc
}
/// basechain address, non-bounceable («UQ…»): what a person copies and pastes
pub fn friendly(hash: &[u8; 32]) -> String {
    let mut b = vec![0x51u8, 0x00];
    b.extend_from_slice(hash);
    b.extend_from_slice(&crc16(&b).to_be_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}
/// a user-friendly mainnet address (either alphabet, any flag): its basechain hash; anything else is refused
pub fn parse(s: &str) -> Option<[u8; 32]> {
    let s = s.trim();
    let b = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(s.replace('+', "-").replace('/', "_")).ok()?;
    if b.len() != 36 || b[0] & 0x80 != 0 || !matches!(b[0] & 0x7f, 0x11 | 0x51) || b[1] != 0 || crc16(&b[..34]).to_be_bytes() != b[34..] { return None }
    b[2..34].try_into().ok()
}
pub fn address(public: &[u8; 32]) -> String { friendly(&v4r2(public)) }

// ---------------------------------------------------------------- balances (TON Center v3, keyless)
static LAST: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
/// one request at a time, at least 1.1 s apart (the keyless limit is 1 per second); a 429 waits and tries once more
async fn get(path: &str) -> Result<Value, String> {
    for attempt in 0..2 {
        let wait = {
            let mut l = LAST.lock().unwrap();
            let now = std::time::Instant::now();
            let at = l.map_or(now, |t| (t + std::time::Duration::from_millis(1100)).max(now));
            *l = Some(at);
            at - now
        };
        tokio::time::sleep(wait).await;
        let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build().map_err(|e| e.to_string())?;
        let r = http.get(format!("https://toncenter.com/api/v3/{path}")).send().await
            .map_err(|e| if e.is_timeout() { "сеть TON не ответила вовремя".to_string() } else { "нет связи с сетью TON".to_string() })?;
        match r.status().as_u16() {
            200 => return r.json().await.map_err(|_| "сеть TON ответила не тем".to_string()),
            429 if attempt == 0 => tokio::time::sleep(std::time::Duration::from_millis(1500)).await,
            429 => return Err("сеть TON просит подождать, попробуйте через минуту".into()),
            s => return Err(format!("узел сети TON ответил ошибкой {s}")),
        }
    }
    Err("сеть TON просит подождать".into())
}

#[derive(serde::Serialize)]
pub struct Watch { pub nano: String, pub usdt: String, pub active: bool }
/// the coin and USD₮ of an address (amounts as integer strings: nanotons, and 6-decimal units)
pub async fn watch(address: &str) -> Result<Watch, String> {
    if parse(address).is_none() { return Err("это не адрес сети TON".into()) }
    let a = get(&format!("account?address={address}")).await?;
    let nano = a["balance"].as_str().filter(|s| s.chars().all(|c| c.is_ascii_digit())).unwrap_or("0").to_string();
    let j = get(&format!("jetton/wallets?owner_address={address}&jetton_address={USDT_MASTER}&limit=1")).await?;
    let usdt = j["jetton_wallets"].get(0).and_then(|w| w["balance"].as_str()).filter(|s| s.chars().all(|c| c.is_ascii_digit())).unwrap_or("0").to_string();
    Ok(Watch { nano, usdt, active: a["status"].as_str() == Some("active") })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore] // network: cargo test --lib ton_live -- --ignored
    fn ton_live() {
        tauri::async_runtime::block_on(async {
            let w = watch("UQAzWZa6nM5mJev91wGc7VCSfBoIsYRqKJpV78N8Add9-RKY").await.unwrap(); // the test phrase's wallet (deployed by strangers)
            assert!(w.active, "deployed wallet reads active");
            let f = watch("UQCD39VS5jcptHL8vMjEXrzGaRcCVYto7HUn4bpAOg8xqEBI").await;
            println!("foundation: {:?}", f.as_ref().map(|w| (&w.nano, &w.usdt)));
        })
    }
    /// vectors from the public BIP39 test phrase «abandon ×11 about» at m/44'/607'/0' (ton-spec.md, three-way checked)
    #[test]
    fn address_matches_the_vectors() {
        let public = hex32("7952e94118f34607c75e23258dd9220d66ccac5a3ee074125c25068e8107bfbf");
        assert_eq!(v4r2(&public), hex32("335996ba9cce6625ebfdd7019ced50927c1a08b1846a289a55efc37c01d77df9"));
        assert_eq!(address(&public), "UQAzWZa6nM5mJev91wGc7VCSfBoIsYRqKJpV78N8Add9-RKY");
        assert_eq!(parse("EQAzWZa6nM5mJev91wGc7VCSfBoIsYRqKJpV78N8Add9-U9d"), Some(v4r2(&public)), "bounceable form of the same wallet");
        assert!(parse("UQAzWZa6nM5mJev91wGc7VCSfBoIsYRqKJpV78N8Add9-RKZ").is_none(), "a typo breaks the checksum");
        assert!(parse("kQAzWZa6nM5mJev91wGc7VCSfBoIsYRqKJpV78N8Add9-fTX").is_none(), "testnet-only form refused on mainnet");
        let seed = crate::vault::ton_key("abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about", 0);
        assert_eq!(ed25519_dalek::SigningKey::from_bytes(&seed).verifying_key().to_bytes(), public, "SLIP-10 m/44'/607'/0'");
    }
}
