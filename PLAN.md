# Talon — план

Кошелёк, ключи которого живут только на устройстве владельца. Сначала компьютер (Windows), потом телефон (Android). Сначала Solana, следом EVM.
Стек: **Tauri 2** — ядро на Rust держит ключи, собирает и подписывает переводы; красивый экран только рисует.
План сверен с Codex (07.10.2026): порядок, хранилище, защита подтверждения.

## Этапы — что можно попробовать на каждом

| # | Этап | Что пробуешь ты |
|---|------|-----------------|
| 0 | **Облик**: образцы, 2–3 направления, выбранное — кликабельный макет без ключей (все экраны, ошибки, подтверждение) | Пролистать экраны, почувствовать анимации |
| 1 | **Скелет + просмотр по адресу**: программа ставится, вставляешь любой адрес — видишь баланс SOL и токенов, «Получить» с QR, выбор сети | Смотреть свой настоящий кошелёк без ключей |
| 2 | **Своё хранилище**: создать/восстановить кошелёк (12 слов, как Phantom), резервная копия, «защищённое окно» для пароля и слов | Создать кошелёк, восстановить те же слова в Phantom — адрес совпадёт |
| 3 | **Отправка SOL** (devnet → mainnet): экран подтверждения — сумма, полная комиссия, сеть, получатель; пароль на каждую отправку; статус и защита от двойной отправки | На devnet — со мной; на mainnet — маленькую сумму сам |
| 4 | **Токены**: обычные SPL, затем проверенная часть Token-2022 | Отправить токен |
| 5 | **Проверка безопасности** вторым агентом — до первых настоящих денег (и повтор перед выпуском) | — |
| 6 | **История, адресная книга, несколько счетов** | Полный кошелёк на каждый день |
| 7 | **Дополнения**: свои интеграции без правки ядра (модель ниже) | Поставить дополнение из файла |
| 8 | **Шлифовка + установщик Windows**: полный проход анимаций, авто-замок | Поставить как обычную программу |
| 9 | **EVM**: Ethereum/Base и др. — отдельный модуль сети | Вторая сеть в том же кошельке |
| 10 | **Телефон (Android)**: та же программа, отпечаток пальца | Поставить на телефон |

## Technical (agents)

**Trust boundary**
- Rust builds and verifies the exact tx; no generic `sign(bytes)` IPC. Main UI gets addresses, balances, tx summaries only.
- ~~Separate secure window~~ → owner's call 07.10 (reviewed with Codex): secrets and the send confirmation are typed in an in-window **sheet**; the window is excluded from capture before anything secret renders (readback-verified). Accepted risk: the bundled frontend is trusted code (strict CSP, no remote scripts); a compromised bundle could read secrets / confirm. Confirmation is one-shot, bound to a core-held draft id, refused while locked; closing cancels in the core first.
- Strict CSP, no remote scripts, no prod DevTools, no telemetry, no secrets in logs/errors/panics/URLs/clipboard.
- Secret windows excluded from capture: `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` per HWND, result checked; Android `FLAG_SECURE` before render.

**Own vault**
- Argon2id (calibrated on device) + XChaCha20-Poly1305; version + params in AAD; bounds-check params/sizes before decrypt. Crates: `argon2`, `chacha20poly1305`, `getrandom`, `zeroize`.
- Passphrase per send (no long-lived KEK). Decrypt → sign → zeroize; no `String`/`Debug`/`Clone`/`serde_json::Value` copies of secrets. Swap/hibernation/dumps remain an accepted residual risk.
- Atomic write (tmp → verify decrypt → rename, keep `.prev`), corruption detection, backup/restore flow.
- Phantom compatibility: BIP39 → SLIP-0010 `m/44'/501'/0'/0'`, test vector.

**Solana**
- Official modular crates: `solana-pubkey/hash/instruction/message/transaction/signature/signer/keypair`, `solana-system-interface`, `solana-compute-budget-interface`, `spl-token`, `spl-token-2022`, `spl-associated-token-account` (minimal features), `solana-rpc-client` (not full `solana-client`).
- `transferChecked` with mint/decimals/owner/program verified; idempotent ATA create with the right token program. Token-2022: allowlist of extensions; transfer hooks / transfer fee / confidential = refused until handled.
- Legacy tx by default; v0 only with ALT, all table addresses resolved before confirm.
- Fee shown = base + CU limit × price + ATA rent. Integer amounts only.
- Blockhash expiry, status tracking, no blind re-send after timeout (check signature status first).
- Network check first: selected cluster ↔ RPC `getGenesisHash`; shown on confirm.
- Own node per network (08.10): Settings → «Свой узел сети» for mainnet/devnet; core checks https + genesis before saving, keeps the URL in `rpc.json` next to the vault (plain: a read key, cannot move money), UI and errors see the host only. Local stays fixed.

**EVM (later)**: `alloy` with selected features (provider, consensus, primitives, signer-local); chain ID, nonce, EIP-1559, gas estimate.

