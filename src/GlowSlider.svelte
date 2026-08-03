<script>
  let {
    value = $bindable(2),
    min = 2,
    max = 10,
    step = 1,
    label = '',
    unit = '',
    precision = 0
  } = $props();

  let isHovered = $state(false);
  let isActive = $state(false);
  
  // Target value that user is controlling
  let tempValue = $state(value);

  // Sync tempValue when parent value changes externally
  $effect(() => {
    tempValue = value;
  });

  // Animated visual ratio (0.0 to 1.0) and hover alpha (0.0 to 1.0)
  let vizRatio = $state(0);
  let hoverAnim = $state(0);

  let lastTime = 0;
  let animFrame;

  function updateAnim(now) {
    if (!lastTime) lastTime = now;
    const dt = Math.min((now - lastTime) / 1000, 0.05); // cap dt at 50ms
    lastTime = now;

    // 2. Exponential Decay Interpolation (speed = 20.0f)
    const targetRatio = Math.max(0, Math.min(1, (tempValue - min) / (max - min)));
    vizRatio += (targetRatio - vizRatio) * Math.min(1.0, dt * 20.0);

    // 3. Smooth Alpha Hover Fade (speed = 10.0f)
    const targetHover = (isHovered || isActive) ? 1.0 : 0.0;
    hoverAnim += (targetHover - hoverAnim) * Math.min(1.0, dt * 10.0);

    animFrame = requestAnimationFrame(updateAnim);
  }

  $effect(() => {
    vizRatio = (value - min) / (max - min);
    lastTime = performance.now();
    animFrame = requestAnimationFrame(updateAnim);
    return () => {
      if (animFrame) cancelAnimationFrame(animFrame);
    };
  });

  function handleInput(e) {
    tempValue = parseFloat(e.target.value);
  }

  function handleChange() {
    // Snap to nearest step on release
    const snapped = Math.round((tempValue - min) / step) * step + min;
    const clamped = Math.max(min, Math.min(max, snapped));
    tempValue = clamped;
    value = clamped;
  }

  function displayValue() {
    return Number(tempValue).toFixed(precision);
  }

</script>

<div class="glow-slider-container">
  <div class="slider-header">
    <span class="slider-label">{label}</span>
    <span class="slider-val">{displayValue()}{unit}</span>
  </div>

  <div
    class="slider-track-wrap"
    role="presentation"
    onmouseenter={() => isHovered = true}
    onmouseleave={() => isHovered = false}
    onmousedown={() => isActive = true}
    onmouseup={() => isActive = false}
  >
    <!-- Hidden Native Range Input -->
    <input
      type="range"
      {min}
      {max}
      step="0.01"
      value={tempValue}
      oninput={handleInput}
      onchange={handleChange}
      class="native-hidden-input"
    />

    <!-- Visual Background Frame -->
    <div class="track-bg"></div>

    <!-- Active Fill Track with Exponential Decay Lerp -->
    <div
      class="track-fill"
      style="width: {vizRatio * 100}%; box-shadow: 0 0 {4 + hoverAnim * 12}px rgba(255, 255, 255, {0.15 + hoverAnim * 0.35});"
    ></div>

    <!-- Grab Circle (Inner Fill + Outer Hover Ring) -->
    <div
      class="grab-circle"
      style="left: calc({vizRatio * 100}% - 7px);"
    >
      <div
        class="grab-ring"
        style="border-color: rgba(255, 255, 255, {hoverAnim * 0.85}); transform: scale({1 + hoverAnim * 0.25});"
      ></div>
      <div class="grab-dot"></div>
    </div>
  </div>
</div>

<style>
  .glow-slider-container {
    width: 100%;
    user-select: none;
  }

  .slider-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 8px;
  }

  .slider-label {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: #888888;
  }

  .slider-val {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 12px;
    font-weight: 700;
    color: #ffffff;
    font-variant-numeric: tabular-nums;
  }

  .slider-track-wrap {
    position: relative;
    width: 100%;
    height: 16px;
    display: flex;
    align-items: center;
    cursor: pointer;
  }

  /* 1. Hidden Native Grab & Background */
  .native-hidden-input {
    position: absolute;
    top: 0;
    left: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    z-index: 10;
    cursor: pointer;
    margin: 0;
  }

  /* Background Track */
  .track-bg {
    position: absolute;
    left: 0;
    width: 100%;
    height: 4px;
    background: rgba(255, 255, 255, 0.08);
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 2px;
  }

  /* 5. Progress Fill & Neon Glow Multicouche */
  .track-fill {
    position: absolute;
    left: 0;
    height: 3px;
    background: #ffffff;
    border-radius: 2px;
    pointer-events: none;
    z-index: 2;
  }

  /* 6. Grab Circle */
  .grab-circle {
    position: absolute;
    top: 50%;
    transform: translateY(-50%);
    width: 14px;
    height: 14px;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    z-index: 3;
  }

  .grab-dot {
    width: 8px;
    height: 8px;
    background: #ffffff;
    border-radius: 50%;
    box-shadow: 0 0 6px rgba(255, 255, 255, 0.8);
  }

  .grab-ring {
    position: absolute;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 1.5px solid transparent;
    transition: transform 0.1s ease;
  }
</style>
