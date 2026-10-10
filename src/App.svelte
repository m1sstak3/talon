<script lang="ts">
  import { onMount, tick } from 'svelte'
  import { Tween } from 'svelte/motion'
  import { slide, fade } from 'svelte/transition'
  import { cubicOut, cubicIn } from 'svelte/easing'
  import { listen } from '@tauri-apps/api/event'
  import { isTauri } from '@tauri-apps/api/core'
  import { onBackButtonPress } from '@tauri-apps/api/app'
  import { createHero, iconURL, type Hero, type Theme } from './lib/hero'
  import {
    chains as seedChains, fmt, short, isAddress, isEvmAddress, familyOf, parseScan, addressFor, coreAvailable, cluster,
    loadWatch, saveWatch, loadContacts, saveContacts, vaultState, vaultLock, vaultUnlock, prepareSend, readHoldings, readHistory,
    readTx, openTx, readEvmHistory, moveText, solText, loadHidden, saveHidden, usdPrices, usd, maxSend, swapQuote, prepareSwap, exact, USDC, SWAP_TO, solTokens, tokenIcon, evmSwapTokens, evmSwapQuote, evmPrepareSwap, EVM_NATIVE, type TokenInfo, type Tx, type SwapQuote,
    prepareEvmSend, evmMax, openAddress, readPerms, prepareRevoke, type Perm, loadCustomNets, saveCustomNets, loadPinned, loadFav, saveFav,
    addonList, addonInspect, addonInstall, addonRemove, type Addon,
    type Card, type Chain, type Account, type Contact, type Sig, type Holdings,
  } from './lib/data'
  import { t, i18n, humanErr } from './lib/i18n.svelte'
  import { pad, countInto, dragDown, pullRefresh, haptic } from './lib/actions'
  import Roll from './ui/Roll.svelte'
  import Icon from './ui/Icon.svelte'
  import Qr from './ui/Qr.svelte'
  import Keypad from './ui/Keypad.svelte'
  import WindowBar from './ui/WindowBar.svelte'
  import Sheet, { type SheetMode, type Draft, type Batch, type Offer } from './ui/Sheet.svelte'
  import Intro, { type Kind } from './ui/Intro.svelte'
  import { invoke, convertFileSrc } from '@tauri-apps/api/core'

  type View = 'locked' | 'home' | 'token' | 'receive' | 'send' | 'swap' | 'history' | 'addresses' | 'settings' | 'addon'

  let canvas: HTMLCanvasElement
  let hero = $state.raw<Hero | null>(null) // raw: the intro writes to hero.birth, a proxy would swallow it
  let view = $state<View>('locked')
  let chainList = $state<Chain[]>([...seedChains.map(c => ({ ...c })), ...loadCustomNets()])
  // ★ favourites: their cards stand on the ring and they lead the network menu; the rest are a search or a fold away.
  // First run (or a pre-11.10 install): the main networks plus the ones picked from «Ещё сети» before
  let fav = $state<string[]>(loadFav() ?? [...[...seedChains, ...loadCustomNets()].filter(c => !c.test && !c.more).map(c => c.id), ...loadPinned()]), moreNets = $state(false), netQ = $state('')
  // last chosen network and account survive restarts (per-device convenience; storage may be unavailable)
  let chainId = $state((() => { try { return localStorage.getItem('lastChain') || 'solana' } catch { return 'solana' } })())
  let accId = $state((() => { try { return localStorage.getItem('lastAccount') || '' } catch { return '' } })())
  let showTest = $state(false)
  let menu = $state<'' | 'net' | 'acc'>('')
  let addingNet = $state(false), plus = $state(false) // account menu: the «+» row folds out create / import / watch
  // menus open and close the same way, from the chip that owns them (owner 09.10: both directions smooth)
  // phone: menus are sheets from the bottom edge; a sheet pulled partway down leaves from where the finger let go
  function pop(n: Element, { out = false } = {}) {
    if (phone) {
      const from = out ? parseFloat((n as HTMLElement).style.transform.match(/translateY\(([\d.]+)px/)?.[1] ?? '0') : 0
      return { duration: still() ? 0 : out ? 220 : 320, easing: out ? cubicIn : cubicOut, css: (k: number) => `transform:translateY(calc(${from * k}px + ${(1 - k) * 100}%))` }
    }
    return { duration: still() ? 0 : out ? 150 : 220, easing: out ? cubicIn : cubicOut, css: (k: number) => `opacity:${k};transform:translateY(${(1 - k) * -6}px) scale(${0.97 + 0.03 * k})` }
  }
  // a click anywhere outside the open menu closes it, like every menu on the system
  function outside(e: PointerEvent) { if (menu && !(e.target as Element).closest?.('.wrap')) menu = '' }
  $effect(() => { if (!menu) { plus = false; addingNet = false; moreNets = false; netQ = '' } })
  let sel = $state<string>('') // selected token id
  // the wall behind the cards: one, chosen for the owner (no picker: owner 07.10 «сам поставь»); «Гравюра» is the other
  const theme: Theme = 'facets'
  let autolock = $state((() => { try { return Number(localStorage.getItem('autolock')) || 15 } catch { return 15 } })())

  let vaultAccounts = $state<Account[]>([])
  let watchAccounts = $state<Account[]>(loadWatch())
  let hasVault = $state(false)
  let vaultOpen = $state(false) // no password: owner's choice, the vault opens by itself
  let sheet = $state<{ mode: SheetMode; wallet?: string; account?: string; origin: DOMRect | null; round?: number; draft?: Draft; batch?: Batch; offer?: Offer } | null>(null)
  let enterAfterSheet = false
  let sheetEl = $state<ReturnType<typeof Sheet>>()
  // sends whose outcome is unknown (lost reply, crash): listed on the home screen until the network decides
  let unresolved = $state<{ sig: string; cluster: string; chain?: number }[]>([])
  let pendingErr = $state('')
  const loadPending = () => invoke<{ sig: string; cluster: string; chain?: number }[]>('pending_list').then(l => { unresolved = l; pendingErr = '' }, e => (pendingErr = humanErr(e)))
  // intro: the scene and the vault state are prepared behind it; the first screen's entrance waits for its signal (M-29)
  let introOn = $state(true)
  let readyRes!: () => void, kindRes!: (k: Kind) => void
  const readyP = new Promise<void>(r => (readyRes = r)), kindP = new Promise<Kind>(r => (kindRes = r))
  let tracedRes!: () => void
  const traced = new Promise<void>(r => (tracedRes = r))
  let startUnlocked = false
  function introOpen(trace: { rect: DOMRect; r: number } | null) {
    document.documentElement.classList.remove('intro') // paused entrances start now, with their own delays
    if (startUnlocked) enter() // no password: the card lands and the deal follows at once
    // first launch: the form unfolds out of the lead card's trace, in the centre, over the far cloud
    else if (!hasVault && coreAvailable()) sheet = { mode: 'welcome', origin: trace?.rect ?? null, round: trace?.r }
  }
  let contactList = $state<Contact[]>(loadContacts())
  let toast = $state<{ text: string; sub?: string; ok?: boolean } | null>(null)
  let copied = $state('')
  let busy = $state(false)
  let addrTab = $state<'mine' | 'contacts' | 'perms'>('mine')
  let qrOpen = $state(''), netsOpen = $state('')

  const accountList = $derived([...vaultAccounts, ...watchAccounts])
  const acc = $derived(accountList.find(a => a.id === accId) ?? accountList[0])
  const chain = $derived(chainList.find(c => c.id === chainId) ?? chainList[0])
  const myAddress = $derived(addressFor(acc, chain))

  // ---------------------------------------------------------------- balances: per account and network, cached
  // last-known balances stay on the device (public chain data, like the history cache) and show dimmed until read again
  let holdings = $state<Record<string, Holdings>>((() => { try { return JSON.parse(localStorage.getItem('holdings') ?? '{}') } catch { return {} } })())
  let fresh = $state<Record<string, true>>({}) // read in this session
  let loadState = $state<Record<string, string>>({}) // '' ok | 'loading' | error text
  const hk = (a: Account | undefined, c: Chain) => `${a?.id}|${c.id}`
  const cur = $derived(holdings[hk(acc, chain)])
  const tokens = $derived(cur?.cards ?? [])
  const curState = $derived(loadState[hk(acc, chain)] ?? '')
  const card = $derived(tokens.find(c => c.id === sel) ?? tokens[0])
  // spam tokens: hidden from the list on this device, one tap to show them again; the native coin is never hidden
  let hidden = $state<string[]>(loadHidden()), showHidden = $state(false)
  const listed = $derived(tokens.filter(c => showHidden || !hidden.includes(c.id)))
  const hiddenCount = $derived(tokens.filter(c => hidden.includes(c.id)).length)
  // ---------------------------------------------------------------- USD: mainnet only, unknown stays unknown (never $0)
  let showUsd = $state((() => { try { return localStorage.getItem('usd') !== 'off' } catch { return true } })())
  function setShowUsd(v: boolean) { showUsd = v; try { localStorage.setItem('usd', v ? 'on' : 'off') } catch {} }
  let prices = $state<Record<string, number>>({}), pricedAt = 0, pricedIds = ''
  const pidOf = (c: Card, ch: Chain) => c.id === 'native' ? (ch.kind === 'solana' ? 'SOL' : ch.symbol) : ch.kind === 'solana' ? c.id : c.verified ? c.sym : ''
  const priceId = (c: Card) => pidOf(c, chain)
  const valueOf = (c: Card): number | undefined => { const p = !chain.test && showUsd ? prices[priceId(c)] : undefined; return p === undefined ? undefined : p * c.amount }
  $effect(() => {
    if (!cur || chain.test || !showUsd || !coreAvailable()) return
    const mints = chain.kind === 'solana' ? tokens.map(priceId) : [], coins = chain.kind === 'evm' ? tokens.map(priceId).filter(Boolean) : []
    const ids = [...mints, ...coins].sort().join(',')
    if (ids === pricedIds && Date.now() - pricedAt < 60_000) return // a minute is fresh enough for a wallet
    pricedIds = ids; pricedAt = Date.now()
    usdPrices(mints, coins).then(p => (prices = { ...prices, ...p }), () => {}) // no price source: amounts only, nothing breaks
  })
  const usdTotal = $derived.by(() => {
    if (chain.test || !showUsd) return null
    const vals = listed.filter(c => !hidden.includes(c.id)).map(valueOf)
    const known = vals.filter((v): v is number => v !== undefined)
    return known.length ? { sum: known.reduce((a, b) => a + b, 0), missing: vals.length - known.length } : null
  })
  function toggleHidden(id: string) { hidden = hidden.includes(id) ? hidden.filter(x => x !== id) : [...hidden, id]; saveHidden(hidden) }
  const total = $derived(cur?.native ?? 0)
  const shown = new Tween(0, { duration: 1100, easing: cubicOut })
  // the balance counts only within one wallet context; a new network/account shows its own number at once
  // and its first number lands as is (owner 08.10: no count-up after every card swap); only a real change counts
  let shownCtx = '', shownLoaded = false
  $effect(() => {
    const ctx = hk(acc, chain), loaded = !!cur
    if (ctx !== shownCtx || !shownLoaded) shown.set(total, { duration: 0 }); else shown.target = total
    shownCtx = ctx; shownLoaded = loaded
  })
  // a network or account switch on home changes data, not the screen: no entrance replays (owner 08.10)
  let quiet = $state(false)
  $effect.pre(() => { void view; quiet = false }) // any real screen change plays its entrance again

  async function load(a: Account | undefined, c: Chain, force = false) {
    const address = addressFor(a, c), key = hk(a, c)
    if (!a || !address || !coreAvailable()) return
    if (!force && ((holdings[key] && fresh[key]) || loadState[key] === 'loading')) return
    loadState[key] = 'loading'
    try {
      holdings[key] = await readHoldings(address, c)
      loadState[key] = ''; fresh[key] = true
      try { localStorage.setItem('holdings', JSON.stringify($state.snapshot(holdings))) } catch {}
      hero?.sync(netDeck())
    } catch (e) { loadState[key] = String(e) }
  }
  const refresh = () => load(acc, chain, true)

  let addons = $state<Addon[]>([]), addonId = $state('') // installed add-ons: a tab each after these
  const NAV = [
    { id: 'home', label: () => t('nav.wallet') },
    { id: 'history', label: () => t('nav.history') },
    { id: 'addresses', label: () => t('nav.addresses') },
  ] as const
  // phone: the sections live in a tab bar under the thumb; settings is one of them
  const TABS = [
    { id: 'home', icon: 'wallet', label: () => t('nav.wallet') },
    { id: 'history', icon: 'history', label: () => t('nav.history') },
    { id: 'addresses', icon: 'book', label: () => t('nav.addresses') },
    { id: 'settings', icon: 'gear', label: () => t('nav.settings') },
  ] as const
  const tabOf = (v: View) => (v === 'history' || v === 'addresses' || v === 'settings' ? v : 'home')
  // the current tab again: back to its top (and home from a token), like every phone app
  function tab(v: View) {
    if (view === v) { colEl?.scrollTo({ top: 0, behavior: still() ? 'auto' : 'smooth' }); return }
    haptic('tick'); go(v)
  }
  // pull to refresh where a screen shows the network's numbers
  const pullFor = (v: View) => v === 'home' ? refresh : v === 'history' ? () => { histAt[histKey(myAddress, chain)] = 0; return loadHistory() } : null
  const navActive = $derived(view === 'history' ? 1 : view === 'addresses' ? 2 : view === 'settings' ? -1 : view === 'addon' ? NAV.length + addons.findIndex(a => a.id === addonId) : 0)

  // ---------------------------------------------------------------- hero: every enabled network is a card on the ring
  const NET_LOOK: Record<string, { tint: string; glyph: Card['glyph'] }> = {
    solana: { tint: '#b8a6ff', glyph: 'sol' }, ethereum: { tint: '#9fb4ff', glyph: 'eth' }, robinhood: { tint: '#d4f56b', glyph: 'hood' },
    base: { tint: '#7fa8ff', glyph: 'base' }, arbitrum: { tint: '#8fd0ff', glyph: 'mono' }, optimism: { tint: '#ff8f8f', glyph: 'op' },
    polygon: { tint: '#c4a2ff', glyph: 'pol' }, bnb: { tint: '#f3d36b', glyph: 'bnb' }, avalanche: { tint: '#ff9a9a', glyph: 'avax' },
  }
  function netCard(c: Chain): Card {
    const look = NET_LOOK[c.id] ?? { tint: c.kind === 'solana' ? '#b8a6ff' : '#9fb4ff', glyph: c.kind === 'solana' ? 'sol' : 'mono' } // a network without its own mark shows its initial, never another network's sign
    // locked: the file's public list is not proven yet, so the cards show neither its addresses nor their balances
    // (no password: the core unlocked it at start, so the deck is painted proven once, before the intro ends)
    const unproven = view === 'locked' && hasVault && !startUnlocked
    const h = unproven ? undefined : holdings[hk(acc, c)], address = unproven ? '' : addressFor(acc, c)
    return {
      id: c.id, sym: c.name, name: acc?.watch ? t('watch.tag') : c.test ? t('net.testTag') : t('net.title'),
      amount: h?.native ?? 0, usd: 0, kind: 'net', address,
      amountText: unproven ? '—' : !address ? t('card.noAddress') : h ? fmt(h.native, c.symbol) : '—', ...look,
    }
  }
  // the ring: favourites, the test networks while shown, and the current network even when it is neither
  const onRing = (c: Chain) => c.id === chainId || (c.test ? showTest : fav.includes(c.id))
  const netDeck = () => chainList.filter(onRing).map(netCard)

  async function run(fn: () => Promise<void> | void) {
    if (busy) return
    busy = true
    try { await fn() } finally { busy = false }
  }

  // ---------------------------------------------------------------- vault and lock
  async function refreshVault() {
    if (!coreAvailable()) return
    const s = await vaultState()
    vaultAccounts = s.accounts
    hasVault = s.accounts.length > 0
    vaultOpen = s.open
    if (!acc) accId = vaultAccounts[0]?.id ?? ''
    hero?.sync(netDeck()) // the locked deck already shows public addresses
    return s
  }
  // lock screen: the field unfolds under the deck; typing loosens the deck; on unlock it folds away, then the deal
  let pass = $state('')
  let gateBusy = $state(false)
  let gateShut = $state(false)
  let gateErr = $state('')
  let gateEl = $state<HTMLFormElement>()
  const still = () => matchMedia('(prefers-reduced-motion: reduce)').matches
  // no scroll on focus: the capsule clips its content while it unfolds
  // touch screens: no field grabs focus by itself (the keyboard would cover what the person came to read)
  const focusSoon = (n: HTMLElement) => { if (matchMedia('(pointer: fine)').matches) setTimeout(() => n.focus({ preventScroll: true }), still() ? 0 : 1000) }
  async function unlockVault(e: SubmitEvent) {
    e.preventDefault()
    if (!pass || gateBusy || !gateSafe) return
    gateBusy = true; gateErr = ''
    const p = pass
    pass = '' // ponytail: a JS string cannot be wiped; the field is emptied at once and the core zeroizes its copy
    try { await vaultUnlock(p) } // success comes back as vault-changed -> enter()
    catch (err) {
      gateErr = humanErr(err)
      hero?.prime(0)
    } finally { gateBusy = false }
  }
  // phone: fingerprint unlock (BioPlugin.kt). The core gets the password from the keystore itself; the sheet shows once
  // per lock (a cancel does not bring it back: the key in the capsule does)
  const android = /Android/i.test(navigator.userAgent)
  let bio = $state({ available: false, enabled: false }), bioAsked = false
  const loadBio = () => { if (android && coreAvailable()) invoke<typeof bio>('bio_state').then(v => (bio = v), () => {}) }
  function bioUnlock() {
    if (gateBusy || !bio.enabled) return
    gateBusy = true; gateErr = ''
    invoke('bio_unlock').then(() => haptic('ok'), e => { const m = String(e); if (m !== 'cancelled') { gateErr = humanErr(m); haptic('err') } loadBio() }).finally(() => (gateBusy = false))
  }
  $effect(() => { if (gateSafe && bio.enabled && !bioAsked && !introOn) { bioAsked = true; setTimeout(bioUnlock, still() ? 0 : 700) } })
  function toggleBio(e: Event) {
    const box = e.currentTarget as HTMLInputElement
    if (box.checked) { box.checked = false; openSheet('bio', e) } // on only through the sheet: password, then the fingerprint
    else invoke('bio_disable').finally(loadBio)
  }
  function enter() {
    if (view !== 'locked') return
    run(async () => {
      if (gateEl) { gateShut = true; await new Promise(r => setTimeout(r, still() ? 0 : 440)) } // fold first, then deal
      view = 'home'; gateShut = false; resetIdle(); load(acc, chain)
      for (const c of chainList.filter(c => c !== chain && !c.test && hasMoney(c)).slice(0, 6)) load(acc, c) // where money was last seen
      hero?.sync(netDeck()) // proven faces now; the core paints the docked one before the deal, the rest when calm
      await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r))) // the column's first layout lands before the deal
      await hero?.unlock()
    })
  }
  function lock() {
    menu = ''
    sheetEl?.shut() // keys or words on screen must not outlive the lock
    if (coreAvailable()) vaultLock()
    startUnlocked = false; bioAsked = false
    run(async () => { view = 'locked'; await hero?.lock() })
  }
  // the sheet grows out of the control that opened it; the deck settles and stops rendering underneath
  function openSheet(mode: SheetMode, e?: Event, wallet?: string, draft?: Draft, account?: string) {
    if (sheet) return
    const origin = (e?.currentTarget as HTMLElement | undefined)?.getBoundingClientRect() ?? null
    menu = ''
    sheet = { mode, wallet, account, origin, draft }
    hero?.rest(true)
  }
  // ---------------------------------------------------------------- agent access: a request from an agent waits for the owner
  let agentOn = $state(false), agentReq = $state<Batch | null>(null)
  function showBatch(b: Batch) {
    const known = accountList.some(a => a.solana === b.to) || contactList.some(c => c.address === b.to)
    agentReq = { ...b, known }
    if (!sheet && view !== 'locked') { menu = ''; sheet = { mode: 'batch', origin: null, batch: agentReq }; hero?.rest(true) }
  }
  // the swap fee wallet («Комиссия»): its receiving accounts are made once, paid from the current account, owner confirms
  let feeInfo = $state<{ owner: string; missing: number } | null>(null)
  const feeName = $derived(accountList.find(a => a.solana === feeInfo?.owner)?.name ?? short(feeInfo?.owner ?? ''))
  $effect(() => { if (view === 'settings' && coreAvailable()) invoke<{ owner: string; missing: number }>('fee_state').then(v => (feeInfo = v), () => (feeInfo = null)) })
  function setupFee(e: Event) {
    if (!acc || acc.watch || preparing) return
    const btn = e.currentTarget as HTMLElement
    preparing = true
    prepareSend(acc.id, 'fee-setup', '0', 'mainnet').then(d => openSheet('send', { currentTarget: btn } as unknown as Event, undefined, d), err => flash(String(err))).finally(() => (preparing = false))
  }
  function setAgent(on: boolean) { invoke<boolean>('agent_access', { on }).then(v => (agentOn = v), e => flash(String(e))) }

  // ---------------------------------------------------------------- add-ons: a tab each, a sandboxed frame, a message bridge
  // The frame runs the installed copy (core: addons.rs) on its own origin; it reaches the wallet only through `bridge`,
  // gated by the permissions shown at install. Keys it offers go through the wallet's own sheet and password.
  let frameEl = $state<HTMLIFrameElement>()
  let addonFile = $state<{ html: string; m: Addon } | null>(null), addonErr = $state('')
  const addonNow = $derived(addons.find(a => a.id === addonId))
  const addonSrc = (id: string) => convertFileSrc(id, 'addon')
  // the core serves only the page of the tab being opened (a frame cannot walk into another add-on's page)
  async function openAddon(id: string) {
    menu = ''
    if (view === 'locked' || (view === 'addon' && addonId === id)) return
    await invoke('addon_open', { id }).catch(() => {})
    if (view === 'receive') hero?.flip(false)
    swap(() => { addonId = id; view = 'addon' })
  }
  async function pickAddon(e: Event) {
    const input = e.currentTarget as HTMLInputElement, f = input.files?.[0]
    input.value = ''; addonErr = ''; addonFile = null
    if (!f) return
    try { const html = await f.text(); addonFile = { html, m: await addonInspect(html) } } catch (err) { addonErr = humanErr(err) }
  }
  async function installAddon() {
    if (!addonFile) return
    try { addons = await addonInstall(addonFile.html); flash(t('addon.installed', { name: addonFile.m.name }), undefined, true); addonFile = null } catch (err) { addonErr = humanErr(err) }
  }
  async function removeAddon(id: string) {
    try { addons = await addonRemove(id); if (addonId === id && view === 'addon') swap(() => (view = 'settings')) } catch (err) { flash(String(err)) }
  }
  // one call at a time per kind is enough for a panel; a flood is dropped, not queued
  let bridgeCalls = 0, bridgeMinute = 0, reading = false
  let keyOffer: ((r: { added: boolean }) => void) | null = null
  const solChain = () => (chain.kind === 'solana' ? chain : chainList.find(c => c.id === 'solana')!)
  async function bridge(method: string, p: Record<string, unknown>, a: Addon): Promise<unknown> {
    const may = (perm: string) => { if (!a.permissions.includes(perm)) throw new Error(`no permission: ${perm}`) }
    switch (method) {
      case 'accounts':
        may('accounts')
        return vaultAccounts.map(x => ({ name: x.name, solana: x.solana, evm: x.evm ?? null }))
      case 'balances': {
        may('balances')
        if (reading) throw new Error('busy: one balances call at a time')
        const list = Array.isArray(p.addresses) ? p.addresses.filter((x): x is string => typeof x === 'string' && isAddress(x)).slice(0, 50) : []
        const c = solChain()
        reading = true
        try {
          return { network: c.name, rows: await Promise.all(list.map(address => readHoldings(address, c).then(
            h => ({ address, sol: h.native, tokens: h.cards.slice(1).map(k => ({ mint: k.id, symbol: k.verified ? k.sym : null, amount: k.amount })) }),
            () => ({ address, sol: null, tokens: [] })))) }
        } finally { reading = false }
      }
      case 'open': { // show one of the owner's accounts in the wallet
        may('accounts')
        const x = vaultAccounts.find(v => v.solana === p.address)
        if (!x) return false
        if (chain.kind !== 'solana') setChain('solana')
        setAccount(x.id); go('home')
        return true
      }
      case 'addKeys': {
        may('keys')
        const items = Array.isArray(p.items) ? p.items : []
        const ok = items.length > 0 && items.length <= 50 && items.every(i => i && typeof i.key === 'string' && i.key.length <= 128 && typeof i.name === 'string' && i.name.length <= 80)
        if (!ok) throw new Error('items: 1..50 of { name, key }')
        if (sheet || keyOffer || view === 'locked') throw new Error('busy')
        const offer = { addon: a.id, addonName: a.name, items: items.map(i => ({ name: String(i.name).trim().slice(0, 40) || a.name, key: String(i.key).trim() })) }
        menu = ''; sheet = { mode: 'addon-keys', origin: frameEl?.getBoundingClientRect() ?? null, offer }; hero?.rest(true)
        return new Promise(r => (keyOffer = r))
      }
    }
    throw new Error(`unknown method: ${method}`)
  }
  function onBridge(e: MessageEvent) {
    const w = frameEl?.contentWindow, a = addonNow
    if (!w || e.source !== w || !a || view !== 'addon') return
    const m = e.data as { talon?: number; id?: unknown; method?: unknown; params?: unknown }
    if (!m || m.talon !== 1 || typeof m.method !== 'string' || !Number.isSafeInteger(m.id)) return
    try { if (JSON.stringify(m).length > 64_000) return } catch { return }
    const now = Math.floor(Date.now() / 60_000)
    if (now !== bridgeMinute) { bridgeMinute = now; bridgeCalls = 0 }
    const reply = (r: { result?: unknown; error?: string }) => w.postMessage({ talon: 1, id: m.id, ...r }, '*') // opaque origin: '*', but only to this frame
    if (++bridgeCalls > 240) return reply({ error: 'too many calls' })
    bridge(m.method, (m.params && typeof m.params === 'object' ? m.params : {}) as Record<string, unknown>, a).then(result => reply({ result }), err => reply({ error: String(err?.message ?? err) }))
  }
  // the frame is told when the owner's accounts change (a key offer landed, a wallet was added)
  $effect(() => { const l = vaultAccounts.map(x => x.solana).join(); if (view === 'addon' && l && addonNow?.permissions.includes('accounts')) frameEl?.contentWindow?.postMessage({ talon: 1, event: 'accounts' }, '*') })
  function sheetClosed(done: boolean) {
    const m = sheet?.mode, wasSwap = !!sheet?.draft?.swap, wasFee = !!feeInfo && sheet?.draft?.to === feeInfo.owner
    if (m === 'addon-keys') { keyOffer?.({ added: done }); keyOffer = null; if (done) flash(t('addon.added'), undefined, true) }
    if (m === 'batch') { if (done) { agentReq = null; flash(t('agent.sent'), undefined, true); setTimeout(refresh, 2500) } else invoke<{ waiting: Batch | null }>('agent_state').then(s => (agentReq = s.waiting ? { ...s.waiting, known: agentReq?.known } : null), () => {}) }
    sheet = null
    hero?.rest(away) // a phone tab without the card keeps the scene asleep
    if (enterAfterSheet) { enterAfterSheet = false; enter() }
    else if (view === 'locked') hero?.prime(0)
    if (done && m === 'send' && wasSwap) { view = 'home'; hero?.swapCard(netCard(chain)); setTimeout(refresh, 2500) } // a swap: one turn of the card, then the new balances
    else if (done && m === 'send' && wasFee) { feeInfo = { ...feeInfo!, missing: 0 }; setTimeout(refresh, 2500) } // stays in Settings
    else if (done && m === 'send') { view = 'token'; hero?.throwCard(netCard(chain)) } // the card leaves toward the recipient, docks back
    if (done && m === 'password') refreshVault()
    if (m === 'password' || m === 'bio') loadBio()
  }

  // the password capsule is on screen: keep the window out of screenshots too (Codex 07.10)
  const gateUp = $derived(view === 'locked' && hasVault && !vaultOpen && coreAvailable())
  // a stale answer from an earlier request must not mark the capsule safe (second security review 07.10)
  let gateSafe = $state(false), gateGen = 0
  $effect(() => {
    const on = gateUp, g = ++gateGen
    gateSafe = false
    if (sheet) return
    invoke('panel_secret', { on }).then(() => { if (g === gateGen) gateSafe = on }, e => { if (g === gateGen && on) gateErr = humanErr(e) })
  })

  // auto-lock after a quiet period
  let idle = 0
  function resetIdle() { clearTimeout(idle); if (view !== 'locked' && !vaultOpen) idle = window.setTimeout(lock, autolock * 60_000) }
  function setAutolock(m: number) { autolock = m; try { localStorage.setItem('autolock', String(m)) } catch {} ; resetIdle() }

  // the core's fixed English network codes, said in Russian; anything else shows as the core wrote it
  // an unknown English line from a node never reaches the screen as is (owner 09.10)
  const netErr = humanErr
  function flash(text: string, sub?: string, ok = false) { toast = { text: humanErr(text), sub, ok }; setTimeout(() => (toast = null), 3600) }

  // phone: the card is part of the page — it stands in a hole at the top of home, a token and its QR, and scrolls away
  // with the page; on the other tabs the screen is all content. Out of sight the scene sleeps (as under a sheet).
  const phoneMQ = matchMedia('(max-width: 860px)')
  let phone = $state(phoneMQ.matches)
  phoneMQ.addEventListener('change', e => (phone = e.matches))
  const cardView = $derived(view === 'home' || view === 'token' || view === 'receive')
  let away = $state(false)
  function setAway(b: boolean) { if (b === away || !hero) return; away = b; if (!sheet) hero.rest(b) }
  let colEl = $state<HTMLElement>(), scrolled = $state(false)
  function band() {
    scrolled = phone && (colEl?.scrollTop ?? 0) > 4 // the header gets its backing once anything passes under it
    if (!hero || !phone || view === 'locked') return setAway(false) // the lock screen and the welcome keep their centred deck
    const hole = colEl?.querySelector('.hole')?.getBoundingClientRect(), top = document.querySelector('.top')?.getBoundingClientRect().bottom ?? 0
    if (!hole || !cardView) return setAway(true)
    setAway(hole.bottom < top + 24)
    hero.setBand(hole.top - 6, hole.bottom - 22) // the card's optical centre sits low in its band: this evens the air
  }
  $effect(() => { void view; void hero; void phone; requestAnimationFrame(band) })
  $effect(() => { addEventListener('resize', band); return () => removeEventListener('resize', band) })

  // ---------------------------------------------------------------- navigation
  // One screen change = one movement of the whole column (owner 08.10: not every line rising on its own). View
  // Transitions on the column only (the 3D scene stays live, root takes no part); a token's ticker flies between its
  // list row and the token page title. After the first entrance the per-line rises are off (`settled`).
  let settled = $state(false), vtId = $state('')
  function swap(change: () => void, shared = '') {
    settled = true
    vtId = shared
    const d = document as Document & { startViewTransition?: (f: () => Promise<void>) => unknown }
    if (!d.startViewTransition || still()) { change(); return }
    d.startViewTransition(async () => { change(); await tick() })
  }
  let before: View = 'home' // where settings opened from: the gear closes them back there
  function go(v: View) {
    menu = ''
    if (view === 'locked') return
    if (v === 'settings' && view === 'settings') v = before
    else if (v === view) return // a click on the docked card at home, or the current tab: nothing replays (owner 09.10)
    else if (v === 'settings') before = view === 'send' || view === 'receive' ? 'token' : view
    if (view === 'receive') hero?.flip(false)
    swap(() => (view = v))
    if (v === 'history') loadHistory()
  }
  function open(id: string) { menu = ''; if (view === 'receive') hero?.flip(false); swap(() => { sel = id; view = 'token' }, id) }
  function receive() {
    sel = card?.id ?? ''
    swap(() => (view = 'receive'))
    void hero?.flip(true, myAddress, chain.name.replace(/ Mainnet$/, ''), t('recv.cardNote', { sym: chain.symbol })) // never dropped behind another animation (critique 10.10)
    loadHistory() // quiet: fills «recent arrivals» when the node (or the EVM explorer) allows
  }
  function send(prefill = '') {
    if (view === 'receive') hero?.flip(false)
    sel = card?.id ?? ''
    to = prefill; amount = ''; touched = false
    swap(() => (view = 'send'))
  }
  function back() {
    if (view === 'receive') { hero?.flip(false); swap(() => (view = 'token')); return }
    if (view === 'send') { swap(() => (view = 'token')); return }
    swap(() => (view = 'home'), view === 'token' ? card?.id : '')
  }

  // ---------------------------------------------------------------- networks and accounts
  function setChain(id: string) {
    menu = ''
    if (id === chainId) return
    quiet = view === 'home'
    const ring = netDeck().map(c => c.id).join()
    chainId = id; sel = ''
    try { localStorage.setItem('lastChain', id) } catch {}
    load(acc, chain)
    if (view === 'locked') return
    view = 'home'
    // context switches now; the ring turns alongside. Not awaited, not behind the busy lock: rapid clicks retarget
    if (netDeck().map(c => c.id).join() !== ring) hero?.setDeck(netDeck(), chainId, 'glass', 'update').then(() => hero?.goTo(id))
    else { hero?.sync(netDeck()); hero?.goTo(id) }
  }
  function setAccount(id: string) {
    menu = ''
    if (id === acc?.id) return
    quiet = view === 'home'
    accId = id
    try { localStorage.setItem('lastAccount', id) } catch {}
    sel = ''; view = 'home'
    load(acc, chain)
    hero?.sync(netDeck())
  }
  let watchAddr = $state(''), watchTouched = $state(false)
  function addWatch(e: Event) {
    e.preventDefault(); watchTouched = true
    const a = watchAddr.trim()
    if (!isAddress(a) && !isEvmAddress(a)) return
    const id = 'watch-' + a
    if (!accountList.some(x => x.id === id)) {
      watchAccounts = [...watchAccounts, { id, name: t('acc.watchName'), solana: isAddress(a) ? a : '', evm: isEvmAddress(a) ? a : null, watch: true }]
      saveWatch(watchAccounts)
    }
    watchAddr = ''; watchTouched = false
    if (isAddress(a) && chain.kind !== 'solana') chainId = 'solana'
    if (isEvmAddress(a) && chain.kind !== 'evm') chainId = 'ethereum'
    setAccount(id)
  }
  function removeWatch(id: string) {
    watchAccounts = watchAccounts.filter(a => a.id !== id); saveWatch(watchAccounts)
    if (acc?.id === id) setAccount(accountList[0]?.id ?? '')
  }

  let nName = $state(''), nRpc = $state(''), nChain = $state(''), nSym = $state(''), nExp = $state(''), nTouched = $state(false)
  const nErr = $derived(
    !nName.trim() ? t('net.errName')
    : !/^https:\/\/[^\s]+\.[^\s]+/.test(nRpc.trim()) ? t('net.errRpc')
    : !/^[1-9]\d*$/.test(nChain.trim()) ? t('net.errChain')
    : chainList.some(c => c.chainId === Number(nChain)) ? t('net.errChainTaken')
    : !/^[A-Za-z0-9]{2,8}$/.test(nSym.trim()) ? t('net.errSymbol') : '')
  function addNet(e: Event) {
    e.preventDefault(); nTouched = true
    if (nErr) return
    const id = 'custom-' + nChain.trim()
    chainList = [...chainList, { id, name: nName.trim(), kind: 'evm', symbol: nSym.trim().toUpperCase(), decimals: 18, chainId: Number(nChain), rpc: nRpc.trim(), explorer: nExp.trim() }]
    saveCustomNets(chainList.filter(c => c.id.startsWith('custom-'))) // they used to vanish on restart
    nName = nRpc = nChain = nSym = nExp = ''; nTouched = false; addingNet = false
    fav = [...fav, id]; saveFav(fav) // added by hand: wanted on the ring
    setChain(id)
  }

  // ---------------------------------------------------------------- send (form checks here; the core re-checks, builds and signs)
  let to = $state(''), amount = $state(''), touched = $state(false), preparing = $state(false)
  const recipients = $derived(accountList.some(a => a.id !== acc?.id && !a.watch && valid(addressFor(a, chain))) || contactList.some(c => valid(c.address)))
  const contactHit = $derived(contactList.find(c => c.name.toLowerCase() === to.trim().toLowerCase()))
  const toAddr = $derived(contactHit ? contactHit.address : to.trim())
  const valid = (a: string) => (chain.kind === 'evm' ? isEvmAddress(a) : isAddress(a))
  const amt = $derived(Number(amount.replace(/\s/g, '').replace(',', '.')))
  const maxAmt = $derived(card?.amount ?? 0)
  // an address of another family: offer its networks (where money is, else the main one); families not sent to yet say so
  const FAM = { solana: 'Solana', evm: 'Ethereum', tron: 'Tron', bitcoin: 'Bitcoin', ton: 'TON' } as const
  const toFam = $derived(familyOf(toAddr))
  const toOther = $derived.by(() => {
    if (!toFam || toFam === chain.kind || (toFam !== 'solana' && toFam !== 'evm')) return []
    const nets = chainList.filter(c => c.kind === toFam && !!c.test === !!chain.test && addressFor(acc, c))
    const rich = nets.filter(hasMoney)
    return (rich.length ? rich : nets).slice(0, 3)
  })
  function hopTo(id: string, keep = to) { setChain(id); send(keep) }
  const toErr = $derived(!to.trim() ? t('send.errTo') : toFam && toFam !== 'solana' && toFam !== 'evm' ? t('send.unsupported', { fam: FAM[toFam] })
    : !valid(toAddr) ? t('send.errAddr', { net: chain.name }) : toAddr === myAddress ? t('send.errSelf') : '')
  const amtErr = $derived(!amount.trim() ? t('send.errAmt') : !(amt > 0) ? t('send.errZero') : amt > maxAmt ? t('send.errMax', { v: fmt(maxAmt, card?.sym) }) : '')
  function reviewSend(e: Event) {
    e.preventDefault(); touched = true
    if (toErr || amtErr || sheet || preparing) return
    const cl = cluster(chain)
    if (!card) return
    if (!acc || acc.watch) return
    const btn = (e as SubmitEvent).submitter ?? undefined
    preparing = true
    ;(cl ? prepareSend(acc.id, toAddr, amount, cl, card.id === 'native' ? undefined : card.id) : prepareEvmSend(acc.id, toAddr, amount, chain, card.id === 'native' ? undefined : card.id))
      .then(d => openSheet('send', btn ? { currentTarget: btn } as unknown as Event : undefined, undefined, d), e => flash(String(e)))
      .finally(() => (preparing = false))
  }

  // the recipient from the clipboard: Android through the app (a WebView may not read it), elsewhere the page's own way
  async function pasteTo() {
    const app = (window as unknown as { __cwApp?: { paste?(): string } }).__cwApp
    try { const v = (app?.paste ? app.paste() : await navigator.clipboard.readText()).trim(); if (v) to = v.slice(0, 200) } catch {}
  }
  // the phone's camera reads the recipient's code: a plain address, a Solana Pay link (solana:<to>?amount=) or an
  // EIP-681 link (ethereum:<to>@chain, …/transfer?address=<to>). What it gives is only typed into the form: the address
  // check and the confirmation stay as they are
  let scanning = $state(false)
  async function scanTo() {
    const bs = await import('@tauri-apps/plugin-barcode-scanner')
    try {
      if ((await bs.checkPermissions()) !== 'granted' && (await bs.requestPermissions()) !== 'granted') { flash(t('send.camNo')); return }
      scanning = true; document.documentElement.classList.add('scanning'); hero?.rest(true)
      // ponytail: the plugin's cancel (2.5.2) forgets the pending scan before rejecting it, so its promise never
      // settles; our own stop wins the race. Drop the race when the plugin is fixed upstream
      const r = await Promise.race([bs.scan({ windowed: true, formats: [bs.Format.QRCode] }), new Promise<null>(res => (stopScan = () => res(null)))])
      if (!r) return
      const p = parseScan(r.content)
      // the code names its network (EIP-681 @chain): send there, never on whatever network is open
      if (p.chainId && p.chainId !== chain.chainId) {
        const c = chainList.find(x => x.chainId === p.chainId)
        if (!c) { flash(t('send.scanNet', { id: p.chainId })); return }
        hopTo(c.id, p.to); haptic('ok'); return
      }
      to = p.to; if (p.amount) amount = p.amount.replace('.', ','); haptic('ok')
    } catch (e) { if (!/cancel/i.test(String(e))) flash(String(e)) }
    finally { stopScan = null; scanning = false; document.documentElement.classList.remove('scanning'); hero?.rest(away || !!sheet) }
  }
  let stopScan: (() => void) | null = null
  const cancelScan = () => { stopScan?.(); import('@tauri-apps/plugin-barcode-scanner').then(bs => bs.cancel()).catch(() => {}) }
  // «Скопировано» only when it really was (owner 11.10: copying felt unreliable); Android through the app's own clipboard
  async function copy(text: string) {
    const app = (window as unknown as { __cwApp?: { copy?(t: string): void } }).__cwApp
    try { if (app?.copy) app.copy(text); else await navigator.clipboard.writeText(text) } catch { haptic('err'); flash(t('act.copyFail')); return }
    haptic('tick')
    copied = text
    setTimeout(() => (copied = ''), 1600)
  }

  // ---------------------------------------------------------------- swap (Solana mainnet; the core builds, checks and dry-runs the route)
  let swIn = $state('SOL'), swOut = $state(USDC), swAmt = $state(''), swQuote = $state<SwapQuote | null>(null), swErr = $state(''), swBusy = $state(false)
  // the native coin's id in a swap: «SOL» on Solana, the zero address on EVM networks (LI.FI's convention)
  const NATIVE_ID = $derived(chain.kind === 'solana' ? 'SOL' : EVM_NATIVE)
  const canSwap = $derived(!chain.test && (chain.id === 'solana' || (chain.kind === 'evm' && !chain.id.startsWith('custom-'))))
  const idOf = (c: Card) => (c.id === 'native' ? NATIVE_ID : c.id)
  const swInCard = $derived(tokens.find(c => idOf(c) === swIn))
  // tokens picked from Jupiter's list: their ticker, name and picture (verified ones name themselves; others carry their mint)
  let picked = $state<Record<string, TokenInfo>>({})
  const symOf = (id: string) => id === NATIVE_ID ? chain.symbol : id === 'SOL' ? 'SOL' : SWAP_TO.find(x => x.id === id)?.sym ?? tokens.find(c => c.id === id && c.verified)?.sym ?? (picked[id]?.verified ? picked[id].symbol : null) ?? short(id) // an unverified ticker never stands alone
  const swIns = $derived(tokens.filter(c => !hidden.includes(c.id)))
  // «Всё» for a swap keeps 0.01 SOL back: new token accounts and the fee come out of SOL
  const swMax = $derived(!swInCard?.raw || swInCard.decimals === undefined ? null
    : swIn === EVM_NATIVE ? null // an EVM network's coin also pays a gas that moves with every block: the amount is typed
    : exact(swIn === 'SOL' ? BigInt(swInCard.raw) - 10_000_000n : BigInt(swInCard.raw), swInCard.decimals))
  // the last answer for this pair stays on screen (dimmed) while a newer amount is being quoted
  let swLast = $state<SwapQuote | null>(null)
  const swShown = $derived(swQuote ?? swLast)
  $effect(() => { if (swQuote) swLast = swQuote })
  $effect(() => { void swIn; void swOut; swLast = null })
  $effect(() => { if (!swAmt.trim()) swLast = null })
  let qTimer = 0, qGen = 0
  $effect(() => { // a live quote 450 ms after the last keystroke; an old answer never overwrites a newer question
    const a = swAmt.trim(), i = swIn, o = swOut
    clearTimeout(qTimer); swQuote = null; swErr = ''
    if (view !== 'swap' || !a || !(Number(a.replace(',', '.')) > 0)) return
    const g = ++qGen
    const c = chain, me = myAddress
    qTimer = window.setTimeout(() => (c.kind === 'solana' ? swapQuote(i, o, a) : evmSwapQuote(c, me, i, o, a)).then(q => { if (g === qGen) swQuote = q }, e => { if (g === qGen) swErr = String(e) }), 450)
  })
  // the default «receive» token: USDC of the network (Solana's mint; on EVM networks the one LI.FI lists as verified)
  const defaultOut = () => chain.kind === 'solana' ? USDC : (topTokens?.find(x => x.symbol === 'USDC' && x.verified)?.id ?? topTokens?.find(x => x.id !== EVM_NATIVE)?.id ?? EVM_NATIVE)
  let swapNet = ''
  function openSwap(from = '') {
    if (swapNet !== chain.id) { topTokens = null; swapNet = chain.id; swOut = chain.kind === 'solana' ? USDC : '' } // another network, another list
    swIn = from || NATIVE_ID; swAmt = ''; picker = ''
    swap(() => (view = 'swap'))
    // the list once per network: the default «receive» token gets its picture and the picker opens full
    const ready = (l: TokenInfo[]) => { topTokens = l; for (const x of l) if (!picked[x.id] && (x.id === swOut || x.verified)) picked[x.id] = x; if (!swOut || swOut === swIn) swOut = swIn === defaultOut() ? NATIVE_ID : defaultOut() }
    if (!topTokens) (chain.kind === 'solana' ? solTokens('') : evmSwapTokens(chain)).then(ready, () => {})
    else if (!swOut || swOut === swIn) swOut = swIn === defaultOut() ? NATIVE_ID : defaultOut()
  }
  function pickIn(id: string) { swIn = id; if (swOut === id) swOut = id === NATIVE_ID ? defaultOut() : NATIVE_ID }
  // the token picker (owner 10.10: like Phantom — a list to scroll and search, not four chips)
  let picker = $state<'' | 'in' | 'out'>(''), pickQ = $state(''), pickList = $state<TokenInfo[]>([]), pickBusy = $state(false), pickErr = $state('')
  let topTokens: TokenInfo[] | null = null, pGen = 0, pTimer = 0
  function openPicker(which: 'in' | 'out') { picker = which; pickQ = ''; pickErr = ''; if (which === 'out') searchTokens('') }
  function searchTokens(q: string) {
    clearTimeout(pTimer); const g = ++pGen
    if (!q.trim() && topTokens) { pickList = chain.kind === 'solana' ? topTokens : topTokens.slice(0, 120); pickBusy = false; return }
    if (chain.kind === 'evm') { // the whole list is already here: search it in place
      const s = q.trim().toLowerCase()
      pickList = (topTokens ?? []).filter(x => x.symbol.toLowerCase().includes(s) || x.name.toLowerCase().includes(s) || x.id.toLowerCase() === s).slice(0, 120)
      pickBusy = !topTokens; return
    }
    pickBusy = true
    pTimer = window.setTimeout(() => solTokens(q).then(l => { if (g !== pGen) return; pickList = l; pickErr = ''; if (!q.trim()) topTokens = l }, e => { if (g === pGen) pickErr = String(e) }).finally(() => { if (g === pGen) pickBusy = false }), q.trim() ? 300 : 0)
  }
  $effect(() => { if (picker === 'out') searchTokens(pickQ) })
  function choose(id: string, info?: TokenInfo) {
    if (info) picked[id] = info
    if (picker === 'in') pickIn(id); else { swOut = id; if (swIn === id) swIn = id === NATIVE_ID ? defaultOut() : NATIVE_ID }
    picker = ''
  }
  // the token's own picture, fetched by the core (the page loads nothing from the web)
  function remoteIcon(img: HTMLImageElement, url: string | null) {
    const load = (u: string | null) => { if (u) tokenIcon(u).then(d => { img.src = d; img.classList.add('in') }, () => {}) }
    load(url)
    return { update: load }
  }
  // desktop: the search field is ready to type into; a phone keeps the keyboard down until the field is tapped
  function focusDesktop(el: HTMLInputElement) { if (!matchMedia('(pointer: coarse)').matches) setTimeout(() => el.focus(), 60) }
  const pickMine = $derived(swIns.filter(c => !pickQ.trim() || (c.sym + ' ' + c.name + ' ' + c.id).toLowerCase().includes(pickQ.trim().toLowerCase())))
  function flipSwap() { const i = swIn; swIn = swOut; swOut = i; swAmt = '' }
  function reviewSwap(e: Event) {
    e.preventDefault()
    if (!acc || acc.watch || !swQuote || sheet || swBusy) return
    const btn = (e as SubmitEvent).submitter ?? undefined
    swBusy = true
    ;(chain.kind === 'solana' ? prepareSwap(acc.id, swIn, swOut, swAmt) : evmPrepareSwap(acc.id, chain, swIn, swOut, swAmt))
      .then(d => { d.symbol = symOf(swIn); if (d.swap) d.swap.sym = symOf(swOut); openSheet('send', btn ? { currentTarget: btn } as unknown as Event : undefined, undefined, d) }, e => flash(String(e)))
      .finally(() => (swBusy = false))
  }

  // ---------------------------------------------------------------- contacts (public addresses, kept on this device)
  let newName = $state(''), newAddr = $state(''), addTouched = $state(false)
  const addErr = $derived(!newName.trim() ? t('addr.errName') : !(isAddress(newAddr) || isEvmAddress(newAddr)) ? t('addr.errAddr') : '')
  function addContact(e: Event) {
    e.preventDefault(); addTouched = true
    if (addErr) return
    contactList = [{ name: newName.trim(), address: newAddr.trim() }, ...contactList]
    saveContacts(contactList)
    newName = newAddr = ''; addTouched = false
  }
  function removeContact(address: string) { contactList = contactList.filter(c => c.address !== address); saveContacts(contactList) }

  // ---------------------------------------------------------------- history (Solana): the list first, then each entry read from the chain
  // Cached per account+network (owner 09.10: no reload on every visit, no node errors in English): what was read is
  // shown at once, also after a restart; the list refreshes quietly behind it, at most every 30 s, backing off when the
  // public node says «too many requests». A read entry never changes, so it is kept (public data, this device only).
  const histKey = (address: string, c: Chain) => `${c.id}|${address}`
  let sigLists = $state<Record<string, Sig[]>>((() => { try { return JSON.parse(localStorage.getItem('hist') ?? '{}') } catch { return {} } })())
  let txs = $state<Record<string, Tx | 'err'>>((() => { try { return JSON.parse(localStorage.getItem('txs') ?? '{}') } catch { return {} } })()) // `${address}|${sig}`
  const hist = $derived(sigLists[histKey(myAddress, chain)] ?? [])
  let histState = $state(''), histStale = $state(false)
  const histAt: Record<string, number> = {}
  const limited = (e: unknown) => /rate-limiting/.test(String(e))
  const pause = (ms: number) => new Promise(r => setTimeout(r, ms))
  function persistHistory() {
    try {
      localStorage.setItem('hist', JSON.stringify(sigLists))
      const keep = Object.entries($state.snapshot(txs)).filter(([, v]) => v !== 'err').slice(-600) // ponytail: newest 600 entries per device
      localStorage.setItem('txs', JSON.stringify(Object.fromEntries(keep)))
    } catch {}
  }
  // ---------------------------------------------------------------- permissions (Адреса → Разрешения)
  let perms = $state<Perm[]>([]), permState = $state(''), permKey = '', revoking = $state(false)
  async function loadPerms() {
    if (!myAddress) return
    const key = `${myAddress}|${chain.id}`, c = chain, address = myAddress
    if (key !== permKey) perms = []
    permKey = key; permState = 'loading'
    try { const l = await readPerms(address, c, tokens); if (permKey === key) { perms = l; permState = '' } }
    catch (e) { if (permKey === key) permState = String(e) }
  }
  function revoke(p: Perm, e: Event) {
    if (!acc || acc.watch || sheet || revoking) return
    revoking = true
    prepareRevoke(acc.id, chain, p).then(d => { d.symbol = p.sym; openSheet('send', e, undefined, d) }, err => flash(String(err))).finally(() => (revoking = false))
  }
  $effect(() => { void chainId; void accId; if (view === 'addresses' && addrTab === 'perms') loadPerms() })

  async function loadHistory() {
    if (!myAddress) return
    if (!cluster(chain)) return loadEvmHistory()
    const address = myAddress, c = chain, key = histKey(address, c)
    histState = ''
    if (Date.now() - (histAt[key] ?? 0) < 30_000) return
    histAt[key] = Date.now()
    if (!sigLists[key]) histState = 'loading'
    let list: Sig[] | null = null
    for (const wait of [0, 4000, 12000]) { // the node asked to slow down: wait, then try again, three times at most
      if (wait) await pause(wait)
      try { list = await readHistory(address, c); break } catch (e) { if (!limited(e)) { if (sigLists[key]) histStale = true; else histState = String(e); return } }
    }
    if (!list) { histAt[key] = 0; if (sigLists[key]) histStale = true; else histState = t('err.the network is rate-limiting requests, try again in a minute'); return }
    sigLists[key] = list; histState = ''; histStale = false
    // two at a time; a rate limit waits and retries, anything else marks the entry and moves on
    const todo = list.filter(x => !txs[`${address}|${x.sig}`])
    const read = async (sig: string): Promise<Tx | 'err'> => {
      for (const wait of [0, 1500, 5000]) { if (wait) await pause(wait); try { return await readTx(sig, address, c) } catch (e) { if (!limited(e)) return 'err' } }
      return 'err'
    }
    const worker = async () => { for (let x; (x = todo.shift()); ) txs[`${address}|${x.sig}`] = await read(x.sig) }
    await Promise.all([worker(), worker()])
    persistHistory()
  }
  // EVM: the explorer gives the list with details in one answer; cached the same way (shown at once, refreshed ≤ 30 s)
  async function loadEvmHistory() {
    const address = myAddress, c = chain, key = histKey(address, c)
    histState = ''
    if (Date.now() - (histAt[key] ?? 0) < 30_000) return
    histAt[key] = Date.now()
    if (!sigLists[key]) histState = 'loading'
    try {
      const l = await readEvmHistory(address, c)
      sigLists[key] = l.map(x => ({ sig: x.sig, time: x.time, ok: x.ok }))
      for (const x of l) txs[`${address}|${x.sig}`] = x
      histState = ''; histStale = false
      persistHistory()
    } catch (e) {
      histAt[key] = 0
      if (sigLists[key]) histStale = true; else histState = String(e).includes('не собирается') ? t('hist.evmSoon') : String(e)
    }
  }
  const txKind = (d: Tx): 'in' | 'out' | 'swap' | 'other' => !d.moves.length ? 'other' : d.moves.some(m => m.incoming) && d.moves.some(m => !m.incoming && m.mint) ? 'swap' : d.moves.some(m => m.incoming) ? 'in' : 'out'
  // receive: the last few arrivals to this address, from the same cache (the card shows QR and address)
  const incoming = $derived(hist.map(x => ({ ...x, d: txs[`${myAddress}|${x.sig}`] }))
    .filter((x): x is Sig & { d: Tx } => !!x.d && x.d !== 'err' && x.ok && txKind(x.d) === 'in').slice(0, 3))
  const peerName = (a: string) => contactList.find(c => c.address === a)?.name ?? short(a)
  let txOpen = $state('') // phone: the operation shown in the sheet below
  $effect(() => { void view; void chainId; void accId; txOpen = '' })
  // the list is cut into days; a row then says only the time
  const dayOf = (s: number | null) => {
    if (!s) return '—'
    const d = new Date(s * 1000), now = new Date(), y = new Date(now); y.setDate(now.getDate() - 1)
    return d.toDateString() === now.toDateString() ? t('hist.today') : d.toDateString() === y.toDateString() ? t('hist.yesterday')
      : d.toLocaleDateString('ru-RU', { day: 'numeric', month: 'long', ...(d.getFullYear() !== now.getFullYear() ? { year: 'numeric' } : {}) })
  }
  const clock = (s: number | null) => (s ? new Date(s * 1000).toLocaleTimeString('ru-RU', { hour: '2-digit', minute: '2-digit' }) : '—')
  const when = (s: number | null) => (s ? new Date(s * 1000).toLocaleString('ru-RU', { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' }) : '—')

  // ---------------------------------------------------------------- the network menu: search, ★, what each network holds
  // Russian and short names people type («эфир», «бинанс»); the coin and the Chain ID are searched too
  const NET_AKA: Record<string, string> = {
    solana: 'солана spl', ethereum: 'эфир эфириум erc20', base: 'бейс база coinbase', arbitrum: 'арбитрум arb', optimism: 'оптимизм op',
    polygon: 'полигон матик matic', bnb: 'бинанс бнб bsc binance bep20', avalanche: 'аваланч лавина', robinhood: 'робинхуд',
    linea: 'линея', unichain: 'юничейн uniswap', sonic: 'соник fantom', gnosis: 'гносис', scroll: 'скролл', ink: 'инк kraken',
    worldchain: 'ворлд world', berachain: 'берачейн', monad: 'монад', hyperevm: 'хайпер hyperliquid',
  }
  const hasMoney = (c: Chain) => { const h = holdings[hk(acc, c)]; return !!h && (h.native > 0 || h.cards.some(x => x.verified && x.amount > 0)) }
  const favNets = $derived(chainList.filter(c => !c.test && (fav.includes(c.id) || c.id === chainId)))
  const moneyNets = $derived(chainList.filter(c => !c.test && !favNets.includes(c) && hasMoney(c)))
  const restNets = $derived(chainList.filter(c => !c.test && !favNets.includes(c) && !moneyNets.includes(c)))
  const netFound = $derived.by(() => {
    const q = netQ.trim().toLowerCase()
    return q ? chainList.filter(c => (showTest || !c.test) && `${c.name} ${c.symbol} ${c.chainId ?? ''} ${NET_AKA[c.id] ?? ''}`.toLowerCase().includes(q)) : []
  })
  // a network's line in the menu: its dollars when prices are known, else its own coin; nothing until it was checked
  function netWorth(c: Chain) {
    const h = holdings[hk(acc, c)]
    if (!h) return ''
    if (!hasMoney(c)) return '0'
    const vals = !c.test && showUsd ? h.cards.map(x => prices[pidOf(x, c)] === undefined ? undefined : prices[pidOf(x, c)] * x.amount).filter((v): v is number => v !== undefined) : []
    return vals.length ? usd(vals.reduce((a, b) => a + b, 0)) : fmt(h.native, c.symbol)
  }
  function toggleFav(id: string) {
    fav = fav.includes(id) ? fav.filter(x => x !== id) : [...fav, id]; saveFav(fav)
    if (view !== 'locked') hero?.setDeck(netDeck(), chainId, 'glass', 'update')
  }
  // «Проверить все»: only on the owner's word (no silent sweep of twenty RPCs); four at a time, public nodes throttle bursts
  let checking = $state(false)
  async function checkAll() {
    if (checking || !acc) return
    checking = true
    const list = chainList.filter(c => !c.test && addressFor(acc, c))
    for (let i = 0; i < list.length; i += 4) await Promise.all(list.slice(i, i + 4).map(c => load(acc, c, true)))
    if (showUsd) {
      const got = list.map(c => [c, holdings[hk(acc, c)]] as const).filter(([, h]) => h)
      const mints = got.filter(([c]) => c.kind === 'solana').flatMap(([c, h]) => h!.cards.map(x => pidOf(x, c)))
      const coins = got.filter(([c]) => c.kind === 'evm').flatMap(([c, h]) => h!.cards.map(x => pidOf(x, c)).filter(Boolean))
      await usdPrices([...new Set(mints)], [...new Set(coins)]).then(p => (prices = { ...prices, ...p }), () => {})
    }
    checking = false
  }
  let lastShowTest = false
  $effect(() => { if (showTest !== lastShowTest) { lastShowTest = showTest; if (view !== 'locked') hero?.setDeck(netDeck(), chainId, 'glass', 'update') } })

  // Android back (gesture or button): one step back, like any app — sheet, token list, menu, screen, home; from home or
  // the lock screen the app goes to the background (not closed: the next open is instant)
  function backPress() {
    const toBackground = () => (window as unknown as { __cwApp?: { home(): void } }).__cwApp?.home()
    resetIdle()
    if (scanning) cancelScan()
    else if (sheet) { if (sheetEl && !sheetEl.dismiss()) toBackground() }
    else if (picker) picker = ''
    else if (txOpen) txOpen = ''
    else if (menu) menu = ''
    else if (view === 'home' || view === 'locked') toBackground()
    else if (view === 'token' || view === 'receive' || view === 'send' || view === 'swap') back()
    else go('home')
  }
  function keys(e: KeyboardEvent) {
    if (e.key === 'Escape' && picker) { picker = ''; return } // the token list closes, the swap stays
    resetIdle()
    if (e.key === 'Escape' && txOpen) { txOpen = ''; return }
    if (e.key === 'Escape' && menu && !sheet) { menu = ''; return }
    if ((e.ctrlKey || e.metaKey) && e.code === 'KeyK' && view !== 'locked' && !sheet) { e.preventDefault(); menu = menu === 'net' ? '' : 'net'; addingNet = false; return }
    if (e.target instanceof HTMLInputElement) return
    if (e.key === 'Escape' && !sheet && view !== 'home' && view !== 'locked') back()
  }

  onMount(() => {
    if (isTauri() && !(globalThis as any).__cwMock && !/Android/i.test(navigator.userAgent)) document.documentElement.style.setProperty('--wc', '126px') // room for the window buttons
    const vs = coreAvailable() ? refreshVault().catch(() => undefined) : Promise.resolve(undefined)
    if (coreAvailable()) loadPending()
    loadBio()
    vs.then(s => { startUnlocked = !!s?.unlocked; kindRes(!coreAvailable() ? 'none' : !s?.accounts.length ? 'welcome' : s.unlocked ? 'open' : 'locked') })
    // the heavy part (WebGL, light, card faces) starts once the intro's circle is traced
    Promise.all([traced, vs]).then(() => {
      try { hero = createHero(canvas, { pick: id => setChain(id), open: id => (id === chainId ? go('home') : setChain(id)) }); hero.setTheme(theme); if (import.meta.env.DEV) (window as any).__hero = hero } // side card -> switch; front -> home
      catch { hero = null } // no WebGL: the wallet works without the hero
      return hero?.prepare(netDeck(), chainId, startUnlocked || !hasVault, !hasVault, !still() && !hasVault) // scattered cards are all in view: every face up front; the coin ceremony is the first launch only
    }).catch(() => {}).finally(readyRes)
    if (!coreAvailable()) return
    const offs: Promise<() => void>[] = [
      listen<{ accounts: Account[]; unlocked: boolean }>('vault-changed', e => {
        vaultAccounts = e.payload.accounts
        hasVault = e.payload.accounts.length > 0
        if (!acc) accId = vaultAccounts[0]?.id ?? ''
        refreshVault() // picks up "no password" and the rest of the public state
        if (e.payload.unlocked) { if (sheet) enterAfterSheet = true; else enter() } // the sheet folds first, then the deal
        hero?.sync(netDeck())
      }),
      listen<{ sig: string; state: 'sent' | 'sent-unknown' | 'confirmed' | 'finalized' | 'failed' | 'failed-allowance' | 'expired' | 'replaced' | 'unknown' }>('send-status', ({ payload: p }) => {
        flash(t(`send.st.${p.state}`), short(p.sig), p.state === 'finalized')
        loadPending()
        if (p.state !== 'sent' && p.state !== 'sent-unknown') { refresh(); if (addrTab === 'perms') loadPerms() }
      }),
    ]
    invoke<{ on: boolean; waiting: Batch | null }>('agent_state').then(s => { agentOn = s.on; if (s.waiting) agentReq = s.waiting }, () => {})
    offs.push(listen<Batch>('agent-request', e => showBatch(e.payload)))
    if (!/Android/i.test(navigator.userAgent)) addonList().then(l => (addons = l), () => {}) // desktop only (addons.rs SUPPORTED)
    else offs.push(onBackButtonPress(backPress).then(l => () => void l.unregister()))
    addEventListener('message', onBridge)
    offs.push(Promise.resolve(() => removeEventListener('message', onBridge)))
    addEventListener('pointerdown', resetIdle)
    return () => { offs.forEach(p => p.then(f => f())); removeEventListener('pointerdown', resetIdle) }
  })
</script>

<svelte:window onkeydown={keys} onpointerdown={outside} />

<WindowBar />
<canvas bind:this={canvas} class="stage" class:off={!hero} class:away aria-hidden="true"></canvas>

<!-- loading: the rows' own shape, quietly breathing, instead of a line of text (the words stay for screen readers) -->
{#snippet skel(label: string)}
  <li class="skel" role="status"><span class="vh">{label}</span>{#each [0, 1, 2] as i}<span class="sk" style="--i:{i}" aria-hidden="true"><i></i><b></b><b></b></span>{/each}</li>
{/snippet}
<!-- a network in the menu: pick it, see what it holds (once checked), ★ it onto the ring -->
{#snippet netRow(c: Chain)}
  {@const st = loadState[hk(acc, c)]}
  {@const w = netWorth(c)}
  <div class="mrow">
    <button class="mi" class:cur={c.id === chainId} onclick={() => setChain(c.id)}>
      {@render chainDot(c)}<span class="nn">{c.name}{#if c.test}<em>{t('net.testTag')}</em>{/if}</span>
      <span class="nv" class:faint={w === '0' || (!!st && st !== 'loading') || !fresh[hk(acc, c)]}>{st === 'loading' ? '…' : st ? t('net.noAnswer') : w}</span>
    </button>
    {#if !c.test}<button class="star" class:on={fav.includes(c.id)} onclick={() => toggleFav(c.id)} aria-pressed={fav.includes(c.id)} aria-label={t(fav.includes(c.id) ? 'net.unstar' : 'net.star', { net: c.name })} title={t(fav.includes(c.id) ? 'net.unstar' : 'net.star', { net: c.name })}><Icon name="star" size={16} /></button>{/if}
  </div>
{/snippet}
{#snippet chainDot(c: Chain)}<span class="cdot" class:test={c.test} class:real={!c.test}></span>{/snippet}
{#snippet tokIcon(c: Card)}<span class="ticon"><img src={iconURL(c)} alt="" /></span>{/snippet}
{#snippet swapIcon(id: string)}{#if tokens.find(c => idOf(c) === id)}{@render tokIcon(tokens.find(c => idOf(c) === id)!)}{:else}<span class="ticon"><img use:remoteIcon={picked[id]?.icon ?? null} alt="" /></span>{/if}{/snippet}

{#if view === 'locked'}
  <div class="col lockf">
    {#if !coreAvailable()}
      <h1 class="display big"><span class="line"><span style="--d:80ms">{t('lock.title')}</span></span></h1>
      <p class="note appear" style="--d:280ms">{t('lock.noCore')}</p>
    {/if}
  </div>
  {#if coreAvailable() && hasVault && !vaultOpen}
    <form class="gate" class:busy={gateBusy} class:shut={gateShut} class:bad={!!gateErr} bind:this={gateEl} onsubmit={unlockVault}>
      {#if bio.enabled}<button type="button" class="gi fp" onclick={bioUnlock} disabled={gateBusy} aria-label={t('bio.unlock')} title={t('bio.unlock')}><Icon name="finger" /></button>
      {:else}<span class="gi"><Icon name="lock" /></span>{/if}
      <input type="password" bind:value={pass} oninput={() => { gateErr = ''; hero?.prime(pass.length / 10) }} readonly={gateBusy} disabled={!gateSafe}
        placeholder={t('lock.password')} aria-label={t('lock.password')} autocomplete="current-password" use:focusSoon />
      <button class="go" aria-label={t('lock.open')} disabled={!pass || gateBusy}><Icon name="chevron" /></button>
    </form>
    <p class="gate-err" class:bad={!!gateErr} role="alert">{gateErr || t('lock.note')}</p>
  {/if}
{:else}
  <header class="top" class:scrolled class:bare={view === 'send' || view === 'swap'}>
    <div class="pickers">
      <div class="wrap w-net">
        <button class="chip" class:real={!chain.test} onclick={() => { menu = menu === 'net' ? '' : 'net'; addingNet = false }} aria-expanded={menu === 'net'} aria-label={t('net.title')}>
          {@render chainDot(chain)}{chain.name}{#if chain.test}<em>{t('net.testTag')}</em>{/if}
          <Icon name="chevron" size={14} />
        </button>
        {#if menu === 'net'}
          <div class="menu panel net-menu" role="dialog" aria-label={t('net.title')} in:pop out:pop={{ out: true }} use:dragDown={() => (menu = '')}>
            <span class="grab" aria-hidden="true"></span>
            {#if !addingNet}
              <div class="nhead">
                <input class="fi" bind:value={netQ} use:focusDesktop placeholder={t('net.search')} aria-label={t('net.search')} spellcheck="false" autocomplete="off"
                  onkeydown={e => e.key === 'Enter' && netFound[0] && setChain(netFound[0].id)} />
                {#if !netQ.trim()}<button class="tag" onclick={checkAll} disabled={checking}>{checking ? t('net.checking') : t('net.checkAll')}</button>{/if}
              </div>
              {#if netQ.trim()}
                {#each netFound as c (c.id)}{@render netRow(c)}{:else}<p class="mh">{t('net.none')}</p>{/each}
              {:else}
                <p class="mh">{t('net.fav')}</p>
                {#each favNets as c (c.id)}{@render netRow(c)}{/each}
                {#if moneyNets.length}<p class="mh">{t('net.money')}</p>{#each moneyNets as c (c.id)}{@render netRow(c)}{/each}{/if}
                {#if restNets.length}
                  <button class="mi more" class:open={moreNets} onclick={() => (moreNets = !moreNets)} aria-expanded={moreNets}><span>{t('net.rest', { n: restNets.length })}</span><Icon name="chevron" size={14} /></button>
                  {#if moreNets}<div class="more-list" transition:slide={{ duration: still() ? 0 : 220 }}>{#each restNets as c (c.id)}{@render netRow(c)}{/each}</div>{/if}
                {/if}
                {#if showTest}
                  <p class="mh">{t('net.test')}</p>
                  {#each chainList.filter(c => c.test) as c (c.id)}{@render netRow(c)}{/each}
                {/if}
              {/if}
              <label class="toggle"><span>{t('net.showTest')}</span><input type="checkbox" class="sw" bind:checked={showTest} /></label>
              <button class="mi add" onclick={() => (addingNet = true)}><Icon name="plus" size={16} />{t('net.add')}</button>
            {:else}
              <form class="netform" onsubmit={addNet} novalidate>
                <p class="mh">{t('net.add')}</p>
                <input class="fi" bind:value={nName} placeholder={t('net.name')} />
                <input class="fi mono" bind:value={nRpc} placeholder={t('net.rpc')} spellcheck="false" />
                <div class="two"><input class="fi mono" bind:value={nChain} placeholder={t('net.chainId')} inputmode="numeric" /><input class="fi mono" bind:value={nSym} placeholder={t('net.symbol')} /></div>
                <input class="fi mono" bind:value={nExp} placeholder={t('net.explorer')} spellcheck="false" />
                {#if nTouched && nErr}<span class="err">{nErr}</span>{:else}<span class="faint small">{t('net.customNote')}</span>{/if}
                <div class="two"><button type="button" class="btn small" onclick={() => (addingNet = false)}>{t('net.cancel')}</button><button class="btn small solid">{t('net.save')}</button></div>
              </form>
            {/if}
          </div>
        {/if}
      </div>
      <div class="wrap w-acc">
        <button class="chip" onclick={() => (menu = menu === 'acc' ? '' : 'acc')} aria-expanded={menu === 'acc'} aria-label={t('acc.title')}>
          <span>{acc?.name ?? '—'}</span><span class="mono faint">{short(myAddress)}</span><Icon name="chevron" size={14} />
        </button>
        {#if menu === 'acc'}
          <div class="menu panel" role="menu" in:pop out:pop={{ out: true }} use:dragDown={() => (menu = '')}>
            <span class="grab" aria-hidden="true"></span>
            <div class="mhead">
              <p class="mh">{t('acc.list')}</p>
              <button class="mplus" class:on={plus} onclick={() => (plus = !plus)} aria-expanded={plus} aria-label={t('acc.add')} title={t('acc.add')}><Icon name="plus" size={16} /></button>
            </div>
            {#if plus}
              <div class="fold" transition:slide={{ duration: still() ? 0 : 240, easing: cubicOut }}>
                <button class="mi add" role="menuitem" onclick={e => openSheet('add-new', e)}><Icon name="plus" size={16} />{t('acc.newWallet')}</button>
                {#if acc?.wallet && acc.evm}<button class="mi add" role="menuitem" onclick={e => openSheet('add-account', e, acc?.wallet)}><Icon name="plus" size={16} />{t('acc.new')}</button>{/if}
                <button class="mi add" role="menuitem" onclick={e => openSheet('add-import', e)}><Icon name="import" size={16} />{t('acc.import')}</button>
                {#if !/Android/i.test(navigator.userAgent)}<button class="mi add" role="menuitem" onclick={e => openSheet('add-ledger', e)}><Icon name="key" size={16} />{t('acc.ledger')}</button>{/if}
                <form class="netform" onsubmit={addWatch} novalidate>
                  <input class="fi mono" bind:value={watchAddr} placeholder={t('acc.watchPh')} spellcheck="false" autocomplete="off" aria-label={t('acc.watch')} />
                  {#if watchTouched && !isAddress(watchAddr) && !isEvmAddress(watchAddr)}<span class="err">{t('acc.watchErr')}</span>{/if}
                  {#if watchAddr.trim()}<button class="btn small solid">{t('acc.watchAdd')}</button>{/if}
                </form>
              </div>
            {/if}
            {#each accountList as a (a.id)}
              <div class="mrow">
                <button class="mi" role="menuitem" class:cur={a.id === acc?.id} onclick={() => setAccount(a.id)}>
                  <span>{a.name}{#if a.watch}<em>{t('watch.tag')}</em>{/if}</span><span class="mono faint">{short(addressFor(a, chain))}</span>
                </button>
                {#if a.watch}<button class="tag sq" onclick={() => removeWatch(a.id)} aria-label={t('acc.removeWatch')} title={t('acc.removeWatch')}><Icon name="close" size={14} /></button>
                {:else if a.wallet && !a.ledger}<button class="tag sq" onclick={e => openSheet('export', e, undefined, undefined, a.id)} aria-label={t('acc.export')} title={t('acc.export')}><Icon name="key" size={15} /></button>{/if}
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>

    <nav class="seg" use:pad={navActive} aria-label={t('nav.sections')}>
      <span class="seg-pad"></span>
      {#each NAV as n, i}<button data-seg class:cur={navActive === i} onclick={() => go(n.id)}>{n.label()}</button>{/each}
      {#each addons as a, i (a.id)}<button data-seg class:cur={navActive === NAV.length + i} onclick={() => openAddon(a.id)}>{a.name}</button>{/each}
    </nav>

    <div class="right">
      <button class="btn icon gear" class:cur={view === 'settings'} aria-label={t('nav.settings')} onclick={() => go('settings')}><Icon name="gear" /></button>
      {#if !vaultOpen}<button class="btn icon" aria-label={t('lock.action')} onclick={lock}><Icon name="lock" /></button>{/if}
    </div>
  </header>

  {#if (menu && phone) || txOpen}<div class="mveil" transition:fade={{ duration: still() ? 0 : 200 }} onclick={() => (txOpen = '')} role="presentation"></div>{/if}
  <main class="col" class:quiet class:settled class:frame={view === 'addon'} class:full={view === 'send' || view === 'swap'} bind:this={colEl} onscroll={band} use:pullRefresh={pullFor(view)}>
    <div class="pullmark" aria-hidden="true"><Icon name="flip" size={18} /></div>
    {#if view === 'home'}
      <div class="hole" aria-hidden="true"></div>
      {#key chainId + acc?.id}
        <p class="muted cap"><span class="line"><span>{t('home.total', { acc: acc?.name ?? '', net: chain.name })}</span></span></p>
        <p class="bal display num"><span class="line"><span style="--d:60ms">{cur ? fmt(shown.current) : '—'}<small class="unit">{chain.symbol}</small></span></span></p>
      {/key}
      {#if !chain.test && showUsd}<p class="usd faint appear" style="--d:100ms">{#if usdTotal}≈ {usd(usdTotal.sum)}{#if usdTotal.missing}{' · '}{t('usd.missing', { n: usdTotal.missing })}{/if}{/if}</p>{/if}
      <div class="acts money appear" style="--d:140ms">
        <button class="btn solid" onclick={() => send()} disabled={!tokens.length || !!acc?.watch}><Icon name="send" />{t('act.send')}</button>
        <button class="btn" onclick={receive} disabled={!myAddress}><Icon name="receive" />{t('act.receive')}</button>
        {#if canSwap && !acc?.watch}<button class="btn" onclick={() => openSwap()} disabled={!tokens.length}><Icon name="swap" />{t('act.swap')}</button>{/if}
      </div>
      {#if agentReq && !sheet}
        <button class="agentbar appear" style="--d:160ms" onclick={() => agentReq && showBatch(agentReq)}>{t('agent.waiting', { n: agentReq.count })}<span>{t('agent.open')}</span></button>
      {/if}
      {#if unresolved.length || pendingErr}
        <div class="unres appear" style="--d:180ms" role="status">
          <p class="warn">{pendingErr || t('send.unresolved')}</p>
          {#each unresolved as u (u.sig)}
            <button class="tag mono" onclick={() => copy(u.sig)} title={u.sig}>{u.chain ? (chainList.find(c => c.chainId === u.chain)?.name ?? u.cluster) : ({ mainnet: 'Solana', devnet: 'Solana Devnet', local: 'Solana Local' } as Record<string, string>)[u.cluster] ?? u.cluster} · {short(u.sig)} · {copied === u.sig ? t('act.copied') : t('act.copy')}</button>
          {/each}
        </div>
      {/if}
      <h3 class="sh appear tokh" style="--d:200ms">{t('home.tokens')}<button class="rf" onclick={refresh} aria-label={t('home.refresh')} title={t('home.refresh')} disabled={curState === 'loading'} class:spin={curState === 'loading'}><Icon name="flip" size={15} /></button></h3>
      <ul class="rows">
        {#each listed as c, i (c.id)}
          {@const v = valueOf(c)}
          <li class="appear" class:dim={hidden.includes(c.id)} style="--d:{220 + i * 40}ms">
            <button class="trow" onclick={() => open(c.id)}>
              {@render tokIcon(c)}
              <span class="tn" class:fake={c.name.startsWith(t('tok.fake'))}><b class="vt" style:view-transition-name={vtId === c.id ? 'sym' : null}>{c.sym}</b><small class="faint">{c.name}</small></span>
              <span class="tv"><span class="num">{fmt(c.amount)}</span><small class="faint num">{v === undefined ? c.sym : usd(v)}</small></span>
            </button>
          </li>
        {:else}
          {#if !myAddress}<li class="note">{t('home.noAddress', { net: chain.name })}</li>
          {:else if curState === 'loading'}{@render skel(t('watch.loading'))}
          {:else if curState}<li class="note">{t('watch.fail', { e: netErr(curState) })} <button class="tag" onclick={refresh}>{t('watch.retry')}</button></li>
          {:else}<li class="faint">{t('home.noTokens')}</li>{/if}
        {/each}
      </ul>
      {#if hiddenCount}<button class="tag hid" onclick={() => (showHidden = !showHidden)}>{showHidden ? t('tok.hideHidden') : t('tok.showHidden', { n: hiddenCount })}</button>{/if}
      {#if cur?.partial}<p class="note">{t('watch.partial')}</p>{/if}

    {:else if view === 'token' && card}
      <div class="hole s" aria-hidden="true"></div>
      <button class="back" onclick={back}><Icon name="back" />{t('act.allTokens')}</button>
      {#key card.id}
        <h2 class="display t s"><span class="line"><span class="vt" style:view-transition-name={vtId === card.id ? 'sym' : null}>{card.sym}</span></span></h2>
        <p class="muted appear" style="--d:80ms">{card.name}</p>
        <p class="amt display num appear" style="--d:140ms">{fmt(card.amount)}<small class="unit">{card.sym}</small></p>
        {#if valueOf(card) !== undefined}<p class="faint appear" style="--d:170ms">≈ {usd(valueOf(card)!)} · 1 {card.sym} = {usd(prices[priceId(card)])}</p>{/if}
        <div class="acts money appear" style="--d:240ms">
          {#if !acc?.watch}<button class="btn solid" onclick={() => send()}><Icon name="send" />{t('act.send')}</button>{/if}
          <button class="btn" onclick={receive}><Icon name="receive" />{t('act.receive')}</button>
          {#if canSwap && !acc?.watch}<button class="btn" onclick={() => openSwap(idOf(card))}><Icon name="swap" />{t('act.swap')}</button>{/if}
        </div>
        {#if card.id !== 'native'}<button class="tag appear" style="--d:300ms" onclick={() => toggleHidden(card.id)}>{hidden.includes(card.id) ? t('tok.unhide') : t('tok.hide')}</button>{/if}
      {/key}

    {:else if view === 'receive'}
      <div class="hole l" aria-hidden="true"></div>
      <button class="back" onclick={back}><Icon name="back" />{t('act.back')}</button>
      <h2 class="display t s"><span class="line"><span>{t('recv.title', { sym: chain.symbol })}</span></span></h2>
      <!-- the card's back carries the QR, the column the address (owner 09.10: nothing twice); without 3D both stay here -->
      <p class="lead appear" class:warn={!chain.test} style="--d:60ms">{t('recv.only', { net: chain.name })}{#if chain.kind === 'evm'} {t('recv.evm')}{/if}</p>
      {#if hero}<p class="muted appear" style="--d:80ms">{t('recv.leadCard')}</p>
        <p class="addr mono appear" style="--d:120ms">{myAddress.match(/.{1,4}/g)?.join(' ')}</p>
      {:else}
        <p class="muted appear" style="--d:80ms">{t('recv.lead')}</p>
        <div class="qrrow appear" style="--d:140ms"><Qr value={myAddress} size={200} /></div>
        <p class="addr mono appear" style="--d:180ms">{myAddress}</p>
      {/if}
      <div class="acts appear" style="--d:220ms">
        <button class="btn solid" onclick={() => copy(myAddress)}><Icon name={copied === myAddress ? 'check' : 'copy'} />{copied === myAddress ? t('act.copied') : t('act.copy')}</button>
        <button class="btn" onclick={() => go('addresses')}>{t('recv.all')}</button>
      </div>
      {#if incoming.length}
        <h3 class="sh appear" style="--d:300ms">{t('recv.recent')}</h3>
        <ul class="tx appear" style="--d:320ms">
          {#each incoming as x (x.sig)}
            <li><span class="dir in"><Icon name="receive" size={16} /></span>
              <span><b class="mono inb">{x.d.moves.filter(m => m.incoming).map(moveText).join('  ')}</b><small class="faint">{#if x.d.peer}{t('hist.from', { a: peerName(x.d.peer) })}{' · '}{/if}{when(x.time)}</small></span><span></span></li>
          {/each}
        </ul>
      {/if}

    {:else if view === 'send' && card}
      <button class="back" onclick={back}><Icon name="back" />{card.sym}</button>
      <h2 class="display t s"><span class="line"><span>{t('send.title', { sym: card.sym })}</span></span></h2>
      <form class="form" onsubmit={reviewSend} novalidate>
        <label class="appear" style="--d:80ms">
          <span class="label">{t('send.to')}</span>
          <span class="field" class:bad={touched && toErr}><input bind:value={to} placeholder={t('send.toPh')} spellcheck="false" autocomplete="off" />
            {#if !to && android}<button type="button" class="tag sq" onclick={pasteTo} aria-label={t('send.paste')} title={t('send.paste')}><Icon name="paste" size={16} /></button>
              <button type="button" class="tag sq" onclick={scanTo} aria-label={t('send.scan')} title={t('send.scan')}><Icon name="scan" size={16} /></button>{/if}</span>
          {#if toOther.length}<span class="hop">{t('send.otherFamily', { fam: FAM[toFam as 'solana' | 'evm'] })}
              {#each toOther as c (c.id)}<button type="button" class="tag" onclick={() => hopTo(c.id)}>{c.name}{#if hasMoney(c)}<span class="faint">{netWorth(c)}</span>{/if}</button>{/each}</span>
          {:else if touched && toErr}<span class="err">{toErr}</span>{:else if contactHit}<span class="ok mono">{short(contactHit.address)}</span>{/if}
        </label>
        {#if recipients}<div class="chips appear" style="--d:120ms">
          <!-- one tap: the owner's other accounts first, then the address book -->
          {#each accountList.filter(a => a.id !== acc?.id && !a.watch && valid(addressFor(a, chain))).slice(0, 3) as a (a.id)}
            <button type="button" class="tag" class:cur={to === addressFor(a, chain)} onclick={() => (to = addressFor(a, chain))}>{a.name}</button>
          {/each}
          {#each contactList.filter(c => valid(c.address)).slice(0, 4) as c}
            <button type="button" class="tag" class:cur={contactHit === c} onclick={() => (to = c.name)}>{c.name}</button>
          {/each}
        </div>{/if}
        <label class="appear amtl" style="--d:160ms">
          <span class="label">{t('send.amount')}</span>
          <span class="field" class:bad={touched && amtErr}>
            <input bind:value={amount} inputmode={phone ? 'none' : 'decimal'} readonly={phone} placeholder="0" class="num" autocomplete="off" />
            <span class="faint">{card.sym}</span>
            {#if maxSend(card, chain.kind) === 'max' && card.amount > 0}<button type="button" class="tag" onclick={() => evmMax(myAddress, chain, valid(toAddr) ? toAddr : undefined).then(v => countInto(x => (amount = x), amount, v), e => flash(String(e)))}>{t('send.max')}</button>
            {:else if maxSend(card) && Number(maxSend(card)) > 0}<button type="button" class="tag" onclick={() => countInto(v => (amount = v), amount, maxSend(card)!)}>{t('send.max')}</button>{/if}
          </span>
          {#if touched && amtErr}<span class="err">{amtErr}</span>{:else}<span class="faint small">{#if amt > 0 && valueOf({ ...card, amount: amt }) !== undefined}≈ {usd(valueOf({ ...card, amount: amt })!)}{' · '}{/if}{t('send.available', { v: fmt(maxAmt, card.sym) })}</span>{/if}
        </label>
        {#if phone && card.raw && BigInt(card.raw) > 0n}<div class="chips parts">
          {#each [1, 2] as q}<button type="button" class="tag" onclick={() => countInto(v => (amount = v), amount, exact(BigInt(card!.raw!) * BigInt(q) / 4n, card!.decimals ?? 0))}>{q === 1 ? t('send.quarter') : t('send.half')}</button>{/each}
          {#if maxSend(card, chain.kind) === 'max' && card.amount > 0}<button type="button" class="tag" onclick={() => evmMax(myAddress, chain, valid(toAddr) ? toAddr : undefined).then(v => countInto(x => (amount = x), amount, v), e => flash(String(e)))}>{t('send.max')}</button>
          {:else if maxSend(card) && Number(maxSend(card)) > 0}<button type="button" class="tag" onclick={() => countInto(v => (amount = v), amount, maxSend(card)!)}>{t('send.max')}</button>{/if}
        </div>{/if}
        <dl class="sum appear" style="--d:200ms">
          <div><dt>{t('send.net')}</dt><dd class:emb={!chain.test}>{chain.name}{!chain.test ? ` · ${t('net.real')}` : ''}</dd></div>
        </dl>
        <button class="btn solid wide appear" style="--d:240ms" disabled={preparing || !!sheet}>{preparing ? t('send.preparing') : t('send.review')}</button>
        {#if phone}<Keypad bind:value={amount} max={Math.min(card.decimals ?? 9, 9)} />{/if}
      </form>

    {:else if view === 'swap'}
      <button class="back" onclick={back}><Icon name="back" />{t('act.back')}</button>
      <h2 class="display t s"><span class="line"><span>{t('swap.title')}</span></span></h2>
      {#if !canSwap}<p class="note">{t('swap.mainnetOnly')}</p>
      {:else}
        <form class="form swapf" onsubmit={reviewSwap} novalidate>
          <label class="swcard">
            <span class="swcap"><span>{t('swap.give')}</span><span>{t('send.available', { v: fmt(swInCard?.amount ?? 0, symOf(swIn)) })}</span></span>
            <span class="swrow">
              <button type="button" class="tokbtn" onclick={() => openPicker('in')} aria-haspopup="dialog">{@render swapIcon(swIn)}<b>{symOf(swIn)}</b><Icon name="chevron" size={14} /></button>
              <input bind:value={swAmt} inputmode={phone ? 'none' : 'decimal'} readonly={phone} placeholder="0" class="num" autocomplete="off" />
              {#if swMax && Number(swMax) > 0}<button type="button" class="tag" onclick={() => countInto(v => (swAmt = v), swAmt, swMax!)}>{t('send.max')}</button>{/if}
            </span>
          </label>
          <div class="seam"><button type="button" class="btn icon" aria-label={t('swap.flip')} onclick={flipSwap}><Icon name="swap" /></button></div>
          <div class="swcard">
            <span class="swcap"><span>{t('swap.get')}</span></span>
            <span class="swrow">
              <button type="button" class="tokbtn" onclick={() => openPicker('out')} aria-haspopup="dialog">{@render swapIcon(swOut)}<b>{symOf(swOut)}</b><Icon name="chevron" size={14} /></button>
              <span class="num getv" class:wait={!swQuote && !!swLast} class:faint={!swShown}>{#if swShown}≈ <Roll text={fmt(Number(swShown.out_amount))} />{:else}{swAmt.trim() && !swErr ? '…' : '0'}{/if}</span>
            </span>
          </div>
          {#if picked[swOut] && !picked[swOut].verified}<span class="err">{t('swap.unverified', { m: short(swOut) })}</span>{/if}
          <!-- the quote rolls into place; a newer question keeps the last numbers dimmed instead of a blank (M-33, M-34) -->
          <dl class="sum" class:wait={!swQuote && !!swLast}>
            {#if swShown}
              <div class="more" transition:slide={{ duration: still() ? 0 : 320, easing: cubicOut }}>
                <div><dt>{t('swap.min')}</dt><dd class="num"><Roll text={fmt(Number(swShown.min_out))} /> {symOf(swOut)}</dd></div>
              </div>
            {/if}
          </dl>
          {#if swErr}<span class="err">{netErr(swErr)}</span>{/if}
          <button class="btn solid wide" disabled={!swQuote || swBusy || !!sheet}>{swBusy ? t('send.preparing') : t('swap.review')}</button>
          {#if phone}<Keypad bind:value={swAmt} max={Math.min(swInCard?.decimals ?? 9, 9)} />{/if}
        </form>
        {#if picker}
          <div class="picker panel" role="dialog" aria-label={t('swap.pick')} transition:pop>
            <div class="phead"><input class="fi" bind:value={pickQ} use:focusDesktop placeholder={t(picker === 'out' ? 'swap.search' : 'swap.searchMine')} spellcheck="false" autocomplete="off" />
              <button type="button" class="tag sq" onclick={() => (picker = '')} aria-label={t('swap.close')}><Icon name="close" size={14} /></button></div>
            <div class="plist">
              {#if pickMine.length}<p class="mh">{t('swap.mine')}</p>{/if}
              {#each pickMine as c (c.id)}
                <button type="button" class="prow" class:cur={idOf(c) === (picker === 'in' ? swIn : swOut)} onclick={() => choose(idOf(c))}>
                  {@render tokIcon(c)}<span><b>{c.verified ? c.sym : short(c.id)}</b><small class="faint">{c.name}</small></span><span class="mono faint">{fmt(c.amount)}</span>
                </button>
              {/each}
              {#if picker === 'out'}
                <p class="mh">{pickQ.trim() ? t('swap.found') : chain.kind === 'solana' ? t('swap.top') : t('swap.all')}</p>
                {#each [...pickList].sort((a, b) => Number(b.verified) - Number(a.verified)).filter(x => !swIns.some(c => idOf(c) === x.id || (c.id === 'native' && (x.id === 'So11111111111111111111111111111111111111112' || x.id === EVM_NATIVE)))) as x (x.id)}
                  <button type="button" class="prow" class:cur={x.id === swOut} onclick={() => choose(x.id, x)}>
                    <span class="ticon"><img use:remoteIcon={x.icon} alt="" /></span>
                    <span><b>{x.verified ? x.symbol : short(x.id)}{#if !x.verified}<em class="unv">{t('swap.notVerified')}</em>{/if}</b><small class="faint">{x.name}</small></span>
                    <span class="mono faint">{x.price !== null ? usd(x.price) : ''}</span>
                  </button>
                {:else}
                  <p class="faint small">{pickBusy ? t('swap.searching') : pickErr ? netErr(pickErr) : t('swap.none')}</p>
                {/each}
              {/if}
            </div>
          </div>
        {/if}
      {/if}

    {:else if view === 'history'}
      <h2 class="display t s"><span class="line"><span>{t('hist.title')}</span></span></h2>
      {#if histState === 'loading'}<ul class="rows">{@render skel(t('hist.loading'))}</ul>
      {:else if histState === t('hist.evmSoon')}<p class="note">{histState}</p>
        {#if chain.explorer && myAddress}<button class="btn small" onclick={() => openAddress(myAddress, chain).catch(e => flash(String(e)))}>{t('hist.explorer')}</button>{/if}
      {:else if histState}<p class="note">{netErr(histState)}</p>
      {:else if histStale}<p class="faint small">{t('hist.stale')}</p>{/if}
      <ul class="tx hist">
        {#each hist as x, i (x.sig)}
          {@const raw = txs[`${myAddress}|${x.sig}`]}
          {@const d = raw && raw !== 'err' ? raw : null}
          {@const kind = d ? txKind(d) : null}
          {#if i === 0 || dayOf(hist[i - 1].time) !== dayOf(x.time)}<li class="day appear" style="--d:{i * 30}ms">{dayOf(x.time)}</li>{/if}
          <!-- a row is one target: what and with whom on the left, the amount in its own column; details open in a sheet -->
          <li class="appear" style="--d:{i * 30}ms">
            <button class="txrow" onclick={() => (txOpen = x.sig)}>
              <span class="dir"><Icon name={!x.ok ? 'close' : kind === 'in' ? 'receive' : kind === 'out' ? 'send' : kind === 'swap' ? 'swap' : 'check'} size={16} /></span>
              <span class="txm"><span>{d && kind ? t(`hist.${kind}`) : short(x.sig)}</span>
                <small class="faint">{#if d?.peer}{t(kind === 'in' ? 'hist.from' : 'hist.to', { a: peerName(d.peer) })}{' · '}{/if}{clock(x.time)}{#if !x.ok}{' · '}{t('hist.failed')}{:else if raw === 'err'}{' · '}{t('hist.detailFail')}{/if}</small></span>
              <span class="txv num">{#each d?.moves ?? [] as m}<span class:in={m.incoming}>{moveText(m)}</span>{/each}</span>
            </button>
          </li>
        {:else}
          {#if !histState}<li class="faint">{t('hist.empty')}</li>{/if}
        {/each}
      </ul>

    {:else if view === 'addresses'}
      <h2 class="display t s"><span class="line"><span>{t('addr.title')}</span></span></h2>
      <div class="seg small appear" use:pad={addrTab === 'mine' ? 0 : addrTab === 'contacts' ? 1 : 2} role="tablist">
        <span class="seg-pad"></span>
        <button data-seg role="tab" aria-selected={addrTab === 'mine'} class:cur={addrTab === 'mine'} onclick={() => (addrTab = 'mine')}>{t('addr.mine')}</button>
        <button data-seg role="tab" aria-selected={addrTab === 'contacts'} class:cur={addrTab === 'contacts'} onclick={() => (addrTab = 'contacts')}>{t('addr.contacts')}</button>
        <button data-seg role="tab" aria-selected={addrTab === 'perms'} class:cur={addrTab === 'perms'} onclick={() => { addrTab = 'perms'; loadPerms() }}>{t('perm.tab')}</button>
      </div>
      {#if addrTab === 'perms'}
        <!-- who may spend this account's tokens on this network, and one tap to take it back (t-019) -->
        <p class="note appear">{t('perm.lead', { net: chain.name })}{#if chain.kind === 'evm'} {t('perm.evmNote')}{/if}</p>
        {#if !myAddress}<p class="faint appear">{t('perm.noAddr')}</p>
        {:else if permState === 'loading' && !perms.length}<p class="faint">{t('perm.loading')}</p>
        {:else if permState && permState !== 'loading'}<p class="note">{netErr(permState)}</p>
        {:else if !perms.length}<p class="faint appear">{t('perm.none')}</p>
        {:else}
          <ul class="tx">
            {#each perms as p, i (p.key)}
              <li class="appear" style="--d:{i * 40}ms">
                {#if tokens.find(c => c.id === p.token)}{@render tokIcon(tokens.find(c => c.id === p.token)!)}{:else}<span class="dir"><span class="ini">{p.sym[0]}</span></span>{/if}
                <span>{p.sym} · {p.unlimited ? t('perm.unlimited') : p.amount}<small class="faint mono" title={p.to}>{p.toName}{p.toName !== short(p.to) ? ` · ${short(p.to)}` : ''}</small></span>
                <span class="row">{#if !acc?.watch}<button class="tag" onclick={e => revoke(p, e)} disabled={!!sheet || revoking}>{t('perm.revoke')}</button>{/if}</span>
              </li>
            {/each}
          </ul>
        {/if}
      {:else if addrTab === 'mine'}
        {#each [{ k: 'solana', title: t('addr.solana'), a: acc?.solana ?? '', nets: chainList.filter(c => c.kind === 'solana' && (showTest || !c.test)) }, { k: 'evm', title: t('addr.evm'), a: acc?.evm ?? '', nets: chainList.filter(c => c.kind === 'evm' && (showTest || !c.test)) }].filter(g => g.a) as g, gi (g.k)}
          <section class="mine appear" style="--d:{80 + gi * 80}ms">
            <h3 class="fam">{g.title}</h3>
            <!-- the address itself is the biggest copy target; groups of four for reading, the raw string goes to the clipboard -->
            <button class="addr mono acopy" onclick={() => copy(g.a)} title={t('addr.tapCopy')}>{g.a.match(/.{1,4}/g)?.join(' ')}</button>
            {#if g.nets.length > 1}<button class="netsl" class:open={netsOpen === g.k} onclick={() => (netsOpen = netsOpen === g.k ? '' : g.k)} aria-expanded={netsOpen === g.k}>{t('addr.nets', { n: g.nets.length })}<Icon name="chevron" size={14} /></button>
              {#if netsOpen === g.k}<p class="netsv" transition:slide={{ duration: still() ? 0 : 220 }}>{g.nets.map(c => c.name.replaceAll(' ', '\u00a0')).join(' · ')}</p>{/if}{/if}
            <div class="acts">
              <button class="btn small solid" onclick={() => copy(g.a)}><Icon name={copied === g.a ? 'check' : 'copy'} size={16} />{copied === g.a ? t('act.copied') : t('act.copy')}</button>
              <button class="btn small" onclick={() => (qrOpen = qrOpen === g.k ? '' : g.k)}>{qrOpen === g.k ? t('addr.hideQr') : t('addr.qr')}</button>
            </div>
            {#if qrOpen === g.k}<div class="qrrow appear"><Qr value={g.a} size={180} /></div>{/if}
          </section>
        {/each}
        {#if acc && !acc.evm}
          <!-- owner 09.10: a Solana-key account had no Ethereum address and nothing said why -->
          <section class="mine appear" style="--d:160ms">
            <h3 class="fam">{t('addr.evm')}</h3>
            <p class="note">{acc.watch ? t('addr.noEvmWatch') : t('addr.noEvm')}</p>
            {#if !acc.watch && !acc.ledger}<div class="acts">
              <button class="btn small solid" onclick={e => openSheet('add-evm', e, undefined, undefined, acc?.id)} disabled={!!sheet}><Icon name="plus" size={16} />{t('addr.addEvm')}</button>
              <button class="btn small" onclick={e => openSheet('add-import', e)} disabled={!!sheet}>{t('addr.byWords')}</button>
            </div>{/if}
          </section>
        {/if}
      {:else}
        <p class="note appear">{t('addr.contactsNote')}</p>
        <form class="addc appear" onsubmit={addContact} novalidate>
          <span class="field" class:bad={addTouched && !newName.trim()}><input bind:value={newName} placeholder={t('addr.name')} autocomplete="off" /></span>
          <span class="field" class:bad={addTouched && !!newName.trim() && !!addErr}><input bind:value={newAddr} placeholder={t('addr.address')} class="mono" spellcheck="false" autocomplete="off" /></span>
          <button class="btn small"><Icon name="plus" size={16} />{t('addr.add')}</button>
          {#if addTouched && addErr}<span class="err">{addErr}</span>{/if}
        </form>
        <ul class="tx">
          {#each contactList as c, i (c.address)}
            <li class="appear" style="--d:{i * 40}ms">
              <span class="dir"><span class="ini">{c.name[0]}</span></span>
              <span>{c.name}<small class="faint mono">{short(c.address)}</small></span>
              <span class="row">
                <button class="tag" onclick={() => { view = 'home'; send(c.name) }} disabled={!valid(c.address) || !!acc?.watch}>{t('addr.send')}</button>
                <button class="tag" onclick={() => removeContact(c.address)} aria-label={t('addr.remove')}><Icon name="close" size={14} /></button>
              </span>
            </li>
          {:else}
            <li class="faint">{t('addr.empty')}</li>
          {/each}
        </ul>
      {/if}

    {:else if view === 'addon' && addonNow}
      {#key addonNow.id}<iframe class="addon appear" bind:this={frameEl} src={addonSrc(addonNow.id)} title={addonNow.name} sandbox="allow-scripts" referrerpolicy="no-referrer"></iframe>{/key}
    {:else if view === 'settings'}
      <!-- owner 09.10: fewer words, one line per setting; the own-node block is gone (other wallets have none; the core keeps it) -->
      <h2 class="display t s"><span class="line"><span>{t('set.title')}</span></span></h2>
      <div class="sets">
        <div class="set"><span>{t('set.lang')}</span><span class="val">Русский</span></div>
        <!-- «Фон» hidden (owner 10.10: the backgrounds are being redone from scratch; one stays meanwhile) -->
        <label class="set"><span>{t('net.showTest')}</span><input type="checkbox" class="sw" bind:checked={showTest} /></label>
        <label class="set"><span>{t('set.usd')}</span><input type="checkbox" class="sw" checked={showUsd} onchange={e => setShowUsd(e.currentTarget.checked)} /></label>
        <div class="set"><span>{t('set.pass')}<small class:warn={vaultOpen}>{vaultOpen ? t('set.passOff') : t('set.passOn')}</small></span>
          <button class="btn small" onclick={e => openSheet('password', e)} disabled={!!sheet}>{vaultOpen ? t('set.passSet') : t('set.passChange')}</button>
        </div>
        {#if feeInfo?.missing && vaultAccounts.some(a => a.solana === feeInfo?.owner)}<!-- a one-time setup for the app owner (his vault holds the fee account); gone once done, users never see it --><div class="set"><span>{t('fee.title')}<small>{feeInfo.missing ? t('fee.missing') : t('fee.ready', { acc: feeName })}</small></span>
          {#if feeInfo.missing}<button class="btn small" onclick={setupFee} disabled={!!sheet || preparing || !acc || !!acc.watch}>{preparing ? t('send.preparing') : t('fee.setup')}</button>{/if}
        </div>{/if}
        {#if !/Android/i.test(navigator.userAgent)}<label class="set"><span>{t('agent.title')}<small>{t('agent.note')}</small></span><input type="checkbox" class="sw" checked={agentOn} onchange={e => setAgent(e.currentTarget.checked)} /></label>{/if}
        {#if !/Android/i.test(navigator.userAgent)}<div class="set addons"><span>{t('addon.title')}<small>{t('addon.note')}</small></span>
          <label class="btn small">{t('addon.install')}<input type="file" accept=".html,text/html" hidden onchange={pickAddon} /></label>
        </div>
        {#each addons as a (a.id)}
          <div class="set sub"><span>{a.name} <em class="faint">{a.version}</em><small>{a.permissions.length ? a.permissions.map(p => t(`addon.perm.${p}` as Parameters<typeof t>[0])).join('; ') : t('addon.asksNothing')}</small></span>
            <button class="btn small" onclick={() => removeAddon(a.id)}>{t('addon.remove')}</button></div>
        {/each}
        {#if addonErr}<p class="err small">{addonErr}</p>{/if}
        {#if addonFile}
          {@const m = addonFile.m}
          <div class="addon-ask panel" transition:slide={{ duration: still() ? 0 : 220 }}>
            <p><b>{m.name}</b> <span class="faint">{m.version}</span>{#if m.description}<br /><span class="muted">{m.description}</span>{/if}</p>
            <p class="muted small">{m.permissions.length ? t('addon.asks') : t('addon.asksNothing')}</p>
            {#if m.permissions.length}<ul class="small">{#each m.permissions as p}<li class:warn={p === 'keys'}>{t(`addon.perm.${p}` as Parameters<typeof t>[0])}</li>{/each}</ul>{/if}
            {#if m.hosts.length}<p class="small muted">{t('addon.hosts', { h: m.hosts.map(h => h.replace('https://', '')).join(', ') })}</p>{/if}
            <p class="faint small">{t('addon.trust')}</p>
            <div class="two"><button class="btn small" onclick={() => (addonFile = null)}>{t('addon.cancel')}</button>
              <button class="btn small solid" onclick={installAddon}>{addons.some(a => a.id === m.id) ? t('addon.update') : t('addon.installIt')}</button></div>
          </div>
        {/if}
        {/if}
        {#if android && bio.available && !vaultOpen}<label class="set"><span>{t('bio.title')}<small>{t('bio.note')}</small></span><input type="checkbox" class="sw" checked={bio.enabled} onchange={toggleBio} /></label>{/if}
        {#if !vaultOpen}<div class="set"><span>{t('set.lock')}</span>
          <div class="seg small" use:pad={[5, 15, 30].indexOf(autolock)}><span class="seg-pad"></span>{#each [5, 15, 30] as m}<button data-seg class:cur={autolock === m} onclick={() => setAutolock(m)}>{t('set.min', { n: m })}</button>{/each}</div>
        </div>{/if}
      </div>
    {/if}
  </main>
  {#if txOpen}
    {@const x = hist.find(h => h.sig === txOpen)}
    {@const raw = txs[`${myAddress}|${txOpen}`]}
    {@const d = raw && raw !== 'err' ? raw : null}
    {@const kind = d ? txKind(d) : null}
    <div class="menu panel txd" role="dialog" aria-label={t('hist.title')} in:pop out:pop={{ out: true }} use:dragDown={() => (txOpen = '')}>
      <span class="grab" aria-hidden="true"></span>
      <p class="mh">{d && kind ? t(`hist.${kind}`) : t('hist.title')}{x && !x.ok ? ` · ${t('hist.failed')}` : ''}</p>
      {#if d?.moves.length}<p class="txamt num">{d.moves.map(moveText).join('  ')}</p>{/if}
      <dl class="sum">
        {#if d?.peer}<div><dt>{kind === 'in' ? t('hist.fromL') : t('hist.toL')}</dt><dd class="mono addr2">{peerName(d.peer) !== short(d.peer) ? `${peerName(d.peer)} · ` : ''}{d.peer}</dd></div>{/if}
        {#if x}<div><dt>{t('hist.when')}</dt><dd>{when(x.time)}</dd></div>{/if}
        {#if d?.fee}<div><dt>{t('hist.feeL')}</dt><dd class="num">{chain.kind === 'evm' ? fmt(Number(d.fee) / 1e18, chain.symbol) : solText(d.fee)}</dd></div>{/if}
        <div><dt>{t('send.net')}</dt><dd>{chain.name}</dd></div>
      </dl>
      <div class="two">
        <button class="btn small" onclick={() => copy(txOpen)}>{copied === txOpen ? t('act.copied') : t('hist.copyId')}</button>
        {#if cluster(chain) !== 'local'}<button class="btn small solid" onclick={() => openTx(txOpen, chain).catch(e => flash(String(e)))}>{t('hist.open')}</button>{/if}
      </div>
    </div>
  {/if}
  {#if view !== 'send' && view !== 'swap'}
    <nav class="tabbar" aria-label={t('nav.sections')}>
      {#each TABS as n (n.id)}
        <button class:cur={tabOf(view) === n.id} aria-current={tabOf(view) === n.id ? 'page' : undefined} onclick={() => tab(n.id)}><span class="ti"><Icon name={n.icon} size={22} /></span>{n.label()}</button>
      {/each}
    </nav>
  {/if}
{/if}

{#if introOn}<Intro ready={readyP} kind={kindP} scene={() => hero} ontraced={() => tracedRes()} onopen={introOpen} ondone={() => (introOn = false)} />{/if}

{#if sheet}
  <Sheet bind:this={sheetEl} mode={sheet.mode} wallet={sheet.wallet} account={sheet.account} origin={sheet.origin} round={sheet.round} draft={sheet.draft} batch={sheet.batch} offer={sheet.offer} fresh={!hasVault} open={vaultOpen} onclose={sheetClosed} />
{/if}

{#if scanning}
  <!-- the camera shows through the page (the scanner puts its preview behind the WebView); only this layer stays -->
  <div class="scanner" role="dialog" aria-label={t('send.scan')}>
    <span class="vf" aria-hidden="true"></span>
    <p>{t('send.scanHint')}</p>
    <button class="btn" onclick={cancelScan}>{t('net.cancel')}</button>
  </div>
{/if}

{#if toast}
  {#key toast.text}
    <div class="toast panel" class:ok={toast.ok} role="status">
      <Icon name={toast.ok ? 'check' : 'send'} />
      <span><b>{toast.text}</b>{#if toast.sub}<small class="muted">{toast.sub}</small>{/if}</span>
    </div>
  {/key}
{/if}

<style>
  .stage { position: fixed; inset: 0; width: 100%; height: 100%; display: block; touch-action: pan-y; }
  .stage.off { display: none; }
  .stage.away { opacity: 0; } /* phone: the scene sleeps out of sight; its last frame must not stay behind the content */
  .stage { transition: opacity 220ms ease; }
  /* cards stand all around us; the wallet column floats over them on a soft dark veil */

  /* content column: the wallet itself, left edge leads */
  .col { position: fixed; left: var(--pad); top: 92px; bottom: 0; width: min(500px, 40vw); z-index: 5; overflow-y: auto; overflow-x: hidden; padding: 12px 8px 64px 0; scrollbar-gutter: stable; }
  .lockf { top: auto; bottom: clamp(56px, 12vh, 120px); overflow: visible; padding: 0; }
  .lockf p { margin: 0; }
  .big { font-size: clamp(44px, 5vw, 76px); margin: 14px 0 32px; }
  .big > .line > span { text-wrap: balance; }
  .row { display: flex; gap: 12px; }
  .small { font-size: 13px; }

  /* the password field: a dot under the deck that stretches into a capsule, and folds back into it on unlock */
  .gate {
    --w: 380px; --fold: cubic-bezier(.6, 0, .8, .4);
    contain: layout paint; /* width unfolds 58 -> 380 once; contained, so it never reflows the page */
    position: fixed; left: 50%; bottom: clamp(72px, 15vh, 140px); z-index: 6; translate: -50% 0; /* under the deck, centred like the whole scene before entry */
    width: var(--w); height: 58px; display: flex; align-items: center; gap: 6px; padding: 0 6px 0 19px; overflow: hidden;
    border-radius: var(--r-pill); background: #18181d; border: 1px solid var(--line);
    box-shadow: var(--metal-edge), var(--lift);
    animation: unfold 760ms var(--ease) 120ms backwards; /* starts when the intro opens (deck already in place) */
    transition: border-color 300ms ease, width 320ms var(--fold), scale 220ms var(--fold) 260ms, opacity 180ms ease 300ms;
  }
  @keyframes unfold { from { width: 58px; scale: .5; opacity: 0; } 34% { width: 58px; scale: 1; opacity: 1; } }
  .gate:focus-within { border-color: var(--line-hi); }
  .gate.bad { border-color: rgb(255 90 46 / .7); transition: border-color 160ms ease; }
  .gate.shut { width: 58px; scale: .3; opacity: 0; }
  .gi { display: grid; color: var(--ink-2); flex: none; }
  .gi.fp { place-items: center; width: 40px; height: 40px; margin-left: -10px; border: 0; border-radius: 50%; background: none; color: var(--ink); cursor: pointer; transition: background-color 160ms ease; }
  .gi.fp:active { background: rgb(238 235 229 / .12); }
  .gate input {
    flex: 1; min-width: 0; height: 100%; background: none; border: 0; outline: 0; color: var(--ink);
    font: 400 16px var(--font); letter-spacing: .14em; animation: appear 400ms var(--ease) 1000ms backwards;
  }
  .gate input::placeholder { color: var(--ink-3); letter-spacing: 0; }
  .gate.shut input, .gate.shut .go { opacity: 0; transition: opacity 120ms ease; }
  .go {
    flex: none; width: 46px; height: 46px; display: grid; place-items: center; border: 0; border-radius: 50%; cursor: pointer;
    background: var(--ink); color: var(--stage); transition: scale 320ms var(--ease), opacity 200ms ease;
    animation: appear 400ms var(--ease) 1080ms backwards;
  }
  .go :global(svg) { rotate: -90deg; }
  .go:active { scale: .94; transition-duration: 120ms; }
  .go:disabled { opacity: .28; cursor: default; }
  /* checking the password: a thin ember light runs along the bottom edge */
  .gate::after {
    content: ''; position: absolute; left: 22px; right: 22px; bottom: 0; height: 1px; opacity: 0; pointer-events: none;
    background: linear-gradient(90deg, transparent, var(--ember), transparent) no-repeat; background-size: 40% 100%;
  }
  .gate.busy::after { opacity: 1; animation: scan 900ms linear infinite; }
  @keyframes scan { from { background-position: -60% 0; } to { background-position: 160% 0; } }
  .gate-err::first-letter { text-transform: uppercase; }
  .gate-err {
    position: fixed; left: 50%; translate: -50% 0; bottom: calc(clamp(72px, 15vh, 140px) - 46px); z-index: 6; margin: 0; width: min(380px, calc(100vw - 32px));
    font-size: 13px; line-height: 1.45; text-align: center; color: var(--ink-3); text-wrap: balance; animation: appear 400ms var(--ease) 1160ms backwards;
  }
  .gate-err.bad { color: var(--ember); }
  @media (prefers-reduced-motion: reduce) {
    .gate, .gate input, .go { animation: fade 200ms ease backwards; transition: none; }
    .gate.busy::after { animation: none; background-size: 100% 100%; }
  }

  .top { position: fixed; top: 0; left: 0; right: 0; z-index: 10; display: grid; grid-template-columns: 1fr auto 1fr; align-items: center; gap: 16px; padding: 18px var(--pad); pointer-events: none; }
  .top > * { pointer-events: auto; }
  .pickers { display: flex; gap: 8px; justify-self: start; }
  .wrap { position: relative; }
  .right { justify-self: end; display: flex; gap: 8px; margin-right: var(--wc, 0px); }
  .chip { display: inline-flex; align-items: center; gap: 9px; min-height: 40px; padding: 0 14px; border: 0; border-radius: var(--r-pill); background: var(--metal); box-shadow: var(--metal-edge); cursor: pointer; font-size: 14px; font-weight: 600; white-space: nowrap; transition: background-color 120ms ease; }
  .chip:hover { background: var(--metal-hi); }
  .chip.real { box-shadow: var(--metal-edge), inset 0 0 0 1px rgb(255 90 46 / .35); }
  .chip .mono { font-size: 12px; }
  @media (max-width: 1100px) { .chip .mono { display: none; } } /* the header keeps one line */
  .chip em, .mi em { font-style: normal; font-size: 12px; padding: 1px 7px; border-radius: 999px; box-shadow: inset 0 0 0 1px var(--line); color: var(--ink-2); }
  .cdot { width: 8px; height: 8px; border-radius: 50%; flex: none; }
  .cdot.real { background: var(--ember); }
  .mi .cdot.real { background: var(--ink-2); } /* ember marks the current network only */
  .mi .cdot { margin: 0 4px; } /* row text starts at 36px */
  .cdot.test { box-shadow: inset 0 0 0 1.5px var(--ink-3); }

  .menu { position: absolute; top: calc(100% + 8px); left: 0; min-width: 300px; padding: 8px; display: grid; gap: 2px; transform-origin: top left; z-index: 20; }
  .mhead { display: flex; align-items: center; justify-content: space-between; padding-right: 2px; }
  .mhead .mh { margin-bottom: 8px; }
  .mplus { width: 32px; height: 32px; display: grid; place-items: center; border: 0; border-radius: 50%; background: none; color: var(--ink-2); cursor: pointer; box-shadow: inset 0 0 0 1px var(--line); transition: background-color 200ms ease, color 160ms ease, box-shadow 160ms ease; }
  .mplus:hover { color: var(--ink); box-shadow: inset 0 0 0 1px rgb(238 235 229 / .3); }
  .mplus.on { background: rgb(238 235 229 / .1); color: var(--ink); box-shadow: inset 0 0 0 1px rgb(238 235 229 / .3); } /* pressed, never an «×» next to the title */
  .mplus :global(svg), .tag.sq :global(svg) { margin: 0; }
  .fold { display: grid; gap: 2px; padding-bottom: 6px; margin-bottom: 4px; border-bottom: 1px solid var(--line); }
  .tag.sq { width: 32px; min-height: 32px; padding: 0; display: grid; place-items: center; flex: none; }
  .mh { margin: 8px 10px 4px; font-size: 12px; color: var(--ink-3); }
  .mi { display: flex; align-items: center; gap: 10px; min-height: 42px; padding: 0 10px; border: 0; border-radius: 12px; background: none; cursor: pointer; text-align: start; transition: background-color 100ms ease; }
  .mi:hover, .mi.cur { background: rgb(238 235 229 / .07); }
  .mi :global(svg) { margin-left: auto; }
  .mi .mono { font-size: 12px; margin-left: auto; }
  .mi.add { color: var(--ink-2); padding-left: 8px; }
  .net-menu { min-width: 340px; max-height: calc(100vh - 90px); overflow-y: auto; overscroll-behavior: contain; } /* twenty networks: the list scrolls inside, the window never does */
  .mi.more { color: var(--ink-2); padding-left: 36px; }
  .mi.more :global(svg) { transition: transform 220ms cubic-bezier(.32,.9,.18,1); }
  .mi.more.open :global(svg) { transform: rotate(180deg); }
  .more-list { display: grid; gap: 2px; }
  .nhead { display: flex; gap: 8px; align-items: center; padding: 2px 2px 6px; }
  .nhead .fi { flex: 1; }
  .nhead .tag { flex: none; min-height: 40px; }
  .nn { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .nv { margin-left: auto; padding-left: 12px; font-size: 13px; color: var(--ink-2); white-space: nowrap; font-variant-numeric: tabular-nums; }
  .star { flex: none; width: 36px; height: 36px; display: grid; place-items: center; border: 0; border-radius: 50%; background: none; color: var(--ink-3); cursor: pointer; transition: color 160ms ease, background-color 160ms ease; }
  .star:hover { color: var(--ink); background: rgb(238 235 229 / .06); }
  .star.on { color: var(--ink-3); } /* filled = on the ring; quiet: nine bright stars were the loudest thing in the menu */
  .star.on :global(svg) { fill: currentColor; }
  .mi.add :global(svg) { margin-left: 0; }
  .toggle { display: flex; align-items: center; justify-content: space-between; gap: 10px; min-height: 42px; padding: 0 10px 0 36px; color: var(--ink-2); white-space: nowrap; cursor: pointer; }
  .netform { display: grid; gap: 8px; padding: 4px; }
  .fi { min-height: 40px; padding: 0 12px; border: 0; border-radius: var(--r-sm); background: none; box-shadow: inset 0 0 0 1px var(--line); outline: none; font-size: 14px; min-width: 0; }
  .fi:focus { box-shadow: inset 0 0 0 1px var(--line-hi); }
  .fi.mono::placeholder { font-family: var(--font); }
  .two { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .two :global(.btn) { justify-content: center; }

  .seg { position: relative; display: inline-flex; padding: 3px; border-radius: var(--r-pill); background: var(--metal); box-shadow: var(--metal-edge); }
  :global(.seg-pad) { position: absolute; inset: 0; border-radius: var(--r-pill); background: var(--ink); transition: clip-path 450ms var(--ease); pointer-events: none; }
  .seg button { position: relative; z-index: 1; min-height: 34px; padding: 0 18px; border: 0; background: none; cursor: pointer; border-radius: var(--r-pill); color: var(--ink-2); font-size: 14px; transition: color 350ms ease; }
  .seg button.cur { color: var(--stage); }
  .seg button:disabled { cursor: not-allowed; opacity: .45; }
  .seg.small button { min-height: 34px; padding: 0 14px; font-size: 13px; }

  :where(.col) p { margin: 0; } /* zero weight: a class's own margin wins without !important */
  .mrow { display: flex; align-items: center; gap: 4px; }
  .mrow .mi { flex: 1; }
  .tx .row { display: flex; gap: 6px; }
  .cap { font-size: 14px; }
  .unit { font-size: .38em; letter-spacing: 0; margin-left: .25em; color: var(--ink-2); }
  .bal { font-size: clamp(48px, 5.6vw, 80px); margin: 8px 0 0; }
  .usd { margin: 8px 0 0; min-height: 22px; }
  .acts { display: flex; gap: 8px; flex-wrap: wrap; }
  .sh { font-weight: 400; font-size: 13px; color: var(--ink-3); margin: 32px 0 8px; }
  .tokh { display: flex; align-items: center; justify-content: space-between; }
  .rf { width: 30px; height: 30px; margin-right: -7px; display: grid; place-items: center; border: 0; border-radius: 50%; background: none; color: var(--ink-3); cursor: pointer; transition: color 300ms ease, background-color 300ms ease; }
  .rf:hover:not(:disabled) { color: var(--ink); background: rgb(238 235 229 / .06); }
  .rf.spin :global(svg) { animation: rf 900ms linear infinite; }
  @keyframes rf { to { rotate: 360deg; } }
  .tn.fake small { color: var(--ink-2); }

  .rows { list-style: none; margin: 0 -10px; padding: 0; }
  .rows > li.note, .rows > li.faint { padding: 0 10px; } /* the list bleeds 10px left for row hover; plain lines must not */
  .trow { width: 100%; display: grid; grid-template-columns: 40px 1fr auto; gap: 14px; align-items: center; min-height: 60px; padding: 8px 10px; border: 0; border-radius: var(--r-sm); background: none; cursor: pointer; text-align: start; transition: background-color 100ms ease; }
  .trow:hover { background: rgb(238 235 229 / .05); }
  .ticon { width: 40px; height: 40px; border-radius: 50%; display: grid; place-items: center; overflow: hidden; background: var(--metal); box-shadow: inset 0 1px 2px rgb(0 0 0 / .5), inset 0 -1px 0 rgb(255 255 255 / .05); }
  .ticon img { width: 22px; height: 22px; }
  .tn, .tv { display: grid; line-height: 20px; }
  .tn b { font-weight: 400; font-size: 15px; letter-spacing: -.025em; }
  /* the ticker flies row <-> title as one word growing: both boxes hug the text, so the two snapshots share a shape
     and the cross-fade has no seam (owner 09.10) */
  .vt { width: fit-content; justify-self: start; }
  .tv { text-align: end; }
  .rows small, .tx small { font-size: 13px; }

  .back { display: inline-flex; align-items: center; gap: 6px; min-height: 36px; padding: 0 12px 0 6px; margin-left: -6px; border: 0; border-radius: var(--r-pill); background: none; color: var(--ink-2); cursor: pointer; transition: color 300ms ease; align-self: flex-start; }
  .back:hover { color: var(--ink); }
  .t { font-size: clamp(52px, 5.4vw, 80px); margin: 16px 0 8px; }
  .t.s { font-size: clamp(34px, 3.4vw, 50px); margin-bottom: 16px; }
  .amt { font-size: clamp(48px, 5.6vw, 80px); margin: 24px 0 8px; } /* the token page leads with the money, like the home total */
  .col .acts { margin: 24px 0 0; }
  .note { font-size: 14px; color: var(--ink-2); line-height: 22px; max-width: 46ch; margin-top: 16px; }
  .unres { display: grid; gap: 8px; margin-top: 16px; justify-items: start; }
  .unres .warn { margin: 0; font-size: 14px; color: var(--ember); max-width: 44ch; }
  .addr { font-size: 15px; line-height: 24px; margin-top: 16px; overflow-wrap: anywhere; color: var(--ink); }
  .qrrow { margin-top: 16px; }

  .tx { list-style: none; margin: 0; padding: 0; }
  .tx li { display: grid; grid-template-columns: 36px 1fr auto; align-items: center; gap: 14px; padding: 10px 8px; margin: 0 -8px; border-radius: 12px; transition: background-color 280ms ease; }
  .tx li:hover { background: rgb(238 235 229 / .05); }
  .tx li > span:nth-child(2) { display: grid; line-height: 20px; }
  .dir { width: 36px; height: 36px; border-radius: 50%; display: grid; place-items: center; box-shadow: inset 0 0 0 1px var(--line); color: var(--ink-2); }
  .ini { font-size: 14px; }
  .tx .mono { font-size: 14px; }
  .rows .dim { opacity: .5; }
  .skel { display: grid; gap: 2px; padding: 0 10px; }
  .sk { display: grid; grid-template-columns: 40px 1fr 64px; gap: 14px; align-items: center; min-height: 60px; animation: skb 1.6s ease-in-out calc(var(--i) * 160ms) infinite; }
  .sk i { width: 40px; height: 40px; border-radius: 50%; background: var(--metal); }
  .sk b { height: 12px; border-radius: 6px; background: var(--metal); }
  .sk b:last-child { justify-self: end; width: 48px; }
  @keyframes skb { 50% { opacity: .45; } }
  .vh { position: absolute; width: 1px; height: 1px; overflow: hidden; clip-path: inset(50%); white-space: nowrap; }
  @media (prefers-reduced-motion: reduce) { .sk { animation: none; } }
  .hid { margin-top: 8px; }
  .tokbtn { display: inline-flex; align-items: center; gap: 8px; flex: none; min-height: 40px; padding: 0 10px 0 4px; border: 0; border-radius: 999px; background: rgb(238 235 229 / .06); color: var(--ink); cursor: pointer; transition: background-color 160ms ease; }
  .tokbtn b { font-weight: 600; }
  .tokbtn:hover { background: rgb(238 235 229 / .11); }
  .tokbtn .ticon { width: 30px; height: 30px; }
  .tokbtn .ticon img { width: 18px; height: 18px; }
  /* swap: two cards, captions inside, the turn key on their seam; amounts large, right, even digits */
  .swapf { gap: 16px; }
  .swcard { display: grid; gap: 4px; padding: 12px 16px 12px 12px; border-radius: var(--r-panel); background: rgb(238 235 229 / .03); box-shadow: inset 0 0 0 1px var(--line); transition: box-shadow 300ms ease; }
  .swcard:focus-within { box-shadow: inset 0 0 0 1px var(--line-hi); }
  .swcap { display: flex; justify-content: space-between; gap: 16px; padding-left: 4px; font-size: 13px; line-height: 20px; color: var(--ink-3); }
  .swrow { display: flex; align-items: center; gap: 12px; min-height: 48px; }
  .swrow input, .getv { flex: 1; min-width: 0; background: none; border: 0; outline: none; text-align: end; font: 400 28px/36px var(--font); letter-spacing: -.02em; font-variant-numeric: tabular-nums; }
  .getv { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; transition: opacity 200ms ease; }
  .seam { position: relative; z-index: 1; height: 0; margin: -12px 0; display: grid; justify-items: center; align-items: center; }
  .seam :global(.btn.icon) { background: var(--metal); box-shadow: var(--metal-edge), 0 0 0 4px var(--stage); }
  .getv.wait { opacity: .45; }
  .picker { position: absolute; left: 0; right: 8px; top: 120px; bottom: 24px; display: grid; grid-template-rows: auto 1fr; gap: 8px; padding: 12px; z-index: 30; }
  .phead { display: flex; gap: 8px; align-items: center; }
  .phead .fi { flex: 1; }
  .plist { overflow-y: auto; overscroll-behavior: contain; display: grid; align-content: start; gap: 2px; }
  .prow { display: grid; grid-template-columns: 40px 1fr auto; align-items: center; gap: 12px; padding: 6px 8px; border: 0; border-radius: var(--r-sm); background: none; color: var(--ink); text-align: start; cursor: pointer; transition: background-color 100ms ease; }
  .prow:hover, .prow.cur { background: rgb(238 235 229 / .07); }
  .prow b { display: block; font-weight: 600; }
  .prow .unv { margin-left: 8px; padding: 1px 7px; border-radius: 999px; font-style: normal; font-size: 12px; font-weight: 600; color: var(--warn, #f2c38a); box-shadow: inset 0 0 0 1px currentColor; vertical-align: 1px; }
  .prow small { display: block; font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .ticon img:not([src]) { opacity: 0; }
  .ticon img:global(.in) { width: 100%; height: 100%; object-fit: cover; animation: tin 240ms ease both; }
  @keyframes tin { from { opacity: 0; } }
  .hist li:not(.day) { display: block; padding: 0; }
  .hist li:hover { background: none; }
  .txrow { width: 100%; display: grid; grid-template-columns: 36px minmax(0, 1fr) auto; gap: 12px; align-items: center; min-height: 60px; padding: 8px; border: 0; border-radius: var(--r-sm); background: none; color: inherit; text-align: start; cursor: pointer; -webkit-tap-highlight-color: transparent; transition: background-color 100ms ease; }
  .txrow:hover { background: rgb(238 235 229 / .05); }
  .txrow:active { background: rgb(238 235 229 / .08); }
  .txm, .txv { display: grid; line-height: 20px; min-width: 0; }
  .txm small { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .txv { justify-items: end; white-space: nowrap; color: var(--ink-2); }
  .txv .in { color: var(--ink); }
  .tx .inb { font-weight: 400; color: var(--ink); white-space: pre; font-size: 14px; }
  .dir.in { color: var(--ink); box-shadow: inset 0 0 0 1px var(--ink-3, var(--line)); }
  .tx li.day { display: block; padding: 16px 0 8px; margin: 0; font-size: 13px; color: var(--ink-3); background: none; }
  .tx li.day:first-child { padding-top: 6px; }

  .form { display: grid; grid-template-columns: minmax(0, 1fr); gap: 16px; }
  .form label { display: grid; grid-template-columns: minmax(0, 1fr); gap: 8px; } /* a field never outgrows a phone (inputs have a min width of their own) */
  .err { font-size: 13px; color: var(--ember); }
  .ok { font-size: 12px; color: var(--ink-3); }
  .hop { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; font-size: 13px; color: var(--ink-2); }
  .hop .tag { display: inline-flex; gap: 8px; align-items: center; color: var(--ink); }
  .chips { display: flex; flex-wrap: wrap; gap: 8px; }
  .form > .chips { margin-top: -8px; }
  .tag { min-height: 30px; padding: 0 12px; border: 0; border-radius: var(--r-pill); background: none; box-shadow: inset 0 0 0 1px var(--line); color: var(--ink-2); font-size: 13px; cursor: pointer; transition: box-shadow 300ms ease, color 300ms ease; }
  .tag:hover, .tag.cur { box-shadow: inset 0 0 0 1px rgb(238 235 229 / .45); color: var(--ink); }
  .tag:disabled { opacity: .4; cursor: not-allowed; }
  .sum { margin: 0; display: grid; gap: 8px; }
  .sum div { display: flex; justify-content: space-between; gap: 16px; font-size: 14px; }
  .sum dt { color: var(--ink-3); }
  .sum dd { margin: 0; }
  .sum .emb { color: var(--ember); }
  .sum.wait dd { opacity: .45; transition: opacity 200ms ease; }
  .lead { font-size: 15px; margin-bottom: 8px; }
  .lead.warn { color: var(--ember); }
  .sum dd { transition: opacity 300ms ease; }
  .sum .more { display: grid; gap: 8px; justify-content: stretch; }
  .wide { justify-content: center; width: 100%; margin-top: 8px; }

  .mine { margin-top: 24px; }
  .mine + .mine { padding-top: 24px; border-top: 1px solid var(--line); }
  .fam { margin: 0; font-size: 16px; font-weight: 600; line-height: 24px; }
  .acopy { display: block; width: 100%; margin-top: 8px; padding: 12px 16px; border: 0; border-radius: var(--r-sm); background: rgb(238 235 229 / .04); box-shadow: inset 0 0 0 1px var(--line); text-align: start; cursor: copy; transition: background-color 160ms ease; }
  .acopy:hover { background: rgb(238 235 229 / .07); }
  .acopy:active { background: rgb(238 235 229 / .1); }
  .netsl { display: inline-flex; align-items: center; gap: 4px; min-height: 32px; margin-top: 4px; padding: 0; border: 0; background: none; color: var(--ink-3); font-size: 13px; cursor: pointer; transition: color 160ms ease; }
  .netsl:hover { color: var(--ink-2); }
  .netsl :global(svg) { transition: transform 220ms var(--ease); }
  .netsl.open :global(svg) { transform: rotate(180deg); }
  .netsv { font-size: 13px; line-height: 20px; color: var(--ink-2); }
  .mine .acts { margin: 16px 0 0; }
  .addc { display: grid; grid-template-columns: minmax(0, .8fr) minmax(0, 1.4fr) auto; gap: 8px; margin: 16px 0; }
  .addc .field, .addc input { min-width: 0; }
  .addc .field { min-height: 44px; }
  .addc .err { grid-column: 1 / -1; }
  .quiet :global(.appear), .quiet :global(.line > span) { animation: none; }
  .settled :global(.appear), .settled :global(.line > span) { animation: none; }
  .col { view-transition-name: col; }
  .sets { display: grid; margin-top: 8px; }
  .set { display: flex; align-items: center; justify-content: space-between; gap: 16px; min-height: 60px; padding: 10px 0; border-bottom: 1px solid var(--line); font-size: 15px; cursor: default; }
  label.set { cursor: pointer; }
  .set > span { display: grid; gap: 2px; flex: 1; min-width: 0; }
  .set > .val { flex: none; color: var(--ink-2); }
  .set :global(.btn) { flex: none; }
  .set small { font-size: 13px; color: var(--ink-3); max-width: 34ch; line-height: 18px; }
  .set small.warn { color: var(--ember); }
  .set.addons { border-bottom: 0; }
  .set.sub { min-height: 0; padding: 4px 0 12px 14px; font-size: 14px; }
  .set em { font-style: normal; font-size: 12px; }
  .addon-ask { display: grid; gap: 8px; padding: 16px; margin: 4px 0 12px; border-radius: 16px; }
  .addon-ask p, .addon-ask ul { margin: 0; }
  .addon-ask ul { padding-left: 18px; display: grid; gap: 4px; color: var(--ink-2); }
  .addon-ask li.warn { color: var(--ember); }
  /* an add-on's tab: its frame fills the column */
  .col.frame { overflow: hidden; padding-bottom: 12px; display: flex; flex-direction: column; }
  iframe.addon { flex: 1; width: 100%; min-height: 0; border: 0; background: transparent; color-scheme: dark; }
  /* a switch drawn from the checkbox itself: track + knob, the world's curve */
  .sw { appearance: none; flex: none; width: 40px; height: 24px; margin: 0; border-radius: 999px; background: var(--metal-hi); box-shadow: inset 0 0 0 1px var(--line); position: relative; cursor: pointer; transition: background-color 220ms ease; }
  .sw::after { content: ''; position: absolute; top: 3px; left: 3px; width: 18px; height: 18px; border-radius: 50%; background: var(--ink-2); transition: translate 260ms var(--ease), background-color 220ms ease; }
  .sw:checked { background: var(--ink); }
  .sw:checked::after { translate: 16px 0; background: var(--stage); }
  .agentbar { display: flex; justify-content: space-between; align-items: center; gap: 12px; width: 100%; margin-top: 16px; padding: 12px 16px; border: 0; border-radius: var(--r-sm); background: rgb(238 235 229 / .06); box-shadow: inset 0 0 0 1px rgb(238 235 229 / .18); color: var(--ink); font-size: 14px; text-align: start; cursor: pointer; transition: background-color 300ms ease; }
  .agentbar:hover { background: rgb(238 235 229 / .1); }
  .agentbar span { color: var(--ink-2); }

  .scanner { position: fixed; inset: 0; z-index: 50; display: grid; place-items: center; align-content: center; gap: 28px; padding: 24px; color: var(--ink); }
  .scanner .vf { width: min(68vw, 280px); aspect-ratio: 1; border-radius: 28px; box-shadow: 0 0 0 2px rgb(238 235 229 / .9), 0 0 0 100vmax rgb(7 7 10 / .55); }
  .scanner p { margin: 0; font-size: 15px; text-align: center; text-shadow: 0 1px 8px rgb(0 0 0 / .8); }
  .scanner .btn { background: rgb(7 7 10 / .7); }
  :global(:root.scanning), :global(:root.scanning body) { background: transparent !important; }
  :global(:root.scanning #app > *:not(.scanner)) { visibility: hidden; }
  .toast { position: fixed; left: 50%; top: 84px; translate: -50% 0; z-index: 40; display: flex; gap: 12px; align-items: center; padding: 12px 18px; animation: tst 220ms var(--ease) both; }
  .toast span { display: grid; }
  .toast b { font-weight: 400; }
  .toast small { font-size: 13px; }
  .toast.ok :global(svg) { color: #c9f2a0; }
  @keyframes tst { from { opacity: 0; transform: translateY(-6px); } }

  .right :global(.gear.cur) { background: rgb(238 235 229 / .1); box-shadow: inset 0 0 0 1px var(--line-hi); }
  /* an operation's details: a centred panel on the desktop, a sheet from the bottom on the phone */
  .mveil { position: fixed; inset: 0; z-index: 19; background: rgb(7 7 10 / .62); }
  .txd { position: fixed; z-index: 26; left: 50%; top: 50%; translate: -50% -50%; width: min(420px, calc(100vw - 32px)); min-width: 0; display: grid; gap: 16px; padding: 20px; transform-origin: 50% 50%; }
  .txd .mh { margin: 0; }
  .txamt { margin: 0; font-size: 28px; line-height: 36px; overflow-wrap: anywhere; }
  .txd .sum div { align-items: baseline; }
  .txd .sum dt { white-space: nowrap; }
  .addr2 { font-size: 13px; word-break: break-all; text-align: end; }
  /* desktop column: no scrollbar seam; content fades out under the bottom edge instead of being cut */
  .col { scrollbar-width: none; mask-image: linear-gradient(#000 calc(100% - 32px), transparent); }

  /* phone-only parts stay out of the desktop */
  .tabbar, .hole, .pullmark, .grab { display: none; }

  /* phone (owner 10.10: the controls people know from phone wallets): one header row, the page scrolls as one with the
     card in a hole at its top, menus are sheets from the bottom, sections in a tab bar under the thumb */
  @media (max-width: 860px) {
    .top { display: flex; align-items: center; gap: 8px; padding: calc(10px + var(--top-inset)) 16px 10px; transition: background-color 240ms ease, box-shadow 240ms ease; }
    .top.scrolled { background: rgb(9 9 12 / .96); box-shadow: 0 1px 0 var(--line); } /* no backdrop-filter: it would pin the fixed menu sheet inside the header */
    .top:has(.menu) { z-index: 25; } /* an open sheet rides over the veil and the tab bar */
    .pickers { display: contents; }
    .w-acc { order: 0; margin-right: auto; min-width: 0; }
    .w-net { order: 1; }
    .right { order: 2; margin: 0; }
    .top nav.seg, .right :global(.gear) { display: none; }
    .chip { padding: 0 12px; }
    .chip .mono { display: none; }
    .w-acc .chip > span:first-child { max-width: 38vw; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
    .seg button { padding: 0 12px; font-size: 13px; }

    .menu, .net-menu {
      position: fixed; left: 0; right: 0; top: auto; bottom: 0; min-width: 0; max-height: 82vh; overflow-y: auto; overscroll-behavior: contain;
      padding: 6px 12px calc(12px + var(--bottom-inset)); border-radius: 22px 22px 0 0;
    }
    .grab { display: block; width: 36px; height: 4px; margin: 4px auto 10px; border-radius: 2px; background: rgb(238 235 229 / .28); }
    .mi { min-height: 52px; }

    .col { left: 0; right: 0; top: 0; bottom: 0; width: auto; padding: calc(64px + var(--top-inset)) 16px calc(92px + var(--bottom-inset)); scrollbar-width: none; overscroll-behavior-y: contain; mask-image: none; }
    .col.full { padding-bottom: calc(16px + var(--bottom-inset)); }
    .lockf { top: auto; bottom: clamp(56px, 12vh, 120px); padding: 0 16px; }
    .hole { display: block; height: clamp(200px, 40vh, 360px); }
    .hole.s { height: clamp(170px, 30vh, 280px); }
    .hole.l { height: clamp(240px, 42vh, 380px); } /* the QR card, with the address and «Скопировать» still in view */
    .t { font-size: 44px; margin: 8px 0; }
    .t.s { font-size: 30px; margin: 8px 0 16px; }

    .tabbar {
      display: grid; grid-template-columns: repeat(4, 1fr); position: fixed; left: 0; right: 0; bottom: 0; z-index: 12;
      padding: 6px 4px calc(6px + var(--bottom-inset)); background: #0b0b0e; box-shadow: 0 -1px 0 var(--line);
    }
    .tabbar button { display: grid; justify-items: center; align-content: center; gap: 4px; min-height: 56px; padding: 0; border: 0; background: none; color: var(--ink-3); font-size: 12px; cursor: pointer; -webkit-tap-highlight-color: transparent; transition: color 200ms ease; }
    .tabbar .ti { display: grid; place-items: center; width: 56px; height: 30px; border-radius: 999px; transition: background-color 260ms var(--ease); }
    .tabbar button.cur { color: var(--ink); }
    .tabbar button.cur .ti { background: rgb(238 235 229 / .12); }
    .tabbar button:active .ti { background: rgb(238 235 229 / .18); }

    .bal { font-size: 48px; }
    .acts :global(.btn) { min-height: 44px; justify-content: center; }
    .col .acts:not(.money) { display: grid; grid-template-columns: 1fr 1fr; } /* two equal keys, edge to edge */
    .field .tag.sq { width: 40px; min-height: 40px; } /* paste and scan: a finger's target */
    /* a fingertip, not a pointer: small controls grow to a thumb's size on the phone */
    .chip { min-height: 44px; }
    .tag { min-height: 40px; padding: 0 14px; }
    .seg button, .seg.small button { min-height: 40px; }
    .rf { width: 44px; height: 44px; margin-right: -8px; }
    /* send / receive / swap: round keys with the word under them */
    .acts.money { display: grid; grid-auto-flow: column; grid-auto-columns: 1fr; gap: 8px; }
    .acts.money :global(.btn) { flex-direction: column; gap: 8px; min-height: 0; padding: 0; background: none; box-shadow: none; color: var(--ink); font-size: 13px; overflow: visible; }
    .acts.money :global(.btn svg) { width: 54px; height: 54px; padding: 17px; border-radius: 50%; background: var(--metal); box-shadow: var(--metal-edge); transition: transform 160ms var(--ease); }
    .acts.money :global(.btn.solid svg) { background: var(--ink); color: var(--stage); }
    .acts.money :global(.btn::before) { display: none; }
    .acts.money :global(.btn:active) { transform: none; }
    .acts.money :global(.btn:active svg) { transform: scale(.94); }

    /* pull to refresh: the mark comes down under the header and turns, then spins while the network answers */
    .pullmark {
      display: grid; place-items: center; position: fixed; left: 50%; top: calc(52px + var(--top-inset)); z-index: 11; width: 36px; height: 36px; margin-left: -18px;
      border-radius: 50%; background: var(--metal); box-shadow: var(--metal-edge), var(--lift); color: var(--ink); pointer-events: none;
      opacity: var(--pull, 0); transform: translateY(var(--pull-y, 0px)) rotate(calc(var(--pull, 0) * 270deg));
    }
    .col:global(.refreshing) .pullmark :global(svg) { animation: rf 900ms linear infinite; }

    .toast { top: calc(68px + var(--top-inset)); max-width: calc(100vw - 32px); }

    /* send and swap take the whole screen: the amount big in the middle, the button and the keypad under the thumb */
    .col.full { display: flex; flex-direction: column; padding-top: calc(8px + var(--top-inset)); }
    .top.bare { display: none; } /* a send is one task: its own screen, the network named in the form */
    .col.full .form { flex: 1; display: flex; flex-direction: column; gap: 12px; }
    .col.full .wide { margin-top: auto; }
    .col.full .amtl { margin-top: auto; } /* the amount sits in the middle of the free space, not under the title */
    .back { min-height: 44px; }
    .amtl { justify-items: center; text-align: center; }
    .amtl .label { justify-self: start; }
    .amtl .field { justify-content: center; min-height: 64px; padding: 0; box-shadow: none; gap: 10px; }
    .amtl .field input { flex: none; field-sizing: content; min-width: 1ch; max-width: 70vw; font: 400 44px/52px var(--font); letter-spacing: -.02em; text-align: center; font-variant-numeric: tabular-nums; }
    .amtl .field > .faint { font-size: 20px; }
    .amtl .field > :global(.tag) { display: none; }
    .parts { justify-content: center; }
    .parts .tag { min-height: 36px; padding: 0 16px; }

    .seam { margin: -8px 0; } /* the form's phone gap is 12: cards stay 8 apart */
    .seam :global(.btn.icon) { width: 44px; min-height: 44px; }

    /* the operation's details: a sheet from the bottom, buttons a finger wide */
    .txd { translate: none; width: auto; padding-left: 20px; padding-right: 20px; }
    .txd .btn { min-height: 44px; }
  }
</style>
