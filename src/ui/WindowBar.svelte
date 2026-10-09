<script lang="ts">
  // Own window chrome (owner 07.10: no stock Windows title bar). A drag strip across the top (double-click
  // maximises, handled by Tauri) and three machined buttons. In a browser tab there is no window: nothing renders.
  import { isTauri } from '@tauri-apps/api/core'
  import { getCurrentWindow } from '@tauri-apps/api/window'
  import { t } from '../lib/i18n.svelte'

  const on = isTauri() && !(globalThis as any).__cwMock && !/Android/i.test(navigator.userAgent) // a phone has no window chrome
  const win = on ? getCurrentWindow() : null
  let max = $state(false)
  if (win) {
    win.isMaximized().then(v => (max = v))
    win.onResized(() => win.isMaximized().then(v => (max = v)))
  }
</script>

{#if win}
  <div class="drag" data-tauri-drag-region></div>
  <div class="ctl" role="group" aria-label={t('win.controls')}>
    <button onclick={() => win.minimize()} aria-label={t('win.min')}>
      <svg viewBox="0 0 12 12"><path d="M2.5 6h7" /></svg>
    </button>
    <button onclick={() => win.toggleMaximize()} aria-label={max ? t('win.restore') : t('win.max')}>
      {#if max}<svg viewBox="0 0 12 12"><path d="M3.5 4.5h4v4h-4zM5 4.5V3h3.5v3.5H7.5" /></svg>
      {:else}<svg viewBox="0 0 12 12"><rect x="2.75" y="2.75" width="6.5" height="6.5" rx="1.2" /></svg>{/if}
    </button>
    <button class="x" onclick={() => win.close()} aria-label={t('win.close')}>
      <svg viewBox="0 0 12 12"><path d="M3 3l6 6M9 3 3 9" /></svg>
    </button>
  </div>
{/if}

<style>
  .drag { position: fixed; inset: 0 0 auto 0; height: 76px; z-index: 9; }
  /* a sheet's veil (z 30) covered the strip: the window could not be moved while a sheet was open (owner 08.10) */
  :global(body:has(.veil)) .drag { z-index: 31; } /* over the veil, under the sheet itself (same z, later in the DOM) */
  .ctl { position: fixed; top: 22px; right: var(--pad); z-index: 60; display: flex; gap: 6px; }
  button { width: 32px; height: 32px; display: grid; place-items: center; border: 0; border-radius: 50%; background: var(--metal); box-shadow: var(--metal-edge); color: var(--ink-2); cursor: pointer; transition: background-color 280ms ease, color 280ms ease, transform 140ms var(--ease-out); }
  button:hover { background: var(--metal-hi); color: var(--ink); }
  button:active { transform: scale(.92); }
  .x:hover { background: var(--ember); color: var(--ember-ink); }
  svg { width: 12px; height: 12px; fill: none; stroke: currentColor; stroke-width: 1.3; stroke-linecap: round; stroke-linejoin: round; }
</style>
