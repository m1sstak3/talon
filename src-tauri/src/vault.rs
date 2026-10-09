//! Own vault: any number of wallets (a 12/24-word phrase, or one imported Solana key), encrypted at rest.
//! File = JSON { v, alg, kdf, salt, accounts[], nonce, ct }. Key = Argon2id(passphrase); cipher =
//! XChaCha20-Poly1305 with the header (v + alg + kdf + salt + accounts) as AAD, so the parameters and the
//! public address list cannot be swapped without breaking decryption. The plaintext (the secrets) lives in
//! Zeroizing buffers, is decrypted for one operation and wiped right after.
//! Accounts are public: they sit next to the ciphertext so balances show without the passphrase.

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{Engine, engine::general_purpose::STANDARD as B64};
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce, aead::{Aead, Payload}};
use ed25519_dalek::SigningKey;
use hmac::{Hmac, Mac};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use serde::{Deserialize, Serialize};
use sha2::Sha512;
use sha3::{Digest, Keccak256};
use std::path::Path;
use zeroize::{Zeroize, Zeroizing};

const VERSION: u32 = 2;
pub const MIN_PASS: usize = 10;
// Argon2id at rest: 64 MiB, 3 passes (OWASP floor is 19 MiB / 2 passes). Tests use a cheap profile.
pub const KDF_DEFAULT: Kdf = Kdf { m: 64 * 1024, t: 3, p: 1 };
const KDF_MAX_M: u32 = 1024 * 1024; // a planted file may not ask for more (it would freeze the machine)

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct Kdf { pub m: u32, pub t: u32, pub p: u32 }

/// public part of one account: shown without the passphrase
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Account {
    pub id: String,     // stable: "<wallet>/<index>"
    pub wallet: String, // wallet id
    pub name: String,
    pub index: u32,
    pub solana: String,
    pub evm: Option<String>, // only phrase wallets derive an EVM address
    // the key lives on a Ledger (m/44'/501'/index'): nothing secret here, the device signs. Absent when false (old AAD kept)
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ledger: bool,
}

/// secret part of one wallet (inside the ciphertext)
#[derive(Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
struct Wallet {
    id: String, phrase: Option<String>, sol_key: Option<String>,
    // a key-imported wallet may carry an EVM key added later (owner 09.10: every account on every network it can be)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evm_key: Option<String>,
    // a Ledger account: its Solana public key only (t-018)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    ledger: Option<String>,
}

#[derive(Serialize, Deserialize, Zeroize)]
#[zeroize(drop)]
struct Secrets { wallets: Vec<Wallet> }

#[derive(Serialize, Deserialize)]
struct Header {
    v: u32, alg: String, kdf: Kdf, salt: String, accounts: Vec<Account>,
    // no passphrase, owner's choice: the key is derived from an empty string, so anyone with the file has the money.
    // Absent when false, so files written before this field keep their exact AAD.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    open: bool,
}

#[derive(Serialize, Deserialize)]
struct File { #[serde(flatten)] h: Header, nonce: String, ct: String }

/// what a new wallet is made from
pub enum Source<'a> { Phrase(&'a str), SolanaKey(&'a str), Ledger { pubkey: &'a str, index: u32 } }

