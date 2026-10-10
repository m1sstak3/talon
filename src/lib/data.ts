// Real wallet data: networks, accounts (from the vault + watch-only), balances and history from the Rust core.
// No keys here: the main window only sees public addresses and numbers.
import { invoke, isTauri } from '@tauri-apps/api/core'
import type { Draft } from '../ui/Sheet.svelte'
import { t } from './i18n.svelte'

export interface Card {
  id: string
  sym: string
  name: string
  amount: number
  usd: number // price per unit; 0 = no price source yet
  tint: string
  glyph: 'sol' | 'usdc' | 'jup' | 'bonk' | 'jto' | 'pyth' | 'ray' | 'eth' | 'mono' | 'hood' | 'pol' | 'op' | 'base' | 'bnb' | 'avax'
  address?: string
  kind?: 'net' // hero card that stands for a whole network
  amountText?: string // hero: overrides the amount line (e.g. «—» when unknown)
  raw?: string // exact base units (Solana); «Всё» in send uses these, never the rounded number
  decimals?: number
  verified?: boolean // a known mint (or the native coin); unverified ones carry their mint wherever they are picked
}

export interface Chain {
  id: string
  name: string
  kind: 'solana' | 'evm'
  symbol: string // native coin
  decimals: number
  test?: boolean
  chainId?: number
  rpc: string // full https URL (EVM); Solana clusters are fixed in the core
  explorer: string
  more?: boolean // in the «Ещё сети» part of the menu until the owner picks it once
}
// public RPC endpoints, each checked to answer with its own chain id (07.10.2026; new networks 09.10.2026). The core keeps
// its own table of names, coins, tokens and explorers by chain id; these URLs only say where to ask.
export const chains: Chain[] = [
  { id: 'solana', name: 'Solana', kind: 'solana', symbol: 'SOL', decimals: 9, rpc: '', explorer: 'https://solscan.io' },
  { id: 'ethereum', name: 'Ethereum', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 1, rpc: 'https://ethereum-rpc.publicnode.com', explorer: 'https://etherscan.io' },
  { id: 'base', name: 'Base', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 8453, rpc: 'https://mainnet.base.org', explorer: 'https://basescan.org' },
  { id: 'arbitrum', name: 'Arbitrum One', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 42161, rpc: 'https://arb1.arbitrum.io/rpc', explorer: 'https://arbiscan.io' },
  { id: 'optimism', name: 'OP Mainnet', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 10, rpc: 'https://mainnet.optimism.io', explorer: 'https://optimistic.etherscan.io' },
  // Robinhood Chain: Arbitrum Orbit L2, mainnet 01.07.2026, chainId 4663
  { id: 'robinhood', name: 'Robinhood Chain', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 4663, rpc: 'https://rpc.mainnet.chain.robinhood.com', explorer: '' },
  { id: 'polygon', name: 'Polygon', kind: 'evm', symbol: 'POL', decimals: 18, chainId: 137, rpc: 'https://polygon-bor-rpc.publicnode.com', explorer: 'https://polygonscan.com' },
  { id: 'bnb', name: 'BNB Chain', kind: 'evm', symbol: 'BNB', decimals: 18, chainId: 56, rpc: 'https://bsc-rpc.publicnode.com', explorer: 'https://bscscan.com' },
  { id: 'avalanche', name: 'Avalanche', kind: 'evm', symbol: 'AVAX', decimals: 18, chainId: 43114, rpc: 'https://avalanche-c-chain-rpc.publicnode.com', explorer: 'https://snowtrace.io' },
  { id: 'linea', name: 'Linea', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 59144, rpc: 'https://rpc.linea.build', explorer: 'https://lineascan.build', more: true },
  { id: 'unichain', name: 'Unichain', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 130, rpc: 'https://mainnet.unichain.org', explorer: 'https://uniscan.xyz', more: true },
  { id: 'sonic', name: 'Sonic', kind: 'evm', symbol: 'S', decimals: 18, chainId: 146, rpc: 'https://rpc.soniclabs.com', explorer: 'https://sonicscan.org', more: true },
  { id: 'gnosis', name: 'Gnosis', kind: 'evm', symbol: 'xDAI', decimals: 18, chainId: 100, rpc: 'https://gnosis-rpc.publicnode.com', explorer: 'https://gnosisscan.io', more: true },
  { id: 'scroll', name: 'Scroll', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 534352, rpc: 'https://rpc.scroll.io', explorer: 'https://scrollscan.com', more: true },
  { id: 'ink', name: 'Ink', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 57073, rpc: 'https://rpc-gel.inkonchain.com', explorer: 'https://explorer.inkonchain.com', more: true },
  { id: 'worldchain', name: 'World Chain', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 480, rpc: 'https://worldchain-mainnet.g.alchemy.com/public', explorer: 'https://worldscan.org', more: true },
  { id: 'berachain', name: 'Berachain', kind: 'evm', symbol: 'BERA', decimals: 18, chainId: 80094, rpc: 'https://rpc.berachain.com', explorer: 'https://berascan.com', more: true },
  { id: 'monad', name: 'Monad', kind: 'evm', symbol: 'MON', decimals: 18, chainId: 143, rpc: 'https://rpc.monad.xyz', explorer: 'https://monadscan.com', more: true },
  { id: 'hyperevm', name: 'HyperEVM', kind: 'evm', symbol: 'HYPE', decimals: 18, chainId: 999, rpc: 'https://rpc.hyperliquid.xyz/evm', explorer: 'https://hyperevmscan.io', more: true },
  { id: 'solana-devnet', name: 'Solana Devnet', kind: 'solana', symbol: 'SOL', decimals: 9, test: true, rpc: '', explorer: 'https://solscan.io' },
  { id: 'sepolia', name: 'Sepolia', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 11155111, test: true, rpc: 'https://ethereum-sepolia-rpc.publicnode.com', explorer: 'https://sepolia.etherscan.io' },
  { id: 'base-sepolia', name: 'Base Sepolia', kind: 'evm', symbol: 'ETH', decimals: 18, chainId: 84532, test: true, rpc: 'https://sepolia.base.org', explorer: 'https://sepolia.basescan.org' },
  { id: 'local', name: 'Solana Local', kind: 'solana', symbol: 'SOL', decimals: 9, test: true, rpc: '', explorer: '' },
]