**Integrations**: private integrations (other tools' wallets, team flows) live in add-ons, never in the core (see «Add-ons»).

**Security review (stage 5, Codex 07.10)** — fixed: local-validator signing (dev builds only), local tx id + honest "sent-unknown", finalized expiry, cancel race, vault write lock + fsync + unique tmp, .prev re-encrypted on password change, phrase normalised into a wiped buffer, verified account list after unlock, capture-gated password capsule, permanent-delegate disclosure, single instance.
Swap (09.10): Jupiter lite API (keyless) quote + swap-instructions; core vets every instruction (allowlist: Jupiter route once, ATA create for us, wrap only into our wSOL, SyncNative, close to us; no foreign signer, no approve), reads ALTs from chain, own compute budget, v0 message, dry run with our accounts; confirmation shows simulated arrival, refuses if below the minimum. Wallet fee 0.2% (FEE_BPS 20) via platformFeeBps + FEE_OWNER token account — FEE_OWNER unset until the owner names it.
Pre-send dry run (08.10, Codex): simulateTransaction of the exact unsigned message before the confirmation; a failure is refused with the program's own error line, an unavailable simulation only warns.
Mainnet OPEN (owner 08.10: «иметь возможность перевести эти средства»): release builds sign mainnet, dev builds still refuse it; every send carries a compute-unit cap + priority price (p75 of recent fees, 1k..1M micro-lamports).
Mainnet gate (history): ~~durable pending-send record + restart recovery~~; ~~decoded signed bytes == confirmation (test)~~; second Codex review fixes in 4b93c67; third Codex review: no high/critical (07.10), its 3 medium fixed. Remaining step: owner's word → remove the mainnet refusal, release build, OWNER sends 0.001 SOL once to his own funded address and checks finalized + exact amounts + journal clear after restart; ~~Token-2022 fixtures~~ (07.10: transfer-fee refused, paused mint, frozen-by-default, frozen destination, memo-required destination refused before signing — live tests); ~~release-build capture test~~ (07.10: release exe in front with the password capsule, screen capture shows the window underneath); ~~`cargo --locked` + `cargo audit` + frozen bun install~~ (07.10: no vulnerabilities; 3 notices — bincode/proc-macro-error unmaintained, glib Linux-only; re-run before release); locked screen shows header addresses dimmed / not receivable.

**Tests**
- Integration: `solana-test-validator --clone` (+ pinned fixtures: programs, mints, token accounts, ALTs); LiteSVM for fast instruction tests.
- Agent signer = throwaway keys only; RPC allowlist localhost/devnet, mainnet blocked in test harness (genesis check is not enough on its own).

**Agent access + batch requests (owner 09.10 idea, design with Codex 09.10)**
- Transport: an MCP stdio adapter for agents -> Windows named pipe into the Rust core (random name + session secret per launch, file with owner-only ACL; secret never shown to the model). No HTTP. A same-user process can reach it: this proves the process, not "Claude".
- Methods only: read addresses/balances, create a request, read its status. No signing, no export, no raw instructions. Agents never sign; the owner confirms in the wallet window with the password.
- Request = structured: own source accounts, one destination, amounts or "max". Core checks ownership and duplicates, builds an immutable plan; the password confirms exactly that plan. One pending request at a time, max 50 sources, 5 requests/min, an «agent access» switch in Settings.
- Confirmation for a batch: full destination, network, number of sources, warning for a never-used destination; rows per source (address, balance, amount, fee, remainder); totals per asset and a fee ceiling. Any change = new confirmation.
- v1 signs N separate transactions after one approval (one tx = atomic but max ~10 SOL senders in 1232 bytes: 102 + 113 N). Journal per tx (plan, signed bytes, signature, blockhash, last valid height, state); after a crash check signatures first; resend the same bytes only; unknown stays unknown; the remainder needs a new approval.
- Abuse: prompt-injected agent (plain data on the confirm, no agent prose), destination swap (sign only the stored plan), fee drain (minimum amount, fee ceiling, no auto-retries), replay (one-time request id + expiry), flooding (global limits, bounded queue, off switch).
- v1 scope: Windows, MCP->pipe, read balances, SOL only from own accounts to one recipient, up to 50 transfers, one password confirmation, journal + recovery. Ships with a skill doc telling agents how to use it.
- Built 09.10 (agent.rs, agent/talon-mcp.mjs, agent/SKILL.md): Settings «Доступ агента» (off by default, flag file
  `agent-on`); pipe `\\.\pipe\talon-<rand>` local clients only, discovery `agent.json` {pipe, secret} next to the
  vault, rewritten per launch, removed when off. Methods: accounts, balances, request_transfer, request_status; all
  need the wallet unlocked (proven account list). Plan fixes the priority price, so «max» rows end at exactly 0;
  confirm rebuilds each row (fresh blockhash), skips rows whose fee/balance moved, signs all with one decryption,
  journals all before sending. Dev builds refuse mainnet requests. Tests: agent::limits, agent::batch_live (local
  validator), e2e dev app + adapter + CDP confirm (3 sources, all finalized, exact sums). Not yet: tokens, EVM.

**Add-ons («Дополнения»; model for the owner's word, risks checked with Codex 09.10)**
- Goal: anyone shapes the wallet without touching the core; private integrations ship as add-ons.
- Package: a folder (or .zip) with `manifest.json` {id, name, version, permissions, hosts} + one HTML/JS bundle. Installed from a
  local file the owner picks; no store in v1. Checked on install: paths (no `..`, no symlinks), size caps, the whole package
  hashed and copied into an immutable app folder; the copy runs, never the original. An update = a new install check; any
  new permission asks again.
- Runtime: a sandboxed iframe on its own origin (`addon-<id>`), `sandbox="allow-scripts"` only (no top navigation, popups,
  forms, same-origin); CSP `default-src 'none'`, `connect-src` = the manifest's hosts (asked separately at install, shown by
  name), no img/form/websocket escape hatches beyond those hosts.
- Bridge: postMessage RPC; the app checks `event.source`/origin/shape and a per-session token, and passes calls to the core
  tagged with the add-on id. The core checks the granted permission on EVERY call; the iframe holds no trusted rights.
- Permissions (owner grants per add-on, revocable in Settings → Дополнения): read addresses · read balances · read history
  (per network) · show a panel tab or a home row · propose a transfer. A proposal is the agent request format
  (structured plan, PLAN «Agent access»): the confirmation and the password live only in the wallet's own sheet with the
  add-on's name on it; an add-on panel can never ask for the password (the sheet says so). Never: keys, words, signing,
  raw instructions, other add-ons' data.
- Limits: message size, calls per minute, one pending proposal, CPU/memory watchdog (a hung frame is unloaded).
- Uninstall: revoke rights and stop the frame first, then wipe its storage, caches and files.
- v1 scope: read-only add-ons + panel; proposals in v2 after a security review.
- Built 10.10 (owner's order 10.10: his team's wallets as a private add-on, a 4th tab): addons.rs, App «addon» view + bridge, Sheet
  «addon-keys», docs/ADDONS.md, addons/hello.html. Simplified vs the model above: ONE html file with the manifest inside
  (`<script type="application/json" id="talon-addon">`), picked with a file input, stored as addons/<id>.html + index.json
  next to the vault, sha256 checked on every load; served by the `addon` URI scheme with the manifest's CSP; every add-on is
  a tab (no tab permission). Permissions: accounts · balances (any Solana address, ≤50 per call) · keys (offer Solana keys
  as new accounts: core previews the addresses, the wallet sheet + password adds them; the add-on knows those keys, said
  at install and on the sheet). Bridge gate is in the app (trusted bundle), `keys` re-checked in the core. Limits: 240
  calls/min, one key offer at a time, 1..50 keys, 2 MB. Desktop only: Android WebView gives its JS bridge to every frame;
  on WebView2 the frame's Tauri IPC was tested dead (fetch blocked by CSP, postMessage not delivered; a side-effect call
  had no effect). Not yet: history, transfer proposals, CPU watchdog.

