<script lang="ts">
  // A number that rolls into place (owner 09.10, swap): each digit is a column of 0-9 on the GPU (transform only),
  // keyed from the right so units stay units when the length changes. Reduced motion: it simply changes.
  let { text }: { text: string } = $props()
  const chars = $derived([...text])
</script>

<span class="roll" aria-label={text}>{#each chars as c, i (chars.length - i)}{#if c >= '0' && c <= '9'}<span class="d" aria-hidden="true"><span class="col" style:--n={+c}>{#each '0123456789' as k}<span>{k}</span>{/each}</span></span>{:else}<span class="s" aria-hidden="true">{c}</span>{/if}{/each}</span>

<style>
  .roll { display: inline-flex; white-space: pre; font-variant-numeric: tabular-nums; }
  .d { user-select: none; display: inline-block; height: 1.25em; overflow: clip; vertical-align: bottom; }
  .col { display: flex; flex-direction: column; transform: translateY(calc(var(--n) * -1.25em)); transition: transform 560ms var(--ease); }
  .col span, .s { display: block; height: 1.25em; line-height: 1.25em; }
  @media (prefers-reduced-motion: reduce) { .col { transition: none; } }
</style>
