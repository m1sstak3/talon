import './app.css'
import { mount } from 'svelte'
import gsap from 'gsap'
import App from './App.svelte'

// the button hover light follows the pointer: one passive listener for the whole app, it only moves two numbers
addEventListener('pointermove', e => {
  const b = (e.target as Element | null)?.closest?.('.btn') as HTMLElement | null
  if (!b) return
  const r = b.getBoundingClientRect()
  b.style.setProperty('--mx', `${e.clientX - r.left}px`); b.style.setProperty('--my', `${e.clientY - r.top}px`)
}, { passive: true })

// dev only: ?slow=0.1 plays every GSAP timeline at that speed, to read motion frame by frame
if (import.meta.env.DEV) { const k = Number(new URLSearchParams(location.search).get('slow') ?? import.meta.env.VITE_SLOW); if (k > 0) gsap.globalTimeline.timeScale(k) }

// dev only: ?mock=welcome|locked|open runs on a fake core (src/dev/mock.ts), no real vault involved
const mock = import.meta.env.DEV ? new URLSearchParams(location.search).get('mock') : null
;(mock ? import('./dev/mock').then(m => m.install(mock)) : Promise.resolve()).then(() => mount(App, { target: document.getElementById('app')! }))