fn err<E>(m: &'static str) -> impl FnOnce(E) -> String { move |_| m.to_string() }

// ---------------------------------------------------------------- phrase and keys
pub fn new_phrase() -> Zeroizing<String> {
    let mut ent = Zeroizing::new([0u8; 16]); // 128 bits = 12 words, like Phantom
    getrandom::getrandom(ent.as_mut()).expect("OS randomness");
    Zeroizing::new(bip39::Mnemonic::from_entropy(ent.as_ref()).expect("16 bytes").to_string())
}

/// normalise and validate a typed phrase (word list + checksum: a typo must not silently make another wallet)
pub fn parse_phrase(words: &str) -> Result<Zeroizing<String>, String> {
    let mut norm = Zeroizing::new(String::with_capacity(words.len()));
    for w in words.split_whitespace() {
        if !norm.is_empty() { norm.push(' ') }
        for ch in w.chars() { norm.push(ch.to_ascii_lowercase()) } // BIP39 English: ASCII only (anything else fails the list)
    }
    let n = norm.split(' ').count();
    if ![12, 15, 18, 21, 24].contains(&n) { return Err(format!("нужно 12 или 24 слова, а их {n}")) }
    bip39::Mnemonic::parse_normalized(&norm).map_err(|e| match e {
        bip39::Error::UnknownWord(i) => format!("слова №{} нет в списке BIP39", i + 1),
        bip39::Error::InvalidChecksum => "контрольная сумма не сходится: проверьте порядок и написание".into(),
        _ => "фраза не прошла проверку".into(),
    })?;
    Ok(norm)
}

/// a Solana private key as Phantom exports it: 64 bytes base58 (seed + public key)
pub fn parse_sol_key(s: &str) -> Result<Zeroizing<[u8; 32]>, String> {
    let b = Zeroizing::new(bs58::decode(s.trim()).into_vec().map_err(err("это не ключ Solana (base58)"))?);
    if b.len() != 64 { return Err("ключ Solana — 64 байта в base58".into()) }
    let mut seed = Zeroizing::new([0u8; 32]);
    seed.copy_from_slice(&b[..32]);
    if SigningKey::from_bytes(&seed).verifying_key().as_bytes() != &b[32..] { return Err("половинки ключа не совпадают".into()) }
    Ok(seed)
}

/// an EVM private key as MetaMask exports it: 32 bytes hex, with or without 0x
pub fn parse_evm_key(s: &str) -> Result<Zeroizing<[u8; 32]>, String> {
    let h = s.trim(); let h = h.strip_prefix("0x").unwrap_or(h);
    if h.len() != 64 || !h.bytes().all(|b| b.is_ascii_hexdigit()) { return Err("ключ Ethereum — 64 шестнадцатеричных знака".into()) }
    let mut k = Zeroizing::new([0u8; 32]);
    for (i, b) in k.iter_mut().enumerate() { *b = u8::from_str_radix(&h[2 * i..2 * i + 2], 16).expect("hex checked") }
    k256::SecretKey::from_slice(k.as_ref()).map_err(err("это не ключ Ethereum"))?;
    Ok(k)
}

fn seed64(phrase: &str) -> Zeroizing<[u8; 64]> {
    Zeroizing::new(bip39::Mnemonic::parse_normalized(phrase).expect("validated phrase").to_seed(""))
}
fn hmac512(key: &[u8], parts: &[&[u8]]) -> Zeroizing<[u8; 64]> {
    let mut mac = <Hmac<Sha512> as Mac>::new_from_slice(key).unwrap();
    for p in parts { mac.update(p) }
    let mut out = Zeroizing::new([0u8; 64]);
    out.copy_from_slice(&mac.finalize().into_bytes());
    out
}

/// SLIP-0010 ed25519, Phantom's path m/44'/501'/index'/0'
pub fn solana_key(phrase: &str, index: u32) -> Zeroizing<[u8; 32]> {
    let mut i = hmac512(b"ed25519 seed", &[seed64(phrase).as_ref()]);
    for c in [44u32, 501, index, 0] {
        let (k, ch) = (Zeroizing::new(i[..32].to_vec()), Zeroizing::new(i[32..].to_vec()));
        i = hmac512(&ch, &[&[0], &k, &(c | 0x8000_0000).to_be_bytes()]);
    }
    let mut k = Zeroizing::new([0u8; 32]);
    k.copy_from_slice(&i[..32]);
    k
}
pub fn sol_address(seed: &[u8; 32]) -> String { bs58::encode(SigningKey::from_bytes(seed).verifying_key().as_bytes()).into_string() }

/// BIP32 secp256k1, MetaMask/Phantom path m/44'/60'/0'/0/index
pub fn evm_key(phrase: &str, index: u32) -> Zeroizing<[u8; 32]> {
    use k256::{Scalar, SecretKey, elliptic_curve::PrimeField};
    let mut i = hmac512(b"Bitcoin seed", &[seed64(phrase).as_ref()]);
    for (c, hard) in [(44u32, true), (60, true), (0, true), (0, false), (index, false)] {
        let k = Zeroizing::new(<[u8; 32]>::try_from(&i[..32]).unwrap());
        let ch = Zeroizing::new(i[32..].to_vec());
        i = if hard {
            hmac512(&ch, &[&[0], k.as_ref(), &(c | 0x8000_0000).to_be_bytes()])
        } else {
            let pk = SecretKey::from_slice(k.as_ref()).unwrap().public_key().to_encoded_point(true);
            hmac512(&ch, &[pk.as_bytes(), &c.to_be_bytes()])
        };
        // child = parse256(IL) + k (mod n); IL >= n is astronomically rare (BIP32 would skip the index)
        let il = Scalar::from_repr((*<&[u8; 32]>::try_from(&i[..32]).unwrap()).into()).unwrap();
        let kp = Scalar::from_repr((*k).into()).unwrap();
        let child = (il + kp).to_bytes();
        i[..32].copy_from_slice(&child);
    }
    let mut k = Zeroizing::new([0u8; 32]);
    k.copy_from_slice(&i[..32]);
    k
}
pub fn evm_address(key: &[u8; 32]) -> String {
    let pk = k256::SecretKey::from_slice(key).unwrap().public_key().to_encoded_point(false);
    let h = Keccak256::digest(&pk.as_bytes()[1..]);
    checksum(&hex(&h[12..]))
}
/// the address a recovered signature key stands for (checks a signature against the account the user saw)
pub fn evm_address_of(vk: &k256::ecdsa::VerifyingKey) -> String {
    let h = Keccak256::digest(&vk.to_encoded_point(false).as_bytes()[1..]);
    checksum(&hex(&h[12..]))
}
fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }
/// EIP-55 mixed-case checksum
fn checksum(lower: &str) -> String {
    let h = hex(&Keccak256::digest(lower.as_bytes()));
    let mut s = String::from("0x");
    for (c, hc) in lower.chars().zip(h.chars()) {
        s.push(if c.is_ascii_digit() || hc.to_digit(16).unwrap() < 8 { c } else { c.to_ascii_uppercase() });
    }
    s
}