export interface Account {
  id: string
  name: string
  solana: string
  evm?: string | null
  wallet?: string // vault wallet id (absent for watch-only)
  index?: number
  watch?: boolean
  ledger?: boolean // the key lives on a Ledger: the device signs, nothing to export
}
export const addressFor = (a: Account | undefined, c: Chain) => (!a ? '' : c.kind === 'evm' ? (a.evm ?? '') : a.solana)

export interface Contact { name: string; address: string }
export interface Sig { sig: string; time: number | null; ok: boolean }
// one history entry as the chain saw it (core: balance changes of our address, fee if we paid, the one counterparty)
export interface Move { mint: string | null; amount: string; decimals: number; incoming: boolean; sym?: string } // sym: EVM history names its own coin/token
export interface Tx { sig: string; time: number | null; ok: boolean; fee: string | null; moves: Move[]; peer: string | null }

export const coreAvailable = () => isTauri()

// ---------------------------------------------------------------- formatting
export const short = (a: string) => (a ? `${a.slice(0, 4)}…${a.slice(-4)}` : '—')
export function fmt(n: number, sym?: string): string {
  const d = n === 0 ? 0 : n >= 1000 ? 2 : n >= 1 ? 4 : n >= 0.0001 ? 6 : 9
  const s = n.toLocaleString('ru-RU', { maximumFractionDigits: d, minimumFractionDigits: 0 })
  return sym ? `${s} ${sym}` : s
}
export const usd = (n: number) => '$' + n.toLocaleString('ru-RU', { minimumFractionDigits: 2, maximumFractionDigits: n > 0 && n < 0.01 ? 6 : 2 })
export const isAddress = (s: string) => /^[1-9A-HJ-NP-Za-km-z]{32,44}$/.test(s.trim())
export const isEvmAddress = (s: string) => /^0x[0-9a-fA-F]{40}$/.test(s.trim())
// ponytail: Number() of base units is exact up to 2^53; enough for display, the send path keeps integers
const units = (amount: string, decimals: number) => Number(amount) / 10 ** decimals

// ---------------------------------------------------------------- local, non-secret lists (per device)
function stored<T>(key: string, fallback: T): T { try { return JSON.parse(localStorage.getItem(key) ?? '') as T } catch { return fallback } }
function store(key: string, v: unknown) { try { localStorage.setItem(key, JSON.stringify(v)) } catch {} }
export const loadWatch = () => stored<Account[]>('watch', [])
export const saveWatch = (l: Account[]) => store('watch', l)
export const loadContacts = () => stored<Contact[]>('contacts', [])
export const saveContacts = (l: Contact[]) => store('contacts', l)
export const loadHidden = () => stored<string[]>('hiddenTokens', []) // token mints the owner hid (spam), per device
export const saveHidden = (l: string[]) => store('hiddenTokens', l)
export const loadCustomNets = () => stored<Chain[]>('customNets', []).filter(c => c.kind === 'evm' && c.id.startsWith('custom-')) // networks the owner added
export const saveCustomNets = (l: Chain[]) => store('customNets', l)
export const loadPinned = () => stored<string[]>('pinnedNets', []) // «Ещё сети» the owner picked once: shown with the main ones
export const savePinned = (l: string[]) => store('pinnedNets', l)

