<script>
  import smoothie from './assets/credits/smoothie.png';
  import ffmpeg from './assets/credits/ffmpeg.ico';
  import tauri from './assets/credits/tauri.svg';
  import svelte from './assets/credits/svelte.svg';
  import plex from './assets/credits/ibm-plex.png';
  import flowframes from './assets/credits/flowframes.png';

  export let kind;

  const logos = { smoothie, ffmpeg, tauri, svelte, plex, flowframes };
  // These upstream projects do not publish a dedicated logo.
  const labels = { rife: 'RIFE', vapoursynth: 'VS' };
</script>

<span class="project-mark" class:project-mark-plex={kind === 'plex'} class:project-mark-flowframes={kind === 'flowframes'} class:project-mark-svelte={kind === 'svelte'} class:project-mark-smoothie={kind === 'smoothie'} aria-hidden="true">
  {#if logos[kind]}
    <img src={logos[kind]} alt="" width="30" height="30" />
  {:else}
    <span class="project-label">{labels[kind] || ''}</span>
  {/if}
</span>

<style>
  .project-mark {
    width: 32px;
    height: 32px;
    flex: 0 0 32px;
    display: grid;
    place-items: center;
    opacity: var(--credit-mark-opacity, 0.72);
    transition: opacity 150ms ease;
  }

  .project-mark img {
    display: block;
    width: 26px;
    height: 26px;
    object-fit: contain;
    filter: grayscale(1) brightness(1.65);
    mix-blend-mode: screen;
  }

  .project-mark-smoothie img {
    width: 30px;
    height: 30px;
    filter: grayscale(1) contrast(1.15) brightness(1.35);
  }

  .project-mark-svelte img {
    filter: grayscale(1) brightness(0) invert(1);
  }

  .project-mark-plex img {
    width: 24px;
    height: 24px;
    filter: grayscale(1) invert(1) contrast(1.7) brightness(1.2);
  }

  .project-mark-flowframes img {
    filter: grayscale(1) contrast(2) brightness(1.3);
  }

  .project-label {
    color: #f0f0f0;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: -0.035em;
  }
</style>
