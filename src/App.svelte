<script>
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { check } from '@tauri-apps/plugin-updater';
  import { relaunch } from '@tauri-apps/plugin-process';
  import { onMount } from 'svelte';
  import GlowSlider from './GlowSlider.svelte';
  import ProjectMark from './ProjectMark.svelte';
  import appLogo from '../src-tauri/icons/128x128@2x.png';

  const appWindow =
    typeof window !== 'undefined' && window.__TAURI_INTERNALS__
      ? getCurrentWindow()
      : null;

  let activePage = $state('smoothie'); // 'smoothie' | 'dashboard' | 'about'
  let isDragging = $state(false);
  let logs = $state([]);
  let progress = $state(0);
  let elapsedTime = $state('00:00');
  let remainingTime = $state('--:--');
  let isHoveringTimer = $state(false);
  let copyFeedback = $state(false);
  let toast = $state({ show: false, message: '', type: 'info' });
  let runtimeSnapshot = $state(null);
  let setupDraft = $state(null);
  let showRuntimeSetup = $state(false);
  let isInstallingRifeEnvironment = $state(false);
  let installStep = $state(0);
  let installTotal = $state(0);
  let installLabel = $state('');
  let appVersion = $state('1.0.2');
  let discordCopyFeedback = $state(false);
  let shouldShowExecutionLogs = $state(false);

  // Auto-Updater State
  let updateState = $state('idle'); // 'idle' | 'checking' | 'available' | 'downloading' | 'ready' | 'error' | 'up-to-date'
  let availableUpdate = $state(null);
  let updateDownloadedBytes = $state(0);
  let updateContentLength = $state(0);
  let updateErrorMessage = $state('');
  let showUpdateModal = $state(false);

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
  let activeRenderJobId = $state('');
  let isRenderPaused = $state(false);
  let isCancellingRender = $state(false);
  let showRenderCancelConfirmation = $state(false);

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

  function cloneConfig(config) {
    return JSON.parse(JSON.stringify(config));
  }

  function hasStoredSettings(value) {
    return Boolean(value && typeof value === 'object' && Object.keys(value).length > 0);
  }

  async function persistUiPreferences() {
    if (!runtimeSnapshot) return;
    try {
      runtimeSnapshot = await invoke('save_ui_preferences', {
        autoRender,
        rifeSettings,
        smoothieSettings
      });
    } catch (e) {
      showToast(`Failed to save preferences: ${e}`, 'error');
    }
  }

  async function refreshRuntimeSnapshot() {
    runtimeSnapshot = await invoke('get_runtime_snapshot');
    setupDraft = cloneConfig(runtimeSnapshot.config);
    return runtimeSnapshot;
  }

  async function initializeRuntime() {
    try {
      const snapshot = await refreshRuntimeSnapshot();
      appVersion = await invoke('get_app_version');
      if (snapshot.config.ui?.migrated) {
        autoRender = Boolean(snapshot.config.ui.autoRender);
        if (hasStoredSettings(snapshot.config.ui.rifeSettings)) {
          rifeSettings = { ...DEFAULT_RIFE, ...snapshot.config.ui.rifeSettings };
        }
        if (hasStoredSettings(snapshot.config.ui.smoothieSettings)) {
          smoothieSettings = { ...DEFAULT_SMOOTHIE, ...snapshot.config.ui.smoothieSettings };
        }
      } else {
        await persistUiPreferences();
      }
      // Smoothie and the media tools are bundled with the app. RIFE is deliberately
      // opt-in because its CUDA environment is a large download.
      showRuntimeSetup = false;
    } catch (e) {
      showToast(`Runtime setup could not load: ${e}`, 'error');
      showRuntimeSetup = false;
    }
  }

  function loadAutoRender() {
    try {
      return localStorage.getItem('rife_auto_render') === 'true';
    } catch {
      return false;
    }
  }

  async function saveAutoRender() {
    await persistUiPreferences();
  }

  function loadRifeSettings() {
    try {
      const saved = localStorage.getItem('rife_settings');
      return saved ? { ...DEFAULT_RIFE, ...JSON.parse(saved) } : { ...DEFAULT_RIFE };
    } catch {
      return { ...DEFAULT_RIFE };
    }
  }

  async function saveRifeSettings() {
    await persistUiPreferences();
    showToast('RIFE settings saved', 'success');
  }

  async function resetRifeSettings() {
    rifeSettings = { ...DEFAULT_RIFE };
    await persistUiPreferences();
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

  const ABOUT_LINKS = [
    { name: 'Practical-RIFE', detail: 'Frame interpolation', mark: 'rife', url: 'https://github.com/hzwer/Practical-RIFE' },
    { name: 'smoothie-rs', detail: 'Frame blending', mark: 'smoothie', url: 'https://github.com/couleur-tweak-tips/smoothie-rs' },
    { name: 'VapourSynth', detail: 'Video processing', mark: 'vapoursynth', url: 'https://github.com/vapoursynth/vapoursynth' },
    { name: 'FFmpeg', detail: 'Media tooling', mark: 'ffmpeg', url: 'https://github.com/FFmpeg/FFmpeg' },
    { name: 'Tauri', detail: 'Desktop runtime', mark: 'tauri', url: 'https://github.com/tauri-apps/tauri' },
    { name: 'Svelte', detail: 'Interface framework', mark: 'svelte', url: 'https://github.com/sveltejs/svelte' },
    { name: 'IBM Plex', detail: 'Interface typography', mark: 'plex', url: 'https://github.com/IBM/plex' },
    { name: 'Flowframes', detail: 'Workflow reference', mark: 'flowframes', url: 'https://github.com/n00mkrad/flowframes' }
  ];
  const PROJECT_REPOSITORY_URL = 'https://github.com/cia213/cia-app';

  function loadSmoothieSettings() {
    try {
      const saved = localStorage.getItem('smoothie_settings');
      return saved ? { ...DEFAULT_SMOOTHIE, ...JSON.parse(saved) } : { ...DEFAULT_SMOOTHIE };
    } catch {
      return { ...DEFAULT_SMOOTHIE };
    }
  }

  async function saveSmoothieSettings() {
    await persistUiPreferences();
    showToast('Render configuration saved', 'success');
  }

  async function resetSmoothieSettings() {
    smoothieSettings = { ...DEFAULT_SMOOTHIE };
    await persistUiPreferences();
    showToast('Render configuration reset to default', 'info');
  }

  let anyProcessing = $derived(isProcessing || isSmoothieProcessing);
  let canRenderSmoothie = $derived(Boolean(rifeOutputPath) && lastOutputPath === rifeOutputPath && !anyProcessing);
  let canCopyLogs = $derived(Boolean(logs.length && shouldShowExecutionLogs));
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
    shouldShowExecutionLogs = false;
    resetTelemetry();
  }

  function beginLogCapture() {
    resetRunState();
    shouldShowExecutionLogs = true;
  }

  function createRenderJobId() {
    if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
    return `cia-render-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  }

  function isCancellation(error) {
    return String(error).includes('CIA_RENDER_CANCELLED');
  }

  async function toggleRenderPause() {
    if (!activeRenderJobId || jobPhase !== 'rife' || isCancellingRender) return;
    try {
      await invoke(isRenderPaused ? 'resume_render' : 'pause_render', { jobId: activeRenderJobId });
      isRenderPaused = !isRenderPaused;
      appendLog(`[cia app] RIFE ${isRenderPaused ? 'paused' : 'resumed'} by user`);
      showToast(isRenderPaused ? 'Interpolation paused' : 'Interpolation resumed', 'info');
    } catch (e) {
      showToast(`Unable to ${isRenderPaused ? 'resume' : 'pause'} interpolation: ${e}`, 'error');
    }
  }

  async function cancelRender() {
    if (!activeRenderJobId || isCancellingRender) return;
    showRenderCancelConfirmation = false;
    isCancellingRender = true;
    try {
      await invoke('cancel_render', { jobId: activeRenderJobId });
      appendLog('[cia app] Cancellation requested by user');
    } catch (e) {
      showToast(`Unable to cancel render: ${e}`, 'error');
      isCancellingRender = false;
    }
  }

  function appendLog(line) {
    logs = [...logs, line].slice(-500);
  }

  function activateOnKeyboard(event, action) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      action();
    }
  }

  function parseLogLine(line) {
    appendLog(line);
    if (line.includes('Finalizing output') || line.includes('FFmpeg') || /^frame=/.test(line)) {
      remainingTime = 'Encoding export...';
      if (progress < 99) progress = 99;
    }
    const rifePct = line.match(/^\s*(\d{1,3})%/);
    if (rifePct) progress = parseInt(rifePct[1], 10);
    const smPct = line.match(/(\d+(?:\.\d+)?)%\s*\u2022/);
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

  async function copyDiscordHandle() {
    try {
      await navigator.clipboard.writeText('cia2013');
      discordCopyFeedback = true;
      showToast('Discord handle copied', 'success');
      setTimeout(() => { discordCopyFeedback = false; }, 2000);
    } catch (e) {
      showToast('Unable to copy the Discord handle', 'error');
    }
  }

  function navigateTo(page) {
    if (activePage !== page) activePage = page;
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
    const u5 = listen('install-progress', (event) => {
      const { step, total, label } = event.payload;
      installStep = step;
      installTotal = total;
      installLabel = label;
    });
    return () => { u1.then(f => f()); u2.then(f => f()); u3.then(f => f()); u4.then(f => f()); u5.then(f => f()); };
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
    activeRenderJobId = createRenderJobId();
    isRenderPaused = false;
    isCancellingRender = false;
    beginLogCapture();
    appendLog('[cia app] RIFE 4.26 started');
    try {
      const outputPath = await invoke('run_time_remap', {
        jobId: activeRenderJobId,
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
      appendLog(`[cia app] RIFE output verified: ${outputPath}`);

      if (autoRender) {
        jobPhase = 'smoothie';
        isRenderPaused = false;
        const smoothiePath = await runSmoothieFor(outputPath, { preserveLogs: true, jobId: activeRenderJobId });
        lastOutputPath = smoothiePath;
        appendLog(`[cia app] Smoothie output verified: ${smoothiePath}`);
      }

      progress = 100;
      jobPhase = 'complete';
      isComplete = true;
      playCompletionChime();
      showToast(autoRender ? 'Interpolation and render complete!' : 'Interpolation complete!', 'success');
    } catch (e) {
      if (isCancellation(e) && !rifeOutputPath) {
        jobError = '';
        jobPhase = 'idle';
        isComplete = false;
        showToast('Interpolation cancelled', 'info');
      } else {
        jobError = isCancellation(e) ? 'Render cancelled. The RIFE output is still available.' : String(e);
      }
      if (rifeOutputPath) {
        lastOutputPath = rifeOutputPath;
        isComplete = true;
        jobPhase = 'failed';
      } else if (!isCancellation(e)) {
        jobPhase = 'failed';
      }
      if (!isCancellation(e)) showToast(`Process failed: ${e}`, 'error');
    } finally {
      isProcessing = false;
      activeRenderJobId = '';
      isRenderPaused = false;
      isCancellingRender = false;
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
    activeRenderJobId = createRenderJobId();
    isCancellingRender = false;
    try {
      const smoothiePath = await runSmoothieFor(rifeOutputPath, { preserveLogs: true, jobId: activeRenderJobId });
      lastOutputPath = smoothiePath;
      appendLog(`[cia app] Smoothie output verified: ${smoothiePath}`);
      progress = 100;
      jobPhase = 'complete';
      isComplete = true;
      playCompletionChime();
      showToast('Render complete!', 'success');
    } catch (e) {
      jobError = isCancellation(e) ? 'Render cancelled. The RIFE output is still available.' : String(e);
      lastOutputPath = rifeOutputPath;
      jobPhase = 'failed';
      isComplete = true;
      showToast(isCancellation(e) ? 'Render cancelled' : `Render failed: ${e}`, isCancellation(e) ? 'info' : 'error');
    } finally {
      isProcessing = false;
      activeRenderJobId = '';
      isCancellingRender = false;
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

  async function runSmoothieFor(inputPath, { preserveLogs = false, jobId = createRenderJobId() } = {}) {
    if (!preserveLogs) beginLogCapture();
    else resetTelemetry();
    appendLog('[cia app] SMOOTHIE started');
    return invoke('run_smoothie', {
      jobId,
      videoPath: inputPath,
      outputFps: Number(smoothieSettings.fps),
      overrides: smoothieOverrides()
    });
  }

  async function startSmoothie() {
    if (!smoothiePath || anyProcessing) return;
    isSmoothieProcessing = true;
    isSmoothieComplete = false;
    smoothieOutputPath = '';
    activeRenderJobId = createRenderJobId();
    isCancellingRender = false;

    try {
      const outPath = await runSmoothieFor(smoothiePath, { jobId: activeRenderJobId });
      progress = 100;
      isSmoothieComplete = true;
      smoothieOutputPath = outPath;
      playCompletionChime();
      showToast('Render complete!', 'success');
    } catch (e) {
      showToast(isCancellation(e) ? 'Render cancelled' : `Render failed: ${e}`, isCancellation(e) ? 'info' : 'error');
    } finally {
      isSmoothieProcessing = false;
      activeRenderJobId = '';
      isCancellingRender = false;
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

  async function openAboutLink(url) {
    try {
      await invoke('open_about_link', { url });
    } catch (e) {
      showToast(`Unable to open link: ${e}`, 'error');
    }
  }

  async function installRifeEnvironment() {
    if (isInstallingRifeEnvironment) return;
    isInstallingRifeEnvironment = true;
    installStep = 0;
    installTotal = 0;
    installLabel = '';
    beginLogCapture();
    appendLog('[cia app] Checking for an available RIFE environment');
    try {
      runtimeSnapshot = await invoke('install_rife_environment');
      setupDraft = cloneConfig(runtimeSnapshot.config);
      showToast('RIFE environment ready', 'success');
    } catch (e) {
      showToast(`RIFE environment installation failed: ${e}`, 'error');
    } finally {
      isInstallingRifeEnvironment = false;
      installStep = 0;
      installTotal = 0;
      installLabel = '';
    }
  }

  function applyDetectedRuntime() {
    if (!runtimeSnapshot?.detected) return;
    const detected = cloneConfig(runtimeSnapshot.detected);
    setupDraft = {
      ...setupDraft,
      rife: { ...setupDraft.rife, ...detected.rife },
      smoothie: { ...setupDraft.smoothie, ...detected.smoothie },
      mediaTools: { ...setupDraft.mediaTools, ...detected.mediaTools }
    };
  }

  async function browseRuntimePath(kind) {
    try {
      const path = await invoke('pick_runtime_path', { kind });
      if (!path || !setupDraft) return;
      if (kind === 'rife_python') setupDraft.rife.pythonExecutable = path;
      if (kind === 'rife_script') setupDraft.rife.script = path;
      if (kind === 'rife_directory') setupDraft.rife.directory = path;
      if (kind === 'rife_model') setupDraft.rife.modelFile = path;
      if (kind === 'smoothie_root') setupDraft.smoothie.root = path;
      if (kind === 'smoothie_executable') setupDraft.smoothie.executable = path;
      if (kind === 'smoothie_recipe') setupDraft.smoothie.recipe = path;
      if (kind === 'ffmpeg') setupDraft.mediaTools.ffmpeg = path;
      if (kind === 'ffprobe') setupDraft.mediaTools.ffprobe = path;
    } catch (e) {
      showToast(`Unable to select path: ${e}`, 'error');
    }
  }

  async function saveRuntimeSetup() {
    if (!setupDraft) return;
    try {
      runtimeSnapshot = await invoke('save_runtime_config', { config: setupDraft });
      setupDraft = cloneConfig(runtimeSnapshot.config);
      if (runtimeSnapshot.rifeReady && runtimeSnapshot.smoothieReady && runtimeSnapshot.mediaToolsReady) {
        showRuntimeSetup = false;
        showToast('Local runtimes are configured', 'success');
      } else {
        showToast('Some required runtime components are still missing', 'error');
      }
    } catch (e) {
      showToast(`Unable to save runtime setup: ${e}`, 'error');
    }
  }

  function formatBytes(bytes) {
    if (!bytes || bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`;
  }

  async function checkForAppUpdates(manual = false) {
    if (typeof window === 'undefined' || !window.__TAURI_INTERNALS__) return;
    try {
      updateState = 'checking';
      updateErrorMessage = '';
      const update = await check();
      if (update && update.available) {
        availableUpdate = update;
        updateState = 'available';
        if (manual) {
          showUpdateModal = true;
        }
      } else {
        availableUpdate = null;
        updateState = 'up-to-date';
        if (manual) {
          showToast('cia app is up to date', 'success');
        }
      }
    } catch (err) {
      console.error('Update check failed:', err);
      updateErrorMessage = String(err?.message || err);
      updateState = 'error';
      if (manual) {
        showToast(`Update check failed: ${updateErrorMessage}`, 'error');
      }
    }
  }

  async function installAppUpdate() {
    if (!availableUpdate) return;
    try {
      updateState = 'downloading';
      updateDownloadedBytes = 0;
      updateContentLength = 0;

      let downloaded = 0;
      let contentLength = 0;

      await availableUpdate.downloadAndInstall((event) => {
        switch (event.event) {
          case 'Started':
            contentLength = event.data.contentLength ?? 0;
            updateContentLength = contentLength;
            break;
          case 'Progress':
            downloaded += event.data.chunkLength;
            updateDownloadedBytes = downloaded;
            break;
          case 'Finished':
            updateState = 'ready';
            break;
        }
      });

      showToast('Update installed. Restarting cia app...', 'success');
      await relaunch();
    } catch (err) {
      console.error('Update install failed:', err);
      updateErrorMessage = String(err?.message || err);
      updateState = 'error';
      showToast(`Update installation failed: ${updateErrorMessage}`, 'error');
    }
  }

  onMount(() => {
    initializeRuntime();
    checkForAppUpdates(false);
  });