// ---------------------------------------------------------------- core calls
export interface VaultState { accounts: Account[]; unlocked: boolean; open: boolean }
export const vaultState = () => invoke<VaultState>('vault_state')
export const vaultLock = () => invoke('vault_lock')
export const vaultUnlock = (pass: string) => invoke('vault_unlock', { pass })
// the core builds, prices and checks the transfer and keeps it; the answer is what the confirmation shows
export const prepareSend = (account: string, to: string, amount: string, cluster: string, mint?: string) => invoke<Draft>('sol_prepare_send', { account, to, amount, cluster, mint })
// EVM: the same contract; the core knows the network by its chain id (name, coin, tokens), the URL only says where to ask
export const prepareEvmSend = (account: string, to: string, amount: string, c: Chain, token?: string) =>
  invoke<Draft>('evm_prepare_send', { account, to, amount, chainId: c.chainId, rpc: c.rpc, token, name: c.name, symbol: c.symbol })
export const evmMax = (address: string, c: Chain, to?: string) => invoke<string>('evm_max', { rpc: c.rpc, chainId: c.chainId, address, to })

// add-ons: one HTML file each, run in a sandboxed frame from the app folder (core: addons.rs)
export interface Addon { id: string; name: string; version: string; description: string; permissions: string[]; hosts: string[] }
export const addonList = () => invoke<Addon[]>('addon_list')
export const addonInspect = (html: string) => invoke<Addon>('addon_inspect', { html })
export const addonInstall = (html: string) => invoke<Addon[]>('addon_install', { html })
export const addonRemove = (id: string) => invoke<Addon[]>('addon_remove', { id })

// well-known Solana mints -> label; anything else shows its shortened mint (no token list fetch yet)
const KNOWN: Record<string, { sym: string; name: string; tint: string; glyph?: Card['glyph'] }> = {
  EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v: { sym: 'USDC', name: 'USD Coin', tint: '#9fd2ff', glyph: 'usdc' },
  Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB: { sym: 'USDT', name: 'Tether USD', tint: '#a8f0de' },
  JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN: { sym: 'JUP', name: 'Jupiter', tint: '#c9f2a0', glyph: 'jup' },
  DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263: { sym: 'BONK', name: 'Bonk', tint: '#ffc98a', glyph: 'bonk' },
  jtojtomepa8beP8AuQc6eXt5FriJwfFMwQx2v2f9mCL: { sym: 'JTO', name: 'Jito', tint: '#a8f0de', glyph: 'jto' },
  HZ1JovNiVvGrGNiiYvEozEVgZ58xaU3RKwX8eACQBCt3: { sym: 'PYTH', name: 'Pyth', tint: '#e2b4ff', glyph: 'pyth' },
  '4k3Dyjzvzp8eMZWUXbBCjEvwSkkk59S5iCNLY3QrkX6R': { sym: 'RAY', name: 'Raydium', tint: '#ffb3c7', glyph: 'ray' },
}
// names spam airdrops borrow so they look official; a token we don't know that carries one is marked, never trusted
const BRANDS = /\b(microsoft|apple|google|alphabet|amazon|tesla|nvidia|openai|meta|binance|coinbase|blackrock|tether|circle|paypal|visa|mastercard|solana foundation|phantom)\b/i
export function cluster(c: Chain): string | null {
  if (c.kind !== 'solana') return null
  return c.id === 'solana' ? 'mainnet' : c.id === 'solana-devnet' ? 'devnet' : 'local'
}

