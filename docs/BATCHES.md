# One place for every batch — design (11.10, reviewed) and what is built

## Built (0.6.0)

Every batch — the owner's folder, an agent (MCP), an add-on — is one `agent::Plan` in the hub: one waits at a time,
10 min TTL, one-shot, confirmed by `batch_confirm` with one password.

| Kind | `network` | `token` | Confirm refreshes |
|---|---|---|---|
| SOL | `mainnet` / `devnet` / `local` | — | the blockhash (`solana::refresh`); fixed price for all rows |
| Solana token | same | mint | the blockhash |
| EVM coin / listed token | chain id or name (core table) | — / symbol or contract | nothing: nonce, balance and dry run re-checked (`evm::recheck`), gas and fees frozen |

- Plan (`agent::plan`): own accounts, one row per account and per sender address, no Ledger, no self-transfer; native
  rows with fee ≥ amount refused. Requesters (agent, add-on): an ordinary wallet as recipient (Solana: empty or
  system-owned; EVM: no code); on Solana mainnet only the core's named mints (USDC, USDT).
- Network gate (`agent::Net::parse`): the core's table only (custom EVM networks are not batched); dev builds test money
  only, local chains with a throwaway vault only; release builds never local.
- Who waits: the owner's own plan replaces whatever waits; a requester waits its turn.
- A requester's plan to an address new on its network: the owner types its last 4 characters (`tail`, core-checked).
- Status for agents: per row from the core's own trackers (`told` → `Hub::seen`), no node calls: pending, confirmed,
  finalized, failed, expired (never landed), not-sent, unknown.
- Add-ons: SOL and Solana tokens under «transfer» (EVM later).
- Removed: `Draft::SolBatch`, `Draft::EvmBatch`, `sol_batch_*`, `evm_batch_*`.
- Not capped: a token row's fee against its amount (fee in the coin); the sheet shows the ceiling.

## Before (0.5.x)

SOL batches lived in the hub; the owner's token and EVM batches lived in the single-send slot (lost when the sheet
closed), and agents / add-ons could request SOL only.

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

## Review (11.10, second agent) — what changes in the design

- **Never rebuild at confirm.** Keep the plan's exact transaction; refresh only what goes stale: Solana — the 32
  blockhash bytes (`solana::refresh`, fee must equal, balance and dry run re-checked); EVM — the nonce, with `gas`,
  `max_fee`, `prio` frozen from the plan (the sheet's hard ceiling is `gas × max_fee`; the L1 part is an estimate
  shown apart). A moved field skips the row; nothing is re-priced silently.
- Names only from the core's tables; an unnamed mint is shown in full. Core warnings travel with the plan to the sheet.
- One row per sender **address**; one EVM confirm at a time; a row whose nonce moved is skipped.
- Agent / add-on plans: recipient-kind guard (SOL: system-owned or empty; tokens: a wallet, not a token account;
  EVM: no code, not a listed token), verified mints and core-table networks only, refuse rows whose fee ≥ amount,
  cap the fee total; for a recipient new on that chain, the owner types the last characters.
- The owner's own batch must not be locked out by a requester holding the slot (owner preempts).
- Status for EVM from the core's own trackers, not receipts per poll.

Done now in the existing batches (0.5.x): token rows = planned message + fresh blockhash; warnings on the sheet; unique
sender addresses; EVM confirms serialized; moved nonce skips a row; native fee ≥ amount refused; tokens to a contract
warned; unnamed mint in full. Live run on local chains (anvil + solana-test-validator): token, ETH (one row skipped
after its nonce moved), SOL — balances exact.