fn account(w: &Wallet, index: u32, name: &str) -> Account {
    let (solana, evm) = match (&w.phrase, &w.sol_key) {
        (Some(p), _) => (sol_address(&solana_key(p, index)), Some(evm_address(&evm_key(p, index)))),
        (None, Some(k)) => (sol_address(&parse_sol_key(k).expect("stored key is valid")), w.evm_key.as_deref().map(|e| evm_address(&parse_evm_key(e).expect("stored key is valid")))),
        _ => (w.ledger.clone().expect("wallet without a key"), None),
    };
    Account { id: format!("{}/{index}", w.id), wallet: w.id.clone(), name: name.trim().chars().take(40).collect(), index, solana, evm, ledger: w.ledger.is_some() }
}

// ---------------------------------------------------------------- file
fn kdf_key(pass: &str, salt: &[u8], k: Kdf) -> Result<Zeroizing<[u8; 32]>, String> {
    if k.m > KDF_MAX_M || k.t > 10 || k.p > 8 || k.m < 8 * k.p { return Err("хранилище повреждено: неверные параметры".into()) }
    let params = Params::new(k.m, k.t, k.p, Some(32)).map_err(err("bad kdf params"))?;
    let mut out = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params).hash_password_into(pass.as_bytes(), salt, out.as_mut()).map_err(err("kdf failed"))?;
    Ok(out)
}
fn aad(h: &Header) -> Vec<u8> { serde_json::to_vec(h).expect("header json") }

fn seal(sec: &Secrets, pass: &str, kdf: Kdf, accounts: Vec<Account>) -> Result<String, String> {
    // empty = no password (owner's risk); anything else must be a real one
    if !pass.is_empty() && pass.chars().count() < MIN_PASS { return Err(format!("пароль: не короче {MIN_PASS} символов")) }
    let plain = Zeroizing::new(serde_json::to_vec(sec).map_err(err("json"))?);
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 24];
    getrandom::getrandom(&mut salt).map_err(err("rng"))?;
    getrandom::getrandom(&mut nonce).map_err(err("rng"))?;
    let h = Header { v: VERSION, alg: "argon2id+xchacha20poly1305".into(), kdf, salt: B64.encode(salt), accounts, open: pass.is_empty() };
    let key = kdf_key(pass, &salt, kdf)?;
    let ct = XChaCha20Poly1305::new(key.as_ref().into())
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: &plain, aad: &aad(&h) })
        .map_err(err("encrypt failed"))?;
    serde_json::to_string_pretty(&File { h, nonce: B64.encode(nonce), ct: B64.encode(ct) }).map_err(err("json"))
}

fn open(text: &str, pass: &str) -> Result<(Secrets, Vec<Account>, Kdf), String> {
    let f: File = serde_json::from_str(text).map_err(err("хранилище повреждено"))?;
    if f.h.v != VERSION { return Err("хранилище другой версии".into()) }
    let salt = B64.decode(&f.h.salt).map_err(err("хранилище повреждено"))?;
    let nonce = B64.decode(&f.nonce).map_err(err("хранилище повреждено"))?;
    let ct = B64.decode(&f.ct).map_err(err("хранилище повреждено"))?;
    if salt.len() != 16 || nonce.len() != 24 || ct.len() > 1 << 20 { return Err("хранилище повреждено".into()) }
    let key = kdf_key(pass, &salt, f.h.kdf)?;
    let plain = Zeroizing::new(XChaCha20Poly1305::new(key.as_ref().into())
        .decrypt(XNonce::from_slice(&nonce), Payload { msg: &ct, aad: &aad(&f.h) })
        .map_err(err("неверный пароль"))?);
    let sec: Secrets = serde_json::from_slice(&plain).map_err(err("хранилище повреждено"))?;
    // the stored public list must match the keys: a swapped address is how money gets stolen
    for a in &f.h.accounts {
        let w = sec.wallets.iter().find(|w| w.id == a.wallet).ok_or("хранилище повреждено: нет кошелька")?;
        if account(w, a.index, &a.name) != *a { return Err("хранилище повреждено: адрес не совпадает с ключом".into()) }
    }
    Ok((sec, f.h.accounts, f.h.kdf))
}