export interface Holdings { cards: Card[]; native: number; partial: boolean }
/** balances of one address on one network: native coin first, then tokens (Solana only for now) */
export async function readHoldings(address: string, c: Chain): Promise<Holdings> {
  const nativeCard = (amount: number, raw?: string): Card => ({ id: 'native', sym: c.symbol, name: c.kind === 'solana' ? 'Solana' : c.name, amount, raw, decimals: c.decimals, verified: true, usd: 0, tint: '#b8a6ff', glyph: c.kind === 'solana' ? 'sol' : (({ ETH: 'eth', POL: 'pol', BNB: 'bnb', AVAX: 'avax' } as const)[c.symbol] ?? 'mono') })
  if (c.kind === 'solana') {
    const w = await invoke<{ lamports: string; tokens: { mint: string; amount: string; decimals: number; token2022: boolean; name: string | null; symbol: string | null }[]; partial: boolean }>('sol_watch', { address, cluster: cluster(c) })
    const native = units(w.lamports, 9)
    return {
      native, partial: w.partial,
      cards: [nativeCard(native, w.lamports), ...w.tokens.map(h => {
        const k = KNOWN[h.mint]
        // names from the chain are written by whoever made the token: unverified ones always carry their mint, and one
        // that borrows a known ticker (a fake «USDC») says so
        const fake = !k && ((!!h.symbol && Object.values(KNOWN).some(x => x.sym.toUpperCase() === h.symbol!.toUpperCase())) || BRANDS.test(`${h.name ?? ''} ${h.symbol ?? ''}`))
        const name = k?.name ?? `${fake ? t('tok.fake') : h.name ?? (h.token2022 ? 'Token-2022' : 'SPL')} · ${short(h.mint)}`
        return { id: h.mint, sym: k?.sym ?? h.symbol ?? short(h.mint), name, amount: units(h.amount, h.decimals), raw: h.amount, decimals: h.decimals, verified: !!k, usd: 0, tint: k?.tint ?? '#d4cdc3', glyph: k?.glyph ?? ('mono' as const) }
      })],
    }
  }
  if (!c.rpc) throw new Error('для этой сети не задан адрес узла')
  // the native coin + the network's known tokens (the core's own list: an unknown contract never shows up as «USDC»)
  const w = await invoke<{ wei: string; tokens: { contract: string; symbol: string; amount: string; decimals: number }[] }>('evm_watch', { rpc: c.rpc, chainId: c.chainId, address })
  const native = units(w.wei, c.decimals)
  return {
    native, partial: false,
    cards: [nativeCard(native, w.wei), ...w.tokens.map(h => ({ id: h.contract, sym: h.symbol, name: EVM_TOKEN_NAME[h.symbol] ?? h.symbol, amount: units(h.amount, h.decimals), raw: h.amount, decimals: h.decimals, verified: true, usd: 0, tint: h.symbol.startsWith('USDC') ? '#9fd2ff' : '#a8f0de', glyph: h.symbol.startsWith('USDC') ? 'usdc' as const : 'mono' as const }))],
  }
}

const EVM_TOKEN_NAME: Record<string, string> = { USDC: 'USD Coin', 'USDC.e': 'USD Coin (мост)', USDT: 'Tether USD', DAI: 'Dai' }

// EVM: one call gives the list and every entry's details (an explorer's index); unsupported networks answer an error
export const readEvmHistory = (address: string, c: Chain) => invoke<(Sig & Tx)[]>('evm_history', { chainId: c.chainId, address })
export const readHistory = (address: string, c: Chain) => invoke<Sig[]>('sol_history', { address, cluster: cluster(c) })
export const readTx = (sig: string, address: string, c: Chain) => invoke<Tx>('sol_tx', { sig, address, cluster: cluster(c) })
// permissions: who may spend this account's tokens (Solana delegates; EVM allowances to the known spenders) and the revoke
export interface Perm { key: string; token: string; sym: string; to: string; toName: string; amount: string; unlimited: boolean; tokenAccount?: string; spender?: string }
export async function readPerms(address: string, c: Chain, known: Card[]): Promise<Perm[]> {
  const cl = cluster(c)
  if (cl) {
    const g = await invoke<{ account: string; mint: string; delegate: string; amount: string; decimals: number }[]>('sol_grants', { address, cluster: cl })
    return g.map(x => ({ key: x.account, token: x.mint, sym: known.find(k => k.id === x.mint && k.verified)?.sym ?? KNOWN[x.mint]?.sym ?? short(x.mint), to: x.delegate, toName: short(x.delegate),
      amount: exact(BigInt(x.amount), x.decimals), unlimited: BigInt(x.amount) >= 2n ** 64n - 1n, tokenAccount: x.account }))
  }
  const a = await invoke<{ token: string; symbol: string; spender: string; name: string; amount: string; unlimited: boolean }[]>('evm_allowances', { rpc: c.rpc, chainId: c.chainId, address })
  return a.map(x => ({ key: x.token + x.spender, token: x.token, sym: x.symbol, to: x.spender, toName: x.name, amount: x.amount, unlimited: x.unlimited, spender: x.spender }))
}
export const prepareRevoke = (account: string, c: Chain, p: Perm) => cluster(c)
  ? invoke<Draft>('prepare_revoke', { account, cluster: cluster(c), tokenAccount: p.tokenAccount })
  : invoke<Draft>('prepare_revoke', { account, chainId: c.chainId, rpc: c.rpc, token: p.token, spender: p.spender })
