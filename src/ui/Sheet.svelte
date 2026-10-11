<script lang="ts" module>
  export type SheetMode = 'welcome' | 'export' | 'create' | 'restore' | 'add-new' | 'add-import' | 'add-account' | 'add-evm' | 'add-ledger' | 'password' | 'send' | 'batch' | 'addon-keys' | 'bio'
  export interface Draft {
    id: string; open: boolean; account: string; from: string; to: string; network: string; test: boolean; newRecipient: boolean; warn: string | null
    amount: string; symbol: string | null; mint: string | null; fee: string; rent: string; solTotal: string
    native?: string // the network's coin (EVM); absent = SOL
    feeMax?: boolean // the fee is a ceiling (EVM: the network takes what the block asks, never more than this)
    revoke?: boolean // takes back a permission to spend (to = who loses it); nothing is sent
    ledger?: boolean // the account's key is on a Ledger: no password, the device shows and approves
    swap?: { outMint: string; out: string; min: string; route: string; feeBps: number; sym?: string } // a swap: what the dry run saw arrive
  }
  // an agent's batch request as the core planned it (agent.rs view): numbers and addresses only, no agent prose
  export interface Batch {
    id: string; network: string; test: boolean; to: string; newRecipient: boolean; open: boolean; count: number; total: string; fees: string
    rows: { name: string; from: string; balance?: string; amount: string; fee: string; left?: string }[]
    feeMax?: boolean // the fees are a ceiling (EVM gas; a Solana token's recipient account is paid for once)
    sym?: string; feeSym?: string // what moves and what pays the fees (absent: SOL)
    warn?: string[] // the core's own warnings for any row (a contract recipient, a token its issuer can take back…)
    known?: boolean // the destination is one of the owner's accounts or contacts (set by the app)
    by?: 'owner' | 'agent' | 'addon' // who made the plan: a folder's «Перевести со всех», an agent, or an add-on (byName)
    byName?: string
  }
  // keys an add-on hands over (bridge «addKeys»): the sheet shows what they open and asks the wallet password
  export interface Offer { addon: string; addonName: string; items: { name: string; key: string }[] }
</script>

