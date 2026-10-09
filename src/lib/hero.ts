// Home hero: every network is a metal card on a ring around us; the current one docks and fills the right half.
// It mirrors the wallet's state and never gates it (no WebGL, reduced motion: the wallet still works).
import * as THREE from 'three'
import { RoundedBoxGeometry } from 'three/examples/jsm/geometries/RoundedBoxGeometry.js'
import { GLTFLoader } from 'three/examples/jsm/loaders/GLTFLoader.js'
import { RectAreaLightUniformsLib } from 'three/examples/jsm/lights/RectAreaLightUniformsLib.js'
import { EffectComposer } from 'three/examples/jsm/postprocessing/EffectComposer.js'
import { RenderPass } from 'three/examples/jsm/postprocessing/RenderPass.js'
import { UnrealBloomPass } from 'three/examples/jsm/postprocessing/UnrealBloomPass.js'
import { OutputPass } from 'three/examples/jsm/postprocessing/OutputPass.js'
import { ShaderPass } from 'three/examples/jsm/postprocessing/ShaderPass.js'
import { Pass, FullScreenQuad } from 'three/examples/jsm/postprocessing/Pass.js'
import gsap from 'gsap'
import { CustomEase } from 'gsap/CustomEase'
import { encode } from 'uqr'
import { fmt, usd, short, type Card } from './data'
import { MARKS } from './marks'
import { lookFor, type Look } from './palettes'

gsap.registerPlugin(CustomEase)
CustomEase.create('glass', '.32,.9,.18,1') // the world's one curve: glass settling, no overshoot

const W = 1.5, H = 2.2, D = 0.08 // matches public/models/card.glb (assets-src/studio.blend, object Card): R .13, rounded edge .018 x 5
const BONE = '#eeebe5'
const reduced = matchMedia('(prefers-reduced-motion: reduce)')
const T = (s: number) => (reduced.matches ? Math.min(s, 0.2) : s)

export type Variant = 'glass' | 'frost'
export interface HeroEvents {
  pick?: (id: string) => void // a side card was clicked or the ring was dragged onto it
  open?: (id: string) => void // the current card was clicked
}

// ---------------------------------------------------------------- card faces (canvas textures)
const FONT_D = '"Manrope", system-ui, sans-serif'
const FONT_M = '"JetBrains Mono", ui-monospace, monospace'
const FONT_C = '"Manrope", system-ui, sans-serif' // card face (Codex review 07.10)
const CW = 1024, CH = 1502

function glyph(g: CanvasRenderingContext2D, c: Card, x: number, y: number, r: number, badge = true) {
  g.save()
  g.translate(x, y)
  g.lineWidth = 5
  g.globalAlpha = 0.9
  if (badge) { g.beginPath(); g.arc(0, 0, r, 0, Math.PI * 2); g.stroke() }
  const mark = MARKS[c.glyph]
  if (mark) { // official network mark (simple-icons, 24x24)
    const k = (badge ? r * 1.1 : r * 1.7) / 24
    g.scale(k, k); g.translate(-12, -12); g.fill(new Path2D(mark)); g.restore(); return
  }
  g.lineWidth = 7
  const k = r * 0.5
  switch (c.glyph) {
    case 'sol': for (const dy of [-0.55, 0, 0.55]) { g.beginPath(); g.moveTo(-k + 10, dy * k - 8); g.lineTo(k + 10, dy * k - 8); g.lineTo(k - 10, dy * k + 8); g.lineTo(-k - 10, dy * k + 8); g.closePath(); g.fill() } break
    case 'usdc': g.beginPath(); g.arc(0, 0, k * 1.15, Math.PI * 0.65, Math.PI * 1.35); g.stroke(); g.beginPath(); g.arc(0, 0, k * 1.15, -Math.PI * 0.35, Math.PI * 0.35); g.stroke(); g.beginPath(); g.moveTo(0, -k); g.lineTo(0, k); g.stroke(); break
    case 'jup': for (const s of [0.5, 0.85, 1.2]) { g.beginPath(); g.arc(-k * 0.4, k * 0.4, k * s, -Math.PI * 0.5, 0); g.stroke() } break
    case 'bonk': for (let i = 0; i < 7; i++) { const a = (i / 7) * Math.PI * 2; g.beginPath(); g.arc(Math.cos(a) * k * 0.75, Math.sin(a) * k * 0.75, 7, 0, Math.PI * 2); g.fill() } g.beginPath(); g.arc(0, 0, 10, 0, Math.PI * 2); g.fill(); break
    case 'jto': g.beginPath(); g.moveTo(0, -k); g.lineTo(k * 0.95, k * 0.7); g.lineTo(-k * 0.95, k * 0.7); g.closePath(); g.stroke(); break
    case 'pyth': g.beginPath(); g.arc(0, 0, k * 0.85, Math.PI * 0.25, Math.PI * 1.85); g.stroke(); g.beginPath(); g.moveTo(-k * 0.2, -k * 0.85); g.lineTo(-k * 0.2, k * 1.1); g.stroke(); break
    case 'ray': g.beginPath(); g.moveTo(0, -k); g.lineTo(k * 0.8, 0); g.lineTo(0, k); g.lineTo(-k * 0.8, 0); g.closePath(); g.stroke(); g.beginPath(); g.moveTo(0, -k); g.lineTo(0, k); g.stroke(); break
    case 'avax': g.beginPath(); g.moveTo(-k * 0.05, -k); g.lineTo(k * 0.95, k * 0.75); g.lineTo(-k * 1.05, k * 0.75); g.closePath(); g.fill() // Avalanche: a peak with the snow cut out
      g.globalCompositeOperation = 'destination-out'; g.beginPath(); g.moveTo(k * 0.2, k * 0.05); g.lineTo(k * 0.55, k * 0.75); g.lineTo(-k * 0.15, k * 0.75); g.closePath(); g.fill(); g.globalCompositeOperation = 'source-over'; break
    case 'eth': g.beginPath(); g.moveTo(0, -k * 1.2); g.lineTo(k * 0.7, 0); g.lineTo(0, k * 0.4); g.lineTo(-k * 0.7, 0); g.closePath(); g.stroke(); g.beginPath(); g.moveTo(-k * 0.7, k * 0.25); g.lineTo(0, k * 1.2); g.lineTo(k * 0.7, k * 0.25); g.stroke(); break
    default: g.font = `600 ${r}px ${FONT_D}`; g.textAlign = 'center'; g.textBaseline = 'middle'; g.fillText(c.sym[0], 0, 4)
  }
  g.restore()
}

// card face (Codex review 07.10): anodized metal, flat pale ink. The face atlas maps onto the GLB's own front face
// (UV = card rect), so the silhouette comes from one mesh: no plate, no ring, clean corners.
const PX = CW / W // px per model unit
const INK = '#dddcd7', INK2 = '#aeb0b2'

function print(g: CanvasRenderingContext2D, c: Card, l: Look) {
  const isNet = c.kind === 'net'
  const x = 0.14 * PX, my = 0.16 * PX + 40
  // the network mark carries the brand colour (Solana: its violet-to-teal), tests a quiet grey
  if (Array.isArray(l.mark)) { const gr = g.createLinearGradient(x - 6, my + 56, x + 106, my - 56); gr.addColorStop(0, l.mark[0]); gr.addColorStop(1, l.mark[1]); g.fillStyle = g.strokeStyle = gr }
  else g.fillStyle = g.strokeStyle = l.mark
  glyph(g, c, x + 50, my, 56, false)
  g.fillStyle = INK
  const title = isNet ? c.sym.replace(/ Chain$/, '') : c.sym
  g.font = `600 64px ${FONT_C}`
  g.textBaseline = 'middle'
  g.fillText(title, x + 132, 0.16 * PX + 44)
  if (l.test) { g.fillStyle = INK2; g.font = `400 34px ${FONT_C}`; g.fillText('ТЕСТОВАЯ СЕТЬ · НЕ НАСТОЯЩИЕ ДЕНЬГИ', x, 0.16 * PX + 150); g.fillStyle = INK }
  g.textBaseline = 'alphabetic'
  const main = c.amountText ?? (isNet ? usd(c.amount) : fmt(c.amount))
  g.font = `400 ${main.length > 12 ? 84 : 100}px ${FONT_C}`
  g.fillText(main, x, CH - 0.14 * PX - 64)
  g.fillStyle = INK2
  g.font = `400 46px ${FONT_M}`
  g.fillText(isNet ? (c.address ? short(c.address) : '') : c.name, x, CH - 0.14 * PX)
}
// CPU canvases (willReadFrequently): drawing costs a few ms more, but the cost lands in our own task at a moment we pick;
// GPU canvases were rastered by the browser 0.1-0.2 s later, one heavy frame per texture, mid-deal (measured 08.10)
function canvas2d(w = CW, h = CH, read = true) {
  const cv = document.createElement('canvas'); cv.width = w; cv.height = h
  return [cv, cv.getContext('2d', { willReadFrequently: read })!] as const
}

