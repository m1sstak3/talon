---
name: talon-wallet
description: Read balances of the owner's Talon and ask the owner to confirm SOL transfers (one destination, up to 50 own source accounts) through the talon MCP server. Use when the user asks about their wallet balances, or to consolidate / sweep / move SOL between their own accounts or to an address.
---

# Talon for agents

The wallet runs on the owner's Windows PC. You talk to it through the `talon` MCP server
(`agent/talon-mcp.mjs` in the install folder). You never sign and never see keys: a transfer is a
**request**; the owner reads the plan in the wallet window and confirms it with the password.

## Setup (once, by the owner)
1. Wallet: Settings -> «Доступ агента» on. The wallet must be open (unlocked).
2. Register the server: `claude mcp add talon -- node "<install folder>\agent\talon-mcp.mjs"`.

## Tools
- `wallet_accounts` -> own accounts: `id`, `name`, `solana`, `evm`.
- `wallet_balances { network?, account? }` -> per account: `watch.lamports` and `watch.tokens` (integers in base units;
  1 SOL = 1e9 lamports). `network`: `mainnet` (default), `devnet`, `local`.
- `wallet_request_transfer { network?, to, sources: [{ account, amount }] }` -> `{ id, state: "waiting", plan }`.
  `amount` is decimal SOL as a string (`"0.25"`) or `"max"` (whole balance minus the network fee; the account ends at 0).
- `wallet_request_status { id }` -> `waiting` | `closed` (+ `reason`) | `confirmed` (+ per transfer `sig`, `state`).

## Rules
- SOL only (no tokens yet). One destination per request. Sources must be the wallet's own accounts.
- One request waits at a time; at most 5 new requests a minute; a request expires after 10 minutes.
- The plan is fixed: amounts, fees and destination the owner sees are exactly what gets signed. To change anything,
  make a new request after the old one is closed.
- After `confirmed`, poll `wallet_request_status` every few seconds until every transfer is `finalized`
  (or `failed` / `not-sent` with its `error`). Do not create the same request again while one is pending: a
  transfer whose state is `pending` or `unknown` may still land.
- Development builds refuse `mainnet` requests; test on `devnet` or `local`.
- Tell the user plainly what you asked for and that they must confirm it in the wallet.

## Example: sweep three accounts into one
```
wallet_accounts {}
wallet_request_transfer { "network": "mainnet", "to": "<address>",
  "sources": [ { "account": "<id1>", "amount": "max" }, { "account": "<id2>", "amount": "max" }, { "account": "<id3>", "amount": "0.5" } ] }
-> tell the user: "Confirm 3 transfers in Talon"
wallet_request_status { "id": "<id>" }   // repeat until all finalized
```
