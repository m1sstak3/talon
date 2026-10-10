//! USD prices, mainnet only. What leaves the machine: the asset ids asked for (token mints, coin names) and our IP;
//! never an address or an amount. A missing or thin price is absent from the answer, never 0.

use serde_json::Value;
use std::collections::HashMap;

const WSOL: &str = "So11111111111111111111111111111111111111112";
/// a pool thinner than this can be pushed to any price: its token gets no price (spam stays out of the total)
const MIN_LIQUIDITY_USD: f64 = 10_000.0;

fn mint_ok(s: &str) -> bool { matches!(bs58::decode(s).into_vec(), Ok(v) if v.len() == 32) }

/// Jupiter price v3 answer -> mint: usd, dropping thin or nonsense entries
pub fn parse_jupiter(v: &Value) -> HashMap<String, f64> {
    v.as_object().into_iter().flatten().filter_map(|(m, e)| {
        let p = e["usdPrice"].as_f64().filter(|p| p.is_finite() && *p > 0.0)?;
        (e["liquidity"].as_f64()? >= MIN_LIQUIDITY_USD).then(|| (m.clone(), p))
    }).collect()
}

/// usd per unit for Solana mints ("SOL" = the native coin) and EVM coins / known tokens by symbol (ETH, POL, USDC...)
pub async fn usd(mints: Vec<String>, coins: Vec<String>) -> Result<HashMap<String, f64>, String> {
    let http = reqwest::Client::builder().timeout(std::time::Duration::from_secs(10)).build().map_err(|e| e.to_string())?;
    let get = |url: String| { let h = http.clone(); async move {
        let r = h.get(url).send().await.map_err(|_| "нет связи с источником цен".to_string())?;
        if !r.status().is_success() { return Err(format!("источник цен ответил {}", r.status().as_u16())) }
        r.json::<Value>().await.map_err(|_| "источник цен ответил не тем".to_string())
    } };
    let mut out = HashMap::new();
    let mut ids: Vec<&str> = mints.iter().map(String::as_str).filter(|m| *m == "SOL" || mint_ok(m)).map(|m| if m == "SOL" { WSOL } else { m }).collect();
    ids.sort_unstable(); ids.dedup();
    for chunk in ids.chunks(50) {
        for (m, p) in parse_jupiter(&get(format!("https://lite-api.jup.ag/price/v3?ids={}", chunk.join(","))).await?) {
            out.insert(if m == WSOL { "SOL".into() } else { m }, p);
        }
    }
    let gecko: Vec<(&str, &str)> = coins.iter().filter_map(|c| Some((c.as_str(), match c.as_str() {
        "ETH" => "ethereum", "POL" => "polygon-ecosystem-token", "BNB" => "binancecoin", "AVAX" => "avalanche-2", "S" => "sonic-3",
        "xDAI" => "xdai", "BERA" => "berachain-bera", "MON" => "monad", "HYPE" => "hyperliquid",
        "XPL" => "plasma", "MNT" => "mantle", "SEI" => "sei-network", "CELO" => "celo", "FLR" => "flare-networks", "RON" => "ronin",
        "OKB" => "okb", "CRO" => "crypto-com-chain", "KAIA" => "kaia",
        "USDC" | "USDC.e" => "usd-coin", "USDT" | "USDT0" => "tether", "DAI" => "dai", "USDG" => "global-dollar", // EVM tokens: only the core's known list ever asks
        _ => return None,
    }))).collect();
    if !gecko.is_empty() {
        let q: Vec<&str> = gecko.iter().map(|(_, id)| *id).collect();
        let v = get(format!("https://api.coingecko.com/api/v3/simple/price?ids={}&vs_currencies=usd", q.join(","))).await?;
        for (sym, id) in gecko { if let Some(p) = v[id]["usd"].as_f64().filter(|p| p.is_finite() && *p > 0.0) { out.insert(sym.into(), p); } }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn thin_and_bad_prices_are_absent() {
        let v = json!({
            "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v": { "usdPrice": 0.9996, "liquidity": 4.5e8 },
            "SpamMint": { "usdPrice": 1200.0, "liquidity": 150.0 },
            "NoLiq": { "usdPrice": 3.0 },
            "Zero": { "usdPrice": 0.0, "liquidity": 1e9 },
        });
        let p = parse_jupiter(&v);
        assert_eq!(p.len(), 1);
        assert!((p["EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"] - 0.9996).abs() < 1e-9);
    }
}
