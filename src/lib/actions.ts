// sliding pad for segmented controls and chip rows (M-26): one pad, clipped to the active item, travels between
// choices. A move first stretches over both (200 ms), then lands on the new one (450 ms); wrapped rows work too.
export function pad(el: HTMLElement, active: number) {
  let last: number[] | null = null, timer = 0
  const box = (i: number) => {
    const item = el.querySelectorAll<HTMLElement>('[data-seg]')[i]
    if (!item) return null
    const l = item.offsetLeft, t = item.offsetTop
    return [t, el.clientWidth - l - item.offsetWidth, el.clientHeight - t - item.offsetHeight, l]
  }
  const set = (p: HTMLElement, b: number[] | null, ms: number) => {
    p.style.transitionDuration = `${ms}ms`
    p.style.clipPath = b ? `inset(${b[0]}px ${b[1]}px ${b[2]}px ${b[3]}px round 999px)` : 'inset(50% 50% 50% 50% round 999px)'
  }
  const place = (i: number, moved = false) => {
    const p = el.querySelector<HTMLElement>('.seg-pad'), b = box(i)
    if (!p) return
    clearTimeout(timer)
    const still = matchMedia('(prefers-reduced-motion: reduce)').matches
    if (moved && last && b && !still && last.some((v, k) => v !== b[k])) {
      set(p, last.map((v, k) => Math.min(v, b[k])), 200) // stretch over both
      timer = window.setTimeout(() => set(p, b, 450), 200)
    } else set(p, b, last ? 450 : 0)
    last = b
  }
  requestAnimationFrame(() => place(active))
  const ro = new ResizeObserver(() => place(active))
  ro.observe(el)
  return { update(i: number) { const moved = i !== active; active = i; place(i, moved) }, destroy() { ro.disconnect(); clearTimeout(timer) } }
}

// phone: a vertical touch drag that starts at the scroller's top edge — follows the finger 1:1 (M-08), `end` decides.
// Touch events, not pointer: the WebView would cancel a pointer drag as soon as it reads the gesture as a scroll.
function topDrag(el: HTMLElement, move: (dy: number) => void, end: (dy: number, fast: boolean) => void) {
  let y0 = 0, t0 = 0, dy = 0, on = false
  const start = (e: TouchEvent) => { on = el.scrollTop <= 0 && !(e.target as Element).closest('input, textarea, iframe'); y0 = e.touches[0].clientY; t0 = e.timeStamp; dy = 0 }
  const go = (e: TouchEvent) => {
    if (!on) return
    dy = e.touches[0].clientY - y0
    if (dy < 0 && dy > -6) return
    if (dy < 0 || el.scrollTop > 0) { on = false; move(0); return } // a scroll up: the list's own business
    e.preventDefault(); move(dy)
  }
  const stop = (e: TouchEvent) => { if (on) { on = false; end(dy, dy / Math.max(1, e.timeStamp - t0) > 0.6) } }
  el.addEventListener('touchstart', start, { passive: true })
  el.addEventListener('touchmove', go, { passive: false })
  el.addEventListener('touchend', stop); el.addEventListener('touchcancel', stop)
  return () => { el.removeEventListener('touchstart', start); el.removeEventListener('touchmove', go); el.removeEventListener('touchend', stop); el.removeEventListener('touchcancel', stop) }
}

// a bottom sheet is pulled down to close: past 80 px or a quick flick it goes, otherwise it springs back
export function dragDown(el: HTMLElement, close: () => void) {
  const off = topDrag(el, dy => { el.style.transition = 'none'; el.style.transform = dy ? `translateY(${dy}px)` : '' }, (dy, fast) => {
    el.style.transition = 'transform 320ms cubic-bezier(.32, .9, .18, 1)'
    if (dy > 80 || (fast && dy > 24)) close(); else el.style.transform = ''
  })
  return { destroy: off }
}

// pull to refresh, Android's way: the page stays, a round mark comes down from under the header with the finger (half
// speed, ≤ 72 px) and turns; let go past 56 px and `run` refreshes while it spins. No `run` (a screen with nothing to
// refresh): no pull. The mark reads `--pull-y`, `--pull` (0..1) and `.refreshing` from the scroller.
export function pullRefresh(el: HTMLElement, run: (() => unknown) | null) {
  let busy = false
  const set = (px: number) => { el.style.setProperty('--pull-y', `${px}px`); el.style.setProperty('--pull', String(Math.min(1, px / 56))) }
  const off = topDrag(el, dy => { if (!busy && run) set(Math.min(72, dy * 0.5)) }, async dy => {
    if (busy || !run) return
    if (dy * 0.5 < 56) { set(0); return }
    busy = true; el.classList.add('refreshing'); set(56); haptic('tick')
    try { await run() } finally { busy = false; el.classList.remove('refreshing'); set(0) }
  })
  return { update(r: (() => unknown) | null) { run = r }, destroy: off }
}

// Android system haptics (MainActivity: no permission, follows the touch-feedback setting); elsewhere nothing
export const haptic = (kind: 'ok' | 'err' | 'tick') => (window as unknown as { __cwApp?: { haptic(k: string): void } }).__cwApp?.haptic(kind)

// «Всё» and other filled-in amounts count into the field instead of appearing (owner 09.10); the exact text lands last
export function countInto(set: (v: string) => void, from: string, to: string, ms = 420) {
  const a = Number(from.replace(',', '.')) || 0, b = Number(to)
  const dec = (to.split('.')[1] ?? '').length
  if (!(b > 0) || matchMedia('(prefers-reduced-motion: reduce)').matches) { set(to.replace('.', ',')); return } // the screen writes 1,5
  const t0 = performance.now()
  const step = (now: number) => {
    const k = Math.min(1, (now - t0) / ms), e = 1 - (1 - k) ** 3
    if (k < 1) { set((a + (b - a) * e).toFixed(Math.min(dec, 6)).replace('.', ',')); requestAnimationFrame(step) } else set(to.replace('.', ','))
  }
  requestAnimationFrame(step)
}