function faceMaps(c: Card, l: Look) {
  const [cv, g] = canvas2d()
  g.fillStyle = l.face // flat alloy: every gradient on the face comes from real light (Codex 07.10: no painted sheen)
  g.fillRect(0, 0, CW, CH)
  print(g, c, l)
  // metal/rough mask (G = roughness, B = metalness): ink is non-metal and matte, so it never turns to chrome
  const [mv, m] = canvas2d()
  m.fillStyle = 'rgb(0,102,230)'; m.fillRect(0, 0, CW, CH) // rough .40, metal .9
  m.save(); m.globalCompositeOperation = 'source-over'
  const [kv, k] = canvas2d(); print(k, c, l) // ink coverage (transparent background)
  m.globalCompositeOperation = 'destination-out'; m.drawImage(kv, 0, 0)
  m.globalCompositeOperation = 'destination-over'; m.fillStyle = 'rgb(0,184,0)'; m.fillRect(0, 0, CW, CH) // print: rough .72, metal 0
  m.restore()
  const mr = new THREE.CanvasTexture(mv); mr.colorSpace = THREE.NoColorSpace; mr.anisotropy = 8
  const [ev, e] = canvas2d()
  e.fillStyle = '#000'; e.fillRect(0, 0, CW, CH); e.drawImage(kv, 0, 0)
  kv.width = 0 // scratch canvas freed now
  return { map: tex(cv), mr, ink: tex(ev) }
}

function qrTexture(address: string, card: Card, net: string, note: string): THREE.CanvasTexture {
  const cv = document.createElement('canvas'); cv.width = CW; cv.height = CH
  const g = cv.getContext('2d')!
  const q = encode(address, { ecc: 'M', border: 0 })
  // owner 09.10: the card QR read soft. Whole-pixel square modules with no gaps (rounded, gapped modules melt to grey
  // when the texture is minified), as large as the card allows
  const n = q.size, m = Math.floor(780 / n), box = m * n, ox = Math.round((CW - box) / 2), oy = 200
  g.fillStyle = BONE
  g.beginPath(); g.roundRect(ox - 52, oy - 52, box + 104, box + 104, 40); g.fill()
  g.fillStyle = '#0b0b0e'
  for (let y = 0; y < n; y++) for (let x = 0; x < n; x++) if (q.data[y][x]) g.fillRect(ox + x * m, oy + y * m, m, m)
  // owner 09.10 (late): no address under the QR (it is on the left, with copy); the network it belongs to instead, with
  // its mark, and the one rule that loses money when broken
  const l = lookFor(card), ty = CH - 0.14 * PX - 120
  if (Array.isArray(l.mark)) { const gr = g.createLinearGradient(80, ty + 50, 180, ty - 50); gr.addColorStop(0, l.mark[0]); gr.addColorStop(1, l.mark[1]); g.fillStyle = g.strokeStyle = gr }
  else g.fillStyle = g.strokeStyle = l.mark
  glyph(g, card, 0.14 * PX + 44, ty, 48, false)
  g.fillStyle = INK
  g.textBaseline = 'middle'
  g.font = `600 64px ${FONT_C}`
  g.fillText(net, 0.14 * PX + 120, ty + 4)
  g.fillStyle = INK2
  g.font = `400 40px ${FONT_C}`
  g.fillText(note, 0.14 * PX, ty + 112)
  return tex(cv)
}

const icons = new Map<string, string>()
export function iconURL(c: Card): string {
  const k = c.glyph + (c.glyph === 'mono' ? c.sym : '')
  let u = icons.get(k)
  if (!u) {
    const cv = document.createElement('canvas'); cv.width = cv.height = 88
    const g = cv.getContext('2d')!
    g.fillStyle = g.strokeStyle = INK
    glyph(g, c, 44, 44, 40, false)
    icons.set(k, (u = cv.toDataURL()))
  }
  return u
}

function tex(cv: HTMLCanvasElement) {
  const t = new THREE.CanvasTexture(cv)
  t.colorSpace = THREE.SRGBColorSpace
  t.anisotropy = 8
  return t
}