<script lang="ts">
  // «Лист»: words, keys, passwords and the send confirmation, inside the main window (owner + Codex, 07.10).
  // The window is excluded from screen capture before anything secret renders; the core re-checks everything.
  import { onMount, tick, untrack } from 'svelte'
  import gsap from 'gsap'
  import { invoke } from '@tauri-apps/api/core'
  import Icon from './Icon.svelte'
  import { haptic } from '../lib/actions'
  import { humanErr } from '../lib/i18n.svelte'

  let { mode: modeIn, wallet = '', account = '', fresh: freshIn = false, open: openIn = false, origin = null, round, draft = null, batch = null, offer = null, onclose }: {
    mode: SheetMode; wallet?: string; account?: string; fresh?: boolean; open?: boolean; origin?: DOMRect | null; round?: number; draft?: Draft | null; batch?: Batch | null; offer?: Offer | null
    onclose: (done: boolean) => void
  } = $props()
  // fixed for the sheet's life: saving a new vault flips `fresh` while the sheet is still folding away
  const { mode: mode0, fresh, open } = untrack(() => ({ mode: modeIn, fresh: freshIn, open: openIn }))
  // first launch: the sheet is the screen's one form; its steps go back to the welcome instead of closing
  const welcome = mode0 === 'welcome'
  let mode = $state<SheetMode>(mode0)

  const MIN = 10
  const still = matchMedia('(prefers-reduced-motion: reduce)').matches
  const glass = gsap.parseEase('glass') ?? ((t: number) => 1 - (1 - t) ** 3)
  const out3 = (t: number) => 1 - (1 - t) ** 3

  type Step = 'start' | 'reveal' | 'keys' | 'words' | 'check' | 'secret' | 'save' | 'name' | 'evm' | 'ledger' | 'pass' | 'confirm' | 'batch' | 'offer' | 'fail' | 'bio'
  const first: Record<SheetMode, Step> = { welcome: 'start', export: 'reveal', create: 'words', 'add-new': 'words', restore: 'secret', 'add-import': 'secret', 'add-account': 'name', 'add-evm': 'evm', 'add-ledger': 'ledger', password: 'pass', send: 'confirm', batch: 'batch', 'addon-keys': 'offer', bio: 'bio' }
  let step = $state<Step>(first[mode0])
  let dir = 1
  let ready = $state(false) // capture protection confirmed
  let busy = $state(false)
  let err = $state('')
  let el: HTMLDivElement, inner: HTMLDivElement
  let h = $state(0)
  // the plate and the content window grow together, so nothing shows past the plate's edge mid-unfold (owner 09.10)
  let vh = $state(innerHeight)
  // the top edge sits near the centre and rises (with the growth) only as far as a tall step needs to fit the window
  // phone: a sheet from the bottom edge, like every phone app; it rises with the keyboard (MainActivity lifts the page)
  const phone = matchMedia('(max-width: 860px)').matches
  const top = $derived(Math.max(24, Math.round(phone ? vh - h : (vh - h) / 2))) // desktop centred (owner 10.10: it sat low); a tall step starts near the top
  const tall = $derived(h > vh - 24 - top)

  // form state lives only here and is wiped on close
  let words = $state<string[]>([])
  let picks = $state<number[]>([])
  let checks = $state<string[]>(['', '', ''])
  let secret = $state('')
  let name = $state(mode0 === 'add-account' ? 'Счёт' : mode0 === 'add-ledger' ? 'Ledger' : fresh ? 'Основной' : 'Кошелёк')
  let p0 = $state(''), p1 = $state(''), p2 = $state('')
  let nopass = $state(false) // new vault without a password, or remove the password
  const needPass = $derived(!(fresh ? nopass : open))

  function go(s: Step, d = 1) { dir = d; err = ''; step = s }

  // ---------------------------------------------------------------- geometry: grow from the pressed control, fold back
  function from(): Keyframe {
    const s = el.getBoundingClientRect()
    const o = origin ?? new DOMRect(s.x + s.width / 2 - 29, s.y + s.height / 2 - 29, 58, 58)
    const dx = o.x + o.width / 2 - (s.x + s.width / 2), dy = o.y + o.height / 2 - (s.y + s.height / 2)
    const iy = Math.max(0, (s.height - o.height) / 2), ix = Math.max(0, (s.width - o.width) / 2)
    return { transform: `translate(${dx}px, ${dy}px)`, clipPath: `inset(${iy}px ${ix}px round ${round ?? Math.min(o.height, o.width) / 2}px)` }
  }
  const whole: Keyframe = { transform: 'translate(0, 0)', clipPath: phone ? 'inset(0px 0px round 22px 22px 0px 0px)' : 'inset(0px 0px round 20px)' }
  const anim = (k: Keyframe[], ms: number, easing: string) => el.animate(k, { duration: still ? 0 : ms, easing, fill: 'forwards' }).finished

  let closing = false
  // the wallet locked (auto-lock too): the sheet goes at once, busy or not, secrets wiped
  export function shut() { close(false, true) }
  export function dismiss() { if (welcome && step === 'start') return false; close(false); return true } // Android back = Escape; false: nothing behind the welcome
  async function close(done: boolean, force = false) {
    if (closing || (busy && !done && !force)) return
    if (welcome && !done && !force) { // back to the welcome, secrets wiped: nothing lies behind this sheet to close to
      if (step === 'start') return
      secret = p0 = p1 = p2 = ''; words = []; checks = ['', '', '']; nopass = false; gen++
      mode = 'welcome'; go('start', -1); return
    }
    closing = true
    holdTween?.kill()
    if (mode === 'send' && !done) await invoke('send_cancel').catch(() => {}) // before any animation: nothing can confirm now
    secret = p0 = p1 = p2 = ''; words = []; checks = ['', '', '']; keys = null
    await tick() // secrets leave the DOM before the window becomes capturable again
    const s = el.getBoundingClientRect()
    if (done) { // key step: fold into a dot, hold, then the deck deals
      const r = 6, iy = s.height / 2 - r, ix = s.width / 2 - r
      await anim([whole, { transform: 'translate(0, 0)', clipPath: `inset(${iy}px ${ix}px round ${r}px)` }], 320, 'cubic-bezier(.6,0,.8,.4)')
      await anim([{ opacity: 1 }, { opacity: 0 }], 120, 'ease') // the deal starts as the dot goes out (no dead beat)
    } else await anim([whole, from()], 320, 'cubic-bezier(.6,0,.8,.4)')
    await invoke('panel_secret', { on: false }).catch(() => {})
    onclose(done)
  }

  onMount(() => {
    const ro = new ResizeObserver(() => (h = inner.offsetHeight))
    ro.observe(inner)
    h = inner.offsetHeight
    anim([from(), whole], 560, 'cubic-bezier(.32,.9,.18,1)')
    // nothing secret renders until the window is proven excluded from capture
    invoke('panel_secret', { on: true }).then(async () => {
      ready = true
      if (step === 'words') phrase()
      if (step === 'offer') preview()
    }, e => { err = humanErr(e); failed = true; step = 'fail' })
    const key = (e: KeyboardEvent) => { if (e.key === 'Escape') close(false) }
    addEventListener('keydown', key)
    return () => { ro.disconnect(); removeEventListener('keydown', key); holdTween?.kill(); closing = true }
  })

  // ---------------------------------------------------------------- step transitions (ConnectKit teardown, world curve)
  function stepOut(node: HTMLElement) {
    Object.assign(node.style, { position: 'absolute', left: '0', right: '0', top: '0', pointerEvents: 'none' })
    const to = still ? 1 : dir > 0 ? 0.96 : 1.04 // reduced motion: opacity only
    return { duration: still ? 0 : 180, easing: out3, css: (t: number) => `opacity:${t};transform:scale(${to + (1 - to) * t})` }
  }
  function stepIn(node: HTMLElement) {
    // back before the old step finished leaving: Svelte reuses that node, so undo what stepOut pinned on it
    Object.assign(node.style, { position: '', left: '', right: '', top: '', pointerEvents: '' })
    const at = still ? 1 : dir > 0 ? 1.04 : 0.96
    return { delay: still ? 0 : 100, duration: still ? 120 : 320, easing: glass, css: (t: number) => `opacity:${t};transform:scale(${at + (1 - at) * t})` }
  }
  // an address to check letter by letter: groups of four (the clipboard and the core keep the raw string)
  const g4 = (a: string) => a.match(/.{1,4}/g)?.join(' ') ?? a
  const focusFirst = (n: HTMLElement) => { if (!matchMedia('(pointer: fine)').matches) return; setTimeout(() => n.querySelector<HTMLElement>('input, textarea, button.go')?.focus({ preventScroll: true }), still ? 0 : 420) }

  // ---------------------------------------------------------------- actions
  // export: password again, then the account's phrase and keys (the core refuses unless capture is off)
  let keys = $state<{ phrase: string | null; solana: string; evm: string | null } | null>(null)
  async function reveal() {
    if (!ready || busy) return
    if (!open && !p1) { err = 'Введите пароль'; return }
    busy = true; err = ''
    try { const k = await invoke<typeof keys>('secure_export', { account, pass: open ? '' : p1 }); p1 = ''; if (closing) return; keys = k; go('keys') }
    catch (e) { err = humanErr(e) } finally { busy = false }
  }

  // words and keys to the clipboard through the core: kept out of Windows clipboard history, wiped after a while
  let copiedWhat = $state(''), clearIn = $state(0)
  async function copySecret(what: string, text: string) {
    // Android: the activity's own clipboard bridge (marked sensitive, wiped); desktop: the core (out of Win+V history, wiped)
    const android = (window as unknown as { __cwClip?: { put(t: string): number } }).__cwClip
    try { clearIn = android ? android.put(text) : await invoke<number>('clip_secret', { text }); copiedWhat = what; setTimeout(() => { if (copiedWhat === what) copiedWhat = '' }, 2400) }
    catch (e) { err = humanErr(e) }
  }
  // an EVM key for an account imported from a Solana key
  // Ledger: the device lists its first addresses, the owner picks one; the core reads that address from the device again
  let found = $state<string[]>([]), pick = $state(0), searching = $state(false)
  async function findLedger() {
    searching = true; err = ''
    try { found = await invoke<string[]>('ledger_accounts'); pick = 0 } catch (e) { err = humanErr(e) } finally { searching = false }
  }
  function addLedger() {
    if (!ready || busy || !found.length) return
    if (!open && !p1) { err = 'Введите пароль'; return }
    run(() => invoke('secure_add_ledger', { index: pick, name: name.trim() || 'Ledger', pass: open ? '' : p1 }))
  }
  function addEvm() {
    if (!ready || busy) return
    if (!secret.trim()) { err = 'Вставьте ключ'; return }
    if (!open && !p1) { err = 'Введите пароль'; return }
    run(() => invoke('secure_add_evm', { account, key: secret.trim(), pass: open ? '' : p1 }))
  }

  // an add-on's keys: the core says which addresses they open and which the wallet already has
  let offered = $state<{ address: string; have: boolean }[]>([])
  const fresh2 = $derived(offered.filter(o => !o.have).length)
  async function preview() {
    if (!offer) return
    try { offered = await invoke('addon_keys_preview', { addon: offer.addon, keys: offer.items.map(i => i.key) }) } catch (e) { err = humanErr(e) }
  }
  function addOffer() {
    if (!ready || busy || !offer || !fresh2) return
    if (!open && !p1) { err = 'Введите пароль'; return }
    run(() => invoke('secure_addon_keys', { addon: offer.addon, names: offer.items.map(i => i.name), keys: offer.items.map(i => i.key), pass: open ? '' : p1 }))
  }

  // a phrase that lands after «back» was pressed belongs to nobody: the generation drops it
  let gen = 0, failed = false
  async function phrase() {
    if (!ready || words.length) return
    const g = ++gen
    try { const w = (await invoke<string>('secure_new_phrase')).split(' '); if (g === gen && step === 'words') words = w } catch (e) { if (g === gen) err = humanErr(e) }
  }
  function begin(m: 'create' | 'restore') {
    if (failed) return go('fail') // no capture protection: the welcome cannot lead to words or keys
    mode = m
    if (m === 'create') { go('words'); phrase() } else go('secret')
  }
  function toCheck() {
    const p: number[] = []
    while (p.length < 3) { const i = crypto.getRandomValues(new Uint32Array(1))[0] % words.length; if (!p.includes(i)) p.push(i) }
    picks = p.sort((a, b) => a - b); checks = ['', '', '']
    go('check')
  }
  const okWord = (k: number) => checks[k].trim().toLowerCase() === words[picks[k]]
  function checkWords() {
    if (![0, 1, 2].every(okWord)) { err = 'Слова не совпадают с записанными'; return }
    go('save')
  }
  function toSave() {
    if (!secret.trim()) { err = 'Вставьте слова или ключ'; return }
    go('save')
  }
  function readPass(): string | null {
    if (!needPass) return ''
    if (fresh && p1.length < MIN) { err = `Пароль не короче ${MIN} символов`; return null }
    if (fresh && p1 !== p2) { err = 'Пароли не совпадают'; return null }
    if (!p1) { err = 'Введите пароль'; return null }
    return p1
  }
  async function run(fn: () => Promise<unknown>) {
    busy = true; err = ''
    try { await fn(); await close(true) } catch (e) { err = humanErr(e); busy = false } // stays busy while it folds: no live button flashes back
  }
  function save() {
    if (!ready) return
    const pass = readPass(); if (pass === null) return
    const kind = mode === 'create' || mode === 'add-new' ? 'new' : /\s/.test(secret.trim()) ? 'phrase' : 'key'
    run(() => invoke('secure_save_wallet', { kind, secret: kind === 'new' ? null : secret.trim(), name, pass }))
  }
  function addAccount() {
    if (!ready) return
    const pass = readPass(); if (pass === null) return
    run(() => invoke('secure_add_account', { wallet, name, pass }))
  }
  function setPassword() {
    if (!ready) return
    if (!open && !p0) { err = 'Введите нынешний пароль'; return }
    if (!nopass) {
      if (p1.length < MIN) { err = `Новый пароль не короче ${MIN} символов`; return }
      if (p1 !== p2) { err = 'Пароли не совпадают'; return }
    }
    run(() => invoke('vault_password', { old: open ? '' : p0, new: nopass ? '' : p1 }))
  }

  // phone: the password goes to the core, which checks it and hands it to the keystore behind the fingerprint sheet
  function enableBio() {
    if (!ready) return
    if (!p1) { err = 'Введите пароль'; return }
    run(() => invoke('bio_enable', { pass: p1 }).catch(e => { throw String(e) === 'cancelled' ? 'Отпечаток не подтверждён' : e }))
  }

  // send: hold to confirm (brief M-36: hold 1.2 s, 200 ms of quiet, then the throw)
  let hold = $state(0), holdTween: gsap.core.Tween | null = null
  function press() {
    const conf = draft ?? batch
    if (busy || closing || !ready || !conf) return
    if (needTail && tail.trim().length !== 4) { err = 'Введите последние 4 знака адреса получателя'; haptic('err'); return }
    if (!conf.open && !p1) { err = 'Введите пароль'; haptic('err'); return }
    holdTween?.kill()
    holdTween = gsap.to({ v: hold }, { v: 1, duration: still ? 0.3 : 1.2 * (1 - hold), ease: 'none', onUpdate() { hold = this.targets()[0].v }, onComplete: () => { haptic('ok'); confirmSend() } }) // the phone answers the full hold
  }
  function release() { if (busy || hold >= 1) return; holdTween?.kill(); holdTween = gsap.to({ v: hold }, { v: 0, duration: 0.32, ease: 'power2.out', onUpdate() { hold = this.targets()[0].v } }) }
  async function confirmSend() {
    const conf = draft ?? batch
    if (!conf || closing) return
    await new Promise(r => setTimeout(r, still ? 0 : 200))
    if (closing) return // closed during the quiet beat
    const pass = conf.open ? '' : p1
    busy = true; err = ''
    try { await invoke(batch ? 'batch_confirm' : 'send_confirm', batch ? { id: conf.id, pass, tail: needTail ? tail.trim() : null } : { id: conf.id, pass }); busy = false; await close(true) }
    catch (e) { err = humanErr(e); busy = false; hold = 0 }
  }

  // the agent's request: «Отклонить» closes it for good; the cross only hides the sheet (it waits up to 10 minutes)
  async function rejectBatch() { if (!batch || busy) return; await invoke('batch_reject', { id: batch.id }).catch(() => {}); close(false) }
  // an agent's or add-on's plan to an address new on its network: the owner types its last 4 characters (the core checks)
  let tail = $state('')
  const needTail = $derived(!!batch && batch.by !== 'owner' && batch.newRecipient)
  const ru = (n: string) => n.replace('.', ',') // the core writes 1.5; the screen says 1,5
  const plural = (n: number) => n % 10 === 1 && n % 100 !== 11 ? 'перевод' : [2, 3, 4].includes(n % 10) && ![12, 13, 14].includes(n % 100) ? 'перевода' : 'переводов'
  const shortA = (a: string) => `${a.slice(0, 4)}…${a.slice(-4)}`
  const titles: Record<Step, string> = $derived({ // derived: the sheet's props are read where they are used
    start: 'С чего начнём?', reveal: 'Экспорт ключей', keys: 'Ключи счёта', words: 'Запишите 12 слов', check: 'Проверим запись', secret: 'Импорт кошелька',
    save: fresh ? 'Название и пароль' : 'Название', name: 'Новый счёт', evm: 'Ключ Ethereum', ledger: 'Подключить Ledger', pass: open ? 'Поставить пароль' : 'Пароль кошелька',
    confirm: draft?.swap ? 'Подтвердите обмен' : 'Подтвердите перевод', batch: batch?.by === 'owner' ? 'Перевод со всех' : batch?.by === 'addon' ? `Запрос «${batch.byName}»` : 'Запрос агента', offer: 'Новые счета', fail: 'Лист не открыть', bio: 'Вход по отпечатку',
  })
