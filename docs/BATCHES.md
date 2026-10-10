# One place for every batch — design (11.10, for review before code)

## Today (0.5.0)

Three ways to move money from many own accounts to one address, two storage places:

| Kind | Made by | Waits in | Confirmed by | Agent can poll |
|---|---|---|---|---|
| SOL | agent, add-on, owner (folder) | `agent::Hub.waiting` (immutable `Plan`, 10 min TTL) | `batch_confirm`: rebuild each row at the plan's price, one decryption, verify, journal, send | yes (`request_status`) |
| Solana token | owner (folder) only | session single-send slot (`Draft::SolBatch`) | `sol_batch_confirm`: rebuild at planned amount, skip a moved row, one decryption | no |
| EVM coin / listed token | owner (folder) only | session single-send slot (`Draft::EvmBatch`) | `evm_batch_confirm`: sign drafted hashes, one decryption, recover signer | no |

Gaps: agents and add-ons cannot request tokens or EVM; the owner's token/EVM batch is lost if the sheet closes (the
single-send slot is shared with ordinary sends); EVM drafts are signed as drafted (stale gas after a long read).

## Proposal

`agent::Plan` gets a body; everything waits in the hub (one batch at a time, whoever made it, 10 min TTL, one-shot):

```rust
pub enum Body {
    Sol   { cluster: Cluster, price: u64, rows: Vec<Row> },                              // as today
    Token { cluster: Cluster, mint: String, sym: String, decimals: u8, rows: Vec<TokRow> },
    Evm   { chain_id: u64, rpc: String, token: Option<String>, sym: String, native: String, decimals: u8, rows: Vec<EvmRow> },
}
pub struct TokRow { account, name, from, amount_text: String, amount: u64, cost: u64 }      // cost = fee + rent, lamports
pub struct EvmRow { account, name, from, amount_text: String, amount: u128, fee_max: u128 }
pub enum Net { Sol(Cluster), Evm { chain_id: u64, rpc: String } }                            // for status after confirm
```

- **Plan**: `plan_sol` (today's `plan`), `plan_token`, `plan_evm` — own accounts only, no duplicates, no Ledger, no
  self-transfer, every row priced and dry-run; «max» written out as the exact amount at plan time.
- **View** (the sheet): one shape for all — total in the asset, fees as a ceiling in the network's coin, rows with
  name, address, amount, fee; `by` / `byName`; no text from the requester.
- **Confirm** (`batch_confirm`, the only confirm): rebuild every row now at its planned amount (Solana: fresh
  blockhash; EVM: fresh nonce and gas) and leave out a row whose amount moved or whose fee rose above the shown
  ceiling; one decryption signs the rest (`vault::sign_many` / `sign_evm_many`); each signature checked against its
  sender; all journaled before a byte leaves; each sent; per-row result kept in `closed` for `request_status`.
- **Status**: Solana — one `getSignatureStatuses`; EVM — `eth_getTransactionReceipt` per hash (≤ 50), mapped to
  pending / confirmed / failed / not-sent.
- **Who may make what**:
  - owner (window): any kind; network RPC from the window for EVM (custom networks included);
  - agent (MCP): any kind on networks in the core's table only (core RPC); dev builds: test networks only;
  - add-on: Solana SOL and tokens under «transfer» (EVM later).
- **Rules unchanged**: read budget, one call at a time, request rate (5/min), TTL, destination/amount discipline in
  the manual; the owner's and add-ons' plan ids stay hidden from agents.
- **Removed**: `Draft::SolBatch`, `Draft::EvmBatch`, `sol_batch_*`, `evm_batch_*` (folded into the hub).

## Risks to check

1. Rebuild at confirm for EVM: a row whose fresh `fee_max` exceeds the plan's is skipped, never re-priced silently.
   Nonce taken fresh: two rows from one account are impossible (one row per account).
2. The sheet must show exactly what can be signed: totals are the planned amounts; a skipped row is reported after.
3. `closed` keyed by plan id; EVM status needs the RPC the plan used — stored in `Net`, never taken from the agent.
4. Token «max» for an agent: the exact balance read at plan time; if it changed by confirm, the row is skipped.
