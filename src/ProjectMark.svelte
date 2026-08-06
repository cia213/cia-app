<script>
  export let kind;
</script>

<svg class="project-mark" viewBox="0 0 48 48" aria-hidden="true" focusable="false">
  {#if kind === 'rife'}
    <rect x="7" y="12" width="18" height="18" rx="3" />
    <rect x="23" y="18" width="18" height="18" rx="3" />
    <path d="M15 21h18M24 12v24" />
  {:else if kind === 'smoothie'}
    <path d="M8 15c8-8 24-8 32 0M8 24c8 8 24 8 32 0M8 33c8-8 24-8 32 0" />
  {:else if kind === 'vapoursynth'}
    <path d="m8 10 16 28L40 10M16 10l8 14 8-14" />
  {:else if kind === 'ffmpeg'}
    <path d="M8 15h32M8 24h32M8 33h21" />
    <path d="m31 27 9-3-9-3z" fill="currentColor" stroke="none" />
  {:else if kind === 'tauri'}
    <circle cx="24" cy="24" r="8" />
    <path d="M7 24c3-10 11-16 17-16s14 6 17 16c-3 10-11 16-17 16S10 34 7 24Z" />
    <circle cx="9" cy="24" r="2" fill="currentColor" stroke="none" />
    <circle cx="39" cy="24" r="2" fill="currentColor" stroke="none" />
  {:else if kind === 'svelte'}
    <path d="M34 12c-3-3-7-4-12-4-7 0-12 4-12 10 0 12 24 5 24 15 0 4-4 7-10 7-5 0-9-2-12-5" />
  {:else if kind === 'plex'}
    <path d="M12 39V9h13c7 0 11 4 11 10s-4 10-11 10H12" />
    <path d="M25 29 38 39" />
  {:else if kind === 'flowframes'}
    <rect x="8" y="10" width="22" height="22" rx="3" />
    <rect x="18" y="18" width="22" height="22" rx="3" />
    <path d="m24 24 7 5-7 5z" fill="currentColor" stroke="none" />
  {/if}
</svg>

<style>
  .project-mark {
    width: 30px;
    height: 30px;
    flex: 0 0 30px;
    color: #e4e4e7;
    fill: none;
    stroke: currentColor;
    stroke-width: 2.1;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
</style>