</script>

<div class="app-root" class:dragging={isDragging}>
  <!-- Custom Windows Titlebar -->
  <div class="titlebar" data-tauri-drag-region>
    <div class="titlebar-brand">
      <span class="titlebar-text">cia app</span>
    </div>
    <div class="titlebar-controls">
      {#if availableUpdate}
        <button class="titlebar-btn update-badge" onclick={() => showUpdateModal = true} aria-label="Update available">
          <span class="update-badge-dot"></span> UPDATE V{availableUpdate.version}
        </button>
      {/if}
      <button class="titlebar-btn setup" onclick={() => showRuntimeSetup = true} aria-label="Open runtime repair">RUNTIME</button>
      <button class="titlebar-btn" onclick={() => appWindow?.minimize()} aria-label="Minimize" disabled={!appWindow}>-</button>
      <button class="titlebar-btn close" onclick={() => appWindow?.close()} aria-label="Close" disabled={!appWindow}>X</button>
    </div>
  </div>

  <nav class="tab-bar">
    <button class:active={activePage === 'smoothie'} onclick={() => navigateTo('smoothie')}>RENDER</button>
    <button class:active={activePage === 'dashboard'} onclick={() => navigateTo('dashboard')}>INTERPOLATION</button>
    <button class:active={activePage === 'about'} onclick={() => navigateTo('about')}>ABOUT</button>
  </nav>

  {#if showRuntimeSetup}
    <main class="runtime-setup" aria-labelledby="setup-title">
      <section class="setup-card">
        <header class="setup-header">
            <span class="about-kicker">cia app / RUNTIME REPAIR</span>
          <h1 id="setup-title">ADVANCED RUNTIME PATHS</h1>
          <p>RENDER works with the bundled Smoothie and media tools. Use this panel only to repair an installation or supply a custom RIFE runtime.</p>
        </header>

        {#if !runtimeSnapshot || !setupDraft}
          <div class="setup-loading">CHECKING LOCAL RUNTIMES...</div>
        {:else}
          {#if runtimeSnapshot.loadError}
            <div class="setup-alert">{runtimeSnapshot.loadError}</div>
          {/if}

          <div class="setup-status-grid">
            {#each runtimeSnapshot.components as component}
              <div class:ready={component.ready} class="setup-status-item">
                <span>{component.label}</span>
                <strong>{component.ready ? 'READY' : 'MISSING'}</strong>
              </div>
            {/each}
          </div>

          <div class="setup-actions">
            <button class="btn-pro-secondary" onclick={applyDetectedRuntime}>USE DETECTED PATHS</button>
            <button class="btn-pro-secondary" onclick={refreshRuntimeSnapshot}>RECHECK</button>
          </div>

          <div class="setup-fields">
            <section>
              <h2>RIFE</h2>
              <label for="setup-rife-python">PYTHON EXECUTABLE</label>
              <div class="path-field"><input id="setup-rife-python" bind:value={setupDraft.rife.pythonExecutable} placeholder="Select python.exe" /><button onclick={() => browseRuntimePath('rife_python')}>BROWSE</button></div>
              <label for="setup-rife-directory">PRACTICAL-RIFE FOLDER</label>
              <div class="path-field"><input id="setup-rife-directory" bind:value={setupDraft.rife.directory} placeholder="Folder containing inference_video.py" /><button onclick={() => browseRuntimePath('rife_directory')}>BROWSE</button></div>
              <label for="setup-rife-model">RIFE MODEL</label>
              <div class="path-field"><input id="setup-rife-model" bind:value={setupDraft.rife.modelFile} placeholder="Select flownet.pkl" /><button onclick={() => browseRuntimePath('rife_model')}>BROWSE</button></div>
              <label for="setup-rife-script">RIFE SCRIPT OVERRIDE <span>(optional)</span></label>
              <div class="path-field"><input id="setup-rife-script" bind:value={setupDraft.rife.script} placeholder="Bundled cia app script is used by default" /><button onclick={() => browseRuntimePath('rife_script')}>BROWSE</button></div>
            </section>

            <section>
              <h2>MEDIA TOOLS</h2>
              <label for="setup-ffmpeg">FFMPEG</label>
              <div class="path-field"><input id="setup-ffmpeg" bind:value={setupDraft.mediaTools.ffmpeg} placeholder="Select ffmpeg.exe" /><button onclick={() => browseRuntimePath('ffmpeg')}>BROWSE</button></div>
              <label for="setup-ffprobe">FFPROBE</label>
              <div class="path-field"><input id="setup-ffprobe" bind:value={setupDraft.mediaTools.ffprobe} placeholder="Select ffprobe.exe" /><button onclick={() => browseRuntimePath('ffprobe')}>BROWSE</button></div>

              <h2 class="smoothie-heading">RENDER ENGINE</h2>
              <label for="setup-smoothie-root">RUNTIME FOLDER</label>
              <div class="path-field"><input id="setup-smoothie-root" bind:value={setupDraft.smoothie.root} placeholder="Select smoothie-rs folder" /><button onclick={() => browseRuntimePath('smoothie_root')}>BROWSE</button></div>
              <label for="setup-smoothie-executable">EXECUTABLE</label>
              <div class="path-field"><input id="setup-smoothie-executable" bind:value={setupDraft.smoothie.executable} placeholder="Select smoothie-rs.exe" /><button onclick={() => browseRuntimePath('smoothie_executable')}>BROWSE</button></div>
              <label for="setup-smoothie-recipe">RECIPE <span>(optional)</span></label>
              <div class="path-field"><input id="setup-smoothie-recipe" bind:value={setupDraft.smoothie.recipe} placeholder="recipe.ini" /><button onclick={() => browseRuntimePath('smoothie_recipe')}>BROWSE</button></div>
            </section>
          </div>

          <div class="setup-footer">
            <span>Configuration is saved to your cia app app-data folder.</span>
            <button class="btn-pro-primary" onclick={saveRuntimeSetup}>SAVE &amp; CONTINUE</button>
          </div>
        {/if}
      </section>
    </main>
  {:else}
  <!-- Main Content Area -->
  <main class="content-area">
    {#key activePage}
      <div class="page-stage">
    <!-- INTERPOLATION PAGE (RIFE) -->
    {#if activePage === 'dashboard'}
      {#if !runtimeSnapshot?.rifeReady}
        <section class="environment-card" aria-labelledby="rife-environment-title">
          <div class="environment-status"><span class="pro-dot"></span> OPTIONAL COMPONENT</div>
          <h1 id="rife-environment-title">RIFE INTERPOLATION</h1>
          <p>Install the local CUDA environment only if you want to multiply frames. RENDER is already available and does not require this download.</p>
          <div class="environment-meta">
            <span>RIFE 4.26</span><span>PYTHON + CUDA</span><span>LARGE DOWNLOAD</span>
          </div>
          {#if isInstallingRifeEnvironment}
            <div class="environment-installing">
              <div class="install-progress-header">
                <span class="pro-dot active"></span>
                <span>STEP {installStep} / {installTotal}</span>
              </div>
              <div class="install-progress-label">{installLabel || 'PREPARING ENVIRONMENT'}</div>
              <div class="pro-progress-row">
                <div class="pro-track">
                  <div class="pro-fill" style="width: {installTotal > 0 ? (installStep / installTotal) * 100 : 0}%"></div>
                </div>
                <span class="pro-percent-readout">{installStep}/{installTotal}</span>
              </div>
            </div>
          {:else}
            <div class="environment-actions">
              <button class="btn-primary" onclick={installRifeEnvironment}>INSTALL ENVIRONMENT</button>
            </div>
          {/if}
        </section>
      {:else if !videoPath}
        <div class="drop-zone" class:dragging={isDragging} onclick={pickFile} onkeydown={(event) => activateOnKeyboard(event, pickFile)} role="button" tabindex="0">
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
                <span class="node-val">{videoInfo.width}x{videoInfo.height} @ {videoInfo.fps.toFixed(0)} FPS</span>
              </div>
              <div class="pipeline-arrow">-&gt;</div>
              <div class="pipeline-node">
                <span class="node-label">OUTPUT</span>
                <span class="node-val">{videoInfo.width}x{videoInfo.height} @ {outputFps.toFixed(0)} FPS ({rifeSettings.factor}x)</span>
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
                  {isRenderPaused
                    ? 'RIFE PAUSED'
                    : jobPhase === 'smoothie'
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
            <div class="render-control-row">
              {#if jobPhase === 'rife'}
                <button class="btn-pro-secondary" onclick={toggleRenderPause} disabled={isCancellingRender}>
                  {isRenderPaused ? 'RESUME' : 'PAUSE'}
                </button>
              {/if}
              <button class="btn-pro-secondary danger-action" onclick={() => showRenderCancelConfirmation = true} disabled={isCancellingRender}>
                {isCancellingRender ? 'CANCELLING...' : 'CANCEL RENDER'}
              </button>
            </div>
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
                <button class="btn-pro-secondary" onclick={renderRifeWithSmoothie}>{jobPhase === 'failed' ? 'RETRY RENDER' : 'RENDER'}</button>
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
              <div class="info-row"><span>Resolution</span><span>{videoInfo.width} x {videoInfo.height}</span></div>
              <div class="info-row"><span>Source FPS</span><span>{videoInfo.fps.toFixed(2)}</span></div>
              <div class="info-row"><span>Duration</span><span>{videoInfo.duration.toFixed(2)}s</span></div>
              <button class="btn-secondary" onclick={() => { videoPath = ''; videoInfo = null; }}>CHANGE VIDEO</button>
            </div>

            <!-- Quick Action Card -->
            <div class="card action-card">
              <div class="card-header">
                <h3>INTERPOLATION FACTOR</h3>
                <button class="btn-icon-settings" onclick={() => showRifeSettings = true}>SETTINGS</button>
              </div>

              <!-- Factor Slider 2x to 10x -->
              <GlowSlider bind:value={rifeSettings.factor} min={2} max={10} step={1} label="FACTOR:" unit="x" />

              <label class="auto-render-toggle">
                <input type="checkbox" bind:checked={autoRender} onchange={saveAutoRender} />
                <span>AUTO-RENDER AFTER INTERPOLATION</span>
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

    <!-- RENDER PAGE (smoothie-rs engine) -->
    {:else if activePage === 'smoothie'}
      {#if !smoothiePath}
        <div class="drop-zone" class:dragging={isDragging} onclick={pickSmoothieFile} onkeydown={(event) => activateOnKeyboard(event, pickSmoothieFile)} role="button" tabindex="0">
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
                <span class="node-val">{smoothieInfo.width}x{smoothieInfo.height} @ {smoothieInfo.fps.toFixed(0)} FPS</span>
              </div>
              <div class="pipeline-arrow">-&gt;</div>
              <div class="pipeline-node">
                <span class="node-label">OUTPUT</span>
                <span class="node-val">{smoothieInfo.width}x{smoothieInfo.height} @ {smoothieSettings.fps} FPS</span>
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
              <div class="info-row"><span>Resolution</span><span>{smoothieInfo.width} x {smoothieInfo.height}</span></div>
              <div class="info-row"><span>Source FPS</span><span>{smoothieInfo.fps.toFixed(2)}</span></div>
              <div class="info-row"><span>Duration</span><span>{smoothieInfo.duration.toFixed(2)}s</span></div>
              <button class="btn-secondary" onclick={() => { smoothiePath = ''; smoothieInfo = null; }}>CHANGE VIDEO</button>
            </div>

            <!-- Quick Action Card -->
            <div class="card action-card">
              <div class="card-header">
                <h3>OUTPUT TARGET FPS</h3>
                <button class="btn-icon-settings" onclick={() => showSmoothieSettings = true}>SETTINGS</button>
              </div>

              <!-- Output FPS Slider 20 to 60 FPS -->
              <GlowSlider bind:value={smoothieSettings.fps} min={20} max={60} step={1} label="TARGET FPS:" unit=" FPS" />

              <div class="output-preview">
                <span>Engine: smoothie-rs</span>
                <span>LUT: {smoothieSettings.lutEnabled === 'yes' ? 'ON' : 'OFF'}</span>
              </div>

              <button class="btn-primary" onclick={startSmoothie} disabled={anyProcessing}>
                {isSmoothieProcessing ? 'PROCESSING...' : 'START RENDER'}
              </button>
            </div>
          </div>
        {/if}
      {/if}
    {:else if activePage === 'about'}
      <section class="about-page" aria-label="Project credits">
        <header class="about-identity">
          <img class="about-app-logo" src={appLogo} alt="cia app logo" />
          <div class="about-app-copy">
            <h1>cia app <span>V{appVersion}</span></h1>
            <p>Local render workflow, credits and contact.</p>
          </div>
          <div class="about-contacts">
            <button class="about-update-btn" onclick={() => checkForAppUpdates(true)} aria-label="Check for cia app updates" disabled={updateState === 'checking' || updateState === 'downloading'}>
              {#if updateState === 'checking'}
                <span>CHECKING...</span>
              {:else if availableUpdate}
                <span class="update-ready-text"><span class="pro-dot active"></span> UPDATE V{availableUpdate.version}</span>
              {:else}
                <span>CHECK FOR UPDATES</span>
              {/if}
            </button>
            <button class="discord-contact" onclick={copyDiscordHandle} aria-label="Copy cia app Discord handle">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M19.5 4.7a16.8 16.8 0 0 0-4.1-1.3l-.5 1.1a15.1 15.1 0 0 0-5.8 0l-.5-1.1A16.9 16.9 0 0 0 4.5 4.7C1.9 8.5 1.2 12.2 1.6 15.8a16.8 16.8 0 0 0 5 2.5l1.2-1.6a9.8 9.8 0 0 1-1.9-.9l.5-.4c3.7 1.7 7.7 1.7 11.4 0l.5.4c-.6.4-1.2.7-1.9.9l1.2 1.6a16.6 16.6 0 0 0 5-2.5c.5-4.2-.8-7.8-3.1-11.1ZM8.7 13.6c-1 0-1.8-.9-1.8-2s.8-2 1.8-2 1.8.9 1.8 2-.8 2-1.8 2Zm6.6 0c-1 0-1.8-.9-1.8-2s.8-2 1.8-2 1.8.9 1.8 2-.8 2-1.8 2Z" /></svg>
              <span>{discordCopyFeedback ? 'COPIED' : 'cia2013'}</span>
            </button>
            <button class="github-contact" onclick={() => openAboutLink(PROJECT_REPOSITORY_URL)} aria-label="Open cia app on GitHub">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M12 .5A11.5 11.5 0 0 0 8.4 22.9c.6.1.8-.3.8-.6v-2.2c-3.4.7-4.1-1.4-4.1-1.4-.6-1.4-1.4-1.8-1.4-1.8-1.1-.8.1-.8.1-.8 1.2.1 1.9 1.3 1.9 1.3 1.1 1.9 2.8 1.3 3.5 1 .1-.8.4-1.3.8-1.6-2.7-.3-5.5-1.3-5.5-6 0-1.3.5-2.4 1.2-3.3-.1-.3-.5-1.6.1-3.3 0 0 1-.3 3.3 1.2a11.3 11.3 0 0 1 6 0c2.3-1.5 3.3-1.2 3.3-1.2.6 1.7.2 3 .1 3.3.8.9 1.2 2 1.2 3.3 0 4.7-2.8 5.7-5.5 6 .4.4.8 1.1.8 2.2v3.2c0 .3.2.7.8.6A11.5 11.5 0 0 0 12 .5Z" /></svg>
            </button>
          </div>
        </header>
        <div class="about-grid">
          {#each ABOUT_LINKS as link}
            <button class="about-link-card" onclick={() => openAboutLink(link.url)} aria-label={`Open ${link.name} website`}>
              <ProjectMark kind={link.mark} />
              <div class="about-link-copy">
                <h2>{link.name}</h2>
                <p>{link.detail}</p>
              </div>
              <span class="about-link-arrow" aria-hidden="true">&gt;</span>
            </button>
          {/each}
        </div>
      </section>
    {/if}
      </div>
    {/key}
  </main>
  {/if}

  {#if showRenderCancelConfirmation}
    <div class="modal-backdrop" onclick={() => showRenderCancelConfirmation = false} role="presentation">
      <div class="modal-card confirmation-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="cancel-render-title" tabindex="0">
        <div class="modal-header">
          <h2 id="cancel-render-title">CANCEL RENDER?</h2>
          <button class="btn-close-modal" onclick={() => showRenderCancelConfirmation = false} aria-label="Close">X</button>
        </div>
        <div class="modal-body confirmation-copy">
          <p>The active render process will stop. Any incomplete file produced by the active phase will be removed.</p>
          {#if jobPhase === 'smoothie' && rifeOutputPath}
            <p>Your completed RIFE output will be kept.</p>
          {/if}
        </div>
        <div class="modal-footer">
          <button class="btn-secondary" onclick={() => showRenderCancelConfirmation = false}>KEEP RENDERING</button>
          <button class="btn-danger-modal" onclick={cancelRender}>CANCEL RENDER</button>
        </div>
      </div>
    </div>
  {/if}

  <!-- RIFE SETTINGS MODAL DRAWER -->
  {#if showRifeSettings}
    <div class="modal-backdrop" onclick={() => showRifeSettings = false} role="presentation">
      <div class="modal-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="0">
        <div class="modal-header">
          <h2>RIFE SETTINGS</h2>
          <button class="btn-close-modal" onclick={() => showRifeSettings = false}>X</button>
        </div>
        <div class="modal-body">
          <div class="setting-group">
            <h3>CORE CONFIGURATION</h3>
            <div class="setting-row">
              <label for="mod-rife-mode" class="has-tooltip" data-tooltip="Slowmo extends video duration; Boost doubles FPS at normal speed.">MODE</label>
              <select id="mod-rife-mode" bind:value={rifeSettings.mode}>
                <option value="boost">FPS Boost (same duration)</option>
                <option value="slowmo">Slowmo (duration x factor)</option>
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

  <!-- RENDER SETTINGS MODAL DRAWER -->
  {#if showSmoothieSettings}
    <div class="modal-backdrop" onclick={() => showSmoothieSettings = false} role="presentation">
      <div class="modal-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="0">
        <div class="modal-header">
          <h2>RENDER CONFIGURATION</h2>
          <button class="btn-close-modal" onclick={() => showSmoothieSettings = false}>X</button>
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

  <!-- Auto-Updater Modal Overlay -->
  {#if showUpdateModal && availableUpdate}
    <div class="modal-backdrop" onclick={(e) => { if (e.target === e.currentTarget && updateState !== 'downloading') showUpdateModal = false; }} role="presentation">
      <div class="modal-card update-modal-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" aria-modal="true" tabindex="0">
        <div class="modal-header">
          <h2>UPDATE AVAILABLE - V{availableUpdate.version}</h2>
          {#if updateState !== 'downloading'}
            <button class="btn-close-modal" onclick={() => showUpdateModal = false}>X</button>
          {/if}
        </div>
        <div class="modal-body">
          <div class="update-version-banner">
            <span class="pro-dot active"></span>
            <span>A new version of cia app is ready to install (current: V{appVersion})</span>
          </div>
          {#if availableUpdate.body}
            <div class="update-notes-box">
              <div class="update-notes-title">RELEASE NOTES</div>
              <div class="update-notes-content">{availableUpdate.body}</div>
            </div>
          {/if}
          {#if updateState === 'downloading'}
            <div class="update-downloading-box">
              <div class="install-progress-header">
                <span class="pro-dot active"></span>
                <span>DOWNLOADING & INSTALLING UPDATE</span>
              </div>
              <div class="pro-progress-row">
                <div class="pro-track">
                  <div class="pro-fill" style="width: {updateContentLength > 0 ? Math.min(100, (updateDownloadedBytes / updateContentLength) * 100) : 0}%"></div>
                </div>
                <span class="pro-percent-readout">
                  {updateContentLength > 0 ? Math.round((updateDownloadedBytes / updateContentLength) * 100) : 0}%
                </span>
              </div>
              <div class="update-bytes-readout">
                {formatBytes(updateDownloadedBytes)} / {updateContentLength > 0 ? formatBytes(updateContentLength) : '...'}
              </div>
            </div>
          {:else if updateState === 'ready'}
            <div class="update-ready-box">
              <span class="pro-dot active"></span>
              <span>UPDATE APPLIED - RESTARTING CIA APP...</span>
            </div>
          {:else if updateState === 'error'}
            <div class="setup-alert" style="margin-top: 14px;">{updateErrorMessage}</div>
          {/if}
        </div>
        <div class="modal-footer">
          {#if updateState === 'downloading' || updateState === 'ready'}
            <span class="update-installing-status">Please wait while the update finishes...</span>
          {:else}
            <button class="btn-pro-secondary" onclick={() => showUpdateModal = false}>LATER</button>
            <button class="btn-primary-modal" onclick={installAppUpdate}>UPDATE & RELAUNCH</button>
          {/if}
        </div>
      </div>
    </div>
  {/if}

  {#if canCopyLogs}
    <footer class="app-footer">
      <button class="btn-copy" onclick={copyLogsToClipboard}>
        {copyFeedback ? 'COPIED TO CLIPBOARD' : 'COPY LOGS'}
      </button>
    </footer>
  {/if}

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
  .titlebar-btn.setup {
    width: auto;
    padding: 0 8px;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.05em;
  }

  /* Navigation Tabs */
  .tab-bar {
    display: flex;
    gap: 4px;
    padding: 8px 12px 0;
    background: #08080a;
    border-bottom: 1px solid #1c1c20;
    min-height: 40px;
  }

  .tab-bar button {
    position: relative;
    z-index: 0;
    min-height: 32px;
    padding: 8px 20px;
    background: #0d0d10;
    border: 1px solid #1c1c20;
    border-bottom-color: #1c1c20;
    border-radius: 6px 6px 0 0;
    color: #71717a;
    cursor: pointer;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.05em;
    transition: color 140ms ease, background-color 140ms ease, border-color 140ms ease;
  }

  .tab-bar button:hover { color: #e4e4e7; background: #16161a; }
  .tab-bar button.active {
    z-index: 1;
    color: #ffffff;
    background: #121215;
    border-color: rgba(255, 255, 255, 0.25);
    border-bottom: 1px solid #121215;
  }
  .tab-bar button:focus-visible {
    z-index: 2;
    outline: none;
    box-shadow: inset 0 0 0 1px #ffffff;
  }

  /* Main Content Area */
  .content-area {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    padding: 16px;
    background: #050507;
    display: flex;
    flex-direction: column;
  }

  .page-stage {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    justify-content: center;
    animation: page-enter 190ms cubic-bezier(0.16, 1, 0.3, 1) both;
  }

  @keyframes page-enter {
    from { opacity: 0; transform: translateY(5px); }
    to { opacity: 1; transform: translateY(0); }
  }

  @media (prefers-reduced-motion: reduce) {
    .page-stage { animation: none; }
  }

  .runtime-setup {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    padding: 16px;
    background: #050507;
  }

  .setup-card {
    width: min(100%, 920px);
    margin: 0 auto;
    padding: 18px;
    background: #09090c;
    border: 1px solid #27272a;
    border-radius: 8px;
  }

  .setup-header h1 {
    margin: 5px 0 7px;
    font-size: 20px;
    letter-spacing: 0.04em;
    color: #ffffff;
  }

  .setup-header p,
  .setup-footer span {
    color: #a1a1aa;
    font-size: 11px;
    line-height: 1.45;
  }

  .setup-loading,
  .setup-alert {
    margin-top: 16px;
    padding: 12px;
    border: 1px solid #3f3f46;
    border-radius: 6px;
    color: #d4d4d8;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 11px;
  }

  .setup-alert { border-color: #7f1d1d; color: #fecaca; }

  .setup-status-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
    margin: 16px 0 10px;
  }

  .setup-status-item {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 7px 8px;
    border: 1px solid #3f3f46;
    border-radius: 4px;
    color: #a1a1aa;
    font-size: 10px;
  }

  .setup-status-item strong {
    color: #fca5a5;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
  }
  .setup-status-item.ready { border-color: #3f3f46; }
  .setup-status-item.ready strong { color: #e4e4e7; }

  .setup-actions {
    display: flex;
    gap: 8px;
    margin-bottom: 14px;
  }

  .setup-fields {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 16px;
  }

  .setup-fields section {
    padding: 14px;
    border: 1px solid #1c1c20;
    border-radius: 6px;
    background: #060608;
  }

  .setup-fields h2 {
    margin: 0 0 12px;
    color: #e4e4e7;
    font-size: 10px;
    letter-spacing: 0.08em;
  }
  .setup-fields .smoothie-heading { margin-top: 18px; }
  .setup-fields label {
    display: block;
    margin: 10px 0 5px;
    color: #71717a;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.05em;
  }
  .setup-fields label span { color: #52525b; font-weight: 400; }

  .path-field { display: flex; gap: 6px; }
  .path-field input {
    min-width: 0;
    flex: 1;
    padding: 7px 8px;
    border: 1px solid #27272a;
    border-radius: 4px;
    background: #0d0d10;
    color: #e4e4e7;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
  }
  .path-field input:focus { outline: none; border-color: rgba(255, 255, 255, 0.45); }
  .path-field button {
    padding: 0 9px;
    border: 1px solid #3f3f46;
    border-radius: 4px;
    background: #141417;
    color: #e4e4e7;
    cursor: pointer;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }
  .path-field button:hover { border-color: rgba(255, 255, 255, 0.4); }

  .setup-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    margin-top: 16px;
  }

  .environment-card {
    width: min(100%, 640px);
    margin: auto;
    padding: 26px;
    border: 1px solid #27272a;
    border-radius: 8px;
    background: #09090c;
  }

  .environment-status {
    display: flex;
    align-items: center;
    gap: 7px;
    color: #a1a1aa;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.09em;
  }

  .environment-card h1 {
    margin: 12px 0 9px;
    color: #fff;
    font-size: 22px;
    letter-spacing: 0.04em;
  }

  .environment-card p {
    max-width: 550px;
    margin: 0;
    color: #a1a1aa;
    font-size: 12px;
    line-height: 1.55;
  }

  .environment-meta {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin: 18px 0;
  }

  .environment-meta span {
    padding: 4px 6px;
    border: 1px solid #27272a;
    border-radius: 3px;
    color: #71717a;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    letter-spacing: 0.04em;
  }

  .environment-actions,
  .environment-installing {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .environment-installing {
    flex-direction: column;
    align-items: stretch;
    gap: 10px;
    color: #e4e4e7;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    letter-spacing: 0.04em;
  }

  .install-progress-header {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .install-progress-label {
    font-size: 11px;
    color: #a1a1aa;
    letter-spacing: 0.06em;
  }

  @media (max-width: 720px) {
    .setup-status-grid, .setup-fields { grid-template-columns: 1fr; }
    .setup-footer { align-items: flex-end; flex-direction: column; }
  }

  /* About */
  .about-page {
    width: min(100%, 920px);
    margin: auto;
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .about-identity {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 88px;
    padding: 14px 16px;
    background: #09090c;
    border: 1px solid #1c1c20;
    border-radius: 8px;
  }

  .about-app-logo {
    width: 52px;
    height: 52px;
    flex: 0 0 auto;
    border-radius: 12px;
  }

  .about-app-copy { min-width: 0; }
  .about-app-copy h1 {
    margin: 4px 0;
    color: #fff;
    font-size: 17px;
    letter-spacing: 0.04em;
  }
  .about-app-copy h1 span {
    margin-left: 6px;
    color: #71717a;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    font-weight: 400;
    letter-spacing: 0.02em;
    vertical-align: middle;
  }
  .about-app-copy p {
    color: #a1a1aa;
    font-size: 11px;
    line-height: 1.35;
  }

  .about-contacts {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    margin-left: auto;
  }
  .discord-contact,
  .github-contact {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    margin-left: 0;
    padding: 8px 10px;
    border: 1px solid #27272a;
    border-radius: 5px;
    background: #0d0d10;
    color: #d4d4d8;
    cursor: pointer;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.03em;
    transition: background-color 150ms ease, border-color 150ms ease, color 150ms ease;
  }
  .discord-contact svg,
  .github-contact svg {
    width: 17px;
    height: 17px;
    fill: currentColor;
  }
  .discord-contact:hover,
  .discord-contact:focus-visible,
  .github-contact:hover,
  .github-contact:focus-visible {
    border-color: rgba(255, 255, 255, 0.42);
    background: #16161a;
    color: #fff;
    outline: none;
  }
  .github-contact {
    justify-content: center;
    width: 37px;
    padding-inline: 0;
  }

  .about-link-card {
    background: #09090c;
    border: 1px solid #1c1c20;
    border-radius: 8px;
  }

  .about-kicker {
    display: block;
    color: #71717a;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.12em;
  }
  .about-link-card p {
    color: #a1a1aa;
    font-size: 11px;
    line-height: 1.35;
  }
  .about-link-card h2 {
    color: #e4e4e7;
    font-size: 11px;
    letter-spacing: 0.06em;
  }
  .about-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
  }
  .about-link-card {
    min-height: 84px;
    padding: 12px;
    display: flex;
    align-items: center;
    text-align: left;
    color: inherit;
    cursor: pointer;
    transition: border-color 0.15s ease, background 0.15s ease, transform 0.15s ease;
    gap: 10px;
  }
  .about-link-card:hover,
  .about-link-card:focus-visible {
    background: #121215;
    border-color: rgba(255, 255, 255, 0.35);
    outline: none;
    transform: translateY(-1px);
  }
  .about-link-copy { min-width: 0; }
  .about-link-card h2 { margin-bottom: 4px; }
  .about-link-arrow { margin-left: auto; color: #71717a; font-size: 15px; }

  @media (max-width: 760px) {
    .about-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
    .page-stage { justify-content: flex-start; overflow-y: auto; }
    .about-identity { align-items: flex-start; }
    .about-contacts { align-self: center; }
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

  .render-control-row {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 18px;
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

  .btn-pro-secondary:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

  .danger-action:hover {
    border-color: #fca5a5;
    color: #fecaca;
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

  .confirmation-card { width: 480px; }
  .confirmation-copy p {
    margin: 0 0 10px;
    color: #d4d4d8;
    font-size: 12px;
    line-height: 1.55;
  }
  .confirmation-copy p:last-child { margin-bottom: 0; }
  .btn-danger-modal {
    padding: 8px 18px;
    border: 1px solid #7f1d1d;
    border-radius: 6px;
    background: #450a0a;
    color: #fecaca;
    cursor: pointer;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
  }
  .btn-danger-modal:hover { background: #7f1d1d; color: #fff; }

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

  /* Auto-Updater Styling */
  .titlebar-btn.update-badge {
    width: auto;
    padding: 0 8px;
    background: rgba(34, 197, 94, 0.15);
    color: #4ade80;
    border: 1px solid rgba(34, 197, 94, 0.3);
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.05em;
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .titlebar-btn.update-badge:hover {
    background: rgba(34, 197, 94, 0.25);
    border-color: #4ade80;
    color: #ffffff;
  }
  .update-badge-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: #4ade80;
    box-shadow: 0 0 6px #4ade80;
  }

  .about-update-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 8px 10px;
    border: 1px solid #27272a;
    border-radius: 5px;
    background: #0d0d10;
    color: #d4d4d8;
    cursor: pointer;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.03em;
    transition: background-color 150ms ease, border-color 150ms ease, color 150ms ease;
  }
  .about-update-btn:hover:not(:disabled) {
    background: #18181b;
    border-color: #52525b;
    color: #ffffff;
  }
  .about-update-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .update-ready-text {
    display: flex;
    align-items: center;
    gap: 6px;
    color: #4ade80;
  }

  .update-modal-card {
    width: 500px;
  }
  .update-version-banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 10px 12px;
    background: rgba(255, 255, 255, 0.04);
    border: 1px solid #27272a;
    border-radius: 6px;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 11px;
    color: #f4f4f5;
    margin-bottom: 14px;
  }
  .update-notes-box {
    margin-bottom: 16px;
  }
  .update-notes-title {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    font-weight: 700;
    color: #71717a;
    letter-spacing: 0.05em;
    margin-bottom: 6px;
  }
  .update-notes-content {
    background: #050507;
    border: 1px solid #1c1c20;
    border-radius: 6px;
    padding: 10px 12px;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 11px;
    color: #a1a1aa;
    line-height: 1.5;
    max-height: 120px;
    overflow-y: auto;
    white-space: pre-wrap;
  }
  .update-downloading-box {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px;
    background: #050507;
    border: 1px solid #1c1c20;
    border-radius: 6px;
  }
  .update-bytes-readout {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    color: #71717a;
    text-align: right;
  }
  .update-ready-box {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 12px;
    background: rgba(34, 197, 94, 0.1);
    border: 1px solid rgba(34, 197, 94, 0.3);
    border-radius: 6px;
    color: #4ade80;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 11px;
    font-weight: 700;
  }
  .update-installing-status {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    color: #71717a;
  }
</style>
