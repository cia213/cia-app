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

<span class="project-mark" aria-hidden="true">
  {#if logos[kind]}
    <img src={logos[kind]} alt="" width="30" height="30" />
  {:else}
    <span class="project-label">{labels[kind] || ''}</span>
  {/if}
</span>

<style>
  .project-mark {
    width: 30px;
    height: 30px;
    flex: 0 0 30px;
    display: grid;
    place-items: center;
  }

  .project-mark img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .project-label {
    color: #e4e4e7;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: -0.04em;
  }
</style>