// ---------------------------------------------------------------- backdrop: the room behind the cards, alive
// The room behind the cards, three rooms with volume that never stand still (owner 09.10 late: «Зал» read static and could
// not be chosen; picker in Settings → «Фон»). Motion in 3 words: «дыхание, дымка, лучи» — light travels slowly, haze drifts
// through it, the ring's turn pushes it. ref fxology-propfirm-video (light in doses, a room that breathes) + leil-saunas-ember-hero.
// All take a held-back tint of the network's mark; a send sends one ember ring from the docked card (a network switch does
// not: owner 08.10). Linear light before ACES; the finish pass dithers. `time` stops under reduced motion.
export const THEMES = ['hall', 'haze', 'rays'] as const
export type Theme = (typeof THEMES)[number]
const backdropMat = new THREE.ShaderMaterial({
  uniforms: {
    ember: { value: 0 }, gain: { value: 0.45 }, theme: { value: 0 }, narrow: { value: 0 }, // gain: 1 home, .45 locked / welcome; narrow: phone, text over the room
    time: { value: 0 }, pan: { value: 0 }, lean: { value: 0 }, wave: { value: 0 }, waveC: { value: new THREE.Color(1, 1, 1) }, tint: { value: new THREE.Color(0.9, 0.9, 0.9) },
  },
  vertexShader: `varying vec2 vUv; void main(){ vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position,1.); }`,
  fragmentShader: `
    varying vec2 vUv; uniform float ember, gain, time, pan, lean, wave, narrow; uniform vec3 waveC, tint; uniform int theme;
    float G(vec2 p, vec2 c, vec2 r){ vec2 d = (p - c) / r; return exp(-dot(d, d)); }
    float h1(float n){ return fract(sin(n * 127.1) * 43758.5453); }
    float h2(vec2 q){ return fract(sin(dot(q, vec2(127.1, 311.7))) * 43758.5453); }
    float n1(float x){ float i = floor(x); return mix(h1(i), h1(i + 1.), smoothstep(0., 1., fract(x))); }
    float n2(vec2 q){ vec2 i = floor(q), f = fract(q); f = f * f * (3. - 2. * f);
      return mix(mix(h2(i), h2(i + vec2(1., 0.)), f.x), mix(h2(i + vec2(0., 1.)), h2(i + 1.), f.x), f.y); }
    float fbm(vec2 q){ float s = 0., a = .5; for (int i = 0; i < 4; i++) { s += a * n2(q); q = q * 2.03 + vec2(1.7, 9.2); a *= .5; } return s / .94; }
    void main(){
      vec2 p = (vUv - .5) * vec2(120., 68.); // metres on the wall; the docked card is in front of (0, 0)
      float t = time;
      vec3 hue = mix(vec3(.92, .9, .86), tint, .22); // the network's colour, held back: a room, not a logo
      float room = G(p, vec2(0.), vec2(15., 8.5)); // lit around the card, dark towards the edges
      // the studio key light the owner liked stays as the base under every theme; it leans with the ring's turn
      vec3 e = .65 * (vec3(.070, .068, .064) * G(p, vec2(-3. + lean, 2.1), vec2(5.4, 3.9)) + vec3(.020, .022, .026) * G(p, vec2(3.8 + lean * .6, -1.8), vec2(4., 3.2)));
      // the stage every room shares (owner 09.10: VOLUME, not patterns): real view rays through this wall pixel give a
      // matte floor under the ring, a soft cove, light high on the far wall (Codex: cyclorama + aerial perspective).
      // Alive (owner, later: «статичный»): the light wanders (~90 s), breathes (~15 s), haze drifts through it.
      // No hard edges, no specks; pan = ring position, lean = turn speed (settles back).
      vec3 ro = vec3(0., .1, 0.), rd = normalize(vec3(p, -16.) - ro);
      float fy = -1.75, tf = rd.y < -1e-4 ? (fy - ro.y) / rd.y : 1e4;
      vec3 hp = ro + rd * tf;
      if (hp.z < -16. || tf > 1e3) hp = vec3(p.x, fy, -16.); // behind the wall: the cove
      float lx = sin(pan * .35) * 1.4 - lean * .9 + sin(t * .21) * 1.3, sx = 5.2 * (1. + abs(lean) * .3); // the light wanders, ~30 s
      float breathe = .8 + .2 * sin(t * .5);
      float hz = fbm(hp.xz * vec2(.16, .3) + vec2(t * .11 + pan * .12, -t * .04)); // haze crossing the light on the floor
      vec2 d = (hp.xz - vec2(lx, -5.)) / vec2(sx, 3.6);
      float pool = (exp(-dot(d, d)) + .35 * exp(-dot(d, d) * .18)) * breathe * (.68 + .55 * hz); // wide penumbra, never a disc
      float sheen = exp(-pow((hp.x - lx * .5) / 1.9, 2.)) * smoothstep(-20., -5., hp.z) * .5;
      float fog = exp(-.055 * length(hp - ro));
      float amt = smoothstep(-1.1, -2.6, p.y); // floor meets wall in a soft cove, no horizon line
      float cove = exp(-pow((p.y + 1.75) / 2.4, 2.)) * G(p, vec2(lx * 2., 0.), vec2(18., 30.));
      float back = G(p, vec2(2.5 + lx, 3.5), vec2(11., 7.)) * breathe;
      float air = fbm(p * vec2(.07, .12) + vec2(t * .06 + pan * .06, t * .02)); // far air, slowly moving
      vec3 wall = e * (.55 + .45 * smoothstep(9., -1., p.y)) + hue * back * (.018 + .05 * air);
      vec3 floorC = mix(wall, hue * (.13 * pool + .035 * sheen), fog); // the far floor fades into the wall's air: no dark seam at the horizon
      if (theme == 1) { // Дымка: low smoke rolls across the stage in three depths, lit from behind the ring
        vec3 smoke = vec3(0.); float cover = 0.;
        for (int i = 0; i < 3; i++) {
          float k = float(i), sc = .11 + k * .05, v = .05 + k * .035;
          float top = 1.2 - k * 1.3; // far layers rise higher on screen, near ones hug the floor
          float dn = mix(smoothstep(-24., -9., p.x), 1., narrow) * smoothstep(top, top - 5.5, p.y) * smoothstep(.32, .78, fbm(vec2(p.x * sc + t * v + pan * (.08 + k * .07), p.y * sc * 1.9 - t * .02 + k * 3.1)));
          vec3 lit = hue * (.05 + .1 * G(p, vec2(lx * 1.5 - 2. + k * 2., -.5), vec2(15. - k * 2., 7.))) * breathe;
          smoke += (1. - cover) * dn * lit; cover += (1. - cover) * dn * .55;
        }
        wall = mix(wall, wall * .8, cover);
        e = mix(wall, floorC, amt) * (1. - cover * .5) + smoke + hue * .025 * cove;
      } else if (theme == 2) { // Лучи: shafts from a high window behind the ring sweep slowly through the haze
        // critique 09.10: a point source read as a stage sunburst. A wide window far up and to the left: the shafts run
        // nearly parallel across the room, a few at a time, and drift as the clouds outside would
        vec2 src = vec2(-34. + lx * 2. + sin(t * .05) * 4., 62.), dv = p - src;
        float ang = atan(dv.x, -dv.y), len = length(dv);
        float sh = pow(n1(ang * 30. + t * .05 + pan * .04), 2.5) * (.4 + .6 * n1(ang * 64. - t * .08));
        float fall = smoothstep(105., 55., len) * smoothstep(-7., 3., p.y + 2.); // the shafts fade into the floor, never stop on a line
        float rays = sh * fall * (.45 + .9 * air) * mix(smoothstep(-26., -6., p.x), 1., narrow);
        e = mix(wall * .7, floorC * (.75 + 1.4 * rays), amt) + hue * (.13 * rays * (1. - amt * .5) * (1. + .8 * narrow) + .028 * cove);
      } else { // Зал: the stage itself, its air moving
        e = mix(wall, floorC, amt) + hue * .032 * cove;
      }
      e *= 1. - .55 * narrow; // phone: the column lies over the room, keep it quiet under the text
      // a switch or a send: one ring leaves the docked card and crosses the room
      float r = wave * 26., ring = exp(-pow((length(p * vec2(1., 1.2)) - r) / 1.8, 2.)) * (1. - wave) * step(.0005, wave);
      e += waveC * .14 * ring * (.35 + .65 * room);
      vec3 c = (.72 + .28 * G(p, vec2(0.), vec2(12., 7.))) * (vec3(.00212, .00212, .00304) + gain * e);
      c += vec3(.30, .08, .02) * exp(-dot(p / vec2(4., 4.6), p / vec2(4., 4.6)) * 2.2) * ember;
      gl_FragColor = vec4(c, 1.);
    }`,
  depthWrite: false,
})

// rack focus: the deck alone (layer CARDS) is rendered into its own target, blurred, composited over the backdrop.
// Full resolution, so at zero blur it is the very same picture and the pass can switch off without a pop.
const CARDS = 1
const BLUR = {
  uniforms: { tex: { value: null as THREE.Texture | null }, dir: { value: new THREE.Vector2() } },
  vertexShader: `varying vec2 vUv; void main(){ vUv = uv; gl_Position = vec4(position.xy, 0., 1.); }`,
  fragmentShader: `
    uniform sampler2D tex; uniform vec2 dir; varying vec2 vUv;
    void main(){ // 9-tap gaussian on premultiplied colour
      vec4 c = texture2D(tex, vUv) * .2270270270;
      c += (texture2D(tex, vUv + dir) + texture2D(tex, vUv - dir)) * .1945945946;
      c += (texture2D(tex, vUv + dir * 2.) + texture2D(tex, vUv - dir * 2.)) * .1216216216;
      c += (texture2D(tex, vUv + dir * 3.) + texture2D(tex, vUv - dir * 3.)) * .0540540541;
      c += (texture2D(tex, vUv + dir * 4.) + texture2D(tex, vUv - dir * 4.)) * .0162162162;
      gl_FragColor = c;
    }`,
}
class DeckPass extends Pass {
  amount = 0 // 0 sharp .. 1 far and soft
  private full: THREE.WebGLRenderTarget
  private a: THREE.WebGLRenderTarget
  private b: THREE.WebGLRenderTarget
  private blur = new THREE.ShaderMaterial({ ...BLUR, uniforms: THREE.UniformsUtils.clone(BLUR.uniforms), depthTest: false, depthWrite: false })
  private comp = new THREE.ShaderMaterial({
    uniforms: { tex: { value: null as THREE.Texture | null } },
    vertexShader: BLUR.vertexShader,
    fragmentShader: `uniform sampler2D tex; varying vec2 vUv; void main(){ gl_FragColor = texture2D(tex, vUv); }`,
    transparent: true, depthTest: false, depthWrite: false,
    blending: THREE.CustomBlending, blendSrc: THREE.OneFactor, blendDst: THREE.OneMinusSrcAlphaFactor,
  })
  private quad = new FullScreenQuad(this.blur)
  private w = 1
  private h = 1
  constructor(private scene: THREE.Scene, private camera: THREE.Camera) {
    super()
    this.needsSwap = false
    const o = { type: THREE.HalfFloatType }
    this.full = new THREE.WebGLRenderTarget(1, 1, { ...o, samples: 4 })
    this.a = new THREE.WebGLRenderTarget(1, 1, o)
    this.b = new THREE.WebGLRenderTarget(1, 1, o)
  }
  setSize(w: number, h: number) { this.w = w; this.h = h; for (const t of [this.full, this.a, this.b]) t.setSize(w, h) }
  render(r: THREE.WebGLRenderer, _w: THREE.WebGLRenderTarget, read: THREE.WebGLRenderTarget) {
    const bg = this.scene.background, clear = r.getClearAlpha(), auto = r.autoClear
    this.scene.background = null
    this.camera.layers.set(CARDS)
    r.setClearAlpha(0)
    r.setRenderTarget(this.full); r.clear(); r.render(this.scene, this.camera)
    this.scene.background = bg; r.setClearAlpha(clear)
    const step = this.amount * 2.4 // device px between taps: up to ~10 px of reach
    this.quad.material = this.blur
    this.blur.uniforms.tex.value = this.full.texture; this.blur.uniforms.dir.value.set(step / this.w, 0)
    r.setRenderTarget(this.a); this.quad.render(r)
    this.blur.uniforms.tex.value = this.a.texture; this.blur.uniforms.dir.value.set(0, step / this.h)
    r.setRenderTarget(this.b); this.quad.render(r)
    this.quad.material = this.comp
    this.comp.uniforms.tex.value = this.b.texture
    r.autoClear = false
    r.setRenderTarget(read); this.quad.render(r)
    r.autoClear = auto
  }
}

