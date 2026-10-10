# Add-ons

An add-on gives Talon a tab of your own: a page next to «Кошелёк / История / Адреса» that can read your accounts and
balances and do whatever your page does with them. Use it for things that belong to you and not to the wallet:
your team's wallets, a dashboard for one project, a tool for your own workflow. Private add-ons stay on your disk;
the wallet's code never learns about them.

Desktop (Windows) for now: Android WebView hands the app bridge to every frame, so add-ons wait there until the frame is isolated.

Start from the sample: [`addons/hello.html`](../addons/hello.html).

## What an add-on is

**One HTML file.** Styles and scripts are inline; there is nothing else to ship. Inside the file, a manifest in this
exact tag:

```html
<script type="application/json" id="talon-addon">
{
  "id": "hello",
  "name": "Hello",
  "version": "1.0.0",
  "description": "What the owner sees before installing",
  "permissions": ["accounts", "balances"],
  "hosts": []
}
</script>
```

| Field | Rule |
|---|---|
| `id` | 2–32 chars: `a-z`, `0-9`, `-`. Installing a file with the same id updates that add-on. |
| `name` | 1–24 chars. It is the tab's title. |
| `version`, `description` | Shown in Settings and before install. |
| `permissions` | Any of `accounts`, `balances`, `keys` (below). Empty = a tab that sees nothing of the wallet. |
| `hosts` | Up to 8 `https://name.domain[:port]` origins your page may `fetch`. Empty = no network at all. |

## Install

Settings → **Дополнения** → **Установить из файла** → pick the `.html`. Talon shows the name, what the add-on asks for
and where it may connect; nothing runs until you press **Установить**. The file is copied into the app folder and that
copy runs (its hash is checked on every load). To update, install the new file again. **Удалить** revokes and deletes it.

## How it runs

- In a sandboxed frame (`sandbox="allow-scripts"`, an opaque origin): no access to the wallet's page, storage,
  cookies or the core. No top navigation, popups or forms.
- Its own Content-Security-Policy: inline scripts and styles of the file, `data:` images and fonts, and `fetch` only
  to the manifest's `hosts`. No child frames or workers; WebRTC is taken away before your first script runs.
- Only the open tab's page is served, so a frame cannot navigate into another add-on's page.
- It talks to the wallet with `postMessage` only. Every call is checked against the permissions you granted.

## The bridge

Copy this helper into your add-on (the sample has it):

```js
const talon = (() => {
  let n = 0; const wait = new Map()
  addEventListener('message', e => {
    const m = e.data
    if (e.source !== parent || !m || m.talon !== 1) return
    if (m.event) return dispatchEvent(new Event('talon:' + m.event))
    const w = wait.get(m.id); if (!w) return
    wait.delete(m.id); m.error ? w.no(new Error(m.error)) : w.ok(m.result)
  })
  return (method, params = {}) => new Promise((ok, no) => { const id = ++n; wait.set(id, { ok, no }); parent.postMessage({ talon: 1, id, method, params }, '*') })
})()
```

| Call | Permission | Returns |
|---|---|---|
| `talon('accounts')` | `accounts` | `[{ name, solana, evm }]`: the owner's accounts (watch-only ones are not included) |
| `talon('balances', { addresses })` | `balances` | `{ network, rows: [{ address, sol, tokens: [{ mint, symbol, amount }] }] }` for up to 50 Solana addresses, on the Solana network chosen in the wallet (mainnet otherwise). `symbol` is set only for known tokens; `sol` is `null` if the node did not answer. |
| `talon('open', { address })` | `accounts` | Shows that account of the owner in the wallet; `false` if it is not theirs. |
| `talon('addKeys', { items: [{ name, key }] })` | `keys` | Offers Solana private keys (base58, 64 bytes, as Phantom exports) as new accounts. The wallet opens its own window with the addresses and asks the owner's password; resolves `{ added: true }` or `{ added: false }` if cancelled. Keys already in the wallet are skipped. |

Events: `talon:accounts` fires when the owner's accounts change (redraw your list).

Limits: 240 calls a minute, one `balances` and one key offer at a time, 1–50 keys per offer (128 chars each),
64 KB per message, 2 MB per file. `balances` is asked through the wallet's own Solana node, so the addresses you pass
reach that node even when your `hosts` is empty.

## What an add-on can never do

See or export the keys or words of accounts already in the wallet, sign anything, see the wallet password (it is
typed only in the wallet's own window, never in your tab), read other add-ons, or reach the network beyond its `hosts`.

A key offer is the one strong permission: the add-on knows the keys it hands over, so whatever lands on those
accounts is as safe as the add-on. Talon says so in the install screen and in the confirmation window.

## Moving money

Add-ons do not send. To move funds from several accounts at once, ask an agent (see [`agent/SKILL.md`](../agent/SKILL.md)):
it requests a batch, and you confirm it in the wallet with your password.