/// write via temp file, prove it decrypts, keep the previous file as .prev, then swap in
// every read-modify-write of the vault holds this lock: two saves must not interleave (security review 07.10)
static WRITE: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// replace `to` with `tmp`; on Windows with write-through, so the rename itself is on disk when this returns
fn replace(tmp: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH};
        let w = |p: &Path| p.as_os_str().encode_wide().chain([0]).collect::<Vec<u16>>();
        let (a, b) = (w(tmp), w(to));
        if unsafe { MoveFileExW(a.as_ptr(), b.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH) } == 0 { return Err(std::io::Error::last_os_error()) }
        Ok(())
    }
    #[cfg(not(windows))]
    std::fs::rename(tmp, to)
}

/// write `text` to `to` through a unique temp file, synced to disk, then replaced in place (also used for the send journal)
pub fn durable(to: &Path, text: &str) -> Result<(), String> {
    use std::io::Write;
    let mut n = [0u8; 6]; getrandom::getrandom(&mut n).map_err(err("rng"))?;
    let tmp = to.with_extension(format!("{}.tmp", hex(&n)));
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).open(&tmp).map_err(err("не удалось записать хранилище"))?;
    f.write_all(text.as_bytes()).and_then(|_| f.sync_all()).map_err(|_| { let _ = std::fs::remove_file(&tmp); "не удалось записать хранилище".to_string() })?;
    drop(f);
    replace(&tmp, to).map_err(|_| { let _ = std::fs::remove_file(&tmp); "не удалось записать хранилище".to_string() })
}

/// write via temp file, prove it decrypts, keep the previous file as .prev, then swap in
fn write_proven(path: &Path, text: &str, pass: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() { std::fs::create_dir_all(dir).map_err(err("не удалось создать папку"))?; }
    if open(text, pass).is_err() { return Err("проверка записи не прошла".into()) }
    if path.exists() { durable(&path.with_extension("prev"), &read(path)?)?; }
    durable(path, text)?;
    if open(&read(path)?, pass).is_err() { return Err("проверка записи не прошла".into()) }
    Ok(())
}

fn read(path: &Path) -> Result<String, String> { std::fs::read_to_string(path).map_err(err("хранилища нет")) }
fn wallet_id() -> String { let mut b = [0u8; 6]; getrandom::getrandom(&mut b).unwrap(); hex(&b) }

fn new_wallet(src: Source) -> Result<Wallet, String> {
    Ok(match src {
        Source::Phrase(p) => Wallet { id: wallet_id(), phrase: Some(parse_phrase(p)?.to_string()), sol_key: None, evm_key: None, ledger: None },
        Source::SolanaKey(k) => { parse_sol_key(k)?; Wallet { id: wallet_id(), phrase: None, sol_key: Some(k.trim().to_string()), evm_key: None, ledger: None } }
        Source::Ledger { pubkey, .. } => {
            if !matches!(bs58::decode(pubkey).into_vec(), Ok(v) if v.len() == 32) { return Err("Ledger дал не адрес Solana".into()) }
            Wallet { id: wallet_id(), phrase: None, sol_key: None, evm_key: None, ledger: Some(pubkey.to_string()) }
        }
    })
}

// ---------------------------------------------------------------- operations
/// public account list (no passphrase); empty when there is no vault yet
pub fn accounts(path: &Path) -> Result<Vec<Account>, String> {
    if !path.exists() { return Ok(vec![]) }
    let f: File = serde_json::from_str(&read(path)?).map_err(err("хранилище повреждено"))?;
    Ok(f.h.accounts)
}

/// the vault has no password (opens by itself)
pub fn is_open(path: &Path) -> bool {
    read(path).ok().and_then(|t| serde_json::from_str::<File>(&t).ok()).is_some_and(|f| f.h.open)
}

/// set, change or remove (empty `new`) the password
pub fn repass(path: &Path, old: &str, new: &str) -> Result<(), String> {
    let _w = WRITE.lock().unwrap();
    let (sec, accs, kdf) = open(&read(path)?, old)?;
    let text = seal(&sec, new, kdf, accs)?;
    if open(&text, new).is_err() { return Err("проверка записи не прошла".into()) }
    // the backup is replaced FIRST: an interruption can leave the primary under the old password (reported as a failure),
    // never a backup that still opens with it (second security review 07.10)
    durable(&path.with_extension("prev"), &text)?;
    durable(path, &text)?;
    if open(&read(path)?, new).is_err() { return Err("проверка записи не прошла".into()) }
    Ok(())
}

/// first wallet: creates the vault; refuses to overwrite an existing one
pub fn create(path: &Path, src: Source, name: &str, pass: &str, kdf: Kdf) -> Result<Vec<Account>, String> {
    let _w = WRITE.lock().unwrap();
    if path.exists() { return Err("хранилище уже есть".into()) }
    let w = new_wallet(src)?;
    let accs = vec![account(&w, 0, name)];
    write_proven(path, &seal(&Secrets { wallets: vec![w] }, pass, kdf, accs.clone())?, pass)?;
    Ok(accs)
}

