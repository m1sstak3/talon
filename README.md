# Talon

A multi-chain crypto wallet for Windows and Android whose keys never leave your device.
Your networks are metal cards on a ring around you; the one you hold docks in front, flips to show its QR code,
and is thrown toward the recipient when you send.

Built with Tauri 2: a Rust core holds the keys, builds, prices, dry-runs and signs every transaction;
the Svelte + three.js interface only shows what the core decided and asks you to confirm it.

> Status: personal project, used with real money by its author. Not audited. Use small amounts first.

**Download:** [latest release](https://github.com/m1sstak3/talon/releases/latest) — the Windows installer and the Android APK.
No need to build anything.

## What it does

- **Solana**: SOL and SPL / Token-2022 tokens (names read from the chain, impersonating tickers flagged), send,
  receive, history, in-wallet swaps through Jupiter (assembled and vetted instruction by instruction in the core).
- **EVM**: 35 networks — Ethereum, Base, Arbitrum, OP, Polygon, BNB Chain, Avalanche, Linea, ZKsync, Scroll, Mantle,
  Celo, Sei, Plasma, Sonic, Gnosis, Unichain, Ink, World Chain, Berachain, Monad, HyperEVM, Robinhood Chain, Soneium,
  Ronin, MegaETH, X Layer, Cronos, Katana, Morph, Flare, Arc, Stable, Kaia, opBNB — plus your own network by RPC URL.
  Native coin and the canonical stablecoins of each network (USDC, USDT / USDT0, USDG); EIP-1559 sends with the rollup
  L1 fee included in the price (OP-stack, Scroll, Morph, Mantle's MNT-priced fee); swaps through LI.FI, each transaction
  decoded and checked by the core (contract, receiver, minimum, tokens) before signing.
- **TON** (read-only for now): TON and USD₮ balances and receiving, at the same address Trust Wallet shows for your words.
- **Networks menu**: search by name, coin, Chain ID or the Russian name; star the networks you keep on the ring; each
  shows what it holds; an explicit «check all» (no silent sweep of every node).
- **Folders**: group your accounts (and watched addresses) into folders; see a folder's total and send from every
  account in it to one address with one password («Перевести со всех», Solana and EVM).
- **Permissions**: see who may spend your tokens (Solana delegates, EVM allowances to well-known routers and
  marketplaces) and revoke them.
- **Ledger** (Solana app, USB): add a Ledger account, sign on the device; nothing secret is stored.
- **AI assistant** (optional, off by default): a local MCP server for Claude Code, Codex or any MCP client. The agent
  reads accounts, folders, balances on every network and history, and may *request* SOL transfers; you confirm them in
  the wallet with your password. Agents never sign or see keys. The server hands every client strict rules, and
  [`agent/SKILL.md`](agent/SKILL.md) is a full operating manual. Settings → «ИИ-помощник» gives the one-line connect command.
- **Add-ons**: your own tab in the wallet from one HTML file (your team's wallets, a project dashboard, a sweep tool).
  Sandboxed, permissions shown before install, never sees keys or the password; with its own permission it may
  *propose* a transfer that you confirm in the wallet's window. Guide: [`docs/ADDONS.md`](docs/ADDONS.md), samples:
  [`addons/hello.html`](addons/hello.html), [`addons/sweep.html`](addons/sweep.html).
- 12-word wallets compatible with Phantom and MetaMask (same derivation paths), key import, watch-only addresses.

## Security model

- Keys live in one file encrypted with Argon2id + XChaCha20-Poly1305; the public account list is bound into the AAD.
  Secrets are decrypted for one operation and wiped.
- The interface never receives a signing primitive: the core keeps the exact transaction it built behind a one-time
  draft id; confirming signs exactly that, once. The signer is verified against the address you saw before broadcast.
- Every send is dry-run on the network before you confirm; the confirmation shows amount, full fee (a ceiling on
  EVM), network and recipient.
- A send is journaled before it leaves; an unknown outcome stays visible and is followed to finality, so you never
  pay twice by retrying.
- Screens with words, keys or a confirmation are excluded from screen capture (Windows: `WDA_EXCLUDEFROMCAPTURE`,
  Android: `FLAG_SECURE`). Copied words skip clipboard history and are wiped after 30 s.
- No telemetry. What leaves the device: RPC calls to public nodes, price ids (never addresses) to price sources,
  and, for EVM history, your address to a public Blockscout instance.

## Build

Requirements: Rust (stable), [Bun](https://bun.sh), the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).

```sh
bun install
bunx tauri dev           # desktop, development build (refuses to sign on mainnets)
bunx tauri build         # installer under src-tauri/target/release/bundle
bunx tauri android build --apk   # Android (needs the Android SDK/NDK)
```

The interface alone runs in a browser with a fake core for design work: `bun run dev`, then open
`http://localhost:1420/?mock=open` (also `?mock=locked`, password `demo-pass`, and `?mock=welcome`).

Tests: `cargo test` in `src-tauri`. Live tests are marked `#[ignore]` and say what they need
(a local `solana-test-validator`, or `anvil --fork-url https://sepolia.base.org --port 8546`).

## License

MIT, see [LICENSE](LICENSE).
