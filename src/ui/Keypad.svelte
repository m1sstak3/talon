<script lang="ts">
  // phone: amounts are typed on the wallet's own keypad, like every phone wallet (no system keyboard over the screen,
  // a comma always where it should be). Writes the field's text as the screen shows it: «1,5».
  import Icon from './Icon.svelte'
  import { haptic } from '../lib/actions'
  let { value = $bindable(''), max = 9 }: { value?: string; max?: number } = $props()
  const KEYS = ['1', '2', '3', '4', '5', '6', '7', '8', '9', ',', '0', 'del'] as const
  function press(k: (typeof KEYS)[number]) {
    haptic('tick')
    if (k === 'del') { value = value.slice(0, -1); return }
    if (k === ',') { if (!value.includes(',')) value = (value || '0') + ','; return }
    const [, frac] = value.split(',')
    if (frac !== undefined && frac.length >= max) return // never more digits than the token has
    value = value === '0' ? k : value + k
  }
  // a long press on «стереть» clears the whole amount, like a phone's own keypad
  let timer = 0
  function hold() { clearTimeout(timer); timer = window.setTimeout(() => { if (value) { value = ''; haptic('tick') } }, 550) }
  const letGo = () => clearTimeout(timer)
</script>

<div class="keypad" role="group" aria-label="Цифры">
  {#each KEYS as k}
    <button type="button" class="key" onclick={() => press(k)} aria-label={k === 'del' ? 'Стереть' : k}
      onpointerdown={() => k === 'del' && hold()} onpointerup={letGo} onpointerleave={letGo} oncontextmenu={e => e.preventDefault()}>{#if k === 'del'}<Icon name="del" size={24} />{:else}{k}{/if}</button>
  {/each}
</div>

<style>
  .keypad { display: grid; grid-template-columns: repeat(3, 1fr); gap: 4px; user-select: none; }
  .key {
    min-height: 56px; border: 0; border-radius: 16px; background: none; color: var(--ink); font: 400 26px var(--font); font-variant-numeric: tabular-nums;
    cursor: pointer; -webkit-tap-highlight-color: transparent; touch-action: manipulation; transition: background-color 160ms ease;
  }
  .key:active { background: rgb(238 235 229 / .1); transition-duration: 40ms; }
  .key :global(svg) { vertical-align: middle; }
</style>