/// check the passphrase (unlock). Secrets are dropped (wiped) before returning.
pub fn unlock(path: &Path, pass: &str) -> Result<Vec<Account>, String> {
    let (_sec, accs, _) = open(&read(path)?, pass)?;
    Ok(accs)
}

/// another wallet (new or imported) inside the same vault
pub fn add_wallet(path: &Path, src: Source, name: &str, pass: &str) -> Result<Vec<Account>, String> {
    let _w = WRITE.lock().unwrap();
    let (mut sec, mut accs, kdf) = open(&read(path)?, pass)?;
    let index = if let Source::Ledger { index, .. } = src { index } else { 0 };
    let w = new_wallet(src)?;
    let a = account(&w, index, name);
    if accs.iter().any(|x| x.solana == a.solana) { return Err("этот кошелёк уже есть".into()) }
    accs.push(a);
    sec.wallets.push(w);
    write_proven(path, &seal(&sec, pass, kdf, accs.clone())?, pass)?;
    Ok(accs)
}

/// next derived account of a phrase wallet
pub fn add_account(path: &Path, wallet: &str, name: &str, pass: &str) -> Result<Vec<Account>, String> {
    let _w = WRITE.lock().unwrap();
    let (sec, mut accs, kdf) = open(&read(path)?, pass)?;
    let w = sec.wallets.iter().find(|w| w.id == wallet).ok_or("нет такого кошелька")?;
    if w.ledger.is_some() { return Err("следующий счёт Ledger добавляется через «Подключить Ledger»".into()) }
    if w.phrase.is_none() { return Err("у импортированного ключа один счёт".into()) }
    let index = accs.iter().filter(|a| a.wallet == wallet).map(|a| a.index).max().map_or(0, |m| m + 1);
    accs.push(account(w, index, name));
    write_proven(path, &seal(&sec, pass, kdf, accs.clone())?, pass)?;
    Ok(accs)
}

/// add an EVM key to a key-imported wallet: its account gains an Ethereum address (phrase wallets derive their own)
pub fn add_evm_key(path: &Path, id: &str, key: &str, pass: &str) -> Result<Vec<Account>, String> {
    let _w = WRITE.lock().unwrap();
    let (mut sec, mut accs, kdf) = open(&read(path)?, pass)?;
    let wid = accs.iter().find(|a| a.id == id).ok_or("нет такого счёта")?.wallet.clone();
    let w = sec.wallets.iter_mut().find(|w| w.id == wid).ok_or("нет такого кошелька")?;
    if w.phrase.is_some() { return Err("у кошелька из слов адрес Ethereum уже есть".into()) }
    if w.ledger.is_some() { return Err("ключ Ethereum для счёта Ledger живёт на самом Ledger (позже)".into()) }
    if w.evm_key.is_some() { return Err("ключ Ethereum уже добавлен".into()) }
    let k = parse_evm_key(key)?;
    let mut norm = String::with_capacity(66); norm.push_str("0x"); norm.push_str(&hex(k.as_ref()));
    w.evm_key = Some(norm); // inside the ciphertext; Wallet zeroizes on drop
    for a in accs.iter_mut().filter(|a| a.wallet == wid) { *a = account(w, a.index, &a.name) }
    write_proven(path, &seal(&sec, pass, kdf, accs.clone())?, pass)?;
    Ok(accs)
}

#[allow(dead_code)] // renaming in the UI comes with account settings
/// rename is public data but still needs the passphrase: the header is part of the AAD
pub fn rename(path: &Path, id: &str, name: &str, pass: &str) -> Result<Vec<Account>, String> {
    let _w = WRITE.lock().unwrap();
    let (sec, mut accs, kdf) = open(&read(path)?, pass)?;
    let a = accs.iter_mut().find(|a| a.id == id).ok_or("нет такого счёта")?;
    a.name = name.trim().chars().take(40).collect();
    write_proven(path, &seal(&sec, pass, kdf, accs.clone())?, pass)?;
    Ok(accs)
}

/// sign with a Solana account key: decrypt -> sign -> wipe. Callers build and verify the message first.
pub fn sign_solana(path: &Path, pass: &str, id: &str, msg: &[u8]) -> Result<[u8; 64], String> {
    Ok(sign_many(path, pass, &[(id, msg)])?[0])
}
/// one decryption, several signatures (a confirmed batch): each (account id, message) in order
pub fn sign_many(path: &Path, pass: &str, items: &[(&str, &[u8])]) -> Result<Vec<[u8; 64]>, String> {
    use ed25519_dalek::Signer;
    let (sec, accs, _) = open(&read(path)?, pass)?;
    items.iter().map(|(id, msg)| {
        let a = accs.iter().find(|a| a.id == *id).ok_or("нет такого счёта")?;
        let w = sec.wallets.iter().find(|w| w.id == a.wallet).ok_or("нет такого кошелька")?;
        let mut seed = match (&w.phrase, &w.sol_key) {
            (Some(p), _) => solana_key(p, a.index),
            (None, Some(k)) => parse_sol_key(k)?,
            _ => return Err("этот счёт подписывает Ledger".into()),
        };
        let sk = SigningKey::from_bytes(&seed);
        seed.zeroize();
        Ok(sk.sign(msg).to_bytes()) // SigningKey zeroizes itself on drop (dalek "zeroize" default feature)
    }).collect()
}