</script>

<svelte:window bind:innerHeight={vh} />
<div class="veil" class:soft={welcome} aria-hidden="true"></div>
<div class="sheet" class:busy class:phone bind:this={el} style:--top="{top}px" role="dialog" aria-modal="true" aria-label={titles[step]} style:--h="{h}px">
  <div class="plate" aria-hidden="true"></div>
  <div class="body" class:scroll={tall}>
  <div class="inner" bind:this={inner}>
    {#key `${step}${nopass}`}
      <section in:stepIn out:stepOut use:focusFirst>
        <header>
          <h2>{titles[step]}</h2>
          {#if step !== 'start' && !(welcome && step === 'fail')}<button class="x" class:lead={welcome} type="button" onclick={() => close(false)} aria-label={welcome ? 'Назад' : 'Закрыть'} disabled={busy}><Icon name={welcome ? 'back' : 'close'} size={16} /></button>{/if}
        </header>

        {#if step === 'start'}
          <p class="hint">Ключи хранятся только на этом устройстве. 12 слов совместимы с Phantom и MetaMask.</p>
          <div class="acts stack">
            <button class="btn solid go" type="button" onclick={() => begin('create')}>Создать кошелёк</button>
            <button class="btn go" type="button" onclick={() => begin('restore')}>Импортировать ключ</button>
          </div>

        {:else if step === 'reveal'}
          <p class="hint warn">Кто увидит эти слова или ключ, тот заберёт деньги. Открывайте, когда рядом никого нет; не фотографируйте и не храните в облаке.</p>
          {#if !open}<label><span class="label">Пароль кошелька</span><span class="field"><input type="password" bind:value={p1} autocomplete="current-password" disabled={!ready}
            onkeydown={e => { if (e.key === 'Enter') reveal() }} /></span></label>{/if}
          <div class="acts"><button class="btn solid go" type="button" onclick={reveal} disabled={busy || !ready}>{busy ? 'Проверяем' : 'Показать'}</button></div>

        {:else if step === 'keys' && keys}
          <p class="hint warn">Кто увидит эти слова или ключ, тот заберёт деньги.</p>
          {#if keys.phrase}
            <span class="label">Секретная фраза</span>
            <ol class="words">{#each keys.phrase.split(' ') as w, i (i)}<li style="--i:{i}"><span class="n">{i + 1}</span><span class="mono">{w}</span></li>{/each}</ol>
            <button class="cp" type="button" onclick={() => copySecret('words', keys!.phrase!)}>{copiedWhat === 'words' ? 'Скопировано' : 'Скопировать слова'}</button>
          {:else}<p class="hint">У этого счёта нет 12 слов: он добавлен закрытым ключом.</p>{/if}
          <div class="key"><span class="label">Закрытый ключ Solana</span><span class="mono keyv">{keys.solana}</span>
            <button class="cp" type="button" onclick={() => copySecret('sol', keys!.solana)}>{copiedWhat === 'sol' ? 'Скопировано' : 'Скопировать ключ'}</button></div>
          {#if keys.evm}<div class="key"><span class="label">Закрытый ключ Ethereum</span><span class="mono keyv">{keys.evm}</span>
            <button class="cp" type="button" onclick={() => copySecret('evm', keys!.evm!)}>{copiedWhat === 'evm' ? 'Скопировано' : 'Скопировать ключ'}</button></div>{/if}
          <p class="hint">{clearIn ? `Скопированное сотрётся из буфера через ${clearIn} с и не попадёт в журнал буфера Windows.` : 'Скопированное сотрётся из буфера через полминуты.'}</p>
          <div class="acts"><button class="btn solid go" type="button" onclick={() => close(false)}>Готово</button></div>

        {:else if step === 'fail'}
          <p class="hint">Окно не удалось скрыть от снимков экрана, поэтому слова и ключи здесь не покажем.</p>

        {:else if step === 'words'}
          <ol class="words">
            {#each words as w, i (i)}<li style="--i:{i}"><span class="n">{i + 1}</span><span class="mono">{w}</span></li>{/each}
            {#if !words.length}{#each Array(12) as _, i}<li class="ghost" style="--i:{i}"><span class="n">{i + 1}</span></li>{/each}{/if}
          </ol>
          <p class="hint">На бумагу, по порядку, офлайн. Кто знает эти слова, тот владеет деньгами: не фотографируйте и не храните в облаке.</p>
          <div class="acts"><button class="btn solid go" type="button" onclick={toCheck} disabled={words.length !== 12}>Я записал</button></div>

        {:else if step === 'check'}
          <div class="grid3">
            {#each picks as p, k}
              <label><span class="label">Слово №{p + 1}</span>
                <span class="field" class:ok={okWord(k)}><input class="mono" bind:value={checks[k]} autocomplete="off" autocapitalize="none" spellcheck="false" /></span></label>
            {/each}
          </div>
          <p class="hint">Так проверим, что слова записаны без ошибок.</p>
          <div class="acts"><button class="btn" type="button" onclick={() => go('words', -1)}>Показать слова</button><button class="btn solid go" type="button" onclick={checkWords}>Дальше</button></div>

        {:else if step === 'secret'}
          <label><span class="label">12 или 24 слова, или закрытый ключ Solana</span>
            <span class="field area"><textarea class="mono" rows="4" bind:value={secret} autocomplete="off" autocapitalize="none" spellcheck="false" disabled={!ready}></textarea></span></label>
          <p class="hint">Слова дают адреса Solana и сетей Ethereum, как в Phantom и MetaMask. Закрытый ключ даёт только адрес Solana.</p>
          <div class="acts"><button class="btn solid go" type="button" onclick={toSave} disabled={!ready}>Дальше</button></div>

        {:else if step === 'save' || step === 'name'}
          <label><span class="label">Название</span><span class="field"><input bind:value={name} maxlength="40" /></span></label>
          {#if fresh}
            <label class="toggle"><input type="checkbox" bind:checked={nopass} /><span>Без пароля</span></label>
          {/if}
          {#if needPass}
            <label><span class="label">{fresh ? 'Пароль' : 'Пароль кошелька'}</span><span class="field"><input type="password" bind:value={p1} autocomplete={fresh ? 'new-password' : 'current-password'} disabled={!ready} /></span></label>
            {#if fresh}<label><span class="label">Ещё раз</span><span class="field"><input type="password" bind:value={p2} autocomplete="new-password" disabled={!ready} /></span></label>
              <p class="hint">Не короче {MIN} символов. Пароль шифрует ключи на этом устройстве. Восстановить его нельзя, только заново из слов.</p>{/if}
          {:else if fresh}
            <p class="hint warn">Ключи лягут на диск открытыми: кто откроет это устройство или файл, тот заберёт деньги. Пароль можно поставить позже в настройках.</p>
          {/if}
          {#if step === 'name'}<p class="hint">Новый адрес из тех же слов: он восстановится вместе с кошельком.</p>{/if}
          <div class="acts">
            {#if step === 'save' && mode !== 'restore' && mode !== 'add-import'}<button class="btn" type="button" onclick={() => go('check', -1)} disabled={busy}>Назад</button>{/if}
            {#if step === 'save' && (mode === 'restore' || mode === 'add-import')}<button class="btn" type="button" onclick={() => go('secret', -1)} disabled={busy}>Назад</button>{/if}
            <button class="btn solid go" type="button" onclick={step === 'name' ? addAccount : save} disabled={busy || !ready}>{busy ? 'Сохраняем' : step === 'name' ? 'Добавить' : 'Сохранить'}</button>
          </div>

        {:else if step === 'evm'}
          <label><span class="label">Закрытый ключ Ethereum (MetaMask: 64 знака)</span>
            <span class="field"><input class="mono" type="password" bind:value={secret} autocomplete="off" autocapitalize="none" spellcheck="false" disabled={!ready} /></span></label>
          {#if !open}<label><span class="label">Пароль кошелька</span><span class="field"><input type="password" bind:value={p1} autocomplete="current-password" disabled={!ready} /></span></label>{/if}
          <p class="hint">Счёт получит адрес во всех сетях Ethereum: Base, Arbitrum и других. Ключ хранится зашифрованным рядом с ключом Solana.</p>
          <div class="acts"><button class="btn solid go" type="button" onclick={addEvm} disabled={busy || !ready}>{busy ? 'Сохраняем' : 'Добавить'}</button></div>

        {:else if step === 'ledger'}
          {#if !found.length}
            <p class="hint">Подключите Ledger по USB, введите на нём PIN и откройте приложение Solana. Ключ остаётся в устройстве: каждый перевод вы подтверждаете на его экране.</p>
            <div class="acts"><button class="btn solid go" type="button" onclick={findLedger} disabled={searching || !ready}>{searching ? 'Ищем' : 'Найти Ledger'}</button></div>
          {:else}
            <p class="hint">Выберите адрес. Первый — тот, что показывает Ledger Live.</p>
            <div class="picks" role="radiogroup">
              {#each found as a, i (a)}<label class="pick"><input type="radio" name="lpick" value={i} bind:group={pick} /><span class="mono">{i + 1}. {shortA(a)}</span><small class="mono faint">{a}</small></label>{/each}
            </div>
            <label><span class="label">Название</span><span class="field"><input bind:value={name} placeholder="Ledger" autocomplete="off" disabled={!ready} /></span></label>
            {#if !open}<label><span class="label">Пароль кошелька</span><span class="field"><input type="password" bind:value={p1} autocomplete="current-password" disabled={!ready} /></span></label>{/if}
            <div class="acts"><button class="btn solid go" type="button" onclick={addLedger} disabled={busy || !ready}>{busy ? 'Сохраняем' : 'Добавить'}</button></div>
          {/if}

        {:else if step === 'pass'}
          {#if !open}
            <label><span class="label">Нынешний пароль</span><span class="field"><input type="password" bind:value={p0} autocomplete="current-password" disabled={!ready} /></span></label>
            <label class="toggle"><input type="checkbox" bind:checked={nopass} /><span>Убрать пароль</span></label>
          {/if}
          {#if !nopass}
            <label><span class="label">Новый пароль</span><span class="field"><input type="password" bind:value={p1} autocomplete="new-password" disabled={!ready} /></span></label>
            <label><span class="label">Ещё раз</span><span class="field"><input type="password" bind:value={p2} autocomplete="new-password" disabled={!ready} /></span></label>
            <p class="hint">Не короче {MIN} символов.</p>
          {:else}
            <p class="hint warn">Ключи лягут на диск открытыми: кто откроет это устройство или файл, тот заберёт деньги.</p>
          {/if}
          <div class="acts"><button class="btn solid go" type="button" onclick={setPassword} disabled={busy || !ready}>{busy ? 'Сохраняем' : 'Сохранить'}</button></div>

        {:else if step === 'bio'}
          <label><span class="label">Пароль кошелька</span><span class="field"><input type="password" bind:value={p1} autocomplete="current-password" disabled={!ready}
            onkeydown={e => { if (e.key === 'Enter') enableBio() }} /></span></label>
          <p class="hint">Телефон сохранит пароль под отпечатком и будет открывать кошелёк по нему. Переводы по-прежнему с паролем.</p>
          <div class="acts"><button class="btn solid go" type="button" onclick={enableBio} disabled={busy || !ready}>{busy ? 'Ждём отпечаток' : 'Включить'}</button></div>

        {:else if step === 'confirm' && draft}
          {@const nat = draft.native ?? 'SOL'}
          {@const sym = draft.mint ? (draft.symbol ?? 'ед. токена') : nat}
          {#if draft.revoke}
            <!-- a revoke moves no money: it takes back «may spend» from one address -->
            <p class="amount display">{sym}<small>разрешение на траты</small></p>
            <dl class="sum">
              <div><dt>У кого</dt><dd class="mono addr">{g4(draft.to)}</dd></div>
              <div><dt>Токен</dt><dd class="mono addr">{draft.mint}</dd></div>
              <div><dt>Комиссия сети</dt><dd class="num">{draft.feeMax ? 'до ' : ''}{ru(draft.fee)} {nat}</dd></div>
              <div><dt>Счёт</dt><dd>{draft.account}<br /><span class="mono addr faint">{g4(draft.from)}</span></dd></div>
              <div><dt>Сеть</dt><dd class:warn={!draft.test}>{draft.network}</dd></div>
            </dl>
            <p class="hint">После отзыва этот адрес больше не сможет тратить ваш токен. Деньги остаются на месте.</p>
          {:else if draft.swap}
            <!-- a swap: what leaves, and what the dry run saw arrive (never Jupiter's promise alone) -->
            <p class="amount display">{ru(draft.amount)}<small>{sym}</small></p>
            <p class="hint">меняется на</p>
            <p class="amount display get">≈ {ru(draft.swap.out)}<small>{draft.swap.sym ?? 'ед. токена'}</small></p>
            <!-- owner 10.10: what leaves, what arrives, the floor, the account. No route, fee or network rows, no explanations -->
            <dl class="sum">
              <div><dt>Не меньше</dt><dd class="num">{ru(draft.swap.min)} {draft.swap.sym ?? ''}</dd></div>
              {#if !draft.swap.sym}<div><dt>Получаете токен</dt><dd class="mono addr">{draft.swap.outMint}</dd></div>{/if}
              <div><dt>Со счёта</dt><dd>{draft.account}</dd></div>
            </dl>
          {:else}
          <p class="amount display">{ru(draft.amount)}<small>{sym}</small></p>
          <dl class="sum">
            <div><dt>Кому</dt><dd class="mono addr">{g4(draft.to)}</dd></div>
            {#if draft.mint}<div><dt>Токен</dt><dd class="mono addr">{draft.mint}</dd></div>{/if}
            <div><dt>Комиссия сети</dt><dd class="num">{draft.feeMax ? 'до ' : ''}{ru(draft.fee)} {nat}</dd></div>
            {#if draft.rent !== '0'}<div><dt>Счёт токена у получателя</dt><dd class="num">{draft.rent} SOL</dd></div>{/if}
            <div><dt>Всего спишется</dt><dd class="num">{draft.feeMax ? 'не больше ' : ''}{draft.mint ? `${ru(draft.amount)} ${sym} + ` : ''}{ru(draft.solTotal)} {nat}</dd></div>
            <div><dt>Со счёта</dt><dd>{draft.account}<br /><span class="mono addr faint">{g4(draft.from)}</span></dd></div>
            <div><dt>Сеть</dt><dd class:warn={!draft.test}>{draft.network}{draft.test ? ' · тестовые деньги' : ' · настоящие деньги'}</dd></div>
          </dl>
          <p class="hint" class:warn={draft.newRecipient}>{draft.newRecipient
            ? (draft.mint && !draft.native ? 'У получателя ещё нет счёта для этого токена: сеть создаст его за ваш SOL. ' : 'Этот адрес в сети ещё пустой. ')
            : ''}Сверьте адрес по буквам: отменить перевод нельзя.</p>
          {/if}
          {#if draft.warn}<p class="hint warn">{draft.warn}</p>{/if}
          {#if !draft.open}<label><span class="label">Пароль кошелька</span><span class="field"><input type="password" bind:value={p1} autocomplete="current-password" disabled={!ready} /></span></label>{/if}
          <div class="acts">
            <button class="btn solid go hold" type="button" style:--hold={hold} disabled={busy || !ready}
              onpointerdown={press} onpointerup={release} onpointerleave={release} oncontextmenu={e => e.preventDefault()}
              onkeydown={e => { if ((e.key === ' ' || e.key === 'Enter') && !e.repeat) { e.preventDefault(); press() } }}
              onkeyup={e => { if (e.key === ' ' || e.key === 'Enter') release() }}>
              <span>{busy ? (draft?.ledger ? 'Подтвердите на Ledger' : draft?.revoke ? 'Отзываем' : draft?.swap ? 'Меняем' : 'Отправляем') : draft?.revoke ? 'Удерживайте, чтобы отозвать' : draft?.swap ? 'Удерживайте, чтобы обменять' : 'Удерживайте, чтобы отправить'}</span>
            </button>
          </div>
        {/if}
        {#if step === 'offer' && offer}
          <p class="hint">Дополнение «{offer.addonName}» передаёт ключи. Они станут счетами этого кошелька.</p>
          <ol class="rows">
            {#each offer.items as it, i (i)}
              <li class:have={offered[i]?.have}><span><b>{it.name}</b><small class="mono">{offered[i] ? shortA(offered[i].address) : '…'}</small></span><span class="faint">{offered[i]?.have ? 'уже есть' : ''}</span></li>
            {/each}
          </ol>
          <p class="hint warn">Ключи этих счетов известны дополнению. Держите на них то, что доверили бы ему.</p>
          {#if !open && fresh2}<label><span class="label">Пароль кошелька</span><span class="field"><input type="password" bind:value={p1} autocomplete="current-password" disabled={!ready}
            onkeydown={e => { if (e.key === 'Enter') addOffer() }} /></span></label>{/if}
          <div class="acts">
            <button class="btn" type="button" onclick={() => close(false)} disabled={busy}>Отмена</button>
            <button class="btn solid go" type="button" onclick={addOffer} disabled={busy || !ready || !fresh2}>{busy ? 'Сохраняем' : fresh2 ? `Добавить ${fresh2}` : 'Все уже есть'}</button>
          </div>
        {/if}
        {#if step === 'batch' && batch}
          <!-- plain data only: the agent's words never reach this sheet (prompt injection has no voice here) -->
          <p class="amount display">{ru(batch.total)}<small>{batch.sym ?? 'SOL'}</small></p>
          <p class="hint">{batch.count} {plural(batch.count)} с ваших счетов на один адрес</p>
          <dl class="sum">
            <div><dt>Кому</dt><dd class="mono addr">{g4(batch.to)}</dd></div>
            <div><dt>Сеть</dt><dd class:warn={!batch.test}>{batch.network}{batch.test ? ' · тестовые деньги' : ' · настоящие деньги'}</dd></div>
            <div><dt>Комиссии сети</dt><dd class="num">{batch.feeMax ? 'до ' : ''}{ru(batch.fees)} {batch.feeSym ?? 'SOL'}</dd></div>
          </dl>
          {#each batch.warn ?? [] as w}<p class="hint warn">{w}</p>{/each}
          {#if !batch.known || batch.newRecipient}<p class="hint warn">{!batch.known ? 'Этого адреса нет среди ваших счетов и контактов. ' : ''}{batch.newRecipient ? 'В сети этот адрес ещё пустой. ' : ''}Сверьте его по буквам: отменить переводы нельзя.</p>{/if}
          <ol class="rows">
            {#each batch.rows as r, i (i)}
              <li><span><b>{r.name}</b><small class="mono">{shortA(r.from)}</small></span><span class="num">−{ru(r.amount)}{#if r.left}<small>останется {ru(r.left)}</small>{:else}<small>комиссия до {ru(r.fee)}</small>{/if}</span></li>
            {/each}
          </ol>
          <p class="hint">Подписано будет ровно это{batch.by === 'owner' ? '' : batch.by === 'addon' ? `: дополнение «${batch.byName}» не может поменять суммы или адрес` : ': агент не может поменять суммы или адрес'}.</p>
          {#if needTail}<label><span class="label">Последние 4 знака адреса — из вашего источника</span><span class="field"><input class="mono" bind:value={tail} maxlength="4" autocomplete="off" autocapitalize="off" spellcheck="false" disabled={!ready} /></span></label>{/if}
          {#if !batch.open}<label><span class="label">Пароль кошелька</span><span class="field"><input type="password" bind:value={p1} autocomplete="current-password" disabled={!ready} /></span></label>{/if}
          <div class="acts">
            <button class="btn" type="button" onclick={rejectBatch} disabled={busy}>Отклонить</button>
            <button class="btn solid go hold" type="button" style:--hold={hold} disabled={busy || !ready}
              onpointerdown={press} onpointerup={release} onpointerleave={release} oncontextmenu={e => e.preventDefault()}
              onkeydown={e => { if ((e.key === ' ' || e.key === 'Enter') && !e.repeat) { e.preventDefault(); press() } }}
              onkeyup={e => { if (e.key === ' ' || e.key === 'Enter') release() }}>
              <span>{busy ? 'Отправляем' : 'Удерживайте, чтобы отправить'}</span>
            </button>
          </div>
        {/if}
        {#if step !== 'start'}<p class="err" role="alert">{err}</p>{/if}
      </section>
    {/key}
  </div>
  </div>
</div>

<style>
  .veil { position: fixed; inset: 0; z-index: 30; background: rgb(7 7 10 / .62); animation: veil 240ms ease both; }
  .veil.soft { background: rgb(7 7 10 / .12); animation-duration: 560ms; } /* first launch: the far cards stay the background */
  @keyframes veil { from { opacity: 0; } }
  .sheet {
    position: fixed; z-index: 31; left: 50%; top: var(--top); translate: -50% 0; transition: top 420ms var(--ease);
    width: min(440px, calc(100vw - 32px)); will-change: transform, clip-path;
  }
  /* the plate changes height on its own; content swaps on top of it (opaque metal: no live blur over the canvas) */
  .plate {
    position: absolute; left: 0; right: 0; top: 0; height: min(var(--h), var(--max)); contain: strict;
    border-radius: 20px; background: var(--metal); box-shadow: var(--metal-edge), 0 0 0 1px var(--line), var(--lift);
    transition: height 420ms var(--ease);
  }
  /* waiting for the core (Argon2, network): ember light runs along the bottom edge, like the unlock capsule */
  .plate::after {
    content: ''; position: absolute; left: 20px; right: 20px; bottom: 0; height: 1px; opacity: 0; transition: opacity 300ms ease;
    background: linear-gradient(90deg, transparent, var(--ember), transparent) no-repeat; background-size: 40% 100%;
  }
  .busy .plate::after { opacity: 1; animation: scan 900ms linear infinite; }
  @keyframes scan { from { background-position: -60% 0; } to { background-position: 160% 0; } }
  .sheet { --max: calc(100vh - var(--top) - 24px); --ink-3: rgb(238 235 229 / .56); } /* small faint text on metal: 5.2:1 */
  .body { position: relative; max-height: var(--max); overflow: hidden; clip-path: inset(0 0 calc(100% - min(var(--h), var(--max))) 0 round 20px); transition: clip-path 420ms var(--ease); }
  .body.scroll { overflow-y: auto; scrollbar-width: thin; }
  .inner { position: relative; }
  .cp { justify-self: start; min-height: 30px; padding: 0 12px; border: 0; border-radius: 999px; background: none; color: var(--ink-2); font-size: 13px; cursor: pointer; box-shadow: inset 0 0 0 1px var(--line); transition: color 160ms ease, box-shadow 160ms ease; }
  .cp:hover { color: var(--ink); box-shadow: inset 0 0 0 1px rgb(238 235 229 / .3); }
  section { padding: 24px 24px 16px; display: grid; grid-template-columns: minmax(0, 1fr); gap: 16px; transform-origin: 50% 0; }
  header { display: flex; align-items: center; gap: 12px; }
  h2 { flex: 1; margin: 0; font-weight: 400; font-size: 24px; letter-spacing: -.02em; }
  .x { width: 32px; height: 32px; display: grid; place-items: center; border: 0; border-radius: 999px; background: none; color: var(--ink-3); cursor: pointer; transition: color 300ms ease, background-color 300ms ease; }
  .x:hover { color: var(--ink); background: rgb(238 235 229 / .06); }
  .x.lead { order: -1; margin: 0 -4px 0 -8px; } /* first launch: «back» sits before the title */
  .x:focus-visible { outline: 1px solid var(--ink-2); outline-offset: 2px; }
  label { display: grid; gap: 6px; }
  .area { padding: 12px 18px; align-items: start; }
  textarea { flex: 1; min-width: 0; background: none; border: 0; outline: none; resize: none; font-size: 15px; line-height: 1.55; color: var(--ink); }
  .field.ok { box-shadow: inset 0 0 0 1px rgb(238 235 229 / .7); }
  .amount.get { font-size: 28px; opacity: .9; } /* what arrives is a consequence of what leaves: one size down (critique 10.10) */
  .hint { margin: 0; font-size: 15px; line-height: 22px; color: var(--ink-2); text-wrap: pretty; }
  .picks { display: grid; gap: 4px; }
  .pick { display: grid; grid-template-columns: auto 1fr; column-gap: 10px; align-items: center; padding: 8px 10px; border-radius: 12px; cursor: pointer; transition: background-color 120ms ease; }
  .pick:hover, .pick:has(input:checked) { background: rgb(238 235 229 / .07); }
  .pick small { grid-column: 2; font-size: 12px; overflow-wrap: anywhere; }
  .warn { color: var(--ember); }
  .acts { display: flex; gap: 10px; margin-top: 4px; }
  .acts .go { flex: 1; justify-content: center; }
  .acts.stack { flex-direction: column; }
  .err { min-height: 18px; margin: -4px 0 0; font-size: 13px; color: var(--ember); transition: opacity 160ms ease; }
  .err:empty { opacity: 0; }
  .toggle { display: flex; align-items: center; gap: 10px; min-height: 36px; font-size: 15px; color: var(--ink-2); cursor: pointer; }
  .toggle input { accent-color: var(--ember); width: 16px; height: 16px; }
  .grid3 { display: grid; gap: 12px; }
  .key { display: grid; gap: 6px; }
  .keyv { padding: 10px 12px; border-radius: 12px; background: rgb(7 7 10 / .55); box-shadow: inset 0 0 0 1px var(--line); font-size: 13px; line-height: 20px; word-break: break-all; user-select: all; }

  /* the 12 words are dealt like cards into a 3x4 grid */
  .words { margin: 0; padding: 0; list-style: none; display: grid; grid-template-columns: repeat(3, 1fr); gap: 8px; }
  .words li {
    display: flex; gap: 8px; align-items: baseline; min-height: 40px; padding: 10px 12px; border-radius: 12px;
    background: rgb(7 7 10 / .55); box-shadow: inset 0 0 0 1px var(--line); user-select: text;
    animation: deal 420ms var(--ease) backwards; animation-delay: calc(var(--i) * 40ms + 120ms);
  }
  .words li.ghost { animation: none; opacity: .35; }
  .words .n { font-size: 12px; color: var(--ink-2); min-width: 14px; font-variant-numeric: tabular-nums; }
  @keyframes deal { from { opacity: 0; transform: translateY(12px) rotate(-6deg); } }

  .amount { margin: 2px 0 0; font-size: 48px; font-weight: 400; font-variant-numeric: tabular-nums; letter-spacing: -.03em; line-height: 1; overflow-wrap: anywhere; }
  .amount small { font-size: 16px; color: var(--ink-2); margin-left: 8px; letter-spacing: 0; }
  .sum { margin: 0; display: grid; gap: 8px; }
  .sum div { display: grid; grid-template-columns: 128px 1fr; gap: 10px; align-items: baseline; }
  .sum dt { font-size: 13px; color: var(--ink-3); }
  .sum dd { margin: 0; font-size: 15px; user-select: text; }
  .addr { font-size: 13px; overflow-wrap: anywhere; text-wrap: balance; line-height: 20px; } /* breaks between groups of four, never inside one */
  .faint { color: var(--ink-3); font-size: 12px; }
  .rows { margin: 0; padding: 0; list-style: none; display: grid; max-height: 220px; overflow-y: auto; border-top: 1px solid var(--line); }
  .rows li { display: flex; justify-content: space-between; gap: 12px; padding: 8px 0; border-bottom: 1px solid var(--line); font-size: 14px; }
  .rows li > span { display: grid; gap: 2px; }
  .rows li.have { opacity: .5; }
  .rows li > span:last-child { text-align: end; }
  .rows b { font-weight: 400; }
  .rows small { font-size: 12px; color: var(--ink-3); }

  /* hold to send: an ember fill grows under the label; release early and it drains */
  /* a phone's long press opens a menu and cancels the touch half way through the hold: no menu, no text selection */
  .hold { position: relative; overflow: hidden; touch-action: none; user-select: none; -webkit-user-select: none; -webkit-touch-callout: none; }
  .hold::before { content: ''; position: absolute; inset: 0; z-index: 0; opacity: 1; background: var(--ember); transform-origin: 0 50%; transform: scaleX(var(--hold, 0)); transition: none; } /* replaces the hover light */
  .hold span { position: relative; }

  /* phone: full width from the bottom edge, square at the bottom, clear of the gesture bar */
  .sheet.phone { left: 0; width: 100vw; translate: none; --max: calc(100vh - var(--top)); }
  .phone .plate { border-radius: 22px 22px 0 0; }
  .phone .body { clip-path: inset(0 0 calc(100% - min(var(--h), var(--max))) 0 round 22px 22px 0 0); }
  .phone section { padding: 20px 20px calc(16px + var(--bottom-inset)); }
  .phone .sum div { grid-template-columns: 112px 1fr; }
  .phone .x { width: 44px; height: 44px; margin-right: -8px; } /* close: a thumb's target */

  @media (prefers-reduced-motion: reduce) {
    .plate, .body, .sheet { transition: none; }
    .words li, .veil { animation: none; }
    .busy .plate::after { animation: none; background-size: 100% 100%; }
  }
</style>
