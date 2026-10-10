// Dev only (?mock=welcome|locked|open): a fake core so the intro and the sheet can be checked in a browser tab without
// touching a real vault. Never bundled: main.ts imports it behind import.meta.env.DEV. Demo password: «demo-pass».
import { mockIPC, mockWindows, mockConvertFileSrc } from '@tauri-apps/api/mocks'
import { emit } from '@tauri-apps/api/event'

export function install(kind: string) {
  ;(globalThis as any).isTauri = true
  ;(globalThis as any).__cwMock = true // a browser tab has no window: no fake title-bar buttons (owner 08.10 tried to drag it)
  mockWindows('main')
  mockConvertFileSrc('windows') // ?addon=<id>: the tab loads http://addon.localhost/<id> (a test serves the file)
  const acc = { id: 'demo', name: 'Основной', solana: '7xKXtg2CW87d97TXJSDpbD5jBkheTqA83TZRuJosgAsU', evm: new URLSearchParams(location.search).has('noevm') ? null : '0x52908400098527886E0F7030069857D2E4169EE7', ton: 'UQAzWZa6nM5mJev91wGc7VCSfBoIsYRqKJpV78N8Add9-RKY', wallet: 'w', index: 0 }
  // ?accounts=4: more own accounts, for folders and batches
  const extra = Number(new URLSearchParams(location.search).get('accounts') ?? 1) - 1
  const more = ['9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM', 'HN7cABqLq46Es1jh92dQQisAq662SmxELLLsHHe4YWrH', '2wmVCSfPxGPjrnMMn7rchp4uaeoTqN39mXFC2zhPdri9', 'DRpbCBMxVnDK7maPM5tGv6MvB3v1sRMC86PZ8okm21hy']
  const ext = more.slice(0, Math.max(0, extra)).map((sol, i) => ({ id: `demo${i + 1}`, name: ['Копилка', 'Ферма 1', 'Ферма 2', 'Ферма 3'][i], solana: sol, evm: ('0x' + String(i + 1).repeat(40)) as string | null, ton: null as string | null, wallet: 'w', index: i + 1 }))
  let accounts = kind === 'welcome' ? [] : [acc, ...ext], unlocked = kind === 'open', open = kind === 'open'
  let folders: unknown[] = JSON.parse(localStorage.getItem('mockFolders') ?? '[]')
  const own: Record<string, string> = {}
  const wait = (ms: number) => new Promise(r => setTimeout(r, ms))
  let agentOn = false, waiting: unknown = null
  // dev: window.__cwBatch() plays an agent's request (3 sources, one destination)
  ;(globalThis as any).__cwBatch = () => emit('agent-request', waiting = { id: 'b1', network: 'Solana Mainnet', test: false, to: 'Fk7aUsdcDemoMint1111111111111111111111111111', newRecipient: true, open, count: 3, total: '2.49997', fees: '0.000015',
    rows: [{ name: 'Основной', from: acc.solana, balance: '1.25', amount: '0.5', fee: '0.000005', left: '0.749995' }, { name: 'Копилка', from: '9WzDXwBbmkg8ZTbNMqUxvQRAyrZzDsGYdLVL9zYtAWWM', balance: '1', amount: '0.999995', fee: '0.000005', left: '0' }, { name: 'Старый', from: 'HAgk14JpMQLgt6rVgv7cBQFJWFto5Dqxi472uT3DKpqk', balance: '1', amount: '0.999995', fee: '0.000005', left: '0' }] })
  const changed = () => setTimeout(() => emit('vault-changed', { accounts, unlocked }))
  mockIPC(async (cmd, a: any) => {
    switch (cmd) {
      case 'vault_state': return { accounts, unlocked, open }
      case 'pending_list': return []
      case 'ton_watch': await wait(300); return { nano: '12500000000', usdt: '42000000', active: true }
      case 'addon_list': { const id = new URLSearchParams(location.search).get('addon'); return id ? [{ id, name: 'Сбор', version: '1.0.0', description: '', permissions: ['accounts', 'balances', 'history', 'transfer'], hosts: [] }] : [] }
      case 'addon_open': return null
      case 'sol_batch_plan': await wait(700); return { id: 'sb1', by: 'owner', kind: 'sol', network: 'Solana Mainnet', test: false, to: a.to, newRecipient: false, open, count: a.sources.length,
        total: String(a.sources.length * 25), sym: 'USDC', fees: (a.sources.length * 0.000005).toFixed(6), feeSym: 'SOL',
        rows: a.sources.map((x: any) => ({ name: accounts.find(y => y.id === x.account)?.name ?? x.account, from: accounts.find(y => y.id === x.account)?.solana ?? '', amount: '25', fee: '0.000005' })) }
      case 'evm_batch_plan': await wait(700); return { id: 'eb1', by: 'owner', kind: 'evm', network: 'Base', test: false, to: a.to, newRecipient: false, open, count: a.sources.length,
        total: (a.sources.length * (a.token ? 25 : 0.4)).toString(), sym: a.token ? 'USDC' : 'ETH', fees: (a.sources.length * 0.0000021).toFixed(7), feeSym: 'ETH',
        rows: a.sources.map((x: any) => ({ name: accounts.find(y => y.id === x.account)?.name ?? x.account, from: accounts.find(y => y.id === x.account)?.evm ?? '', amount: a.token ? '25' : '0.4', fee: '0.0000021' })) }
      case 'folders_get': return folders
      case 'folders_set': folders = a.list; localStorage.setItem('mockFolders', JSON.stringify(folders)); return null
      case 'batch_plan': await wait(700); return waiting = { id: 'b2', by: a.addon ? 'addon' : 'owner', byName: a.addon ? 'Сбор' : undefined, network: 'Solana Mainnet', test: false, to: a.to, newRecipient: false, open, count: a.sources.length,
        total: (a.sources.length * 0.4).toFixed(6), fees: (a.sources.length * 0.000005).toFixed(6),
        rows: a.sources.map((x: any) => ({ name: accounts.find(y => y.id === x.account)?.name ?? x.account, from: accounts.find(y => y.id === x.account)?.solana ?? '', balance: '0.400005', amount: '0.4', fee: '0.000005', left: '0' })) }
      case 'sol_history': await wait(400); return [1, 2, 3].map(i => ({ sig: `demoSig${i}`.padEnd(88, String(i)), time: 1791400000 - i * 86400, ok: true }))
      case 'sol_tx': await wait(300); return { sig: a.sig, time: 1791400000, ok: true, fee: a.sig.endsWith('2') ? '5000' : null, peer: 'Fk7aUsdcDemoMint1111111111111111111111111111',
        moves: a.sig.endsWith('2') ? [{ mint: null, amount: '100000000', decimals: 9, incoming: false }] : [{ mint: a.sig.endsWith('3') ? 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v' : null, amount: a.sig.endsWith('3') ? '5000000' : '250000000', decimals: a.sig.endsWith('3') ? 6 : 9, incoming: true }] }
      case 'panel_secret': return null
      case 'fee_state': return { owner: '9C8S2aM3bgTApyiZoSLhmcdDzhYiFdvbTLZdLbY4StBJ', bps: 20, missing: 3 }
      case 'agent_state': return { on: agentOn, waiting }
      case 'agent_access': agentOn = a.on; return agentOn
      case 'agent_paths': return { mcp: String.raw`C:\Program Files\Talon\agent\talon-mcp.mjs`, skill: String.raw`C:\Program Files\Talon\agent\SKILL.md` }
      case 'batch_reject': waiting = null; return null
      case 'batch_confirm': await wait(900); if (!open && a.pass !== 'demo-pass') throw 'неверный пароль'; waiting = null; return { sent: 3, total: 3 }
      case 'clip_secret': await navigator.clipboard.writeText(a.text).catch(() => {}); return 30
      case 'secure_add_evm': await wait(500); accounts = accounts.map(x => ({ ...x, evm: '0x2c7536E3605D9C16a7a3D7b1898e529396a65c23' })); changed(); return null
      case 'secure_new_phrase': return 'orbit velvet canyon ember ledger quartz harbor maple signal tunnel cobalt frost'
      case 'secure_save_wallet': await wait(700); accounts = [acc]; unlocked = true; open = !a.pass; changed(); return null
      case 'vault_unlock': await wait(600); if (a.pass !== 'demo-pass') throw 'Неверный пароль'; unlocked = true; changed(); return null
      case 'vault_lock': unlocked = false; return null
      case 'sol_watch': await wait(300); return { lamports: '1250000000', partial: false, tokens: [ // a real mint, a fake «USDC», a spam token
        { mint: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v', amount: '25000000', decimals: 6, token2022: false, name: 'USD Coin', symbol: 'USDC' },
        { mint: 'Fk7aUsdcDemoMint1111111111111111111111111111', amount: '1000000000', decimals: 6, token2022: false, name: 'USD Coin', symbol: 'USDC' },
        { mint: '8wnFf4DemoSpamMint111111111111111111111111111', amount: '777000000', decimals: 6, token2022: true, name: 'MICROSOFT AI', symbol: 'MICROAI' }] }
      case 'evm_watch': await wait(300); return { wei: a.chainId === 8453 ? '420000000000000000' : '0', native: 'ETH', tokens: a.chainId === 8453 ? [{ contract: '0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913', symbol: 'USDC', amount: '125500000', decimals: 6 }] : [] }
      case 'evm_history': await wait(400); if (a.chainId !== 8453) throw 'для этой сети история пока не собирается'; return [{ sig: '0x' + 'ab'.repeat(32), time: 1791400000, ok: true, fee: '42000000000000', peer: '0x70997970c51812dc3a010c7d01b50e0d17dc79c8', moves: [{ mint: null, amount: '50000000000000000', decimals: 18, incoming: false, sym: 'ETH' }] }, { sig: '0x' + 'cd'.repeat(32), time: 1791300000, ok: true, fee: null, peer: '0x70997970c51812dc3a010c7d01b50e0d17dc79c8', moves: [{ mint: '0x833589fcd6edb6e08f4c7c32d4f71b54bda02913', amount: '25000000', decimals: 6, incoming: true, sym: 'USDC' }] }]
      case 'sol_tokens': await wait(300); return [
        { id: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v', symbol: 'USDC', name: 'USD Coin', icon: null, decimals: 6, verified: true, price: 1, liquidity: 1e9 },
        { id: 'DEkqHyPN7GMRJ5cArtQFAWefqbZb33Hyf6s5iCwjEonT', symbol: 'USDe', name: 'USDe', icon: null, decimals: 9, verified: true, price: 0.9993, liquidity: 7e6 },
        { id: 'JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN', symbol: 'JUP', name: 'Jupiter', icon: null, decimals: 6, verified: true, price: 0.42, liquidity: 5e7 },
        { id: 'FakeUsdcMint11111111111111111111111111111111', symbol: 'USDC', name: 'USD Coin', icon: null, decimals: 6, verified: false, price: 1, liquidity: 2e4 },
      ].filter(x => !a.query || (x.symbol + x.name).toLowerCase().includes(a.query.toLowerCase()))
      case 'token_icon': throw 'no icons in the mock'
      case 'evm_max': await wait(300); return '0.41998'
      case 'sol_grants': await wait(400); return [{ account: 'DemoTokenAcct1111111111111111111111111111111', mint: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v', delegate: 'JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4', amount: '18446744073709551615', decimals: 6 }]
      case 'evm_allowances': await wait(400); return a.chainId === 8453 ? [{ token: '0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913', symbol: 'USDC', spender: '0x000000000022D473030F116dDEE9F6B43aC78BA3', name: 'Permit2 (Uniswap)', amount: '', unlimited: true }] : []
      case 'prepare_revoke': await wait(600); return { id: 'rv1', open, account: 'Основной', from: a.chainId ? '0x52908400098527886E0F7030069857D2E4169EE7' : acc.solana, to: a.spender ?? 'JUP6LkbZbjS1jKKwapdHNy74zcZ3tLUZoi5QNyVTaV4', network: a.chainId ? 'Base' : 'Solana Mainnet', test: false, newRecipient: false, warn: null, amount: '0', symbol: 'USDC', mint: a.token ?? 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v', native: a.chainId ? 'ETH' : undefined, feeMax: !!a.chainId, fee: a.chainId ? '0.0000021' : '0.000005', rent: '0', solTotal: '0.000005', revoke: true }
      case 'sol_prepare_send': case 'evm_prepare_send': await wait(600); return { id: 'send1', open, account: 'Основной', from: a.chainId ? '0x52908400098527886E0F7030069857D2E4169EE7' : acc.solana, to: a.to, network: a.chainId ? a.name : 'Solana Mainnet', test: false, newRecipient: true, warn: null,
        amount: a.amount, symbol: null, mint: a.mint ?? a.token ?? null, native: a.chainId ? a.symbol : undefined, feeMax: !!a.chainId, fee: a.chainId ? '0.0000021' : '0.000005', rent: '0', solTotal: a.chainId ? a.amount : (Number(String(a.amount).replace(',', '.')) + 0.000005).toFixed(9).replace(/0+$/, '') }
      case 'sol_swap_quote': await wait(400); return { in_amount: a.amount, out_amount: (Number(a.amount.replace(',', '.')) * 109.3).toFixed(4), min_out: (Number(a.amount.replace(',', '.')) * 108.75).toFixed(4), out_decimals: 6, impact: '0', route: 'Whirlpool → Manifest', fee_bps: 20 }
      case 'sol_prepare_swap': await wait(800); return { id: 'swap1', open, account: 'Основной', from: acc.solana, to: 'Jupiter · Whirlpool → Manifest', network: 'Solana Mainnet', test: false, newRecipient: false, warn: null,
        amount: a.amount, symbol: 'SOL', mint: null, fee: '0.0000051', rent: '0', solTotal: a.amount, swap: { outMint: 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v', out: '1.1022', min: '1.0967', route: 'Whirlpool → Manifest', feeBps: 20 } }
      case 'usd_prices': return { SOL: 109.37, EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v: 0.9996 } // the fake and the spam token get none
      case 'rpc_own': return own
      case 'rpc_set': await wait(500); if (a.url && !a.url.startsWith('https://')) throw 'нужен адрес, который начинается с https://'
        if (a.url) own[a.cluster] = new URL(a.url).host; else delete own[a.cluster]; return { ...own }
      case 'secure_export': await wait(500); if (!open && a.pass !== 'demo-pass') throw 'неверный пароль'; return { phrase: 'orbit velvet canyon ember ledger quartz harbor maple signal tunnel cobalt frost', solana: '4Z7cXSyeFR8wNGMVXUE1TwtKn5D5Vu7FzEv69dokLv7KrQk7h6pu4LF8ZRR9yQBhc7uSM6RTTZtU1fmaxiNrxXrs', evm: '0x' + 'ab'.repeat(32) }
    }
  }, { shouldMockEvents: true })
}