/// sign a 32-byte EVM transaction hash with the account's secp256k1 key: decrypt -> sign -> wipe.
/// Returns (r||s, y parity). Callers recover the signer and compare it with the address the user saw.
pub fn sign_evm(path: &Path, pass: &str, id: &str, hash: &[u8; 32]) -> Result<([u8; 64], u8), String> {
    let (sec, accs, _) = open(&read(path)?, pass)?;
    let a = accs.iter().find(|a| a.id == id).ok_or("нет такого счёта")?;
    let w = sec.wallets.iter().find(|w| w.id == a.wallet).ok_or("нет такого кошелька")?;
    let key = match (&w.phrase, &w.evm_key) {
        (Some(p), _) => evm_key(p, a.index),
        (None, Some(k)) => parse_evm_key(k)?,
        _ => return Err("у этого счёта нет ключа Ethereum".into()),
    };
    let sk = k256::ecdsa::SigningKey::from_slice(key.as_ref()).map_err(|_| "ключ Ethereum повреждён".to_string())?; // zeroizes on drop
    let (sig, rec) = sk.sign_prehash_recoverable(hash).map_err(|_| "не удалось подписать".to_string())?;
    Ok((sig.to_bytes().into(), rec.to_byte() & 1))
}

/// what export hands out for one account: the wallet's phrase (phrase wallets), the account's Solana keypair in the
/// Phantom / solana-keygen form (base58 of seed + public key), and its EVM private key (0x hex, MetaMask import)
pub struct Export { pub phrase: Option<Zeroizing<String>>, pub solana: Zeroizing<String>, pub evm: Option<Zeroizing<String>> }

