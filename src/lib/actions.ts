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
