<script lang="ts">
  // Receive QR in plain SVG: works without WebGL, dark modules on a light plate so every scanner reads it.
  import { encode } from 'uqr'
  let { value, size = 220 }: { value: string; size?: number } = $props()
  const q = $derived(encode(value, { ecc: 'M', border: 0 }))
  const d = $derived(q.data.flatMap((row, y) => row.map((on, x) => (on ? `M${x} ${y}h1v1h-1z` : ''))).join(''))
</script>

<div class="plate" style="width:{size}px">
  <svg viewBox="0 0 {q.size} {q.size}" shape-rendering="crispEdges" role="img" aria-label="QR‑код"><path {d} fill="#0b0b0e" /></svg>
</div>

<style>
  .plate { padding: 14px; border-radius: 16px; background: var(--ink); aspect-ratio: 1; }
  svg { display: block; width: 100%; height: 100%; }
</style>
