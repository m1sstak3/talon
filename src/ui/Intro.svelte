<script lang="ts" module>
  export type Kind = 'welcome' | 'locked' | 'open' | 'none'
</script>

<script lang="ts">
  // Intro «Точка, оборот, след» (brief 08.10, beats with Codex). A flat bone dot turns like a coin on the compositor
  // while the scene really loads; once ready it spins up and grows into the lead card, and at an edge-on instant the
  // 3D card takes over (same projection, angle and angular speed, same frame: hero renders on GSAP's ticker). The card
  // brakes face-on under one pass of light, the rest of the deck comes out from behind it, focus goes soft, and the
  // form unfolds from the card's trace in the centre.
  import { onMount } from 'svelte'
  import gsap from 'gsap'
  import type { Hero } from '../lib/hero'
  import { t } from '../lib/i18n.svelte'

  let { ready, kind, scene, ontraced, onopen, ondone }: {
    ready: Promise<unknown>; kind: Promise<Kind>; scene: () => Hero | null
    ontraced: () => void; onopen: (trace: { rect: DOMRect; r: number } | null) => void; ondone: () => void
  } = $props()

  let veil: HTMLDivElement, coin: HTMLDivElement, hello: HTMLDivElement
  const BONE = [238, 235, 229]
  const W0 = 10, SPIN0 = (2 * Math.PI) / 1.25 // the loading coin: one turn per 1.25 s (CSS below)
  const hex = (h: string) => [1, 3, 5].map(i => parseInt(h.slice(i, i + 2), 16))

  onMount(() => {
    const still = matchMedia('(prefers-reduced-motion: reduce)').matches
    const t0 = performance.now(), mark = (m: string) => { if (import.meta.env.DEV) console.warn(`[intro] ${m} +${Math.round(performance.now() - t0)}ms`) }
    const done = Promise.race([ready, new Promise(r => setTimeout(r, 6000))]).catch(() => {}) // a broken asset must not hold the door shut
    const live: gsap.core.Animation[] = []
    const frames = (n = 2) => new Promise<void>(r => { const f = () => (--n ? requestAnimationFrame(f) : r()); requestAnimationFrame(f) })

    ;(async () => {
      // the ceremony is for the first launch only (owner 08.10): later the deck is simply there and the veil lifts.
      // The vault's state answers in a few ms, so nothing is drawn before we know which launch this is.
      const k = await kind
      if (still || k !== 'welcome') {
        ontraced()
        // a turning dot only if loading takes a moment
        const dot = still ? 0 : window.setTimeout(() => { coin.animate([{ scale: 0 }, { scale: 1 }], { duration: 300, easing: 'ease-out', fill: 'both' }); coin.classList.add('spin') }, 250)
        await done; clearTimeout(dot); await frames(); mark(`ready ${k} (short)`)
        onopen(null)
        live.push(gsap.to(veil.parentElement, { opacity: 0, duration: still ? 0.2 : 0.55, ease: 'power1.inOut', onComplete: ondone })); return
      }
      // 1 · the dot, unhurried, and a greeting under it: the first launch takes its time
      await coin.animate([{ scale: 0 }, { scale: 1 }], { duration: 700, easing: 'cubic-bezier(.45,.05,.2,1)' }).finished
      ontraced()
      coin.classList.add('spin') // the app's heavy work starts now; the coin keeps turning on the compositor
      hello.classList.add('on')
      await Promise.all([done, new Promise(r => setTimeout(r, 2600))]) // read the greeting even on a fast machine
      const hero = scene()
      await frames() // the scene's first frames are on the canvas
      mark(`ready ${k}`)
      hello.classList.remove('on') // the words go as the coin starts to grow

      // 2 · take the coin over from wherever CSS left it, keeping its speed
      const m = new DOMMatrix(getComputedStyle(coin).transform)
      const a0 = Math.atan2(-m.m13, m.m11)
      const L = hero?.leadRect() ?? { w: innerHeight * 0.3, h: innerHeight * 0.44, r: innerHeight * 0.026, t: innerHeight * 0.016, f: innerHeight * 1.37, face: '#4d4e4f', edge: '#2e2e2e' }
      coin.parentElement!.style.perspective = `${L.f}px` // = the camera's focal length: CSS and 3D project the same
      const face = hex(L.face), edge = hex(L.edge)
      // spin-up: starts at the coin's speed, ends edge-on (a = pi/2 + n*pi) after at least ~0.6 turn
      const T1 = 0.9, aH = Math.PI / 2 + Math.ceil((a0 + 3.8 - Math.PI / 2) / Math.PI) * Math.PI
      const c = aH - a0 - SPIN0 * T1, spinEnd = SPIN0 + (2 * c) / T1
      // brake to face-on (a multiple of 2pi) with constant deceleration from spinEnd: pick the stop nearest ~0.42 s
      let aF = Math.ceil(aH / (2 * Math.PI)) * 2 * Math.PI
      while (2 * (aF - aH) / spinEnd < 0.3) aF += 2 * Math.PI
      const T2 = (2 * (aF - aH)) / spinEnd
      const glass = gsap.parseEase('glass')
      const p = { u: 0 }
      const paint = () => {
        const u = p.u, g = glass(u), a = a0 + SPIN0 * T1 * u + c * u * u
        const w = W0 + (L.w - W0) * g, h = W0 + (L.h - W0) * g, r = W0 / 2 + (L.r - W0 / 2) * g, t = (L.t * w) / L.w
        // white flat dot -> the card's own alloy, lit by how square it faces us
        const lit = 0.62 + 0.38 * Math.abs(Math.cos(a)), mix = (x: number[]) => x.map((v, i) => Math.round((BONE[i] + (v - BONE[i]) * g) * lit))
        const s = coin.style
        s.transform = `translate(-50%, -50%) rotateY(${a}rad)`
        s.width = `${w}px`; s.height = `${h}px`
        s.setProperty('--r', `${Math.min(r, w / 2)}px`); s.setProperty('--t', `${t}px`)
        s.setProperty('--c', `rgb(${mix(face)})`); s.setProperty('--e', `rgb(${mix(edge)})`)
      }
      coin.classList.remove('spin'); paint() // same frame: the coin never snaps back to 0 deg between CSS and GSAP
      const settled = new Promise<void>(res => {
        const tl = gsap.timeline({ onComplete: () => res() })
          .to(p, { u: 1, duration: T1, ease: 'none', onUpdate: paint }, 0)
          .to(veil, { opacity: 0, duration: T1, ease: 'power1.in' }, 0) // the room shows behind the growing card
        if (hero) {
          tl.add(() => { coin.style.visibility = 'hidden'; hero.birth.lit = true; mark('hand-over') }, T1)
            .fromTo(hero.birth, { a: aH }, { a: aF, duration: T2, ease: 'power1.out', immediateRender: false }, T1)
            .add(() => hero.glint(), T1 + 0.05)
        } else tl.to(coin, { opacity: 0, duration: 0.2, ease: 'none' }, T1) // no WebGL: the coin bows out, the form takes over
        live.push(tl)
      })
      await settled
      mark('settled')

      // 3 · the deck comes out from behind the card; the form unfolds from its trace
      const trace = { rect: new DOMRect(innerWidth / 2 - L.w / 2, innerHeight / 2 - L.h / 2, L.w, L.h), r: L.r }
      const out = hero?.release('scatter', 1.8) ?? Promise.resolve() // first launch: the far cloud
      live.push(gsap.delayedCall(1.1, () => { mark('open'); onopen(trace) })) // the sheet unfolds once the cloud has settled
      await out
      await new Promise(r => setTimeout(r, 520))
      mark('done'); ondone()
    })()
    return () => live.forEach(t => t.kill())
  })
