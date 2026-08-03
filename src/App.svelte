<script>
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import GlowSlider from './GlowSlider.svelte';

  const appWindow = getCurrentWindow();

  let activePage = $state('dashboard'); // 'dashboard' | 'smoothie'
  let isDragging = $state(false);
  let logs = $state([]);
  let progress = $state(0);
  let elapsedTime = $state('00:00');
  let remainingTime = $state('--:--');
  let isHoveringTimer = $state(false);
  let copyFeedback = $state(false);
  let toast = $state({ show: false, message: '', type: 'info' });

  // Drawers / Modal Overlays
  let showRifeSettings = $state(false);
  let showSmoothieSettings = $state(false);

  // --- RIFE State & Settings ---
  let videoPath = $state('');
  let isLoading = $state(false);
  let isProcessing = $state(false);
  let isComplete = $state(false);
  let videoInfo = $state(null);
  let lastOutputPath = $state('');
  let rifeOutputPath = $state('');
  let jobPhase = $state('idle');
  let jobError = $state('');

  const DEFAULT_RIFE = {
    mode: 'boost',
    factor: 2,
    crf: 18,
    preset: 'medium',
    sceneThreshold: 0.05,
    blendCuts: 0
  };

  let rifeSettings = $state(loadRifeSettings());
  let autoRender = $state(loadAutoRender());

  function loadAutoRender() {
    try {
      return localStorage.getItem('rife_auto_render') === 'true';
    } catch {
      return false;
    }
  }

  function saveAutoRender() {
    try {
      localStorage.setItem('rife_auto_render', String(autoRender));
    } catch {
      showToast('Failed to save auto-render preference', 'error');
    }
  }

  function loadRifeSettings() {
    try {
      const saved = localStorage.getItem('rife_settings');
      return saved ? { ...DEFAULT_RIFE, ...JSON.parse(saved) } : { ...DEFAULT_RIFE };
    } catch {
      return { ...DEFAULT_RIFE };
    }
  }

  function saveRifeSettings() {
    try {
      localStorage.setItem('rife_settings', JSON.stringify(rifeSettings));
      showToast('RIFE settings saved to memory', 'success');
    } catch {
      showToast('Failed to save settings', 'error');
    }
  }

  function resetRifeSettings() {
    rifeSettings = { ...DEFAULT_RIFE };
    try { localStorage.removeItem('rife_settings'); } catch {}
    showToast('RIFE settings reset to default', 'info');
  }

  let outputFps = $derived(videoInfo ? (rifeSettings.mode === 'boost' ? videoInfo.fps * rifeSettings.factor : videoInfo.fps) : 0);
  let outputDuration = $derived(videoInfo ? (rifeSettings.mode === 'slowmo' ? videoInfo.duration * rifeSettings.factor : videoInfo.duration) : 0);

  // --- Smoothie State & Settings ---
  let smoothiePath = $state('');
  let isSmoothieLoading = $state(false);
  let isSmoothieProcessing = $state(false);
  let isSmoothieComplete = $state(false);
  let smoothieInfo = $state(null);
  let smoothieOutputPath = $state('');

  const DEFAULT_SMOOTHIE = {
    fps: 30,
    blendIntensity: 1.0,
    brightness: 1.1,
    saturation: 1.1,
    contrast: 1.0,
    lutEnabled: 'yes',
    lutOpacity: 0.67,
    borderless: 'no'
  };

  let smoothieSettings = $state(loadSmoothieSettings());

  function loadSmoothieSettings() {
    try {
      const saved = localStorage.getItem('smoothie_settings');
      return saved ? { ...DEFAULT_SMOOTHIE, ...JSON.parse(saved) } : { ...DEFAULT_SMOOTHIE };
    } catch {
      return { ...DEFAULT_SMOOTHIE };
    }
  }

  function saveSmoothieSettings() {
    try {
      localStorage.setItem('smoothie_settings', JSON.stringify(smoothieSettings));
      showToast('Smoothie config saved to memory', 'success');
    } catch {
      showToast('Failed to save config', 'error');
    }
  }

  function resetSmoothieSettings() {
    smoothieSettings = { ...DEFAULT_SMOOTHIE };
    try { localStorage.removeItem('smoothie_settings'); } catch {}
    showToast('Smoothie config reset to default', 'info');
  }

  let anyProcessing = $derived(isProcessing || isSmoothieProcessing);
  let canRenderSmoothie = $derived(Boolean(rifeOutputPath) && lastOutputPath === rifeOutputPath && !anyProcessing);
  let rifeSliderPct = $derived(((rifeSettings.factor - 2) / (10 - 2)) * 100);
  let smoothieSliderPct = $derived(((smoothieSettings.fps - 20) / (60 - 20)) * 100);

  function showToast(message, type = 'info') {
    toast = { show: true, message, type };
    setTimeout(() => { toast.show = false; }, 4000);
  }

  function playCompletionChime() {
    try {
      const AudioCtx = window.AudioContext || window.webkitAudioContext;
      if (!AudioCtx) return;
      const ctx = new AudioCtx();
      const now = ctx.currentTime;
      const osc1 = ctx.createOscillator();
      const gain1 = ctx.createGain();
      osc1.type = 'sine';
      osc1.frequency.setValueAtTime(523.25, now);
      gain1.gain.setValueAtTime(0.15, now);
      gain1.gain.exponentialRampToValueAtTime(0.001, now + 0.15);
      osc1.connect(gain1);
      gain1.connect(ctx.destination);
      osc1.start(now);
      osc1.stop(now + 0.15);
      const osc2 = ctx.createOscillator();
      const gain2 = ctx.createGain();
      osc2.type = 'sine';
      osc2.frequency.setValueAtTime(659.25, now + 0.12);
      gain2.gain.setValueAtTime(0.15, now + 0.12);
      gain2.gain.exponentialRampToValueAtTime(0.001, now + 0.35);
      osc2.connect(gain2);
      gain2.connect(ctx.destination);
      osc2.start(now + 0.12);
      osc2.stop(now + 0.35);
    } catch (e) {
      console.error('Audio playback error', e);
    }
  }

  function resetTelemetry() {
    progress = 0;
    elapsedTime = '00:00';
    remainingTime = '--:--';
  }

  function resetRunState() {
    logs = [];
    resetTelemetry();
  }

  function appendLog(line) {
    logs = [...logs, line].slice(-500);
  }

  function parseLogLine(line) {
    appendLog(line);
    if (line.includes('Finalizing output') || line.includes('FFmpeg') || /^frame=/.test(line)) {
      remainingTime = 'Encoding export...';
      if (progress < 99) progress = 99;
    }
    const rifePct = line.match(/^\s*(\d{1,3})%/);
    if (rifePct) progress = parseInt(rifePct[1], 10);
    const smPct = line.match(/(\d+(?:\.\d+)?)%\s*•/);
    if (smPct) progress = Math.round(parseFloat(smPct[1]));
    const rifeTimer = line.match(/\[(\d+(?::\d+)+)<(\d+(?::\d+)+)/);
    if (rifeTimer) { elapsedTime = rifeTimer[1]; remainingTime = rifeTimer[2]; }
    const smTimer = line.match(/(\d+:\d{2})\s*>\s*(\d+:\d{2})/);
    if (smTimer) { elapsedTime = smTimer[1]; remainingTime = smTimer[2]; }
  }

  async function copyLogsToClipboard() {
    if (logs.length === 0) {
      showToast('No execution logs recorded yet', 'info');
      return;
    }
    try {
      await navigator.clipboard.writeText(logs.join('\n'));
      copyFeedback = true;
      showToast('Logs copied to clipboard', 'success');
      setTimeout(() => { copyFeedback = false; }, 2000);
    } catch (e) {
      showToast('Failed to copy logs', 'error');
    }
  }

  $effect(() => {
    const u1 = listen('tauri://drag-drop', async (event) => {
      isDragging = false;
      const paths = event.payload.paths;
      if (paths && paths.length > 0) {
        if (activePage === 'smoothie') await loadSmoothie(paths[0]);
        else await loadVideo(paths[0]);
      }
    });
    const u2 = listen('tauri://drag-enter', () => { isDragging = true; });
    const u3 = listen('tauri://drag-leave', () => { isDragging = false; });
    const u4 = listen('live-log', (event) => { parseLogLine(event.payload); });
    return () => { u1.then(f => f()); u2.then(f => f()); u3.then(f => f()); u4.then(f => f()); };
  });

  // --- RIFE Handlers ---
  async function loadVideo(path) {
    videoPath = path;
    isLoading = true;
    isComplete = false;
    lastOutputPath = '';
    rifeOutputPath = '';
    jobPhase = 'idle';
    jobError = '';
    resetRunState();
    try {
      videoInfo = await invoke('analyze_video', { videoPath: path });
      showToast(`Loaded ${videoInfo.width}x${videoInfo.height} @ ${videoInfo.fps.toFixed(2)} FPS`, 'success');
    } catch (e) {
      showToast(`Error: ${e}`, 'error');
      videoPath = '';
      videoInfo = null;
    } finally {
      isLoading = false;
    }
  }

  async function pickFile() {
    const path = await invoke('open_file_dialog');
    if (path) await loadVideo(path);
  }

  async function startProcessing() {
    if (!videoPath || anyProcessing) return;
    isProcessing = true;
    isComplete = false;
    lastOutputPath = '';
    rifeOutputPath = '';
    jobError = '';
    jobPhase = 'rife';
    resetRunState();
    appendLog('[CIA RENDER] RIFE 4.26 started');
    try {
      const outputPath = await invoke('run_time_remap', {
        videoPath,
        mode: rifeSettings.mode,
        factor: Number(rifeSettings.factor),
        crf: Number(rifeSettings.crf),
        preset: rifeSettings.preset,
        sceneThreshold: Number(rifeSettings.sceneThreshold),
        blendCuts: Number(rifeSettings.blendCuts)
      });
      rifeOutputPath = outputPath;
      lastOutputPath = outputPath;
      appendLog(`[CIA RENDER] RIFE output verified: ${outputPath}`);

      if (autoRender) {
        jobPhase = 'smoothie';
        const smoothiePath = await runSmoothieFor(outputPath, { preserveLogs: true });
        lastOutputPath = smoothiePath;
        appendLog(`[CIA RENDER] Smoothie output verified: ${smoothiePath}`);
      }

      progress = 100;
      jobPhase = 'complete';
      isComplete = true;
      playCompletionChime();
      showToast(autoRender ? 'Interpolation and Smoothie render complete!' : 'Interpolation complete!', 'success');
    } catch (e) {
      jobError = String(e);
      if (rifeOutputPath) {
        lastOutputPath = rifeOutputPath;
        isComplete = true;
      }
      jobPhase = 'failed';
      showToast(`Process failed: ${e}`, 'error');
    } finally {
      isProcessing = false;
    }
  }

  function resetInterpolation() {
    videoPath = '';
    videoInfo = null;
    isComplete = false;
    rifeOutputPath = '';
    lastOutputPath = '';
    jobPhase = 'idle';
    jobError = '';
    resetRunState();
  }

  async function renderRifeWithSmoothie() {
    if (!rifeOutputPath || anyProcessing) return;
    isProcessing = true;
    isComplete = false;
    jobError = '';
    jobPhase = 'smoothie';
    try {
      const smoothiePath = await runSmoothieFor(rifeOutputPath, { preserveLogs: true });
      lastOutputPath = smoothiePath;
      appendLog(`[CIA RENDER] Smoothie output verified: ${smoothiePath}`);
      progress = 100;
      jobPhase = 'complete';
      isComplete = true;
      playCompletionChime();
      showToast('Smoothie render complete!', 'success');
    } catch (e) {
      jobError = String(e);
      lastOutputPath = rifeOutputPath;
      jobPhase = 'failed';
      isComplete = true;
      showToast(`Smoothie failed: ${e}`, 'error');
    } finally {
      isProcessing = false;
    }
  }

  async function openFile() {
    if (!lastOutputPath) return;
    try { await invoke('open_target_file', { path: lastOutputPath }); }
    catch (e) { showToast(`Failed to open file: ${e}`, 'error'); }
  }

  async function openFolder() {
    if (!lastOutputPath) return;
    try { await invoke('open_target_folder', { path: lastOutputPath }); }
    catch (e) { showToast(`Failed to open folder: ${e}`, 'error'); }
  }

  // --- Smoothie Handlers ---
  async function loadSmoothie(path) {
    smoothiePath = path;
    isSmoothieLoading = true;
    isSmoothieComplete = false;
    smoothieOutputPath = '';
    resetRunState();
    try {
      smoothieInfo = await invoke('analyze_video', { videoPath: path });
      showToast(`Loaded ${smoothieInfo.width}x${smoothieInfo.height} @ ${smoothieInfo.fps.toFixed(2)} FPS`, 'success');
    } catch (e) {
      showToast(`Error: ${e}`, 'error');
      smoothiePath = '';
      smoothieInfo = null;
    } finally {
      isSmoothieLoading = false;
    }
  }

  async function pickSmoothieFile() {
    const path = await invoke('open_file_dialog');
    if (path) await loadSmoothie(path);
  }

  function smoothieOverrides() {
    return [
      `frame blending;fps;${smoothieSettings.fps}`,
      `frame blending;intensity;${Number(smoothieSettings.blendIntensity).toFixed(1)}`,
      `color grading;brightness;${smoothieSettings.brightness}`,
      `color grading;saturation;${smoothieSettings.saturation}`,
      `color grading;contrast;${smoothieSettings.contrast}`,
      `lut;enabled;${smoothieSettings.lutEnabled}`,
      `lut;opacity;${smoothieSettings.lutOpacity}`,
      `console;borderless;${smoothieSettings.borderless}`
    ];
  }

  async function runSmoothieFor(inputPath, { preserveLogs = false } = {}) {
    if (!preserveLogs) resetRunState();
    else resetTelemetry();
    appendLog('[CIA RENDER] SMOOTHIE started');
    return invoke('run_smoothie', { videoPath: inputPath, overrides: smoothieOverrides() });
  }

  async function startSmoothie() {
    if (!smoothiePath || anyProcessing) return;
    isSmoothieProcessing = true;
    isSmoothieComplete = false;
    smoothieOutputPath = '';

    try {
      const outPath = await runSmoothieFor(smoothiePath);
      progress = 100;
      isSmoothieComplete = true;
      smoothieOutputPath = outPath;
      playCompletionChime();
      showToast('Smoothie render complete!', 'success');
    } catch (e) {
      showToast(`Smoothie failed: ${e}`, 'error');
    } finally {
      isSmoothieProcessing = false;
    }
  }

  async function openSmoothieFile() {
    if (!smoothieOutputPath) return;
    try { await invoke('open_target_file', { path: smoothieOutputPath }); }
    catch (e) { showToast(`Failed to open file: ${e}`, 'error'); }
  }

  async function openSmoothieFolder() {
    if (!smoothieOutputPath) return;
    try { await invoke('open_target_folder', { path: smoothieOutputPath }); }
    catch (e) { showToast(`Failed to open folder: ${e}`, 'error'); }
  }
</script>

<div class="app-root" class:dragging={isDragging}>
  <!-- Custom Windows Titlebar -->
  <div class="titlebar" data-tauri-drag-region>
    <div class="titlebar-brand">
      <span class="titlebar-text">CIA RENDER</span>
    </div>
    <div class="titlebar-controls">
      <button class="titlebar-btn" onclick={() => appWindow.minimize()} aria-label="Minimize">─</button>
      <button class="titlebar-btn close" onclick={() => appWindow.close()} aria-label="Close">✕</button>
    </div>
  </div>

  <nav class="tab-bar">
    <button class:active={activePage === 'dashboard'} onclick={() => activePage = 'dashboard'}>INTERPOLATION</button>
    <button class:active={activePage === 'smoothie'} onclick={() => activePage = 'smoothie'}>SMOOTHIE</button>
  </nav>

  <!-- Main Content Area -->
  <main class="content-area">
    <!-- DASHBOARD PAGE (RIFE) -->
    {#if activePage === 'dashboard'}
      {#if !videoPath}
        <div class="drop-zone" class:dragging={isDragging} onclick={pickFile} role="button" tabindex="0">
          <p>DRAG VIDEO</p>
        </div>
      {:else if isLoading}
        <div class="loading-state"><p>ANALYZING VIDEO MATRIX...</p></div>
      {:else if videoInfo}
        {#if isProcessing}
          <div class="pro-render-card">
            <header class="pro-header">
              <div class="pro-title-group">
                <span class="pro-dot active"></span>
                <h3 class="pro-filename">{videoPath.split(/[\\/]/).pop()}</h3>
              </div>
              <span class="pro-engine-badge">{jobPhase === 'smoothie' ? 'SMOOTHIE-RS ENGINE' : 'RIFE 4.26 ENGINE'}</span>
            </header>

            <div class="pro-pipeline-box">
              <div class="pipeline-node">
                <span class="node-label">INPUT</span>
                <span class="node-val">{videoInfo.width}×{videoInfo.height} @ {videoInfo.fps.toFixed(0)} FPS</span>
              </div>
              <div class="pipeline-arrow">➔</div>
              <div class="pipeline-node">
                <span class="node-label">OUTPUT</span>
                <span class="node-val">{videoInfo.width}×{videoInfo.height} @ {outputFps.toFixed(0)} FPS ({rifeSettings.factor}x)</span>
              </div>
              <div class="pipeline-tags">
                <span class="chip">H.264 CRF {rifeSettings.crf}</span>
                <span class="chip">{rifeSettings.preset}</span>
              </div>
            </div>

            <div class="pro-telemetry-grid">
              <div class="telemetry-cell">
                <span class="telemetry-label">STATUS</span>
                <span class="telemetry-val highlight">
                  {jobPhase === 'smoothie'
                    ? (progress >= 99 || remainingTime === 'Encoding export...' ? 'SMOOTHIE ENCODING' : 'SMOOTHIE RENDERING')
                    : (progress >= 99 || remainingTime === 'Encoding export...' ? 'RIFE ENCODING' : 'RIFE PROCESSING')}
                </span>
              </div>
              <div class="telemetry-cell">
                <span class="telemetry-label">ELAPSED</span>
                <span class="telemetry-val mono">{elapsedTime}</span>
              </div>
              <div class="telemetry-cell">
                <span class="telemetry-label">EST. REMAINING</span>
                <span class="telemetry-val mono">{remainingTime}</span>
              </div>
              <div class="telemetry-cell">
                <span class="telemetry-label">PROGRESS</span>
                <span class="telemetry-val mono">{progress}%</span>
              </div>
            </div>

            <div class="pro-progress-row">
              <div class="pro-track">
                <div class="pro-fill" style="width: {progress}%"></div>
              </div>
              <span class="pro-percent-readout">{progress}%</span>
            </div>
            {#if progress >= 99 || remainingTime === 'Encoding export...'}
              <div class="pro-progress-row">
                <div class="pro-track">
                  <div class="pro-fill-encoding"></div>
                </div>
                <span class="pro-percent-readout encoding-label">ENCODING</span>
              </div>
            {/if}
          </div>
        {:else if isComplete}
          <div class="pro-complete-card">
            {#if jobError}
              <span class="completion-error">{jobError}</span>
            {/if}
            <div class="pro-output-box">
              <span class="box-label">EXPORTED FILE</span>
              <span class="box-path">{lastOutputPath.split(/[\\/]/).pop()}</span>
            </div>

            <div class="complete-actions-row">
              <button class="btn-pro-secondary" onclick={openFile}>OPEN FILE</button>
              <button class="btn-pro-secondary" onclick={openFolder}>REVEAL IN EXPLORER</button>
              {#if canRenderSmoothie}
                <button class="btn-pro-secondary" onclick={renderRifeWithSmoothie}>{jobPhase === 'failed' ? 'RETRY SMOOTHIE' : 'RENDER (SMOOTHIE)'}</button>
              {/if}
              <button class="btn-pro-secondary" onclick={resetInterpolation}>NEW RENDER</button>
            </div>
          </div>
        {:else}
          <div class="minimal-grid">
            <!-- Video Summary Card -->
            <div class="card">
              <h3>VIDEO INFO</h3>
              <div class="info-row"><span>File</span><span class="mono">{videoPath.split(/[\\/]/).pop()}</span></div>
              <div class="info-row"><span>Resolution</span><span>{videoInfo.width} × {videoInfo.height}</span></div>
              <div class="info-row"><span>Source FPS</span><span>{videoInfo.fps.toFixed(2)}</span></div>
              <div class="info-row"><span>Duration</span><span>{videoInfo.duration.toFixed(2)}s</span></div>
              <button class="btn-secondary" onclick={() => { videoPath = ''; videoInfo = null; }}>CHANGE VIDEO</button>
            </div>

            <!-- Quick Action Card -->
            <div class="card action-card">
              <div class="card-header">
                <h3>INTERPOLATION FACTOR</h3>
                <button class="btn-icon-settings" onclick={() => showRifeSettings = true}>⚙ SETTINGS</button>
              </div>

              <!-- Factor Slider 2x to 10x -->
              <GlowSlider bind:value={rifeSettings.factor} min={2} max={10} step={1} label="FACTOR:" unit="x" />

              <label class="auto-render-toggle">
                <input type="checkbox" bind:checked={autoRender} onchange={saveAutoRender} />
                <span>AUTO-RENDER → SMOOTHIE</span>
              </label>

              <div class="output-preview">
                <span>Out: {outputFps.toFixed(0)} FPS</span>
                <span>Dur: {outputDuration.toFixed(2)}s</span>
              </div>

              <button class="btn-primary" onclick={startProcessing} disabled={anyProcessing}>
                {isProcessing ? 'PROCESSING...' : 'START INTERPOLATION'}
              </button>
            </div>
          </div>
        {/if}
      {/if}

    <!-- SMOOTHIE PAGE -->
    {:else if activePage === 'smoothie'}
      {#if !smoothiePath}
        <div class="drop-zone" class:dragging={isDragging} onclick={pickSmoothieFile} role="button" tabindex="0">
          <p>DRAG VIDEO</p>
        </div>
      {:else if isSmoothieLoading}
        <div class="loading-state"><p>ANALYZING VIDEO MATRIX...</p></div>
      {:else if smoothieInfo}
        {#if isSmoothieProcessing}
          <div class="pro-render-card">
            <header class="pro-header">
              <div class="pro-title-group">
                <span class="pro-dot active"></span>
                <h3 class="pro-filename">{smoothiePath.split(/[\\/]/).pop()}</h3>
              </div>
              <span class="pro-engine-badge">SMOOTHIE-RS ENGINE</span>
            </header>

            <div class="pro-pipeline-box">
              <div class="pipeline-node">
                <span class="node-label">INPUT</span>
                <span class="node-val">{smoothieInfo.width}×{smoothieInfo.height} @ {smoothieInfo.fps.toFixed(0)} FPS</span>
              </div>
              <div class="pipeline-arrow">➔</div>
              <div class="pipeline-node">
                <span class="node-label">OUTPUT</span>
                <span class="node-val">{smoothieInfo.width}×{smoothieInfo.height} @ {smoothieSettings.fps} FPS</span>
              </div>
              <div class="pipeline-tags">
                <span class="chip">LUT: {smoothieSettings.lutEnabled === 'yes' ? 'ON' : 'OFF'}</span>
                <span class="chip">CRF 18</span>
              </div>
            </div>

            <div class="pro-telemetry-grid">
              <div class="telemetry-cell">
                <span class="telemetry-label">STATUS</span>
                <span class="telemetry-val highlight">
                  {progress >= 99 || remainingTime === 'Encoding export...' ? 'ENCODING EXPORT' : 'RENDERING'}
                </span>
              </div>
              <div class="telemetry-cell">
                <span class="telemetry-label">ELAPSED</span>
                <span class="telemetry-val mono">{elapsedTime}</span>
              </div>
              <div class="telemetry-cell">
                <span class="telemetry-label">EST. REMAINING</span>
                <span class="telemetry-val mono">{remainingTime}</span>
              </div>
              <div class="telemetry-cell">
                <span class="telemetry-label">PROGRESS</span>
                <span class="telemetry-val mono">{progress}%</span>
              </div>
            </div>

            <div class="pro-progress-row">
              <div class="pro-track">
                <div class="pro-fill" style="width: {progress}%"></div>
              </div>
              <span class="pro-percent-readout">{progress}%</span>
            </div>
            {#if progress >= 99 || remainingTime === 'Encoding export...'}
              <div class="pro-progress-row">
                <div class="pro-track">
                  <div class="pro-fill-encoding"></div>
                </div>
                <span class="pro-percent-readout encoding-label">ENCODING</span>
              </div>
            {/if}
          </div>
        {:else if isSmoothieComplete}
          <div class="pro-complete-card">
            <div class="pro-output-box">
              <span class="box-label">EXPORTED FILE</span>
              <span class="box-path">{smoothieOutputPath.split(/[\\/]/).pop()}</span>
            </div>

            <div class="complete-actions-row">
              <button class="btn-pro-secondary" onclick={openSmoothieFile}>OPEN FILE</button>
              <button class="btn-pro-secondary" onclick={openSmoothieFolder}>REVEAL IN EXPLORER</button>
              <button class="btn-pro-secondary" onclick={() => { smoothiePath = ''; smoothieInfo = null; isSmoothieComplete = false; }}>NEW RENDER</button>
            </div>
          </div>
        {:else}
          <div class="minimal-grid">
            <!-- Video Summary Card -->
            <div class="card">
              <h3>VIDEO INFO</h3>
              <div class="info-row"><span>File</span><span class="mono">{smoothiePath.split(/[\\/]/).pop()}</span></div>
              <div class="info-row"><span>Resolution</span><span>{smoothieInfo.width} × {smoothieInfo.height}</span></div>
              <div class="info-row"><span>Source FPS</span><span>{smoothieInfo.fps.toFixed(2)}</span></div>
              <div class="info-row"><span>Duration</span><span>{smoothieInfo.duration.toFixed(2)}s</span></div>
              <button class="btn-secondary" onclick={() => { smoothiePath = ''; smoothieInfo = null; }}>CHANGE VIDEO</button>
            </div>

            <!-- Quick Action Card -->
            <div class="card action-card">
              <div class="card-header">
                <h3>OUTPUT TARGET FPS</h3>
                <button class="btn-icon-settings" onclick={() => showSmoothieSettings = true}>⚙ SETTINGS</button>
              </div>

              <!-- Output FPS Slider 20 to 60 FPS -->
              <GlowSlider bind:value={smoothieSettings.fps} min={20} max={60} step={1} label="TARGET FPS:" unit=" FPS" />

              <div class="output-preview">
                <span>Engine: smoothie-rs</span>
                <span>LUT: {smoothieSettings.lutEnabled === 'yes' ? 'ON' : 'OFF'}</span>
              </div>

              <button class="btn-primary" onclick={startSmoothie} disabled={anyProcessing}>
                {isSmoothieProcessing ? 'PROCESSING...' : 'START SMOOTHIE'}
              </button>
            </div>
          </div>
        {/if}
      {/if}
    {/if}
  </main>

  <!-- RIFE SETTINGS MODAL DRAWER -->
  {#if showRifeSettings}
    <div class="modal-backdrop" onclick={() => showRifeSettings = false} role="presentation">
      <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
        <div class="modal-header">
          <h2>RIFE SETTINGS</h2>
          <button class="btn-close-modal" onclick={() => showRifeSettings = false}>✕</button>
        </div>
        <div class="modal-body">
          <div class="setting-group">
            <h3>CORE CONFIGURATION</h3>
            <div class="setting-row">
              <label for="mod-rife-mode" class="has-tooltip" data-tooltip="Slowmo extends video duration; Boost doubles FPS at normal speed.">MODE</label>
              <select id="mod-rife-mode" bind:value={rifeSettings.mode}>
                <option value="boost">FPS Boost (same duration)</option>
                <option value="slowmo">Slowmo (duration × factor)</option>
              </select>
            </div>
            <div class="setting-row">
              <label for="mod-rife-factor" class="has-tooltip" data-tooltip="Multiplier factor (2x to 10x).">FACTOR</label>
              <input id="mod-rife-factor" type="number" min="2" max="10" bind:value={rifeSettings.factor} />
            </div>
          </div>

          <div class="setting-group">
            <h3>ADVANCED PARAMETERS</h3>
            <div class="setting-row">
              <label for="mod-rife-thresh" class="has-tooltip" data-tooltip="Threshold for detecting hard scene changes (0.01 - 0.50).">SCENE THRESHOLD</label>
              <input id="mod-rife-thresh" type="number" step="0.01" min="0.01" max="0.5" bind:value={rifeSettings.sceneThreshold} />
            </div>
            <div class="setting-row">
              <label for="mod-rife-blend" class="has-tooltip" data-tooltip="Crossfade frames at scene cuts (0 = hard cut).">BLEND CUTS</label>
              <input id="mod-rife-blend" type="number" step="1" min="0" max="30" bind:value={rifeSettings.blendCuts} />
            </div>
            <div class="setting-row">
              <label for="mod-rife-crf" class="has-tooltip" data-tooltip="H.264 CRF quality factor (18 = visually lossless).">CRF QUALITY</label>
              <input id="mod-rife-crf" type="number" step="1" min="0" max="51" bind:value={rifeSettings.crf} />
            </div>
            <div class="setting-row">
              <label for="mod-rife-preset" class="has-tooltip" data-tooltip="H.264 encoding preset speed vs compression ratio.">ENCODING PRESET</label>
              <select id="mod-rife-preset" bind:value={rifeSettings.preset}>
                <option value="ultrafast">ultrafast</option>
                <option value="fast">fast</option>
                <option value="medium">medium</option>
                <option value="slow">slow</option>
              </select>
            </div>
          </div>
        </div>
        <div class="modal-footer">
          <button class="btn-secondary" onclick={resetRifeSettings}>RESET DEFAULTS</button>
          <button class="btn-primary-modal" onclick={() => { saveRifeSettings(); showRifeSettings = false; }}>SAVE SETTINGS</button>
        </div>
      </div>
    </div>
  {/if}

  <!-- SMOOTHIE SETTINGS MODAL DRAWER -->
  {#if showSmoothieSettings}
    <div class="modal-backdrop" onclick={() => showSmoothieSettings = false} role="presentation">
      <div class="modal-card" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true">
        <div class="modal-header">
          <h2>SMOOTHIE CONFIGURATION</h2>
          <button class="btn-close-modal" onclick={() => showSmoothieSettings = false}>✕</button>
        </div>
        <div class="modal-body">
          <div class="setting-group">
            <h3>OUTPUT PARAMS</h3>
            <div class="setting-row">
              <label for="mod-sm-fps" class="has-tooltip" data-tooltip="Target frame blending output FPS.">OUTPUT FPS</label>
              <input id="mod-sm-fps" type="number" min="20" max="60" bind:value={smoothieSettings.fps} />
            </div>
            <div class="slider-row">
              <GlowSlider bind:value={smoothieSettings.blendIntensity} min={0} max={4} step={0.1} precision={1} label="BLEND INTENSITY:" />
            </div>
          </div>

          <div class="setting-group">
            <h3>COLOR GRADING</h3>
            <div class="slider-row">
              <div class="slider-header"><span class="slider-label">BRIGHTNESS:</span><span class="slider-val">{smoothieSettings.brightness}</span></div>
              <input type="range" min="0.0" max="2.0" step="0.05" bind:value={smoothieSettings.brightness} class="custom-slider" />
            </div>
            <div class="slider-row">
              <div class="slider-header"><span class="slider-label">SATURATION:</span><span class="slider-val">{smoothieSettings.saturation}</span></div>
              <input type="range" min="0.0" max="2.0" step="0.05" bind:value={smoothieSettings.saturation} class="custom-slider" />
            </div>
            <div class="slider-row">
              <div class="slider-header"><span class="slider-label">CONTRAST:</span><span class="slider-val">{smoothieSettings.contrast}</span></div>
              <input type="range" min="0.0" max="2.0" step="0.05" bind:value={smoothieSettings.contrast} class="custom-slider" />
            </div>
          </div>

          <div class="setting-group">
            <h3>LUT &amp; DISPLAY</h3>
            <div class="setting-row">
              <label for="mod-sm-lutenable" class="has-tooltip" data-tooltip="Enable colorcia.cube LUT application.">LUT ENABLED</label>
              <select id="mod-sm-lutenable" bind:value={smoothieSettings.lutEnabled}>
                <option value="yes">yes</option>
                <option value="no">no</option>
              </select>
            </div>
            <div class="slider-row">
              <div class="slider-header"><span class="slider-label">LUT OPACITY:</span><span class="slider-val">{(smoothieSettings.lutOpacity * 100).toFixed(0)}%</span></div>
              <input type="range" min="0.0" max="1.0" step="0.05" bind:value={smoothieSettings.lutOpacity} class="custom-slider" />
            </div>
            <div class="setting-row">
              <label for="mod-sm-borderless" class="has-tooltip" data-tooltip="Window borderless console toggle.">BORDERLESS</label>
              <select id="mod-sm-borderless" bind:value={smoothieSettings.borderless}>
                <option value="yes">yes</option>
                <option value="no">no</option>
              </select>
            </div>
          </div>
        </div>
        <div class="modal-footer">
          <button class="btn-secondary" onclick={resetSmoothieSettings}>RESET DEFAULTS</button>
          <button class="btn-primary-modal" onclick={() => { saveSmoothieSettings(); showSmoothieSettings = false; }}>SAVE CONFIG</button>
        </div>
      </div>
    </div>
  {/if}

  <!-- Footer -->
  <footer class="app-footer">
    <button class="btn-copy" onclick={copyLogsToClipboard}>
      {copyFeedback ? 'COPIED TO CLIPBOARD' : 'COPY LOGS'}
    </button>
  </footer>

  <!-- Toast Notification Overlay -->
  {#if toast.show}
    <div class="toast" class:success={toast.type === 'success'} class:error={toast.type === 'error'}>
      {toast.message}
    </div>
  {/if}
</div>

<style>
  /* REFINED INDUSTRIAL DARK SLATE DESIGN SYSTEM */
  *, *::before, *::after {
    box-sizing: border-box;
    margin: 0;
    padding: 0;
    font-family: 'IBM Plex Sans Variable', 'IBM Plex Sans', -apple-system, sans-serif;
    -webkit-font-smoothing: antialiased;
  }

  :global(html), :global(body), :global(#app) {
    margin: 0;
    height: 100%;
    background: #050507;
    color: #e4e4e7;
    overflow: hidden;
    user-select: none;
  }

  .app-root {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: #050507;
    border: 1px solid #1c1c20;
  }

  /* Titlebar */
  .titlebar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    height: 34px;
    padding: 0 12px;
    background: #08080a;
    border-bottom: 1px solid #1c1c20;
  }

  .titlebar-brand { display: flex; align-items: center; }
  .titlebar-text { font-size: 11px; font-weight: 700; letter-spacing: 0.06em; color: #71717a; }
  .titlebar-controls { display: flex; gap: 2px; }

  .titlebar-btn {
    width: 32px;
    height: 24px;
    border: none;
    background: transparent;
    color: #71717a;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 11px;
    border-radius: 4px;
    transition: all 0.15s ease;
  }

  .titlebar-btn:hover { background: #1c1c20; color: #ffffff; }
  .titlebar-btn.close:hover { background: #dc2626; color: #ffffff; }

  /* Navigation Tabs */
  .tab-bar {
    display: flex;
    gap: 4px;
    padding: 8px 12px 0;
    background: #08080a;
    border-bottom: 1px solid #1c1c20;
  }

  .tab-bar button {
    padding: 8px 20px;
    background: #0d0d10;
    border: 1px solid #1c1c20;
    border-bottom: none;
    border-radius: 6px 6px 0 0;
    color: #71717a;
    cursor: pointer;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .tab-bar button:hover { color: #e4e4e7; background: #16161a; }
  .tab-bar button.active {
    color: #ffffff;
    background: #121215;
    border-color: rgba(255, 255, 255, 0.25);
    border-bottom: 1px solid #121215;
  }

  /* Main Content Area */
  .content-area {
    flex: 1;
    overflow-y: auto;
    padding: 16px;
    background: #050507;
    display: flex;
    flex-direction: column;
    justify-content: center;
  }

  /* Drop Zone */
  .drop-zone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 380px;
    border: 1px solid rgba(255, 255, 255, 0.16);
    border-radius: 8px;
    background: #09090c;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .drop-zone:hover {
    border-color: rgba(255, 255, 255, 0.4);
    background: #111116;
    box-shadow: inset 0 0 20px rgba(255, 255, 255, 0.02);
  }

  .drop-zone p {
    font-size: 16px;
    font-weight: 700;
    letter-spacing: 0.05em;
    margin: 0 0 6px;
    color: #e4e4e7;
  }

  .drop-zone span {
    color: #71717a;
    font-size: 11px;
    letter-spacing: 0.05em;
  }

  .loading-state {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 200px;
    color: #71717a;
    font-size: 12px;
    letter-spacing: 0.05em;
    font-weight: 700;
  }

  /* Minimal Cards Grid */
  .minimal-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(320px, 1fr));
    gap: 16px;
    align-content: center;
  }

  .card {
    background: #09090c;
    border: 1px solid #1c1c20;
    border-radius: 8px;
    padding: 18px;
    transition: all 0.2s ease;
  }

  .card:hover { border-color: rgba(255, 255, 255, 0.18); }

  .card h3 {
    margin: 0 0 16px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: #71717a;
  }

  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 16px;
  }

  .card-header h3 { margin: 0; }

  .btn-icon-settings {
    background: #141417;
    color: #e4e4e7;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    padding: 4px 10px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.05em;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .btn-icon-settings:hover {
    border-color: rgba(255, 255, 255, 0.35);
    background: #1c1c20;
  }

  .info-row, .setting-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }

  .info-row span:first-child, .setting-row label {
    color: #888888;
    font-size: 12px;
    font-weight: 600;
  }

  /* Smooth Round Range Sliders */
  .slider-row {
    margin-bottom: 16px;
  }

  .slider-header {
    display: flex;
    justify-content: space-between;
    margin-bottom: 6px;
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

  .custom-slider {
    -webkit-appearance: none;
    appearance: none;
    width: 100%;
    height: 6px;
    background: linear-gradient(90deg, #ffffff 0%, #ffffff var(--pct, 0%), #141417 var(--pct, 0%), #141417 100%);
    border: 1px solid #27272a;
    border-radius: 10px;
    outline: none;
    cursor: pointer;
    transition: background 0.1s ease, border-color 0.15s ease;
  }

  .custom-slider:hover {
    border-color: rgba(255, 255, 255, 0.35);
  }

  .custom-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50% !important;
    background: #ffffff;
    border: 2px solid #ffffff;
    box-shadow: 0 0 6px rgba(255, 255, 255, 0.3);
    cursor: pointer;
    transition: transform 0.15s ease, box-shadow 0.15s ease;
  }

  .custom-slider::-webkit-slider-thumb:hover {
    transform: scale(1.15);
    box-shadow: 0 0 10px rgba(255, 255, 255, 0.5);
  }

  .custom-slider::-webkit-slider-thumb:active {
    transform: scale(1.1);
  }

  .custom-slider::-moz-range-thumb {
    width: 14px;
    height: 14px;
    border-radius: 50% !important;
    background: #ffffff;
    border: 2px solid #ffffff;
    box-shadow: 0 0 6px rgba(255, 255, 255, 0.3);
    cursor: pointer;
    transition: transform 0.15s ease, box-shadow 0.15s ease;
  }

  .custom-slider::-moz-range-thumb:hover {
    transform: scale(1.15);
    box-shadow: 0 0 10px rgba(255, 255, 255, 0.5);
  }

  .custom-slider::-moz-range-thumb:active {
    transform: scale(1.1);
  }

  .mono {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 12px;
    max-width: 60%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: #e4e4e7;
  }

  .setting-row select, .setting-row input[type="number"] {
    background: #050507;
    border: 1px solid #27272a;
    border-radius: 6px;
    color: #ffffff;
    padding: 6px 10px;
    font-size: 12px;
    min-width: 150px;
    outline: none;
    transition: all 0.15s ease;
  }

  .setting-row select:focus, .setting-row input[type="number"]:focus {
    border-color: rgba(255, 255, 255, 0.4);
  }

  .output-preview {
    display: flex;
    justify-content: space-between;
    margin: 16px 0;
    padding: 10px;
    background: #050507;
    border: 1px solid #1c1c20;
    border-radius: 6px;
    font-size: 12px;
    color: #a1a1aa;
  }

  /* Buttons */
  .btn-primary {
    width: 100%;
    padding: 12px;
    background: #18181b;
    color: #ffffff;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 6px;
    font-weight: 700;
    font-size: 12px;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .btn-primary:hover:not(:disabled) {
    background: #242429;
    border-color: rgba(255, 255, 255, 0.4);
    box-shadow: 0 0 15px rgba(255, 255, 255, 0.08);
  }

  .btn-primary:disabled { opacity: 0.3; cursor: not-allowed; }

  .btn-secondary {
    margin-top: 10px;
    padding: 8px 14px;
    background: #141417;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    color: #d4d4d8;
    cursor: pointer;
    font-size: 11px;
    font-weight: 700;
    transition: all 0.15s ease;
  }

  .btn-secondary:hover {
    border-color: rgba(255, 255, 255, 0.35);
    background: #1c1c20;
  }

  /* Process Panel */
  .process-panel {
    margin-top: 16px;
    background: #09090c;
    border: 1px solid #1c1c20;
    border-radius: 8px;
    padding: 14px;
  }

  .progress-wrap {
    height: 6px;
    background: #050507;
    border: 1px solid #1c1c20;
    border-radius: 6px;
    overflow: hidden;
    margin-bottom: 12px;
  }

  .progress-bar {
    height: 100%;
    background: rgba(255, 255, 255, 0.85);
    border-radius: 6px;
    transition: width 0.15s linear;
  }

  .status-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .status-text {
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: #e4e4e7;
  }

  .timer-box {
    background: #050507;
    border: 1px solid #27272a;
    border-radius: 4px;
    padding: 4px 10px;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 11px;
    color: #ffffff;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .timer-box:hover { border-color: rgba(255, 255, 255, 0.4); }

  .complete-banner {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: #e4e4e7;
  }

  .action-buttons { display: flex; gap: 8px; }

  .btn-action {
    background: #18181b;
    color: #ffffff;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 6px;
    padding: 6px 14px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .btn-action:hover {
    background: #242429;
    border-color: rgba(255, 255, 255, 0.4);
    box-shadow: 0 0 12px rgba(255, 255, 255, 0.08);
  }

  /* Professional Render Card (Industrial Telemetry Layout) */
  .pro-render-card {
    background: #09090c;
    border: 1px solid #1c1c20;
    border-radius: 8px;
    padding: 18px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: 100%;
    min-height: 380px;
    justify-content: space-between;
  }

  .pro-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .pro-title-group {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .pro-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: #71717a;
  }
  .pro-dot.active { background: #ffffff; box-shadow: 0 0 6px rgba(255, 255, 255, 0.4); }
  .pro-dot.complete { background: #ffffff; box-shadow: 0 0 6px rgba(255, 255, 255, 0.4); }

  .pro-filename {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 13px;
    font-weight: 700;
    color: #ffffff;
    margin: 0;
  }

  .pro-engine-badge {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: #a1a1aa;
    background: #141417;
    border: 1px solid #27272a;
    border-radius: 4px;
    padding: 3px 8px;
  }

  /* Pipeline Transformation Box */
  .pro-pipeline-box {
    display: flex;
    align-items: center;
    gap: 14px;
    background: #050507;
    border: 1px solid #1c1c20;
    border-radius: 6px;
    padding: 12px 16px;
  }

  .pipeline-node {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .node-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: #71717a;
  }

  .node-val {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 12px;
    font-weight: 700;
    color: #e4e4e7;
  }

  .pipeline-arrow {
    color: #71717a;
    font-size: 12px;
  }

  .pipeline-tags {
    margin-left: auto;
    display: flex;
    gap: 6px;
  }

  .chip {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    color: #a1a1aa;
    background: #141417;
    border: 1px solid #27272a;
    border-radius: 4px;
    padding: 3px 8px;
  }

  /* Telemetry Grid */
  .pro-telemetry-grid {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 12px;
    background: #050507;
    border: 1px solid #1c1c20;
    border-radius: 6px;
    padding: 14px 16px;
  }

  .telemetry-cell {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .telemetry-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: #71717a;
  }

  .telemetry-val {
    font-size: 13px;
    font-weight: 700;
    color: #ffffff;
  }

  .telemetry-val.mono {
    font-family: 'IBM Plex Mono', monospace;
  }

  .auto-render-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    color: #a1a1aa;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.05em;
    cursor: pointer;
  }

  .auto-render-toggle input {
    appearance: none;
    width: 14px;
    height: 14px;
    margin: 0;
    border: 1px solid #52525b;
    border-radius: 3px;
    background: #09090c;
    display: grid;
    place-content: center;
  }

  .auto-render-toggle input::before {
    content: '';
    width: 7px;
    height: 7px;
    transform: scale(0);
    background: #ffffff;
    transition: transform 0.12s ease;
  }

  .auto-render-toggle input:checked::before { transform: scale(1); }
  .auto-render-toggle:hover { color: #ffffff; }

  .telemetry-val.highlight {
    color: #ffffff;
  }

  /* Integrated Progress Row */
  .pro-progress-row {
    display: flex;
    align-items: center;
    gap: 14px;
  }

  .pro-track {
    flex: 1;
    height: 6px;
    background: #050507;
    border: 1px solid #27272a;
    border-radius: 4px;
    overflow: hidden;
  }

  .pro-fill {
    height: 100%;
    background: #ffffff;
    border-radius: 4px;
    transition: width 0.15s linear;
  }

  .pro-fill-encoding {
    height: 100%;
    width: 40%;
    background: linear-gradient(90deg, transparent, #ffffff 50%, transparent);
    border-radius: 4px;
    animation: encoding-sweep 1.5s cubic-bezier(0.4, 0, 0.2, 1) infinite;
  }

  @keyframes encoding-sweep {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(350%); }
  }

  .pro-percent-readout {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 14px;
    font-weight: 800;
    color: #ffffff;
    min-width: 48px;
    text-align: right;
  }

  .encoding-label {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: #a1a1aa;
    min-width: 80px;
  }

  /* Professional Complete Card */
  .pro-complete-card {
    background: #09090c;
    border: 1px solid #1c1c20;
    border-radius: 8px;
    padding: 24px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 20px;
    height: 100%;
    min-height: 380px;
  }

  .complete-header-bar {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .complete-title {
    font-size: 13px;
    font-weight: 800;
    letter-spacing: 0.06em;
    color: #ffffff;
  }

  .pro-output-box {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    background: #050507;
    border: 1px solid #1c1c20;
    border-radius: 6px;
    padding: 12px 24px;
    width: 100%;
    max-width: 460px;
  }

  .box-label {
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: #71717a;
  }

  .box-path {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 12px;
    font-weight: 700;
    color: #e4e4e7;
  }

  .completion-error {
    max-width: 460px;
    color: #d4d4d8;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    line-height: 1.45;
    text-align: center;
  }

  .complete-actions-row {
    display: flex;
    gap: 12px;
  }

  .btn-pro-primary {
    background: #ffffff;
    color: #000000;
    border: 1px solid #ffffff;
    border-radius: 4px;
    padding: 9px 18px;
    font-size: 11px;
    font-weight: 800;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .btn-pro-primary:hover {
    background: #e4e4e7;
  }

  .btn-pro-secondary {
    background: #141417;
    color: #ffffff;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    padding: 9px 18px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .btn-pro-secondary:hover {
    background: #1c1c20;
    border-color: rgba(255, 255, 255, 0.4);
  }

  .btn-pro-ghost {
    background: transparent;
    color: #71717a;
    border: 1px solid #27272a;
    border-radius: 4px;
    padding: 9px 18px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .btn-pro-ghost:hover {
    color: #ffffff;
    border-color: rgba(255, 255, 255, 0.3);
  }

  /* Modal Settings Overlay Drawer */
  .modal-backdrop {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.75);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 3000;
  }

  .modal-card {
    background: #09090c;
    border: 1px solid rgba(255, 255, 255, 0.25);
    border-radius: 12px;
    width: 540px;
    max-width: 90vw;
    max-height: 85vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 10px 30px rgba(0, 0, 0, 0.8);
    overflow: hidden;
  }

  .modal-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 18px;
    background: #08080a;
    border-bottom: 1px solid #1c1c20;
  }

  .modal-header h2 {
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.05em;
    color: #ffffff;
  }

  .btn-close-modal {
    background: transparent;
    border: none;
    color: #71717a;
    cursor: pointer;
    font-size: 12px;
  }

  .btn-close-modal:hover { color: #ffffff; }

  .modal-body {
    padding: 18px;
    overflow-y: auto;
    flex: 1;
  }

  .setting-group {
    margin-bottom: 20px;
    padding-bottom: 16px;
    border-bottom: 1px solid #1c1c20;
  }

  .setting-group:last-child {
    border-bottom: none;
    margin-bottom: 0;
    padding-bottom: 0;
  }

  .setting-group h3 {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: #71717a;
    margin-bottom: 14px;
  }

  .modal-footer {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 14px 18px;
    background: #08080a;
    border-top: 1px solid #1c1c20;
  }

  .btn-primary-modal {
    background: #ffffff;
    color: #000000;
    border: 1px solid #ffffff;
    border-radius: 6px;
    padding: 8px 18px;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .btn-primary-modal:hover {
    background: #000000;
    color: #ffffff;
    border-color: #ffffff;
  }

  /* Tooltip System */
  .has-tooltip { position: relative; cursor: help; }
  .has-tooltip::after {
    content: attr(data-tooltip);
    position: absolute;
    bottom: 130%;
    left: 0;
    background: #141418;
    color: #e4e4e7;
    border: 1px solid rgba(255, 255, 255, 0.2);
    border-radius: 4px;
    padding: 6px 10px;
    font-size: 11px;
    font-weight: 400;
    white-space: nowrap;
    pointer-events: none;
    opacity: 0;
    visibility: hidden;
    z-index: 4000;
    box-shadow: 0 4px 15px rgba(0, 0, 0, 0.6);
  }
  .has-tooltip:hover::after { opacity: 1; visibility: visible; }

  /* Footer */
  .app-footer {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    height: 36px;
    padding: 0 14px;
    background: #08080a;
    border-top: 1px solid #1c1c20;
  }

  .btn-copy {
    background: #141417;
    color: #e4e4e7;
    border: 1px solid rgba(255, 255, 255, 0.15);
    border-radius: 6px;
    padding: 4px 12px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .btn-copy:hover { border-color: rgba(255, 255, 255, 0.35); background: #1c1c20; }

  /* Toast Overlay */
  .toast {
    position: fixed;
    bottom: 46px;
    right: 14px;
    padding: 8px 14px;
    background: #121215;
    border: 1px solid rgba(255, 255, 255, 0.25);
    border-radius: 6px;
    color: #ffffff;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    z-index: 5000;
  }
</style>