**EVM sends, permissions, Ledger, history (built 09.10, Codex-reviewed in parts)**
- evm.rs keeps its own table by chain id (name, coin, known tokens with on-chain-checked addresses, L1 fee kind, explorer):
  the UI only says which RPC to ask. 18 mainnets (Ethereum, Base, Arbitrum, OP, Polygon, BNB, Avalanche; «Ещё сети»: Linea,
  Unichain, Sonic, Gnosis, Scroll, Ink, World Chain, Berachain, Monad, HyperEVM, Robinhood) + Sepolia, Base Sepolia, anvil.
- Send = EIP-1559 type 2, RLP by hand (bytes equal `cast mktx`), nonce and balances from «pending», tip ≥ median of
  feeHistory, max fee = 2×base + tip, gas = estimate (+20% unless a plain 21000), L1 part via GasPriceOracle (OP stack) /
  L1GasPriceOracle (Scroll) with +50%; «Всё» priced twice with the real amount. Signed hash = the drafted one, signer
  recovered and compared before broadcast; journal first. Outcomes only on finality (receipt re-read; replaced only past a
  finalized nonce; never «expired» for what a node may still hold). Dev builds sign test networks only.
- Permissions: Solana delegates (SPL + Token-2022 layouts), Revoke through the normal draft; EVM allowances of the known
  tokens to 19 known spenders (batch eth_call), revoke = approve(spender, 0). Live tests: local validator, Base Sepolia fork.
- Ledger (Solana app, USB HID via hidapi windows-native): framing + APDUs unit-tested, statically checked by Codex; a device
  test is the owner's. Vault stores only the public key + index; sends/swaps/revokes of such an account sign on the device.
- EVM history: Blockscout v2 / Routescan (no keys); Base, Arbitrum, Polygon, BNB explorers sit behind a bot wall → link only.