</script>

<div class="intro" aria-hidden="true">
  <div class="veil" bind:this={veil}></div>
  <div class="stage"><div class="coin" bind:this={coin}><i class="f"></i><i class="b"></i><i class="l"></i><i class="r"></i></div></div>
  <div class="hello" bind:this={hello}><b>{t('intro.hello')}</b><span>{t('intro.sub')}</span></div>
</div>

<style>
  /* obsidian over the canvas until the card grows; the coin is a thin 3D box so its edge matches the GLB's at hand-over */
  .intro { position: fixed; inset: 0; z-index: 60; pointer-events: none; }
  .veil { position: absolute; inset: 0; background: var(--stage); }
  .stage { position: absolute; inset: 0; perspective: 1000px; }
  .coin {
    --r: 5px; --t: 0px; --c: var(--ink); --e: var(--ink);
    position: absolute; left: 50%; top: 50%; width: 10px; height: 10px;
    transform: translate(-50%, -50%); transform-style: preserve-3d; will-change: transform;
  }
  .coin i { position: absolute; display: block; backface-visibility: hidden; }
  .f, .b { inset: 0; border-radius: var(--r); background: var(--c); }
  .b { background: var(--e); } /* the back is bare metal, darker, like the model's */
  .f { transform: translateZ(calc(var(--t) / 2)); }
  .b { transform: rotateY(180deg) translateZ(calc(var(--t) / 2)); }
  .l, .r { top: var(--r); bottom: var(--r); width: var(--t); background: var(--e); }
  .l { left: calc(var(--t) / -2); transform: rotateY(-90deg); }
  .r { right: calc(var(--t) / -2); transform: rotateY(90deg); }
  /* loading: the flat coin turns, compositor only (smooth while the main thread compiles shaders) */
  /* first launch: a greeting under the turning dot, in slowly, out as the coin grows */
  .hello { position: absolute; left: 50%; top: calc(50% + 44px); translate: -50% 0; display: grid; gap: 10px; text-align: center; white-space: nowrap; opacity: 0; transform: translateY(8px); transition: opacity 700ms cubic-bezier(.45,.05,.2,1), transform 700ms cubic-bezier(.45,.05,.2,1); }
  .hello:global(.on) { opacity: 1; transform: none; transition-duration: 1100ms; }
  .hello b { font-family: var(--display, inherit); font-weight: 300; font-size: 30px; letter-spacing: -.01em; color: var(--ink); }
  .hello span { font-size: 14px; color: var(--ink-2); }
  .coin:global(.spin) { animation: coin 1.25s linear infinite; }
  @keyframes coin { from { transform: translate(-50%, -50%) rotateY(0deg); } to { transform: translate(-50%, -50%) rotateY(360deg); } }
</style>