// last pass, in display space: the left veil behind the wallet column (was a CSS gradient: DOM gradients band) and a
// static interleaved-gradient-noise dither of one 8-bit step, so no gradient on screen shows steps
const FINISH = {
  uniforms: { tDiffuse: { value: null }, veil: { value: 0.46 }, veilK: { value: 0 } }, // veilK: 0 centred scene, 1 home
  vertexShader: `varying vec2 vUv; void main(){ vUv = uv; gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.); }`,
  fragmentShader: `
    uniform sampler2D tDiffuse; uniform float veil, veilK; varying vec2 vUv;
    void main(){
      vec4 c = texture2D(tDiffuse, vUv);
      float x = vUv.x / veil;
      float a = x < .62 ? mix(.94, .82, x / .62) : mix(.82, 0., smoothstep(0., 1., (x - .62) / .38));
      c.rgb = mix(c.rgb, vec3(7., 7., 10.) / 255., a * step(x, 1.) * veilK);
      c.rgb += (fract(52.9829189 * fract(dot(gl_FragCoord.xy, vec2(.06711056, .00583715)))) - .5) / 255.;
      gl_FragColor = vec4(clamp(c.rgb, 0., 1.), c.a);
    }`,
}

interface Slot {
  card: Card
  key: string // face content version: the face is repainted only when this changes
  slot: THREE.Group // ring pose (or stack pose while gathered)
  fly: THREE.Group // throw offsets
  flip: THREE.Group // receive flip / swap turn
  pivot: THREE.Group // hover tilt + idle sway
  body: THREE.Mesh
  face: THREE.Mesh
  back: THREE.Mesh
  tilt: THREE.Vector2
  phase: number
  bob: number
  spread: { v: number } // 0 = in the deck (stack), 1 = at its place on the ring
  born: { v: number } // intro: 0 = stacked behind the spinning lead card, 1 = its own pose
  deferred: boolean // face not painted yet (back card of the locked deck): plain metal until paintRest
}

// The scene (owner 07.10, final idea): we stand in the middle of a ring of network cards.
// Locked: the deck drifts slowly in front of us. Typing the password loosens it; unlock deals the cards out
// around us, then the current network's card glides in and fills most of the right half. Neighbours stay
// out of view; switching turns the ring. The camera barely moves; only the backdrop breathes.
const RI = 5.4 // ring radius around the camera
const STEP = 1.06 // radians between cards: neighbours sit outside the frame (0.92 let the left one peek in behind the column, 08.10)
const DOCK = 1.55 // how far the current card comes toward us when docked
const DECK_Z = -7.2 // where the locked deck floats
const BIRTH_Z = 7.2 // intro (brief 08.10): the lead card spins at the deck's depth, dead centre of the screen

function ringPose(d: number, docked: number) {
  const a = d * STEP, r = RI - Math.max(0, 1 - Math.abs(d)) * DOCK * docked
  return { x: Math.sin(a) * r, y: 0, z: -Math.cos(a) * r, ry: -a, show: Math.abs(a) < 2.2 }
}
// welcome (no wallet yet): the deck hangs scattered in the distance, each card drifting on its own slow loop
const SCATTER = [ // x, y, z, ry, rz — hand-placed around the scene's centre, deeper ones further out
  [0.15, 0.05, -7.4, 0.28, -0.05], [-1.75, 0.95, -9.3, 0.55, 0.12], [1.85, -0.85, -8.6, -0.42, -0.10],
  [-1.25, -1.2, -10.4, 0.36, 0.20], [2.35, 1.1, -11.2, -0.62, 0.06], [-2.9, -0.15, -12.2, 0.72, -0.14],
  [0.95, 1.65, -13.0, -0.25, 0.16], [3.2, 0.1, -13.8, -0.8, -0.08], [-0.6, -1.9, -12.8, 0.15, -0.18],
]
function scatterPose(i: number, t: number) {
  const p = SCATTER[i % SCATTER.length], w = 0.2618 / (1 + (i % 4) * 0.35), ph = i * 1.9 // 24-49 s loops, out of step
  // pushed back and opened out: far, small, many (owner 07.10: «далеко, без резкости»)
  return { x: p[0] * 1.7 + Math.sin(t * w + ph) * 0.12, y: p[1] * 1.45 + Math.sin(t * w * 0.8 + ph * 1.3) * 0.09, z: p[2] * 1.55, ry: p[3] + Math.sin(t * w + ph) * 0.05, rz: p[4] }
}
function deckPose(i: number, t: number, loose: number) {
  // a stack that breathes as one piece. Cards are spaced deeper than their thickness and fan only in their own
  // plane (rz), so they can never cut through each other; `loose` (0..1) opens the fan as the password is typed
  const o = i - 1.5
  return {
    x: o * 0.07 * loose,
    y: -0.08 + Math.sin(t * 0.2618) * 0.025, // one loop, 24 s period
    z: DECK_Z - i * 0.13,
    ry: 0.12 + Math.sin(t * 0.2618 + 1.3) * 0.0175,
    rz: -0.02 + o * (0.012 + 0.05 * loose),
  }
}
const contentKey = (c: Card, v: Variant) => [c.id, c.sym, c.amount, c.amountText, c.address, c.tint, v].join('|')