/// decrypt with the password -> copy out one account's secrets -> the rest is wiped on drop
pub fn export(path: &Path, pass: &str, id: &str) -> Result<Export, String> {
    let (sec, accs, _) = open(&read(path)?, pass)?;
    let a = accs.iter().find(|a| a.id == id).ok_or("нет такого счёта")?;
    let w = sec.wallets.iter().find(|w| w.id == a.wallet).ok_or("нет такого кошелька")?;
    let seed = match (&w.phrase, &w.sol_key) {
        (Some(p), _) => solana_key(p, a.index),
        (None, Some(k)) => parse_sol_key(k)?,
        _ if w.ledger.is_some() => return Err("ключи этого счёта живут только на Ledger: выгрузить их нельзя".into()),
        _ => return Err("хранилище повреждено: у кошелька нет ключа".into()), // an error, not a panic: the sheet must not hang
    };
    let mut pair = Zeroizing::new([0u8; 64]);
    pair[..32].copy_from_slice(seed.as_ref());
    pair[32..].copy_from_slice(SigningKey::from_bytes(&seed).verifying_key().as_bytes());
    Ok(Export {
        phrase: w.phrase.as_ref().map(|p| Zeroizing::new(p.clone())),
        solana: Zeroizing::new(bs58::encode(pair.as_ref()).into_string()),
        evm: match (&w.phrase, &w.evm_key) {
            (Some(p), _) => { // written straight into a wiped buffer: no unwiped hex copy on the way
                use std::fmt::Write;
                let mut s = Zeroizing::new(String::with_capacity(66));
                s.push_str("0x");
                for b in evm_key(p, a.index).iter() { let _ = write!(s, "{b:02x}"); }
                Some(s)
            }
            (None, Some(k)) => Some(Zeroizing::new(k.clone())),
            _ => None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const CHEAP: Kdf = Kdf { m: 64, t: 1, p: 1 };
    const ABANDON: &str = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

    #[test]
    fn phantom_and_metamask_compatible() {
        // Solana reference: an independent JS derivation (checked against solana-keygen and Phantom, 2026-09-30)
        assert_eq!(sol_address(&solana_key(ABANDON, 0)), "HAgk14JpMQLgt6rVgv7cBQFJWFto5Dqxi472uT3DKpqk");
        let w2 = parse_phrase("Legal winner thank year wave sausage worth useful legal winner thank   yellow").unwrap();
        assert_eq!(sol_address(&solana_key(&w2, 0)), "BLeUXTx9thHGT7VJUtF9vHEmfMDgW1nnKZ9UVer2CoLX");
        // EVM reference: the well-known first MetaMask address of the "abandon … about" phrase
        assert_eq!(evm_address(&evm_key(ABANDON, 0)), "0x9858EfFD232B4033E47d90003D41EC34EcaEda94");
    }

    #[test]
    fn phrase_and_key_checks() {
        assert!(parse_phrase(ABANDON).is_ok());
        assert!(parse_phrase(&ABANDON.replace("about", "abandon")).unwrap_err().contains("контрольная"));
        assert!(parse_phrase(&ABANDON.replace("about", "abuot")).unwrap_err().contains("№12"));
        assert!(parse_phrase("abandon about").is_err());
        assert_eq!(new_phrase().split(' ').count(), 12);
        let seed = solana_key(ABANDON, 0);
        let full = [seed.as_ref(), SigningKey::from_bytes(&seed).verifying_key().as_bytes()].concat();
        assert_eq!(sol_address(&parse_sol_key(&bs58::encode(&full).into_string()).unwrap()), sol_address(&seed));
        let mut bad = full.clone(); bad[40] ^= 1;
        assert!(parse_sol_key(&bs58::encode(&bad).into_string()).is_err());
    }

    #[test]
    fn ledger_account() {
        // a Ledger account holds only its public key: listed with its device index, never signs here, nothing to export
        let dir = std::env::temp_dir().join(format!("cw-ledger-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join("vault.json");
        let pass = "correct horse battery";
        create(&p, Source::Phrase(ABANDON), "Основной", pass, CHEAP).unwrap();
        let pk = "7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU";
        let accs = add_wallet(&p, Source::Ledger { pubkey: pk, index: 2 }, "Ledger", pass).unwrap();
        let l = &accs[1];
        assert_eq!((l.solana.as_str(), l.index, l.ledger, l.evm.is_none()), (pk, 2, true, true));
        assert!(!accs[0].ledger);
        assert_eq!(unlock(&p, pass).unwrap(), accs);
        assert!(sign_solana(&p, pass, &l.id, b"m").unwrap_err().contains("Ledger"));
        assert!(export(&p, pass, &l.id).err().unwrap().contains("Ledger"));
        assert!(add_account(&p, &l.wallet, "x", pass).unwrap_err().contains("Ledger"));
        assert!(add_wallet(&p, Source::Ledger { pubkey: pk, index: 2 }, "again", pass).unwrap_err().contains("уже есть"));
        assert!(add_wallet(&p, Source::Ledger { pubkey: "nope", index: 0 }, "bad", pass).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn vault_roundtrip() {
        let dir = std::env::temp_dir().join(format!("cw-vault-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = dir.join("vault.json");
        let pass = "correct horse battery";
        assert!(accounts(&p).unwrap().is_empty());
        let accs = create(&p, Source::Phrase(ABANDON), "Основной", pass, CHEAP).unwrap();
        assert_eq!(accs[0].solana, "HAgk14JpMQLgt6rVgv7cBQFJWFto5Dqxi472uT3DKpqk");
        assert_eq!(accs[0].evm.as_deref(), Some("0x9858EfFD232B4033E47d90003D41EC34EcaEda94"));
        assert!(create(&p, Source::Phrase(ABANDON), "x", pass, CHEAP).is_err(), "never overwrite");
        assert_eq!(unlock(&p, "wrong password!").unwrap_err(), "неверный пароль");
        assert_eq!(unlock(&p, pass).unwrap(), accs);
        assert_eq!(accounts(&p).unwrap(), accs);
        let raw = std::fs::read_to_string(&p).unwrap();
        assert!(!raw.contains("abandon"), "plaintext never on disk");
        // a swapped public address breaks the AAD check
        std::fs::write(dir.join("forged.json"), raw.replace("HAgk14JpMQLgt6rVgv7cBQFJWFto5Dqxi472uT3DKpqk", "BLeUXTx9thHGT7VJUtF9vHEmfMDgW1nnKZ9UVer2CoLX")).unwrap();
        assert!(unlock(&dir.join("forged.json"), pass).is_err());
        // second account, second wallet (imported key), duplicates refused, rename
        let wid = accs[0].wallet.clone();
        let accs = add_account(&p, &wid, "Копилка", pass).unwrap();
        assert_eq!(accs[1].solana, sol_address(&solana_key(ABANDON, 1)));
        let seed = solana_key("legal winner thank year wave sausage worth useful legal winner thank yellow", 0);
        let key = bs58::encode([seed.as_ref(), SigningKey::from_bytes(&seed).verifying_key().as_bytes()].concat()).into_string();
        let accs = add_wallet(&p, Source::SolanaKey(&key), "Импорт", pass).unwrap();
        assert_eq!(accs.len(), 3);
        assert_eq!(accs[2].solana, "BLeUXTx9thHGT7VJUtF9vHEmfMDgW1nnKZ9UVer2CoLX");
        assert!(accs[2].evm.is_none());
        assert!(add_wallet(&p, Source::SolanaKey(&key), "again", pass).is_err());
        assert!(add_account(&p, &accs[2].wallet, "x", pass).is_err());
        let accs = rename(&p, &accs[1].id, "Сбережения", pass).unwrap();
        assert_eq!(accs[1].name, "Сбережения");
        // signatures verify for derived and imported accounts
        for a in [&accs[1], &accs[2]] {
            let sig = sign_solana(&p, pass, &a.id, b"hello").unwrap();
            let pk = ed25519_dalek::VerifyingKey::from_bytes(&bs58::decode(&a.solana).into_vec().unwrap().try_into().unwrap()).unwrap();
            assert!(pk.verify_strict(b"hello", &ed25519_dalek::Signature::from_bytes(&sig)).is_ok());
        }
        assert!(p.with_extension("prev").exists());
        // export: right password only; the keys re-import to the same addresses
        assert!(export(&p, "wrong password!", &accs[1].id).is_err());
        let e = export(&p, pass, &accs[1].id).unwrap();
        assert_eq!(e.phrase.as_deref().map(|s| s.as_str()), Some(ABANDON));
        assert_eq!(sol_address(&parse_sol_key(&e.solana).unwrap()), accs[1].solana);
        let k: [u8; 32] = (0..32).map(|i| u8::from_str_radix(&e.evm.as_ref().unwrap()[2 + 2 * i..4 + 2 * i], 16).unwrap()).collect::<Vec<_>>().try_into().unwrap();
        assert_eq!(Some(evm_address(&k)), accs[1].evm);
        let e = export(&p, pass, &accs[2].id).unwrap(); // imported key: no phrase, no EVM
        assert!(e.phrase.is_none() && e.evm.is_none());
        assert_eq!(sol_address(&parse_sol_key(&e.solana).unwrap()), accs[2].solana);
        // an EVM key added to the imported account: address shown, key exported, phrase wallets refuse, twice refused
        let ek = format!("0x{}", "4c0883a69102937d6231471b5dbb6204fe5129617082792ae468d01a3f362318");
        assert!(add_evm_key(&p, &accs[2].id, "0x1234", pass).is_err());
        assert!(add_evm_key(&p, &accs[1].id, &ek, pass).is_err());
        let accs = add_evm_key(&p, &accs[2].id, &ek.to_uppercase().replace("0X", ""), pass).unwrap();
        assert_eq!(accs[2].evm.as_deref(), Some("0x2c7536E3605D9C16a7a3D7b1898e529396a65c23"));
        assert_eq!(unlock(&p, pass).unwrap(), accs);
        assert_eq!(export(&p, pass, &accs[2].id).unwrap().evm.as_deref().map(|s| s.as_str()), Some(ek.as_str()));
        assert!(add_evm_key(&p, &accs[2].id, &ek, pass).is_err());
        // no password: owner's choice, opens with the empty string; set and remove again
        assert!(!is_open(&p));
        repass(&p, pass, "").unwrap();
        assert!(is_open(&p) && unlock(&p, "").is_ok() && unlock(&p, pass).is_err());
        assert!(unlock(&p.with_extension("prev"), pass).is_err(), "no backup keeps the old password");
        assert!(repass(&p, "", "short").is_err(), "a real password keeps its minimum");
        repass(&p, "", pass).unwrap();
        assert!(!is_open(&p) && unlock(&p, "").is_err());
        assert!(unlock(&p.with_extension("prev"), "").is_err(), "no backup stays without a password");
        // a forged "open" flag on an encrypted vault opens nothing: the flag is part of the AAD
        let raw = std::fs::read_to_string(&p).unwrap().replacen("\"v\"", "\"open\": true, \"v\"", 1);
        std::fs::write(dir.join("flag.json"), raw).unwrap();
        assert!(is_open(&dir.join("flag.json")) && unlock(&dir.join("flag.json"), "").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // UI checks need a wallet that opens straight to home: a no-password vault of the public «abandon … about» phrase.
    // CW_VAULT=<temp>/vault.json cargo test demo_vault -- --ignored ; then run the dev app with the same CW_VAULT
    #[test]
    #[ignore]
    fn demo_vault() {
        let p = std::path::PathBuf::from(std::env::var("CW_VAULT").expect("CW_VAULT = a throwaway path"));
        assert!(!p.exists(), "refusing to overwrite {p:?}");
        let accs = create(&p, Source::Phrase(ABANDON), "Демо", "", CHEAP).unwrap();
        assert!(is_open(&p) && accs.len() == 1);
        // two more accounts of the same phrase: batch requests need several own sources
        add_account(&p, &accs[0].wallet, "Копилка", "").unwrap();
        assert_eq!(add_account(&p, &accs[0].wallet, "Старый", "").unwrap().len(), 3);
    }
}