export const openAddress = (address: string, c: Chain) => invoke('open_address', { address, cluster: cluster(c) ?? `evm:${c.chainId}` })
export const openTx = (sig: string, c: Chain) => invoke('open_tx', { sig, cluster: cluster(c) ?? `evm:${c.chainId}` })
export const usdPrices = (mints: string[], coins: string[]) => invoke<Record<string, number>>('usd_prices', { mints, coins })
// swap (Solana mainnet): a live quote, then the core builds, checks and dry-runs the exact transaction
export interface SwapQuote { in_amount: string; out_amount: string; min_out: string; out_decimals: number; impact: string; route: string; fee_bps: number }
export const swapQuote = (input: string, output: string, amount: string) => invoke<SwapQuote>('sol_swap_quote', { input, output, amount, cluster: 'mainnet' })
export const prepareSwap = (account: string, input: string, output: string, amount: string) => invoke<Draft>('sol_prepare_swap', { account, input, output, amount, cluster: 'mainnet' })
export const USDC = 'EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v'
// any token Jupiter can route (owner 10.10: a list like Phantom's, searchable): today's most traded, or a search
export interface TokenInfo { id: string; symbol: string; name: string; icon: string | null; decimals: number; verified: boolean; price: number | null; liquidity: number | null }
export const solTokens = (query = '') => invoke<TokenInfo[]>('sol_tokens', { query })
// EVM swaps (LI.FI through the core, which vets every transaction): the network's token list, a quote, the confirmation draft
export const EVM_NATIVE = '0x0000000000000000000000000000000000000000'
export const evmSwapTokens = (c: Chain) => invoke<Omit<TokenInfo, 'liquidity'>[]>('evm_swap_tokens', { chainId: c.chainId }).then(l => l.map(x => ({ ...x, liquidity: null })))
export const evmSwapQuote = (c: Chain, address: string, input: string, output: string, amount: string) => invoke<SwapQuote>('evm_swap_quote', { chainId: c.chainId, address, input, output, amount })
export const evmPrepareSwap = (account: string, c: Chain, input: string, output: string, amount: string) => invoke<Draft>('evm_prepare_swap', { account, chainId: c.chainId, rpc: c.rpc, input, output, amount })
const iconCache = new Map<string, Promise<string>>()
export const tokenIcon = (url: string) => { let p = iconCache.get(url); if (!p) { p = invoke<string>('token_icon', { url }); iconCache.set(url, p) } return p }
export const SWAP_TO = [{ id: 'SOL', sym: 'SOL' }, { id: USDC, sym: 'USDC' }, { id: 'Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB', sym: 'USDT' },
  { id: 'JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN', sym: 'JUP' }, { id: 'DezXAZ8z7PnrnRJjz3wXBoRgixCa6xjnB7YaB1pPB263', sym: 'BONK' }]
/** "+1,25 SOL" / "−0,5 USDC" for one move */
export function moveText(m: Move): string {
  const k = m.mint ? KNOWN[m.mint] : null
  return (m.incoming ? '+' : '−') + fmt(units(m.amount, m.decimals), m.sym ?? (m.mint ? (k?.sym ?? short(m.mint)) : 'SOL'))
}
export const solText = (lamports: string) => fmt(units(lamports, 9), 'SOL')
/** exact decimal text of base units ("1500000", 6 -> "1.5"), BigInt all the way: for «Всё», never a rounded float */
export function exact(raw: bigint, decimals: number): string {
  if (raw <= 0n) return '0'
  const s = raw.toString().padStart(decimals + 1, '0'), int = s.slice(0, s.length - decimals), frac = s.slice(s.length - decimals).replace(/0+$/, '')
  return frac ? `${int}.${frac}` : int
}
// ponytail: base fee of one signature; the core prices the real fee and says the true maximum if this is off
export const SOL_BASE_FEE = 5000n
/** the most that can leave: a token's whole balance, or SOL minus the network fee (the account may go to 0) */
export const maxSend = (c: Card, kind: Chain['kind'] = 'solana'): string | null => !c.raw || c.decimals === undefined ? null
  : c.id === 'native' && kind === 'evm' ? 'max' // the EVM fee moves with every block: the core prices «Всё» (evmMax)
  : exact(c.id === 'native' ? BigInt(c.raw) - SOL_BASE_FEE : BigInt(c.raw), c.decimals)