export function createHero(canvas: HTMLCanvasElement, on: HeroEvents = {}) {
  const renderer = new THREE.WebGLRenderer({ canvas, antialias: true, powerPreference: 'high-performance' })
  // DPR capped (Codex 07.10: 165 Hz budget); thin rims stay clean through the 4x MSAA targets
  const DPR = Math.min(devicePixelRatio, innerWidth < 860 ? 2 : 1.25) // phones: sharp card print at a phone's density; desktop keeps the 165 Hz budget
  renderer.setPixelRatio(DPR)
  renderer.toneMapping = THREE.ACESFilmicToneMapping
  renderer.toneMappingExposure = 1.05
  renderer.outputColorSpace = THREE.SRGBColorSpace

  const scene = new THREE.Scene()
  scene.background = new THREE.Color('#07070a')
  const fog = new THREE.FogExp2('#07070a', 0) // depth for the scattered welcome deck: far cards sink into obsidian
  scene.fog = fog
  // one coherent light (Codex 07.10, owner: «странно смотрится тень и источник света»): a uniform neutral environment
  // gives the metal its brightness without phantom softboxes; the three area lights below are the only shapes it reflects
  const pmrem = new THREE.PMREMGenerator(renderer)
  const envScene = new THREE.Scene()
  envScene.background = new THREE.Color().setRGB(0.18, 0.18, 0.18, THREE.LinearSRGBColorSpace)
  scene.environment = pmrem.fromScene(envScene, 0).texture
  scene.environmentIntensity = 0.6
  const envReady = Promise.resolve()

  const camera = new THREE.PerspectiveCamera(40, 1, 0.1, 100)
  camera.position.set(0, 0.1, 0)

  RectAreaLightUniformsLib.init()
  const C = new THREE.Vector3(0, 0, -RI + DOCK) // the docked card
  const key = new THREE.RectAreaLight('#fff8f0', 4, 4, 4); key.position.set(-1.4, 1.8, -1.3); key.lookAt(C); scene.add(key)
  const fill = new THREE.RectAreaLight('#ffffff', 0.8, 3, 4); fill.position.set(1.8, 0.4, -1.5); fill.lookAt(C); scene.add(fill)
  const strip = new THREE.RectAreaLight('#fff8f0', 1.8, 0.45, 4.5) // the one light that moves: a slow band across the docked card
  strip.position.set(0.65, 0.15, -1.8)
  scene.add(strip)
  for (const l of [key, fill, strip]) l.layers.enableAll() // lights reach the deck layer too

  const backdrop = new THREE.Mesh(new THREE.PlaneGeometry(120, 68), backdropMat) // wider than the phone's tall camera sees (it showed a dark strip at the bottom)
  backdrop.position.set(0, 0, -16)
  scene.add(backdrop)

  const ring = new THREE.Group()
  scene.add(ring)

  const composer = new EffectComposer(renderer, new THREE.WebGLRenderTarget(1, 1, { type: THREE.HalfFloatType, samples: 4 }))
  composer.addPass(new RenderPass(scene, camera))
  const deckPass = new DeckPass(scene, camera)
  composer.addPass(deckPass)
  composer.addPass(new UnrealBloomPass(new THREE.Vector2(1, 1), 0.12, 0.4, 1.0)) // print stays under the threshold
  composer.addPass(new OutputPass())
  const finish = new ShaderPass(FINISH)
  composer.addPass(finish)
  const focus = { v: 1 } // 1 = deck far and soft (locked / welcome), 0 = sharp

  // until the GLB arrives: a rounded box body and a flat face plane; then the model's own body + front face
  let bodyGeo: THREE.BufferGeometry = new RoundedBoxGeometry(W, H, D, 6, 0.12)
  let faceGeo: THREE.BufferGeometry = new THREE.PlaneGeometry(W - 0.02, H - 0.02).translate(0, 0, D / 2 + 0.001)
  const glbReady = new Promise<void>(res => new GLTFLoader().load('/models/card.glb', g => {
    const parts: THREE.Mesh[] = []
    g.scene.traverse(o => { if ((o as THREE.Mesh).isMesh) parts.push(o as THREE.Mesh) })
    const isFront = (m: THREE.Mesh) => { const n = m.geometry.getAttribute('normal'); return n.count > 0 && n.getZ(0) > 0.99 && !!m.geometry.getAttribute('uv') }
    // the model names its parts by material: Face = the flat printed front, Body = everything else
    const front = parts.find(m => (m.material as THREE.Material).name === 'Face') ?? parts.find(isFront), body = parts.find(m => m !== front)
    if (!front || !body) return res()
    // glTF UVs assume flipY=false; our canvas textures are flipY=true, so mirror v once here
    const uv = front.geometry.getAttribute('uv')
    for (let i = 0; i < uv.count; i++) uv.setY(i, 1 - uv.getY(i))
    bodyGeo = body.geometry; faceGeo = front.geometry
    for (const s of slots) { s.body.geometry = bodyGeo; s.face.geometry = faceGeo }
    res()
  }, undefined, () => res()))
  const backGeo = new THREE.PlaneGeometry(W - 0.04, H - 0.04)

  let slots: Slot[] = []
  let variant: Variant = 'glass'
  let frozen = false, locked = true, ember = false
  let held = false // current card flipped/thrown: keep sway off
  let dealing = false // the deck is flying out: no face is repainted mid-flight (one face = ~100 ms of paint + upload)
  let deferFaces = false // prepare: back cards start as plain metal, painted after the intro
  let resting = false, restTimer = 0 // a sheet covers the deck: no WebGL work at all, the last frame stays on screen
  const mouse = new THREE.Vector2(), mouseS = new THREE.Vector2()
  const ringPos = { v: 0 } // index of the card in front (float while turning)
  const dock = { v: 0 } // 0 = cards on the ring, 1 = current card docked in front
  const loose = { v: 0 } // deck loosening while the password is typed
  const scatter = { v: 0 } // 1 = welcome: cards scattered in the distance
  const shift = { v: 0 } // 0 = scene centred (intro, welcome, lock), 1 = home: docked card at 72%, wallet column left
  // intro: the CSS coin hands over to the lead card edge-on; `a` is its spin (rad), driven by the intro's timeline
  const birth = { on: false, lit: false, peel: false, a: 0 }
  const haze = { v: 1 } // fog share: 0 while the lead card spins up close, 1 for the far cloud
  const glint = { x: 0, w: 0 } // one pass of the strip light across the landing card
  let posTween: gsap.core.Tween | null = null
  let settle: (() => void) | null = null // resolves the turn in flight, also when a newer turn replaces it
  let dragging = false, downX = 0, downPos = 0, lastX = 0, lastT = 0, moved = 0, vel = 0

  // each network its own smoked alloy (palettes.ts)
  function faceMaterial(card: Card) {
    const { map, mr, ink } = faceMaps(card, lookFor(card))
    for (const t of [map, mr, ink]) renderer.initTexture(t) // upload now, not mid-animation
    // brushed anodized face: anisotropic highlights stretch along the brushing (card height) and travel with the strip
    return new THREE.MeshPhysicalMaterial({
      map, roughnessMap: mr, metalnessMap: mr, roughness: 1, metalness: 1, 
      emissiveMap: ink, emissive: '#ffffff', emissiveIntensity: 0.025, // matte print, no glow (Codex 07.10)
      envMapIntensity: 0.6,
    })
  }
  // owner 09.10 (late): once the model loaded (CSP fix) its darker polished bevel read as an outline drawn round the
  // face. The edge is the same alloy as the face now: one block of metal, light alone tells the bevel
  function bodyMaterial(card: Card) {
    return new THREE.MeshStandardMaterial({
      color: new THREE.Color(lookFor(card).face), metalness: 0.9, roughness: 0.4,
      envMapIntensity: 0.8,
    })
  }
  const ink = (map: THREE.Texture | null = null) => new THREE.MeshBasicMaterial({ map, alphaTest: 0.02, alphaToCoverage: true, toneMapped: false })

  function freeSlot(s: Slot) {
    s.slot.removeFromParent()
    ;(s.body.material as THREE.Material).dispose()
    for (const m of [s.face, s.back]) { const mm = m.material as THREE.MeshStandardMaterial; mm.map?.dispose(); mm.roughnessMap?.dispose(); mm.emissiveMap?.dispose(); mm.dispose() }
  }

  function makeSlot(card: Card, i: number): Slot {
    const slot = new THREE.Group(), fly = new THREE.Group(), flip = new THREE.Group(), pivot = new THREE.Group()
    const body = new THREE.Mesh(bodyGeo, bodyMaterial(card))
    const face = new THREE.Mesh(faceGeo, deferFaces && i >= 2 ? bodyMaterial(card) : faceMaterial(card))
    const back = new THREE.Mesh(backGeo, ink())
    back.position.z = -D / 2 - 0.004
    back.rotation.y = Math.PI
    back.visible = false
    for (const m of [face, back, body]) m.layers.set(CARDS)
    pivot.add(face, back, body); flip.add(pivot); fly.add(flip); slot.add(fly)
    ring.add(slot)
    const deferred = deferFaces && i >= 2
    return { card, key: deferred ? '' : contentKey(card, variant), slot, fly, flip, pivot, body, face, back, tilt: new THREE.Vector2(), phase: i * 1.7, bob: 1, spread: { v: 0 }, born: { v: 1 }, deferred }
  }

  // now: the card is out of sight this very moment (throw, swap). Otherwise the face waits for a calm scene and is
  // painted with paintRest, one per idle slot: a balance landing mid-deal never costs a frame (owner 08.10: start lag)
  function setFace(s: Slot, card: Card, now = false) {
    s.card = card
    if (contentKey(card, variant) === s.key) return
    if (now) paint(s); else { s.deferred = true; paintRest() }
  }
  function paint(s: Slot) {
    s.deferred = false
    s.key = contentKey(s.card, variant)
    const card = s.card
    const old = s.face.material as THREE.MeshStandardMaterial
    for (const t of [old.map, old.roughnessMap, old.emissiveMap]) { if (t?.image instanceof HTMLCanvasElement) t.image.width = 0; t?.dispose() } // canvases freed now, not by the collector
    old.dispose()
    s.face.material = faceMaterial(card)
    ;(s.body.material as THREE.MeshStandardMaterial).color.set(lookFor(card).face)
  }

  // reuse slots by id, repaint only changed faces, keep order = ring order
  function build(cards: Card[]) {
    const byId = new Map(slots.map(s => [s.card.id, s]))
    const next: Slot[] = cards.map((c, i) => {
      const s = byId.get(c.id)
      if (s) { byId.delete(c.id); setFace(s, c); return s }
      return makeSlot(c, i)
    })
    for (const s of byId.values()) freeSlot(s)
    slots = next
    slots.forEach((s, i) => (s.body.userData.i = s.face.userData.i = i))
  }

  const N = () => Math.max(1, slots.length)
  // signed ring offset of slot i from the float position, wrapped to the shortest side
  function offset(i: number) {
    const n = N()
    let d = (i - ringPos.v) % n
    if (d > n / 2) d -= n
    if (d < -n / 2) d += n
    return d
  }
  const current = () => slots[((Math.round(ringPos.v) % N()) + N()) % N()]

  function turnTo(i: number): Promise<void> {
    posTween?.kill()
    settle?.() // a newer turn replaces the old one: its awaiters must not hang (that froze rapid clicking)
    const n = N(), from = ringPos.v
    let d = (i - from) % n
    if (d > n / 2) d -= n
    if (d < -n / 2) d += n
    if (Math.abs(d) < 1e-3) return Promise.resolve()
    count('turn')
    // the card swings out of frame, the next one swings in; long jumps are not slower
    const t = T(0.48)
    return new Promise(res => {
      settle = () => { settle = null; res() }
      posTween = gsap.to(ringPos, { v: from + d, duration: t, ease: 'glass', onComplete: () => { ringPos.v = ((Math.round(ringPos.v) % n) + n) % n; settle?.() } })
    })
  }

  function gather(): Promise<void> {
    dealing = false // a lock mid-deal: the faces may paint again once the deck rests
    return new Promise(res => {
      gsap.to(dock, { v: 0, duration: T(0.5), ease: 'power2.inOut' })
      gsap.to(slots.map(s => s.spread), { v: 0, duration: T(0.7), ease: 'power2.inOut', stagger: 0.03, delay: T(0.2), onComplete: () => res() })
    })
  }
  // unlock: the deck flies out around us, slowly; then the current card glides in
  function deal(slow = true): Promise<void> {
    count('deal')
    dealing = true
    gsap.to(loose, { v: 0, duration: T(0.6) })
    const order = [...slots].sort((a, b) => Math.abs(offset(slots.indexOf(b))) - Math.abs(offset(slots.indexOf(a)))) // far ones leave first
    return new Promise(res => {
      gsap.to(order.map(s => s.spread), {
        v: 1, duration: T(slow ? 1.5 : 0.8), ease: 'power2.inOut', stagger: T(slow ? 0.07 : 0.03),
        onComplete: () => { gsap.to(dock, { v: 1, duration: T(slow ? 1.1 : 0.6), ease: 'glass', onComplete: () => { dealing = false; res() } }) },
      })
    })
  }

  // ---------------------------------------------------------------- input: heavy drag turns the ring, snaps to a card
  const ray = new THREE.Raycaster()
  ray.layers.enableAll() // cards live on their own layer
  let hovered: Slot | null = null
  function ndc(e: PointerEvent) {
    const r = canvas.getBoundingClientRect()
    return new THREE.Vector2(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1)
  }
  function pick(e: PointerEvent) {
    ray.setFromCamera(ndc(e), camera)
    const hit = ray.intersectObjects(slots.flatMap(s => (s.slot.visible ? [s.face, s.body] : [])), false)[0]
    if (!hit) return null
    const s = slots[hit.object.userData.i]
    const local = s.pivot.worldToLocal(hit.point.clone())
    return { s, uv: new THREE.Vector2(local.x / W, local.y / H) }
  }
  const PX_PER_CARD = 520
  canvas.addEventListener('pointerdown', e => {
    if (frozen || locked || !pick(e)) return
    posTween?.kill(); settle?.()
    dragging = true; moved = 0; downX = lastX = e.clientX; downPos = ringPos.v; lastT = performance.now(); vel = 0
    canvas.setPointerCapture(e.pointerId)
  })
  canvas.addEventListener('pointermove', e => {
    mouse.copy(ndc(e))
    if (dragging) {
      const now = performance.now(), dx = e.clientX - lastX, dt = Math.max(1, now - lastT) / 1000
      moved += Math.abs(dx)
      ringPos.v = downPos - (e.clientX - downX) / PX_PER_CARD
      vel = vel * 0.7 + (-dx / PX_PER_CARD / dt) * 0.3
      lastX = e.clientX; lastT = now
      return
    }
    const h = frozen || locked ? null : pick(e)
    hovered = h ? h.s : null
    if (h) h.s.tilt.set(h.uv.x, h.uv.y)
    canvas.style.cursor = h ? 'grab' : 'default'
  })
  const up = (e: PointerEvent) => {
    if (!dragging) return
    dragging = false
    if (moved < 6) { const h = pick(e); if (h) on.open?.(h.s.card.id); return }
    // release: a flick or half a card's travel moves one network, never more
    const target = Math.round(ringPos.v + Math.max(-0.5, Math.min(0.5, vel * 0.15)))
    const idx = ((target % N()) + N()) % N()
    on.pick?.(slots[idx].card.id) // the app switches network and calls goTo
  }
  canvas.addEventListener('pointerup', up)
  canvas.addEventListener('pointercancel', up)
  canvas.addEventListener('pointerleave', () => { hovered = null })

  // ---------------------------------------------------------------- loop
  const timer = new THREE.Timer()
  let vw = 0, vh = 0, viewK = -1
  // the QR on the card's back is read by a phone camera: full device resolution while it faces the viewer (the ring is
  // still then), the capped ratio again before it turns away
  function sharp(on: boolean) {
    const r = on ? Math.min(devicePixelRatio, 2) : DPR
    if (renderer.getPixelRatio() === r) return
    renderer.setPixelRatio(r); composer.setPixelRatio(r); layout()
  }
  function layout() {
    const w = canvas.clientWidth, h = canvas.clientHeight
    if (!w || !h) return
    vw = w; vh = h
    renderer.setSize(w, h, false)
    composer.setSize(w, h)
    finish.uniforms.veil.value = w / h < 0.9 ? 0.0001 : 0.46 // phone: content sits below the card, no side veil
    camera.aspect = w / h
    view()
  }
  // home: the docked card fills the right half (centre at ~72%), phone: it sits above the content; before entry the
  // scene is centred. The offset glides with the deal (the eye follows the cards, so it never reads as a pan)
  let band: [number, number] | null = null
  function view() {
    const k = (viewK = shift.v)
    if (vw / vh < 0.9) { // phone: the docked card fills the band between the header and the column (App measures it)
      const top = band?.[0] ?? vh * .16, bot = band?.[1] ?? vh * .53, hpx = Math.max(120, (bot - top) * .9)
      camera.fov = THREE.MathUtils.radToDeg(2 * Math.atan((H * vh) / (2 * (RI - DOCK) * hpx)))
      camera.setViewOffset(vw, vh, 0, (vh / 2 - (top + bot) / 2) * k, vw, vh)
      backdropMat.uniforms.narrow.value = 1
    }
    else { camera.fov = 40; camera.setViewOffset(vw, vh, -vw * 0.22 * k, 0, vw, vh); backdropMat.uniforms.narrow.value = 0 }
    camera.updateProjectionMatrix()
    finish.uniforms.veilK.value = k
  }

  let lastRing = NaN, lean = 0
  const tmp = new THREE.Euler(), qB = new THREE.Quaternion(), qT = new THREE.Quaternion(), vB = new THREE.Vector3()
  function frame() {
    timer.update()
    const dt = Math.min(timer.getDelta(), 1 / 20), t = timer.getElapsed()
    const still = reduced.matches || frozen
    const ease = (tau: number) => 1 - Math.exp(-dt / tau) // same feel at 60, 144 and 165 Hz
    if (vw && shift.v !== viewK) view()
    fog.density = scatter.v * 0.034 * haze.v
    backdropMat.uniforms.time.value = still ? 0 : t
    // the wall follows the ring: its marks drift ~1.8 m per card, the light leans with the turn's speed and settles back
    let dr = Number.isNaN(lastRing) ? 0 : ringPos.v - lastRing; const nn = N(); if (dr > nn / 2) dr -= nn; if (dr < -nn / 2) dr += nn; lastRing = ringPos.v
    if (!still) { backdropMat.uniforms.pan.value += dr * 1.8; lean += (Math.max(-1.6, Math.min(1.6, (dr / Math.max(dt, 1e-3)) * 0.9)) - lean) * ease(0.35) }
    backdropMat.uniforms.lean.value = lean
    const sx = still ? 0.65 : 0.65 + 0.65 * Math.sin(t * 0.1) // 63 s sweep
    strip.position.x = sx + (glint.x - sx) * glint.w
    strip.lookAt(C)
    if (!dragging && !posTween?.isActive()) {
      const target = Math.round(ringPos.v)
      ringPos.v += (target - ringPos.v) * ease(0.12)
    }
    const cur = current(), ci = slots.indexOf(cur), n = N()
    slots.forEach((s, i) => {
      const d = offset(i), p = ringPose(d, dock.v), k = s.spread.v
      const rank = (i - ci + n) % n // the current network's card lies on top of the deck
      const q = deckPose(rank, still ? 0 : t, loose.v), w = scatter.v
      if (w > 0) { const c = scatterPose(i, still ? 0 : t); q.x += (c.x - q.x) * w; q.y += (c.y - q.y) * w; q.z += (c.z - q.z) * w; q.ry += (c.ry - q.ry) * w; q.rz += (c.rz - q.rz) * w }
      s.slot.visible = p.show || k < 0.98
      // fly along the circle, not across the screen: angle and radius are blended in polar space
      const pa = d * STEP, a = pa * k, pr = Math.hypot(p.x, p.z), r0 = -q.z, r = r0 + (pr - r0) * k + Math.sin(Math.PI * k) * 2.6 // r0: own deck depth; mid-flight the card swings out wide, round the circle
      const ox = q.x * (1 - k), oy = q.y + (p.y - q.y) * k // the deck's own wobble fades as it leaves
      s.slot.position.set(Math.sin(a) * r + ox, oy, -Math.cos(a) * r)
      tmp.set(0, -a + (q.ry + a) * (1 - k) * (1 - k), q.rz * (1 - k))
      s.slot.rotation.copy(tmp)
      const b = s.born.v
      if (b < 1) { // intro: from the stack behind the spinning lead card to its own pose (spacing > thickness: no cut-through)
        const lead = s === cur
        vB.set(0, 0.1, -BIRTH_Z - rank * 0.13)
        qB.setFromEuler(tmp.set(0, lead ? birth.a : 0, 0))
        // the others clear the centre sideways first, then sink: the receding lead never cuts through one of them
        const side = lead ? b : 1 - (1 - b) ** 3, sp = s.slot.position
        sp.set(vB.x + (sp.x - vB.x) * side, vB.y + (sp.y - vB.y) * side, vB.z + (sp.z - vB.z) * b)
        s.slot.quaternion.slerpQuaternions(qB, qT.copy(s.slot.quaternion), b)
        s.slot.visible = lead ? birth.lit : birth.peel
      }
      const isHover = s === hovered
      const tx = isHover ? -s.tilt.y * 0.12 : 0, ty = isHover ? s.tilt.x * 0.16 : 0 // a heavy plate: a few degrees
      s.pivot.rotation.x += (tx - s.pivot.rotation.x) * ease(0.5)
      s.pivot.rotation.y += (ty - s.pivot.rotation.y) * ease(0.5)
      s.bob += ((still || locked || (held && s === cur) ? 0 : 1) - s.bob) * ease(0.4)
      s.pivot.position.y = Math.sin(t * 0.5 + s.phase) * 0.03 * s.bob
      s.pivot.rotation.z = Math.sin(t * 0.4 + s.phase) * 0.008 * s.bob
    })
    if (!still && !birth.on) mouseS.lerp(mouse, ease(0.55)) // the camera holds still while the CSS coin hands over
    camera.rotation.set(mouseS.y * 0.006, -mouseS.x * 0.01, 0) // the camera barely moves
    draw()
  }

  function draw() {
    if (!canvas.clientWidth || !canvas.clientHeight) return
    deckPass.amount = focus.v
    deckPass.enabled = focus.v > 0.002 // sharp: the plain render, no extra targets
    if (deckPass.enabled) camera.layers.set(0); else camera.layers.enableAll()
    composer.render()
    camera.layers.enableAll()
  }

  const ro = new ResizeObserver(layout)
  ro.observe(canvas)
  layout()
  // frames run on GSAP's ticker, after its tweens: the intro's CSS coin and the 3D card change in the very same frame
  let looping = false
  const loop = (on: boolean) => { if (on === looping) return; looping = on; if (on) gsap.ticker.add(frame); else gsap.ticker.remove(frame) }
  document.addEventListener('visibilitychange', () => loop(!document.hidden && !resting))
  canvas.addEventListener('webglcontextrestored', () => { layout(); loop(true) })
  if (import.meta.env.DEV) (window as any).__motion = (window as any).__motion ?? {}
  const count = (k: string) => { if (import.meta.env.DEV) (window as any).__motion[k] = ((window as any).__motion[k] ?? 0) + 1 }
  const fontsReady = () => Promise.all(['600 64px Manrope', '400 64px Manrope', '400 40px "JetBrains Mono"'].map(f => document.fonts.load(f))).catch(() => {})

  // first frame prepared off screen: fonts, model, light, shaders, two warm renders (composer passes compile too)
  // all = every face now (the deal follows at once); otherwise the two front cards now, the rest while idle
  async function prepare(cards: Card[], currentId: string, all = false, wide = false, intro = false) {
    scatter.v = wide ? 1 : 0
    if (import.meta.env.DEV) console.warn(`[prepare] parallel compile: ${!!renderer.getContext().getExtension('KHR_parallel_shader_compile')}`)
    const t0 = performance.now(), mark = (m: string) => { if (import.meta.env.DEV) console.warn(`[prepare] ${m} +${Math.round(performance.now() - t0)}ms`) }
    await Promise.all([fontsReady(), glbReady, envReady]); mark('assets')
    // one card per frame: the intro's circle keeps drawing while the faces are painted and uploaded
    // build once, one painted face per frame (the intro circle keeps drawing); back cards stay plain until paintRest
    deferFaces = !all
    for (const s of slots) freeSlot(s)
    slots = []
    for (const [i, c] of cards.entries()) { slots.push(makeSlot(c, i)); if (i < 2 || all) await new Promise(r => setTimeout(r)) }
    slots.forEach((s, i) => (s.body.userData.i = s.face.userData.i = i))
    deferFaces = false
    await api.setDeck(cards, currentId, 'glass', 'locked'); mark(`deck ${cards.length}`)
    tintTo(current().card, 0)
    camera.layers.enableAll()
    // compile the variants the composer really uses: linear offscreen target, no tone mapping in the scene shaders
    renderer.setRenderTarget(composer.renderTarget1)
    await renderer.compileAsync(scene, camera).catch(() => {}); mark('compiled')
    renderer.setRenderTarget(null)
    draw(); draw(); mark('drawn')
    focus.v = 0; draw() // the sharp path is warm too (a returning unlock pulls focus to it at once)
    if (intro && !birth.lit) { // (the intro gave up waiting: never hide a deck nobody will release) every card waits unseen behind the lead, which waits for the coin
      Object.assign(birth, { on: true, lit: false, peel: false, a: 0 })
      haze.v = 0
      for (const s of slots) { s.born.v = 0; s.bob = 0 }
    } else focus.v = 1 // returning launch: the deck is simply there, far and soft, as the intro would have left it
    loop(!resting)
    paintRest()
  }
  // hidden cards of the locked deck get their faces one per idle slot, long before a deal needs them
  // stale faces, the docked card first, only while nothing flies (deal, intro, turn, drag), and spaced out: each face
  // costs the next frame ~90 ms in the browser's canvas raster (measured 08.10), one skipped frame of a slow sway is
  // not seen, six in a row are. ponytail: spacing, not a worker raster; move faces to OffscreenCanvas if it still shows
  let painting = false
  function paintRest() {
    if (painting) return
    painting = true
    const idle = (window as any).requestIdleCallback ?? ((f: () => void) => setTimeout(f, 32))
    const next = () => {
      const s = [current(), ...slots].find(x => x?.deferred)
      if (!s) { painting = false; return }
      if (dealing || birth.on || dragging || posTween?.isActive()) { setTimeout(next, 150); return }
      idle(() => { if (dealing || birth.on || dragging || posTween?.isActive()) return next(); if (s.deferred) paint(s); setTimeout(next, 700) }) // motion may have started meanwhile
    }
    next()
  }

  // one ring of light across the room; a newer event restarts it
  function pulse(c: string) {
    backdropMat.uniforms.waveC.value.set(c)
    gsap.fromTo(backdropMat.uniforms.wave, { value: 0.001 }, { value: 1, duration: T(1.8), ease: 'power1.out', overwrite: true })
  }
  function tintTo(card: Card, d = 0.9) {
    const m = lookFor(card).mark, c = new THREE.Color(Array.isArray(m) ? m[0] : m)
    gsap.to(backdropMat.uniforms.tint.value, { r: c.r, g: c.g, b: c.b, duration: T(d), ease: 'glass', overwrite: true })
  }

  const api = {
    // resolves when the first real frame is on the canvas; the intro opens only then
    prepare,
    // rack focus: 1 = far and soft, 0 = sharp
    focus(v: number, ms = 420) { return gsap.to(focus, { v, duration: T(ms / 1000), ease: 'glass' }) },
    // intro hand-over: where the lead card stands on screen (CSS px), so the CSS coin can grow into exactly it
    birth,
    leadRect() {
      const f = vh / 2 / Math.tan(THREE.MathUtils.degToRad(camera.fov / 2)), k = f / BIRTH_Z, l = lookFor(current()?.card ?? { id: '' } as Card)
      return { x: vw / 2, y: vh / 2, w: W * k, h: H * k, r: 0.13 * k, t: D * k, f, face: l.face, edge: l.face } // the model's edge is the face alloy now
    },
    // one pass of the strip light across the landing card, then it hands back to its slow loop
    glint(ms = 700) {
      gsap.timeline()
        .fromTo(glint, { x: -3.2, w: 1 }, { x: 3.2, duration: T(ms / 1000), ease: 'sine.inOut' })
        .to(glint, { w: 0, duration: T(0.4), ease: 'none' })
    },
    // the cards come out from behind the lead one into the far cloud (welcome) or the locked deck; focus goes soft
    // k: slowness (first launch 1.8: the welcome takes its time, owner 08.10)
    release(to: 'scatter' | 'deck', k = 1): Promise<void> {
      if (!birth.on) return Promise.resolve()
      scatter.v = to === 'scatter' ? 1 : 0
      birth.peel = true
      const lead = current(), others = slots.filter(s => s !== lead)
      const tl = gsap.timeline()
        .to(others.map(s => s.born), { v: 1, duration: T(0.72 * k), ease: 'glass', stagger: T(0.045 * k) }, 0)
        .to(lead.born, { v: 1, duration: T(0.8 * k), ease: 'glass' }, T(0.12 * k))
      // unlocked meanwhile (no password, or typed blind under the intro): unlock() owns focus and haze
      if (locked) tl.to(haze, { v: 1, duration: T(0.5 * k), ease: 'glass' }, T(0.2 * k)).to(focus, { v: 1, duration: T(0.38 * k), ease: 'glass' }, T(0.32 * k))
      return new Promise(res => tl.eventCallback('onComplete', () => { birth.on = false; res() }))
    },
    // the whole deck: every enabled network, ring order = list order
    async setDeck(cards: Card[], currentId: string, v: Variant, mode: 'locked' | 'swap' | 'update') {
      await fontsReady() // card print is canvas text: fonts first or it falls back to system faces
      if (mode === 'swap' && slots.length && !locked) await gather()
      if (v !== variant) { variant = v; for (const s of slots) s.key = '' } // material family changed: repaint all
      build(cards)
      const i = Math.max(0, cards.findIndex(c => c.id === currentId))
      if (mode !== 'update') { posTween?.kill(); settle?.(); ringPos.v = i }
      if (mode === 'locked') { locked = true; dock.v = 0; for (const s of slots) s.spread.v = 0 }
      if (mode === 'swap' && !locked) await deal(false)
      if (mode === 'update' && !locked) for (const s of slots) s.spread.v = 1
    },
    // password being typed: the deck loosens a little with each character (0..1)
    prime(f: number) { if (locked) gsap.to(loose, { v: Math.max(0, Math.min(1, f)), duration: T(0.6), ease: 'glass' }) },
    async unlock() {
      // the docked face turns proven first and its raster lands while the deck still rests, not inside the deal
      dealing = true // no back face may start painting while we wait for the docked one
      const cur = current()
      if (cur?.deferred) { // the browser rasters a new face 0.1-0.2 s later, in one or two heavy frames (measured 08.10):
        paint(cur)           // start the deal only after 0.25 s and three calm frames in a row; 0.7 s at most
        const t0 = performance.now()
        await new Promise<void>(r => {
          let calm = 0, last = t0
          const f = (now: number) => {
            calm = now - last < 40 ? calm + 1 : 0; last = now
            if (now - t0 > 700 || (now - t0 > 250 && calm >= 3)) r(); else requestAnimationFrame(f)
          }
          requestAnimationFrame(f)
        })
      }
      locked = false
      gsap.killTweensOf([focus, haze]) // no-password start: the intro's soft focus must not win over the pull
      haze.v = 1
      gsap.to(focus, { v: 0, duration: T(0.42), ease: 'glass' }) // focus pulls as the deal starts
      gsap.to(scatter, { v: 0, duration: T(1.2), ease: 'glass' }) // the far cloud's haze clears on the way in
      gsap.to(backdropMat.uniforms.gain, { value: 1, duration: reduced.matches ? 0 : 0.8, ease: 'power2.inOut' }) // the room lights up with the deal
      gsap.to(shift, { v: 1, duration: T(0.65), ease: 'glass' }) // the scene moves right as the cards fly out
      await deal(false)
    }, // focus pulls as the deal starts
    async lock() { locked = true; held = false; sharp(false); scatter.v = 0; gsap.to(backdropMat.uniforms.gain, { value: 0.45, duration: reduced.matches ? 0 : 0.6, ease: 'power2.inOut' }); gsap.to(focus, { v: 1, duration: T(0.6), ease: 'glass', delay: T(0.3) }); gsap.to(shift, { v: 0, duration: T(0.65), ease: 'glass', delay: T(0.2) }); await gather() },
    // turn the ring to a network card: fast, data already on the face
    goTo(id: string) { const i = slots.findIndex(s => s.card.id === id); if (i < 0) return Promise.resolve(); if (slots[i] !== current()) tintTo(slots[i].card); if (slots[i].deferred) paint(slots[i]); return turnTo(i) }, // no ring of light on a switch (owner 08.10); the face is final before it turns in
    async flip(back: boolean, address = '', net = '', note = '') {
      const s = current()
      if (!s) return
      held = back
      if (back) {
        const m = s.back.material as THREE.MeshBasicMaterial
        m.map?.dispose(); m.map = qrTexture(address, s.card, net, note); m.needsUpdate = true
        s.back.visible = true
      }
      if (!back) sharp(false)
      count('flip')
      await gsap.to(s.flip.rotation, { y: back ? Math.PI : 0, duration: T(0.9), ease: 'glass' })
      s.back.visible = back
      if (back && held) sharp(true)
    },
    // send: the card is thrown away toward the recipient and docks back with the new balance
    async throwCard(next: Card) {
      const s = current()
      if (!s) return
      held = true
      count('throw')
      pulse('#ff5a2e')
      await gsap.timeline()
        .to(s.fly.position, { x: 6, y: 1.5, z: -3, duration: T(0.55), ease: 'power3.in' })
        .to(s.fly.rotation, { x: -0.3, y: 1.2, z: -0.6, duration: T(0.55), ease: 'power3.in' }, '<')
        .add(() => { setFace(s, next, true); s.fly.position.set(0, -3.5, 0); s.fly.rotation.set(0.5, 0, 0) }, '+=0.15')
        .to(s.fly.position, { y: 0, duration: T(0.8), ease: 'glass' })
        .to(s.fly.rotation, { x: 0, duration: T(0.8), ease: 'glass' }, '<')
      held = false
    },
    // swap: one turn of the current card, the new balance shows when it comes face-on
    async swapCard(next: Card) {
      const s = current()
      if (!s) return
      held = true
      count('swap')
      await gsap.timeline()
        .to(s.flip.rotation, { y: Math.PI / 2, duration: T(0.36), ease: 'power2.in' })
        .add(() => setFace(s, next, true))
        .fromTo(s.flip.rotation, { y: -Math.PI / 2 }, { y: 0, duration: T(0.6), ease: 'glass' })
      held = false
    },
    // balances changed: repaint only the faces whose content changed, no motion
    sync(cards: Card[]) { for (const c of cards) { const s = slots.find(x => x.card.id === c.id); if (s) setFace(s, c) } },
    freeze(b: boolean) { frozen = b; if (b) hovered = null },
    // settle (sway off) for 240 ms, then stop rendering; wake resumes at once
    rest(b: boolean) {
      clearTimeout(restTimer); resting = b; frozen = b
      if (b) { hovered = null; restTimer = window.setTimeout(() => loop(false), 240) }
      else loop(true)
    },
    tint: tintTo,
    // phone: where the card may stand (px from the top: header bottom, column top)
    setBand(top: number, bottom: number) { band = [top, bottom]; view() },
    setTheme(t: Theme) { backdropMat.uniforms.theme.value = Math.max(0, THEMES.indexOf(t)) },
    setEmber(b: boolean) { if (b === ember) return; ember = b; gsap.to(backdropMat.uniforms.ember, { value: b ? 0.4 : 0, duration: T(0.8) }) },
  }
  return api
}

export type Hero = ReturnType<typeof createHero>
