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
  let isEncodingPhase = $state(false);
  let encodingProgress = $state(0);
  let encodingSpeed = $state('');
  let encodingFps = $state('');
  let encodingFrame = $state(0);
  let encodingTime = $state('');
  let copyFeedback = $state(false);
  let toast = $state({ show: false, message: '', type: 'info' });
  let runtimeSnapshot = $state(null);
  let setupDraft = $state(null);
  let showRuntimeSetup = $state(false);
  let isInstallingRifeEnvironment = $state(false);
  let installStep = $state(0);
  let installTotal = $state(0);
  let installLabel = $state('');
  let appVersion = $state('1.0.3');
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
  let rifePreviewSet = $state(null);
  let rifeOutputPreview = $state('');
  let rifePreviewFrameIndex = $state(-1);
  let rifePreviewHovered = false;

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
  let smoothieAspectRatio = $derived(
    smoothieInfo?.width && smoothieInfo?.height
      ? `${smoothieInfo.width} / ${smoothieInfo.height}`
      : '16 / 9'
  );
  let smoothieOutputPath = $state('');
  let smoothiePreviewSet = $state(null);
  let smoothieOutputPreview = $state('');
  let smoothiePreviewFrameIndex = $state(-1);
  let smoothiePreviewHovered = false;
  let liveRenderPreview = $state('');
  let isLiveRenderPreviewLoading = $state(false);
  let rifePreviewRequest = 0;
  let smoothiePreviewRequest = 0;
  let rifeOutputPreviewRequest = 0;
  let smoothieOutputPreviewRequest = 0;
  let liveRenderPreviewRequest = 0;
  let lastLiveRenderPreviewAt = 0;
  let rifePreviewTimer = null;
  let smoothiePreviewTimer = null;

  // --- Browser-like Navigation History ---
  let historyStack = $state([
    { page: 'smoothie', smoothiePath: '', videoPath: '' }
  ]);
  let historyIndex = $state(0);
  let canGoBack = $derived(historyIndex > 0);
  let canGoForward = $derived(historyIndex < historyStack.length - 1);

  function pushNavigation(entry) {
    const current = historyStack[historyIndex];
    if (
      current &&
      current.page === entry.page &&
      current.smoothiePath === entry.smoothiePath &&
      current.videoPath === entry.videoPath
    ) {
      return;
    }
    historyStack = [...historyStack.slice(0, historyIndex + 1), entry];
    historyIndex = historyStack.length - 1;
  }

  function handleGoBack() {
    if (!canGoBack) return;
    historyIndex -= 1;
    applyHistoryEntry(historyStack[historyIndex]);
  }

  function handleGoForward() {
    if (!canGoForward) return;
    historyIndex += 1;
    applyHistoryEntry(historyStack[historyIndex]);
  }

  async function applyHistoryEntry(entry) {
    if (!entry) return;
    activePage = entry.page;

    // Synchronize Smoothie State
    if (!entry.smoothiePath) {
      smoothiePath = '';
      smoothieInfo = null;
      isSmoothieLoading = false;
      isSmoothieProcessing = false;
    } else if (smoothiePath !== entry.smoothiePath) {
      await loadSmoothie(entry.smoothiePath, false);
    }

    // Synchronize RIFE State
    if (!entry.videoPath) {
      videoPath = '';
      videoInfo = null;
      isLoading = false;
      isProcessing = false;
    } else if (videoPath !== entry.videoPath) {
      await loadVideo(entry.videoPath, false);
    }
  }

  function handleNavKeyDown(event) {
    if (event.altKey && event.key === 'ArrowLeft') {
      event.preventDefault();
      handleGoBack();
    } else if (event.altKey && event.key === 'ArrowRight') {
      event.preventDefault();
      handleGoForward();
    }
  }

  function clearSmoothieSelection(resetComplete = false) {
    smoothiePreviewRequest += 1;
    smoothieOutputPreviewRequest += 1;
    resetLiveRenderPreview();
    smoothiePath = '';
    smoothieInfo = null;
    resetSourcePreview('smoothie');
    smoothieOutputPreview = '';
    if (resetComplete) isSmoothieComplete = false;
    pushNavigation({ page: 'smoothie', smoothiePath: '', videoPath });
  }

  function clearVideoSelection() {
    rifePreviewRequest += 1;
    rifeOutputPreviewRequest += 1;
    videoPath = '';
    videoInfo = null;
    isComplete = false;
    resetSourcePreview('rife');
    rifeOutputPreview = '';
    pushNavigation({ page: 'dashboard', smoothiePath, videoPath: '' });
  }

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
    isEncodingPhase = false;
    encodingProgress = 0;
    encodingSpeed = '';
    encodingFps = '';
    encodingFrame = 0;
    encodingTime = '';
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

  function resetLiveRenderPreview() {
    liveRenderPreviewRequest += 1;
    lastLiveRenderPreviewAt = 0;
    liveRenderPreview = '';
    isLiveRenderPreviewLoading = false;
  }

  function sourcePreviewIsCurrent(kind, requestId, path) {
    return kind === 'rife'
      ? requestId === rifePreviewRequest && videoPath === path
      : requestId === smoothiePreviewRequest && smoothiePath === path;
  }

  function clearSourcePreviewTimer(kind) {
    const timer = kind === 'rife' ? rifePreviewTimer : smoothiePreviewTimer;
    if (timer) clearInterval(timer);
    if (kind === 'rife') rifePreviewTimer = null;
    else smoothiePreviewTimer = null;
  }

  function setSourcePreviewFrameIndex(kind, index) {
    if (kind === 'rife') rifePreviewFrameIndex = index;
    else smoothiePreviewFrameIndex = index;
  }

  function startSourcePreviewCycle(kind) {
    if (kind === 'rife') rifePreviewHovered = true;
    else smoothiePreviewHovered = true;

    const previewSet = kind === 'rife' ? rifePreviewSet : smoothiePreviewSet;
    const frames = previewSet?.frames || [];
    if (frames.length !== 8) return;

    clearSourcePreviewTimer(kind);
    const currentIndex = kind === 'rife' ? rifePreviewFrameIndex : smoothiePreviewFrameIndex;
    setSourcePreviewFrameIndex(kind, currentIndex >= 0 ? (currentIndex + 1) % frames.length : 0);
    const timer = setInterval(() => {
      const index = kind === 'rife' ? rifePreviewFrameIndex : smoothiePreviewFrameIndex;
      setSourcePreviewFrameIndex(kind, (index + 1) % frames.length);
    }, 500);
    if (kind === 'rife') rifePreviewTimer = timer;
    else smoothiePreviewTimer = timer;
  }

  function stopSourcePreviewCycle(kind) {
    if (kind === 'rife') rifePreviewHovered = false;
    else smoothiePreviewHovered = false;
    clearSourcePreviewTimer(kind);
  }

  function resetSourcePreview(kind) {
    clearSourcePreviewTimer(kind);
    setSourcePreviewFrameIndex(kind, -1);
    if (kind === 'rife') {
      rifePreviewHovered = false;
      rifePreviewSet = null;
    } else {
      smoothiePreviewHovered = false;
      smoothiePreviewSet = null;
    }
  }

  async function loadSourcePreviewCover(kind, path, duration, requestId) {
    try {
      const image = await invoke('generate_video_preview_frame', {
        videoPath: path,
        timestamp: Math.max(0, duration * 0.12),
        blendFrames: 1
      });
      if (!sourcePreviewIsCurrent(kind, requestId, path)) return;
      const previewSet = kind === 'rife' ? rifePreviewSet : smoothiePreviewSet;
      if (previewSet?.frames?.length === 8) return;
      if (kind === 'rife') rifePreviewSet = { cover: image, frames: [] };
      else smoothiePreviewSet = { cover: image, frames: [] };
    } catch (error) {
      appendLog(`[cia render] Preview cover unavailable: ${error}`);
    }
  }

  async function loadSourcePreviewFrames(kind, path, duration, requestId) {
    try {
      const previewSet = await invoke('generate_video_preview_set', { videoPath: path, duration });
      if (!sourcePreviewIsCurrent(kind, requestId, path)) return;
      if (kind === 'rife') rifePreviewSet = previewSet;
      else smoothiePreviewSet = previewSet;

      const isHovered = kind === 'rife' ? rifePreviewHovered : smoothiePreviewHovered;
      if (isHovered) startSourcePreviewCycle(kind);
    } catch (error) {
      // A preview is convenience UI. It must never prevent the selected video from rendering.
      appendLog(`[cia render] Preview sequence unavailable: ${error}`);
    }
  }

  function loadSourcePreview(kind, path, duration) {
    const requestId = kind === 'rife' ? ++rifePreviewRequest : ++smoothiePreviewRequest;
    resetSourcePreview(kind);
    void (async () => {
      // The single still gets the disk/decoder first. Only afterwards does the
      // optional eight-frame scan begin, so the initial view is never queued
      // behind seven extra FFmpeg processes.
      await loadSourcePreviewCover(kind, path, duration, requestId);
      setTimeout(() => void loadSourcePreviewFrames(kind, path, duration, requestId), 0);
    })();
  }

  async function loadOutputPreview(kind, path, duration) {
    if (kind === 'rife') {
      rifeOutputPreview = rifePreviewSet?.cover || '';
    } else {
      smoothieOutputPreview = liveRenderPreview || smoothiePreviewSet?.cover || '';
    }
  }

  async function refreshLiveRenderPreview(path, duration, currentProgress) {
    if (isLiveRenderPreviewLoading || isRenderPaused) return;
    const now = Date.now();
    if (now - lastLiveRenderPreviewAt < 1000) return;
    lastLiveRenderPreviewAt = now;
    const requestId = ++liveRenderPreviewRequest;
    isLiveRenderPreviewLoading = true;
    const safeProgress = Math.max(0, Math.min(99.8, Number(currentProgress) || 0));

    try {
      const image = await invoke('generate_video_preview_frame', {
        videoPath: path,
        timestamp: duration * (safeProgress / 100),
        blendFrames: Math.min(24, Math.max(4, Math.round(
          ((smoothieInfo?.fps || 30) / Math.max(1, smoothieSettings.fps)) *
          (1 + Number(smoothieSettings.blendIntensity || 0))
        )))
      });
      if (
        requestId === liveRenderPreviewRequest &&
        isSmoothieProcessing &&
        !isRenderPaused &&
        smoothiePath === path
      ) {
        liveRenderPreview = image;
      }
    } catch (error) {
      appendLog(`[cia render] Live preview unavailable: ${error}`);
    } finally {
      if (requestId === liveRenderPreviewRequest) isLiveRenderPreviewLoading = false;
    }
  }

  $effect(() => {
    const path = smoothiePath;
    const duration = smoothieInfo?.duration || 0;
    const currentProgress = progress;
    if (isSmoothieProcessing && !isRenderPaused && path && duration > 0) {
      void refreshLiveRenderPreview(path, duration, currentProgress);
    } else if (!isSmoothieProcessing) {
      resetLiveRenderPreview();
    }
  });

  function createRenderJobId() {
    if (globalThis.crypto?.randomUUID) return globalThis.crypto.randomUUID();
    return `cia-render-${Date.now()}-${Math.random().toString(16).slice(2)}`;
  }

  function isCancellation(error) {
    return String(error).includes('CIA_RENDER_CANCELLED');
  }

  function interpolationStatusLabel() {
    if (isRenderPaused) return jobPhase === 'smoothie' ? 'RENDER PAUSED' : 'RIFE PAUSED';
    if (isEncodingPhase) {
      if (encodingSpeed) return `ENCODING (${encodingSpeed})`;
      if (encodingFps) return `ENCODING (${encodingFps} FPS)`;
      return 'ENCODING EXPORT';
    }
    return jobPhase === 'smoothie' ? 'SMOOTHIE RENDERING' : 'RIFE PROCESSING';
  }

  function smoothieStatusLabel() {
    if (isRenderPaused) return 'RENDER PAUSED';
    if (isEncodingPhase) {
      if (encodingSpeed) return `ENCODING (${encodingSpeed})`;
      if (encodingFps) return `ENCODING (${encodingFps} FPS)`;
      return 'ENCODING EXPORT';
    }
    return 'RENDERING';
  }

  function activeProcessLabel() {
    return isSmoothieProcessing || jobPhase === 'smoothie' ? 'Render' : 'Interpolation';
  }

  async function toggleRenderPause() {
    if (!activeRenderJobId || isCancellingRender) return;
    const processLabel = activeProcessLabel();
    try {
      await invoke(isRenderPaused ? 'resume_render' : 'pause_render', { jobId: activeRenderJobId });
      isRenderPaused = !isRenderPaused;
      appendLog(`[cia render] ${processLabel} ${isRenderPaused ? 'paused' : 'resumed'} by user`);
    } catch (e) {
      appendLog(`[cia render] Unable to ${isRenderPaused ? 'resume' : 'pause'} ${processLabel.toLowerCase()}: ${e}`);
    }
  }

  async function cancelRender() {
    if (!activeRenderJobId || isCancellingRender) return;
    showRenderCancelConfirmation = false;
    isCancellingRender = true;
    try {
      await invoke('cancel_render', { jobId: activeRenderJobId });
      appendLog('[cia render] Cancellation requested by user');
    } catch (e) {
      appendLog(`[cia render] Unable to cancel render: ${e}`);
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

    if (line.includes('Finalizing output with FFmpeg') || line.includes('[cia render] Finalizing output')) {
      isEncodingPhase = true;
      progress = 0;
      encodingProgress = 0;
      encodingFrame = 0;
      encodingFps = '';
      encodingSpeed = '';
      encodingTime = '';
      remainingTime = '--:--';
      return;
    }

    if (line.includes('[cia render] ENCODING frame=')) {
      isEncodingPhase = true;
      const frameMatch = line.match(/frame=(\d+)/);
      if (frameMatch) encodingFrame = parseInt(frameMatch[1], 10);

      const totalMatch = line.match(/total_frames=(\d+)/);
      const totalFrames = totalMatch ? parseInt(totalMatch[1], 10) : 0;

      const fpsMatch = line.match(/fps=([\d.]+)/);
      if (fpsMatch) encodingFps = fpsMatch[1];

      const speedMatch = line.match(/speed=([\d.]+)x/);
      if (speedMatch) encodingSpeed = `${speedMatch[1]}x`;

      const timeMatch = line.match(/time=(\d{2}:\d{2}:\d{2})/);
      if (timeMatch) {
        encodingTime = timeMatch[1];
        elapsedTime = timeMatch[1].slice(3);
      }

      const pctMatch = line.match(/pct=(\d+)%/);
      if (pctMatch) {
        const pctVal = Math.min(100, Math.max(0, parseInt(pctMatch[1], 10)));
        progress = pctVal;
        encodingProgress = pctVal;
      }

      if (fpsMatch && totalFrames > 0 && frameMatch) {
        const fpsVal = parseFloat(fpsMatch[1]);
        const curF = parseInt(frameMatch[1], 10);
        if (fpsVal > 0 && curF < totalFrames) {
          const remSec = Math.max(0, Math.round((totalFrames - curF) / fpsVal));
          const remMins = Math.floor(remSec / 60);
          const remSecs = remSec % 60;
          remainingTime = `${remMins.toString().padStart(2, '0')}:${remSecs.toString().padStart(2, '0')}`;
        } else if (curF >= totalFrames) {
          remainingTime = '00:00';
        }
      }
      return;
    }

    const smPct = line.match(/(\d+(?:\.\d+)?)%\s*\u2022/);
    if (smPct) {
      if (activePage === 'smoothie') isEncodingPhase = true;
      const pctVal = Math.min(100, Math.max(0, Math.round(parseFloat(smPct[1]))));
      progress = pctVal;
      encodingProgress = pctVal;

      const smFrames = line.match(/•\s*(\d+)\s*\/\s*(\d+)\s*•/);
      if (smFrames) {
        encodingFrame = parseInt(smFrames[1], 10);
      }

      const smFps = line.match(/•\s*([0-9.]+)\s*FPS/);
      if (smFps && smFps[1] !== 'NaN') {
        encodingFps = smFps[1];
      }

      const smTimer = line.match(/(\d+:\d{2})\s*>\s*(\d+:\d{2})/);
      if (smTimer) {
        elapsedTime = smTimer[1];
        remainingTime = smTimer[2];
      }
      return;
    }

    const isFfmpegProgress = line.includes('frame=') && line.includes('time=') && !line.includes('time=N/A');
    if (isFfmpegProgress) {
      isEncodingPhase = true;
      const frameMatch = line.match(/frame=\s*(\d+)/);
      if (frameMatch) encodingFrame = parseInt(frameMatch[1], 10);

      const fpsMatch = line.match(/fps=\s*([\d.]+)/);
      if (fpsMatch && fpsMatch[1] !== '0.0') encodingFps = fpsMatch[1];

      const speedMatch = line.match(/speed=\s*([\d.]+)x/);
      if (speedMatch) encodingSpeed = `${speedMatch[1]}x`;

      const timeMatch = line.match(/time=(\d{2}):(\d{2}):(\d{2}(?:\.\d+)?)/);
      if (timeMatch) {
        const hours = parseInt(timeMatch[1], 10);
        const minutes = parseInt(timeMatch[2], 10);
        const seconds = parseFloat(timeMatch[3]);
        const curSec = hours * 3600 + minutes * 60 + seconds;
        encodingTime = `${timeMatch[1]}:${timeMatch[2]}:${Math.floor(seconds).toString().padStart(2, '0')}`;
        elapsedTime = `${timeMatch[2]}:${Math.floor(seconds).toString().padStart(2, '0')}`;

        let targetDuration = 0;
        if (activePage === 'smoothie' && smoothieInfo?.duration) {
          targetDuration = smoothieInfo.duration;
        } else if (videoInfo?.duration) {
          targetDuration = rifeSettings.mode === 'slowmo'
            ? videoInfo.duration * (Number(rifeSettings.factor) || 2)
            : videoInfo.duration;
        }

        if (targetDuration > 0 && curSec > 0) {
          const calculatedPct = Math.min(99, Math.max(1, Math.round((curSec / targetDuration) * 100)));
          encodingProgress = calculatedPct;
          progress = calculatedPct;

          if (speedMatch) {
            const speedVal = parseFloat(speedMatch[1]);
            if (speedVal > 0) {
              const remSec = Math.max(0, Math.round((targetDuration - curSec) / speedVal));
              const remMins = Math.floor(remSec / 60);
              const remSecs = remSec % 60;
              remainingTime = `${remMins.toString().padStart(2, '0')}:${remSecs.toString().padStart(2, '0')}`;
            }
          }
        }
      }
      return;
    }

    if (!isEncodingPhase) {
      const rifePct = line.match(/^\s*(\d{1,3})%/);
      if (rifePct) progress = parseInt(rifePct[1], 10);

      const rifeTimer = line.match(/\[(\d+(?::\d+)+)<(\d+(?::\d+)+)/);
      if (rifeTimer) { elapsedTime = rifeTimer[1]; remainingTime = rifeTimer[2]; }
    }
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
    if (activePage !== page) {
      activePage = page;
      pushNavigation({ page, smoothiePath, videoPath });
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
    const u5 = listen('install-progress', (event) => {
      const { step, total, label } = event.payload;
      installStep = step;
      installTotal = total;
      installLabel = label;
    });
    return () => { u1.then(f => f()); u2.then(f => f()); u3.then(f => f()); u4.then(f => f()); u5.then(f => f()); };
  });

  // --- RIFE Handlers ---
  async function loadVideo(path, pushNav = true) {
    rifePreviewRequest += 1;
    rifeOutputPreviewRequest += 1;
    videoPath = path;
    isLoading = true;
    isComplete = false;
    lastOutputPath = '';
    rifeOutputPath = '';
    resetSourcePreview('rife');
    rifeOutputPreview = '';
    jobPhase = 'idle';
    jobError = '';
    resetRunState();
    try {
      videoInfo = await invoke('analyze_video', { videoPath: path });
      void loadSourcePreview('rife', path, videoInfo.duration);
      showToast(`Loaded ${videoInfo.width}x${videoInfo.height} @ ${videoInfo.fps.toFixed(2)} FPS`, 'success');
      if (pushNav) {
        pushNavigation({ page: 'dashboard', smoothiePath, videoPath: path });
      }
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
    rifeOutputPreviewRequest += 1;
    rifeOutputPreview = '';
    jobError = '';
    jobPhase = 'rife';
    activeRenderJobId = createRenderJobId();
    isRenderPaused = false;
    isCancellingRender = false;
    beginLogCapture();
    appendLog('[cia render] RIFE 4.26 started');
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
      appendLog(`[cia render] RIFE output verified: ${outputPath}`);

      if (autoRender) {
        jobPhase = 'smoothie';
        isRenderPaused = false;
        const smoothiePath = await runSmoothieFor(outputPath, { preserveLogs: true, jobId: activeRenderJobId });
        lastOutputPath = smoothiePath;
        appendLog(`[cia render] Smoothie output verified: ${smoothiePath}`);
      }

      progress = 100;
      jobPhase = 'complete';
      isComplete = true;
      void loadOutputPreview('rife', lastOutputPath, outputDuration);
      playCompletionChime();
      showToast(autoRender ? 'Interpolation and render complete!' : 'Interpolation complete!', 'success');
    } catch (e) {
      if (isCancellation(e) && !rifeOutputPath) {
        jobError = '';
        jobPhase = 'idle';
        isComplete = false;
      } else {
        jobError = isCancellation(e) ? 'Render cancelled. The RIFE output is still available.' : String(e);
      }
      if (rifeOutputPath) {
        lastOutputPath = rifeOutputPath;
        isComplete = true;
        jobPhase = 'failed';
        void loadOutputPreview('rife', lastOutputPath, outputDuration);
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
    rifePreviewRequest += 1;
    rifeOutputPreviewRequest += 1;
    videoPath = '';
    videoInfo = null;
    isComplete = false;
    rifeOutputPath = '';
    lastOutputPath = '';
    jobPhase = 'idle';
    jobError = '';
    resetSourcePreview('rife');
    rifeOutputPreview = '';
    resetRunState();
  }

  async function renderRifeWithSmoothie() {
    if (!rifeOutputPath || anyProcessing) return;
    isProcessing = true;
    isComplete = false;
    jobError = '';
    jobPhase = 'smoothie';
    activeRenderJobId = createRenderJobId();
    isRenderPaused = false;
    isCancellingRender = false;
    try {
      const smoothiePath = await runSmoothieFor(rifeOutputPath, { preserveLogs: true, jobId: activeRenderJobId });
      lastOutputPath = smoothiePath;
      appendLog(`[cia render] Smoothie output verified: ${smoothiePath}`);
      progress = 100;
      jobPhase = 'complete';
      isComplete = true;
      void loadOutputPreview('rife', lastOutputPath, outputDuration);
      playCompletionChime();
      showToast('Render complete!', 'success');
    } catch (e) {
      jobError = isCancellation(e) ? 'Render cancelled. The RIFE output is still available.' : String(e);
      lastOutputPath = rifeOutputPath;
      jobPhase = 'failed';
      isComplete = true;
      if (!isCancellation(e)) showToast(`Render failed: ${e}`, 'error');
    } finally {
      isProcessing = false;
      activeRenderJobId = '';
      isRenderPaused = false;
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
  async function loadSmoothie(path, pushNav = true) {
    smoothiePreviewRequest += 1;
    smoothieOutputPreviewRequest += 1;
    resetLiveRenderPreview();
    smoothiePath = path;
    isSmoothieLoading = true;
    isSmoothieComplete = false;
    smoothieOutputPath = '';
    resetSourcePreview('smoothie');
    smoothieOutputPreview = '';
    resetRunState();
    try {
      smoothieInfo = await invoke('analyze_video', { videoPath: path });
      void loadSourcePreview('smoothie', path, smoothieInfo.duration);
      showToast(`Loaded ${smoothieInfo.width}x${smoothieInfo.height} @ ${smoothieInfo.fps.toFixed(2)} FPS`, 'success');
      if (pushNav) {
        pushNavigation({ page: 'smoothie', smoothiePath: path, videoPath });
      }
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
    appendLog('[cia render] SMOOTHIE started');
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
    smoothieOutputPreviewRequest += 1;
    smoothieOutputPreview = smoothiePreviewSet?.cover || '';
    resetLiveRenderPreview();
    activeRenderJobId = createRenderJobId();
    isRenderPaused = false;
    isCancellingRender = false;
    isEncodingPhase = true;

    try {
      const outPath = await runSmoothieFor(smoothiePath, { jobId: activeRenderJobId });
      progress = 100;
      smoothieOutputPath = outPath;
      smoothieOutputPreview = liveRenderPreview || smoothiePreviewSet?.cover || '';
      isSmoothieComplete = true;
      void loadOutputPreview('smoothie', outPath, smoothieInfo?.duration || 0);
      playCompletionChime();
      showToast('Render complete!', 'success');
    } catch (e) {
      if (!isCancellation(e)) showToast(`Render failed: ${e}`, 'error');
    } finally {
      isSmoothieProcessing = false;
      activeRenderJobId = '';
      isRenderPaused = false;
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
    appendLog('[cia render] Checking for an available RIFE environment');
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
          showToast('cia render is up to date', 'success');
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

      showToast('Update installed. Restarting cia render...', 'success');
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

<svelte:window onkeydown={handleNavKeyDown} />

<div class="app-root" class:dragging={isDragging}>
  <!-- Custom Windows Titlebar -->
  <div class="titlebar" data-tauri-drag-region>
    <div class="titlebar-brand">
      <div class="nav-arrows" data-tauri-drag-region="false">
        <button
          class="nav-arrow-btn"
          onclick={handleGoBack}
          disabled={!canGoBack}
          aria-label="Previous page"
          title="Précédent"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M19 12H5M12 19l-7-7 7-7" />
          </svg>
        </button>
        <button
          class="nav-arrow-btn"
          onclick={handleGoForward}
          disabled={!canGoForward}
          aria-label="Next page"
          title="Suivant"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M5 12h14M12 5l7 7-7 7" />
          </svg>
        </button>
      </div>
      <span class="titlebar-text">cia render</span>
    </div>
    <div class="titlebar-controls">
      {#if availableUpdate}
        <button class="titlebar-btn update-badge" onclick={() => showUpdateModal = true} aria-label="Update available">
          <span class="update-badge-dot"></span> UPDATE V{availableUpdate.version}
        </button>
      {/if}
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
            <span class="about-kicker">cia render / RUNTIME REPAIR</span>
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
              <div class="path-field"><input id="setup-rife-script" bind:value={setupDraft.rife.script} placeholder="Bundled cia render script is used by default" /><button onclick={() => browseRuntimePath('rife_script')}>BROWSE</button></div>
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
            <span>Configuration is saved to your cia render app-data folder.</span>
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
                  <div class="pro-indeterminate-sweep"></div>
                </div>
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
          <div class="drop-center-content">
            <div class="drop-icon-box">
              <svg width="34" height="34" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
                <rect x="2" y="4" width="20" height="16" rx="2" />
                <path d="M7 4v16M17 4v16M2 12h20M2 8h5M2 16h5M17 8h5M17 16h5" />
              </svg>
            </div>
            <span class="drop-prompt-label">DRAG CLIP</span>
          </div>
        </div>
      {:else if isLoading}
        <div class="loading-state"><p>ANALYZING VIDEO MATRIX...</p></div>
      {:else if videoInfo}
        {#if isProcessing}
          <div class="pro-render-card">
            <header class="pro-header">
              <h3 class="pro-filename" title={videoPath.split(/[\\/]/).pop()}>{videoPath.split(/[\\/]/).pop()}</h3>
            </header>

            <div class="pro-pipeline-box">
              <div class="fps-signature" aria-label={`Frame rate transformation: ${videoInfo.fps.toFixed(0)} frames per second to ${outputFps.toFixed(0)} frames per second`}>
                <span class="fps-value">{videoInfo.fps.toFixed(0)}</span>
                <span class="fps-transition" class:paused={isRenderPaused} aria-hidden="true">
                  <span class="fps-dot"></span><span class="fps-dot"></span><span class="fps-dot"></span>
                </span>
                <span class="fps-value">{outputFps.toFixed(0)}</span>
                <span class="fps-unit">FPS</span>
              </div>
            </div>

            <div class="pro-progress-card">
              <div class="pro-progress-header">
                <span class="progress-stage-name">{interpolationStatusLabel()}</span>
                <div class="progress-summary">
                  <span class="pro-percent-hero">{progress}%</span>
                  <span
                    class="progress-time"
                    aria-label={`Estimated remaining: ${remainingTime}. Elapsed: ${elapsedTime}.`}
                  ><span class="eta-time" aria-hidden="true">{remainingTime}</span><span class="elapsed-time" aria-hidden="true">{elapsedTime}</span></span>
                </div>
              </div>
              <div class="pro-track" class:paused={isRenderPaused}>
                <div class="pro-indeterminate-sweep"></div>
              </div>
            </div>

            <div class="render-control-row">
              <button
                class="btn-pro-icon"
                class:is-resume={isRenderPaused}
                onclick={toggleRenderPause}
                disabled={isCancellingRender}
                aria-label={isRenderPaused ? 'Resume interpolation' : 'Pause interpolation'}
              >
                {#if isRenderPaused}
                  <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5.5v13l10-6.5z" fill="currentColor" /></svg>
                {:else}
                  <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="5" width="4" height="14" rx="0.75" fill="currentColor" /><rect x="14" y="5" width="4" height="14" rx="0.75" fill="currentColor" /></svg>
                {/if}
              </button>
              <button
                class="btn-pro-icon cancel-control"
                onclick={() => showRenderCancelConfirmation = true}
                disabled={isCancellingRender}
                aria-label="Cancel interpolation"
              >
                <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6.5" y="6.5" width="11" height="11" rx="0.75" fill="currentColor" /></svg>
              </button>
            </div>
          </div>
        {:else if isComplete}
          <div class="pro-complete-card" class:interrupted={Boolean(jobError)}>
            {#if jobError}
              <span class="completion-error">{jobError}</span>
            {/if}
            <button class="completion-preview" onclick={openFile} aria-label="Open rendered video">
              {#if rifeOutputPreview || rifePreviewSet?.cover}
                <img src={rifeOutputPreview || rifePreviewSet.cover} alt="Preview of the rendered video" />
              {:else}
                <span class="completion-preview-loading">PREPARING PREVIEW</span>
              {/if}
            </button>
            <span class="completion-output-name" title={lastOutputPath}>{lastOutputPath.split(/[\\/]/).pop()}</span>

            <div class="complete-actions-row">
              <button class="btn-pro-secondary completion-action" onclick={openFolder}>REVEAL IN EXPLORER</button>
              {#if canRenderSmoothie}
                <button class="btn-pro-secondary completion-action" onclick={renderRifeWithSmoothie}>{jobPhase === 'failed' ? 'RETRY RENDER' : 'RENDER'}</button>
              {/if}
              <button class="btn-pro-secondary completion-action" onclick={resetInterpolation}>NEW RENDER</button>
            </div>
          </div>
        {:else}
          <div class="settings-workspace">
            <section class="settings-unified-panel">
              <div class="settings-source-stage">
                <header class="settings-panel-header">
                  <h3 class="settings-media-name" title={videoPath}>{videoPath.split(/[\\/]/).pop()}</h3>
                </header>
                <div class="source-preview" role="group" aria-label="Eight-frame source video preview" onmouseenter={() => startSourcePreviewCycle('rife')} onmouseleave={() => stopSourcePreviewCycle('rife')}>
                  {#if rifePreviewSet?.frames?.[rifePreviewFrameIndex]}
                    <img class="source-preview-cover" src={rifePreviewSet.frames[rifePreviewFrameIndex]} alt="Preview of selected source video" />
                  {:else if rifePreviewSet?.cover}
                    <img class="source-preview-cover" src={rifePreviewSet.cover} alt="Preview of selected source video" />
                  {:else}
                    <span class="source-preview-loading" aria-label="Preparing video preview"></span>
                  {/if}
                  <button class="settings-change-btn" onclick={clearVideoSelection} aria-label="Choose another video">
                    <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7h9m0 0-3-3m3 3-3 3M17 17H8m0 0 3 3m-3-3 3-3" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" /></svg>
                  </button>
                </div>
              </div>

              <div class="settings-config-stage">
                <header class="settings-panel-header">
                  <h3 class="settings-title">settings</h3>
                  <button class="btn-details" onclick={() => showRifeSettings = true}>DETAILS</button>
                </header>

                <div class="settings-main-control">
                  <GlowSlider bind:value={rifeSettings.factor} min={2} max={10} step={1} label="" unit="x" />
                </div>

                <label class="auto-render-toggle">
                  <input type="checkbox" bind:checked={autoRender} onchange={saveAutoRender} />
                  <span>AUTO-RENDER AFTER INTERPOLATION</span>
                </label>

                <button class="btn-pro-secondary settings-start" onclick={startProcessing} disabled={anyProcessing}>
                  {isProcessing ? 'PROCESSING...' : 'START INTERPOLATION'}
                </button>
              </div>
            </section>
          </div>
        {/if}
      {/if}

    <!-- RENDER PAGE (smoothie-rs engine) -->
    {:else if activePage === 'smoothie'}
      {#if !smoothiePath}
        <div class="drop-zone" class:dragging={isDragging} onclick={pickSmoothieFile} onkeydown={(event) => activateOnKeyboard(event, pickSmoothieFile)} role="button" tabindex="0">
          <div class="drop-center-content">
            <div class="drop-icon-box">
              <svg width="34" height="34" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" stroke-linejoin="round">
                <rect x="2" y="4" width="20" height="16" rx="2" />
                <path d="M7 4v16M17 4v16M2 12h20M2 8h5M2 16h5M17 8h5M17 16h5" />
              </svg>
            </div>
            <span class="drop-prompt-label">DRAG CLIP</span>
          </div>
        </div>
      {:else if isSmoothieLoading}
        <div class="loading-state"><p>ANALYZING VIDEO MATRIX...</p></div>
      {:else if smoothieInfo}
        {#if isSmoothieProcessing}
          <div class="pro-render-card render-processing-card">
            <header class="render-processing-header">
              <h3 class="pro-filename" title={smoothiePath.split(/[\\/]/).pop()}>{smoothiePath.split(/[\\/]/).pop()}</h3>
              <div class="fps-signature" aria-label={`Frame rate transformation: ${smoothieInfo.fps.toFixed(0)} frames per second to ${smoothieSettings.fps} frames per second`}>
                <span class="fps-value">{smoothieInfo.fps.toFixed(0)}</span>
                <span class="fps-transition" class:paused={isRenderPaused} aria-hidden="true">
                  <span class="fps-dot"></span><span class="fps-dot"></span><span class="fps-dot"></span>
                </span>
                <span class="fps-value">{smoothieSettings.fps}</span>
                <span class="fps-unit">FPS</span>
              </div>
            </header>

            <div class="render-preview-stage">
              <section
                class="live-render-preview"
                style={`aspect-ratio: ${smoothieAspectRatio};`}
                aria-label="Low resolution render timeline preview"
              >
                {#if liveRenderPreview || smoothiePreviewSet?.cover}
                  <img src={liveRenderPreview || smoothiePreviewSet.cover} alt="Current blended source frame at the render timeline position" />
                {:else}
                  <span class="live-preview-loading" aria-label="Preparing live preview"></span>
                {/if}
              </section>
            </div>

            <div class="render-bottom-bar">
              <div class="render-progress-card">
                <div class="render-progress-header">
                  <span class="render-stage-label">{smoothieStatusLabel()}</span>
                  <div class="render-progress-stats">
                    <span class="render-percent">{progress}%</span>
                    <span
                      class="render-eta"
                      aria-label={`Estimated remaining: ${remainingTime}. Elapsed: ${elapsedTime}.`}
                    ><span class="eta-time" aria-hidden="true">{remainingTime}</span><span class="elapsed-time" aria-hidden="true">{elapsedTime}</span></span>
                  </div>
                </div>
                <div class="pro-track" class:paused={isRenderPaused}>
                  <div class="pro-indeterminate-sweep"></div>
                </div>
              </div>

              <div class="render-control-row">
                <button
                  class="btn-render-action"
                  class:is-resume={isRenderPaused}
                  onclick={toggleRenderPause}
                  disabled={isCancellingRender}
                  aria-label={isRenderPaused ? 'Resume render' : 'Pause render'}
                >
                  {#if isRenderPaused}
                    <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M8 5.5v13l10-6.5z" fill="currentColor" /></svg>
                  {:else}
                    <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="5" width="4" height="14" rx="0.75" fill="currentColor" /><rect x="14" y="5" width="4" height="14" rx="0.75" fill="currentColor" /></svg>
                  {/if}
                </button>
                <button
                  class="btn-render-action cancel-control"
                  onclick={() => showRenderCancelConfirmation = true}
                  disabled={isCancellingRender}
                  aria-label="Cancel render"
                >
                  <svg viewBox="0 0 24 24" aria-hidden="true"><rect x="6" y="6" width="12" height="12" rx="0.75" fill="currentColor" /></svg>
                </button>
              </div>
            </div>
          </div>
        {:else if isSmoothieComplete}
          <div class="pro-complete-card">
            <button class="completion-preview" onclick={openSmoothieFile} aria-label="Open rendered video">
              {#if smoothieOutputPreview || liveRenderPreview || smoothiePreviewSet?.cover}
                <img src={smoothieOutputPreview || liveRenderPreview || smoothiePreviewSet.cover} alt="Preview of the rendered video" />
              {:else}
                <span class="completion-preview-loading">PREPARING PREVIEW</span>
              {/if}
            </button>
            <span class="completion-output-name" title={smoothieOutputPath}>{smoothieOutputPath.split(/[\\/]/).pop()}</span>

            <div class="complete-actions-row">
              <button class="btn-pro-secondary completion-action" onclick={openSmoothieFolder}>REVEAL IN EXPLORER</button>
              <button class="btn-pro-secondary completion-action" onclick={() => clearSmoothieSelection(true)}>NEW RENDER</button>
            </div>
          </div>
        {:else}
          <div class="settings-workspace">
            <section class="settings-unified-panel">
              <div class="settings-source-stage">
                <header class="settings-panel-header">
                  <h3 class="settings-media-name" title={smoothiePath}>{smoothiePath.split(/[\\/]/).pop()}</h3>
                </header>
                <div class="source-preview" role="group" aria-label="Eight-frame source video preview" onmouseenter={() => startSourcePreviewCycle('smoothie')} onmouseleave={() => stopSourcePreviewCycle('smoothie')}>
                  {#if smoothiePreviewSet?.frames?.[smoothiePreviewFrameIndex]}
                    <img class="source-preview-cover" src={smoothiePreviewSet.frames[smoothiePreviewFrameIndex]} alt="Preview of selected source video" />
                  {:else if smoothiePreviewSet?.cover}
                    <img class="source-preview-cover" src={smoothiePreviewSet.cover} alt="Preview of selected source video" />
                  {:else}
                    <span class="source-preview-loading" aria-label="Preparing video preview"></span>
                  {/if}
                  <button class="settings-change-btn" onclick={() => clearSmoothieSelection(false)} aria-label="Choose another video">
                    <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M7 7h9m0 0-3-3m3 3-3 3M17 17H8m0 0 3 3m-3-3 3-3" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round" /></svg>
                  </button>
                </div>
              </div>

              <div class="settings-config-stage">
                <header class="settings-panel-header">
                  <h3 class="settings-title">settings</h3>
                  <button class="btn-details" onclick={() => showSmoothieSettings = true}>DETAILS</button>
                </header>

                <div class="settings-main-control">
                  <GlowSlider bind:value={smoothieSettings.fps} min={20} max={60} step={1} label="" unit=" fps" editableMin={10} />
                </div>

                <div class="settings-blur-control">
                  <GlowSlider bind:value={smoothieSettings.blendIntensity} min={0} max={4} step={0.1} precision={1} label="blur intensity" />
                </div>

                <button class="btn-pro-secondary settings-start" onclick={startSmoothie} disabled={anyProcessing}>
                  {isSmoothieProcessing ? 'PROCESSING...' : 'START RENDER'}
                </button>
              </div>
            </section>
          </div>
        {/if}
      {/if}
    {:else if activePage === 'about'}
      <section class="about-page" aria-label="Project credits">
        <header class="about-identity">
          <img class="about-app-logo" src={appLogo} alt="cia render logo" />
          <div class="about-app-copy">
            <h1>cia render <span>V{appVersion}</span></h1>
            <p>Local render workflow, credits and contact.</p>
          </div>
          <div class="about-contacts">
            <button class="about-update-btn" class:has-update={!!availableUpdate} onclick={() => checkForAppUpdates(true)} aria-label="Check for cia render updates" disabled={updateState === 'checking' || updateState === 'downloading'}>
              {#if updateState === 'checking'}
                <span>CHECKING...</span>
              {:else if availableUpdate}
                <span class="update-ready-text"><span class="pro-dot active"></span> UPDATE V{availableUpdate.version}</span>
              {:else}
                <span>CHECK FOR UPDATES</span>
              {/if}
            </button>
            <button class="discord-contact" onclick={copyDiscordHandle} aria-label="Copy cia render Discord handle">
              <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M19.5 4.7a16.8 16.8 0 0 0-4.1-1.3l-.5 1.1a15.1 15.1 0 0 0-5.8 0l-.5-1.1A16.9 16.9 0 0 0 4.5 4.7C1.9 8.5 1.2 12.2 1.6 15.8a16.8 16.8 0 0 0 5 2.5l1.2-1.6a9.8 9.8 0 0 1-1.9-.9l.5-.4c3.7 1.7 7.7 1.7 11.4 0l.5.4c-.6.4-1.2.7-1.9.9l1.2 1.6a16.6 16.6 0 0 0 5-2.5c.5-4.2-.8-7.8-3.1-11.1ZM8.7 13.6c-1 0-1.8-.9-1.8-2s.8-2 1.8-2 1.8.9 1.8 2-.8 2-1.8 2Zm6.6 0c-1 0-1.8-.9-1.8-2s.8-2 1.8-2 1.8.9 1.8 2-.8 2-1.8 2Z" /></svg>
              <span>{discordCopyFeedback ? 'COPIED' : 'cia2013'}</span>
            </button>
            <button class="github-contact" onclick={() => openAboutLink(PROJECT_REPOSITORY_URL)} aria-label="Open cia render on GitHub">
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

  <!-- RIFE DETAILS MODAL -->
  {#if showRifeSettings}
    <div class="modal-backdrop" onclick={() => showRifeSettings = false} role="presentation">
      <div class="modal-card settings-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="rife-settings-title" tabindex="0">
        <div class="modal-header">
          <div class="settings-title-group">
            <span class="settings-kicker">INTERPOLATION</span>
            <h2 id="rife-settings-title">DETAILS</h2>
          </div>
          <button class="btn-close-modal" onclick={() => showRifeSettings = false} aria-label="Close interpolation settings">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m7 7 10 10M17 7 7 17" /></svg>
          </button>
        </div>
        <div class="modal-body settings-body">
          <div class="settings-section">
            <h3>CORE CONFIGURATION</h3>
            <div class="setting-row settings-control">
              <label for="mod-rife-mode" class="has-tooltip" data-tooltip="Slowmo extends video duration; Boost doubles FPS at normal speed.">MODE</label>
              <select id="mod-rife-mode" bind:value={rifeSettings.mode}>
                <option value="boost">FPS Boost (same duration)</option>
                <option value="slowmo">Slowmo (duration x factor)</option>
              </select>
            </div>
            <div class="setting-row settings-control">
              <label for="mod-rife-factor" class="has-tooltip" data-tooltip="Multiplier factor (2x to 10x).">FACTOR</label>
              <input id="mod-rife-factor" type="number" min="2" max="10" bind:value={rifeSettings.factor} />
            </div>
          </div>

          <div class="settings-section">
            <h3>ADVANCED PARAMETERS</h3>
            <div class="setting-row settings-control">
              <label for="mod-rife-thresh" class="has-tooltip" data-tooltip="Threshold for detecting hard scene changes (0.01 - 0.50).">SCENE THRESHOLD</label>
              <input id="mod-rife-thresh" type="number" step="0.01" min="0.01" max="0.5" bind:value={rifeSettings.sceneThreshold} />
            </div>
            <div class="setting-row settings-control">
              <label for="mod-rife-blend" class="has-tooltip" data-tooltip="Crossfade frames at scene cuts (0 = hard cut).">BLEND CUTS</label>
              <input id="mod-rife-blend" type="number" step="1" min="0" max="30" bind:value={rifeSettings.blendCuts} />
            </div>
            <div class="setting-row settings-control">
              <label for="mod-rife-crf" class="has-tooltip" data-tooltip="H.264 CRF quality factor (18 = visually lossless).">CRF QUALITY</label>
              <input id="mod-rife-crf" type="number" step="1" min="0" max="51" bind:value={rifeSettings.crf} />
            </div>
            <div class="setting-row settings-control">
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

  <!-- RENDER DETAILS MODAL -->
  {#if showSmoothieSettings}
    <div class="modal-backdrop" onclick={() => showSmoothieSettings = false} role="presentation">
      <div class="modal-card settings-card" onclick={(e) => e.stopPropagation()} onkeydown={(e) => e.stopPropagation()} role="dialog" aria-modal="true" aria-labelledby="smoothie-settings-title" tabindex="0">
        <div class="modal-header">
          <div class="settings-title-group">
            <span class="settings-kicker">RENDER</span>
            <h2 id="smoothie-settings-title">DETAILS</h2>
          </div>
          <button class="btn-close-modal" onclick={() => showSmoothieSettings = false} aria-label="Close render settings">
            <svg viewBox="0 0 24 24" aria-hidden="true"><path d="m7 7 10 10M17 7 7 17" /></svg>
          </button>
        </div>
        <div class="modal-body settings-body">
          <div class="settings-section">
            <h3>OUTPUT PARAMS</h3>
            <div class="setting-row settings-control">
              <label for="mod-sm-fps" class="has-tooltip" data-tooltip="Target frame blending output fps.">output fps</label>
              <input id="mod-sm-fps" type="number" min="10" max="60" bind:value={smoothieSettings.fps} />
            </div>
            <div class="slider-row settings-slider">
              <GlowSlider bind:value={smoothieSettings.blendIntensity} min={0} max={4} step={0.1} precision={1} label="blur intensity" />
            </div>
          </div>

          <div class="settings-section">
            <h3>COLOR GRADING</h3>
            <div class="slider-row settings-slider">
              <div class="slider-header"><span class="slider-label">BRIGHTNESS:</span><span class="slider-val">{smoothieSettings.brightness}</span></div>
              <input type="range" min="0.0" max="2.0" step="0.05" bind:value={smoothieSettings.brightness} class="custom-slider" />
            </div>
            <div class="slider-row settings-slider">
              <div class="slider-header"><span class="slider-label">SATURATION:</span><span class="slider-val">{smoothieSettings.saturation}</span></div>
              <input type="range" min="0.0" max="2.0" step="0.05" bind:value={smoothieSettings.saturation} class="custom-slider" />
            </div>
            <div class="slider-row settings-slider">
              <div class="slider-header"><span class="slider-label">CONTRAST:</span><span class="slider-val">{smoothieSettings.contrast}</span></div>
              <input type="range" min="0.0" max="2.0" step="0.05" bind:value={smoothieSettings.contrast} class="custom-slider" />
            </div>
          </div>

          <div class="settings-section">
            <h3>LUT &amp; DISPLAY</h3>
            <div class="setting-row settings-control">
              <label for="mod-sm-lutenable" class="has-tooltip" data-tooltip="Enable colorcia.cube LUT application.">LUT ENABLED</label>
              <select id="mod-sm-lutenable" bind:value={smoothieSettings.lutEnabled}>
                <option value="yes">yes</option>
                <option value="no">no</option>
              </select>
            </div>
            <div class="slider-row settings-slider">
              <div class="slider-header"><span class="slider-label">LUT OPACITY:</span><span class="slider-val">{(smoothieSettings.lutOpacity * 100).toFixed(0)}%</span></div>
              <input type="range" min="0.0" max="1.0" step="0.05" bind:value={smoothieSettings.lutOpacity} class="custom-slider" />
            </div>
            <div class="setting-row settings-control">
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
            <span>A new version of cia render is ready to install (current: V{appVersion})</span>
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
                  <div class="pro-indeterminate-sweep"></div>
                </div>
              </div>
              <div class="update-bytes-readout">
                {formatBytes(updateDownloadedBytes)} / {updateContentLength > 0 ? formatBytes(updateContentLength) : '...'}
              </div>
            </div>
          {:else if updateState === 'ready'}
            <div class="update-ready-box">
              <span class="pro-dot active"></span>
              <span>UPDATE APPLIED - RESTARTING CIA RENDER...</span>
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

  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: #27272a transparent;
  }

  :global(::-webkit-scrollbar) {
    width: 6px;
    height: 6px;
  }

  :global(::-webkit-scrollbar-track) { background: transparent; }
  :global(::-webkit-scrollbar-thumb) {
    background: #27272a;
    border-radius: 999px;
  }
  :global(::-webkit-scrollbar-thumb:hover) { background: #71717a; }

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
  .nav-arrows {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    margin-right: 10px;
    -webkit-app-region: no-drag;
  }
  .nav-arrow-btn {
    width: 22px;
    height: 22px;
    border: none;
    background: transparent;
    color: #71717a;
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    cursor: pointer;
    transition: all 0.15s ease;
  }
  .nav-arrow-btn:hover:not(:disabled) {
    background: #1c1c20;
    color: #ffffff;
  }
  .nav-arrow-btn:disabled {
    opacity: 0.2;
    cursor: default;
    pointer-events: none;
  }
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
  .titlebar-btn.close:hover { background: #1c1c20; color: #ffffff; border-color: #ffffff; }

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
    overflow-y: auto;
    overflow-x: hidden;
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

  .setup-alert {
    background: #000000;
    border-color: #ffffff;
    color: #ffffff;
  }

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
    color: #a1a1aa;
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
  .about-update-btn,
  .discord-contact,
  .github-contact {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    height: 36px;
    box-sizing: border-box;
    margin: 0;
    padding: 0 12px;
    border: 1px solid #27272a;
    border-radius: 5px;
    background: #0d0d10;
    color: #d4d4d8;
    cursor: pointer;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.03em;
    line-height: 1;
    transition: background-color 150ms ease, border-color 150ms ease, color 150ms ease;
  }
  .discord-contact svg,
  .github-contact svg {
    width: 17px;
    height: 17px;
    fill: currentColor;
    flex-shrink: 0;
  }
  .about-update-btn:hover:not(:disabled),
  .discord-contact:hover,
  .discord-contact:focus-visible,
  .github-contact:hover,
  .github-contact:focus-visible {
    border-color: rgba(255, 255, 255, 0.42);
    background: #16161a;
    color: #ffffff;
    outline: none;
  }
  .about-update-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .about-update-btn.has-update {
    background: #000000;
    border-color: #ffffff;
    color: #ffffff;
  }
  .about-update-btn.has-update:hover:not(:disabled) {
    background: #18181b;
    border-color: #ffffff;
    color: #ffffff;
  }
  .github-contact {
    width: 36px;
    padding: 0;
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

  /* Drop Zone Screen 1 Card */
  .drop-zone {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    min-height: 380px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    background: #08080a;
    cursor: pointer;
    transition: all 0.2s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .drop-zone:hover {
    border-color: rgba(255, 255, 255, 0.25);
    background: #0d0d10;
  }

  .drop-zone.dragging {
    border-color: rgba(255, 255, 255, 0.5);
    background: #111115;
  }

  .drop-center-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 16px;
    pointer-events: none;
    user-select: none;
  }

  .drop-icon-box {
    display: flex;
    align-items: center;
    justify-content: center;
    color: rgba(255, 255, 255, 0.25);
    transition: color 0.2s ease, transform 0.2s ease;
  }

  .drop-zone:hover .drop-icon-box {
    color: rgba(255, 255, 255, 0.45);
    transform: scale(1.04);
  }

  .drop-prompt-label {
    font-family: 'IBM Plex Sans', -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    font-size: 13px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: #f4f4f5;
    text-transform: uppercase;
    transition: color 0.2s ease;
  }

  .drop-zone:hover .drop-prompt-label {
    color: #ffffff;
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

  /* Primary settings workspace */
  .settings-workspace {
    display: flex;
    justify-content: center;
    align-items: center;
    width: 100%;
    margin: auto 0;
  }

  .settings-unified-panel {
    width: 100%;
    max-width: 860px;
    min-height: 0;
    padding: 20px 24px;
    background: #08080a;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    display: grid;
    grid-template-columns: 1.15fr 1fr;
    gap: 28px;
    align-items: center;
  }

  .settings-source-stage {
    width: 100%;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .settings-config-stage {
    width: 100%;
    min-width: 0;
    display: flex;
    flex-direction: column;
    padding-top: 0;
    border-top: none;
  }

  .settings-panel-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
  }

  .settings-media-name {
    margin: 0;
    min-width: 0;
    color: #ffffff;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 13px;
    font-weight: 700;
    letter-spacing: -0.02em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .settings-title {
    margin: 0;
    color: #d4d4d8;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 12px;
    font-weight: 700;
    letter-spacing: -0.02em;
  }

  .source-preview {
    position: relative;
    width: 100%;
    aspect-ratio: 16 / 9;
    margin-top: 12px;
    overflow: hidden;
    background: #050507;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 7px;
  }

  .source-preview-cover,
  .source-preview-loading {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }

  .source-preview-cover {
    object-fit: cover;
  }

  .source-preview-loading {
    background: linear-gradient(100deg, #09090c 18%, #19191e 42%, #09090c 66%);
    background-size: 220% 100%;
    animation: preview-loading-sweep 1.1s linear infinite;
  }

  @keyframes preview-loading-sweep {
    0% { background-position: 100% 0; }
    100% { background-position: -120% 0; }
  }

  .settings-change-btn {
    position: absolute;
    top: 9px;
    right: 9px;
    z-index: 1;
    display: grid;
    width: 31px;
    height: 31px;
    place-items: center;
    padding: 0;
    background: rgba(9, 9, 12, 0.8);
    border: 0;
    border-radius: 5px;
    color: #a1a1aa;
    cursor: pointer;
    backdrop-filter: blur(6px);
    opacity: 0.42;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease, transform 0.15s ease;
  }

  .settings-change-btn svg {
    width: 17px;
    height: 17px;
  }

  .settings-change-btn:hover {
    background: rgba(24, 24, 27, 0.92);
    border-color: rgba(255, 255, 255, 0.55);
    color: #ffffff;
    opacity: 1;
    transform: translateY(-1px);
  }

  .source-preview:hover .settings-change-btn { opacity: 0.72; }

  .btn-details {
    flex: none;
    padding: 5px 9px;
    background: transparent;
    border: 0;
    border-radius: 4px;
    color: #d4d4d8;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.06em;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }

  .btn-details:hover {
    background: #141417;
    border-color: rgba(255, 255, 255, 0.4);
    color: #ffffff;
  }

  .settings-main-control { margin-top: 18px; }

  .settings-blur-control { margin-top: 16px; }

  .settings-config-stage .auto-render-toggle {
    margin: 16px 0 0;
  }

  .settings-start {
    width: 100%;
    min-height: 38px;
    margin-top: 20px;
    padding: 9px 18px;
    box-shadow: none;
  }

  @media (max-width: 680px) {
    .settings-unified-panel { padding: 18px; }
  }

  .setting-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 12px;
  }

  .setting-row label {
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

  /* Buttons */
  .btn-primary {
    width: 100%;
    padding: 12px;
    background: #18181b;
    color: #ffffff;
    border: 0;
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
    border: 0;
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

  /* Professional Render Card (Industrial Telemetry Workstation) */
  .pro-render-card {
    background: #08080a;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 20px 22px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    height: 100%;
    min-height: 380px;
  }

  .pro-header {
    display: flex;
    align-items: center;
    min-width: 0;
  }

  .pro-filename {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 13px;
    font-weight: 700;
    color: #ffffff;
    margin: 0;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* Source-to-target job summary */
  .pro-pipeline-box {
    display: flex;
    align-items: center;
  }

  .fps-signature {
    display: inline-flex;
    align-items: baseline;
    gap: 10px;
    font-family: 'IBM Plex Mono', monospace;
    font-variant-numeric: tabular-nums;
    line-height: 1;
  }

  .fps-value {
    font-size: 20px;
    font-weight: 700;
    letter-spacing: -0.04em;
    color: #ffffff;
  }

  .fps-transition {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    height: 20px;
    margin: 0 3px;
  }

  .fps-dot {
    width: 4px;
    height: 4px;
    border-radius: 50%;
    background: #e4e4e7;
    opacity: 0.35;
    animation: fps-dot-wave 1.05s ease-in-out infinite;
  }

  .fps-dot:nth-child(2) { animation-delay: 0.14s; }
  .fps-dot:nth-child(3) { animation-delay: 0.28s; }
  .fps-transition.paused .fps-dot { animation-play-state: paused; }

  @keyframes fps-dot-wave {
    0%, 60%, 100% { transform: translateY(0); opacity: 0.35; }
    30% { transform: translateY(-4px); opacity: 1; }
  }

  .fps-unit {
    margin-left: -3px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: #a1a1aa;
  }

  .render-processing-card {
    min-height: 0;
    padding: 22px 24px;
    display: flex;
    flex-direction: column;
    justify-content: space-between;
    gap: 12px;
  }

  .render-processing-header {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    min-width: 0;
  }

  .render-processing-header .pro-filename {
    max-width: 100%;
    font-size: 13px;
    letter-spacing: 0.02em;
  }

  .render-processing-header .fps-signature {
    margin-top: 0;
  }

  .render-preview-stage {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 100%;
    padding-left: 56px;
    margin: -6px 0 2px;
    overflow: hidden;
  }

  .live-render-preview {
    position: relative;
    top: -10px;
    left: -5px;
    max-width: min(100%, 475px);
    max-height: 262px;
    height: 100%;
    width: auto;
    aspect-ratio: 16 / 9;
    border-radius: 0;
    overflow: hidden;
    background: #050507;
    border: 1px solid rgba(255, 255, 255, 0.08);
    box-shadow: 0 16px 40px rgba(0, 0, 0, 0.3);
    flex-shrink: 0;
  }

  .live-render-preview img {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .live-preview-loading {
    position: absolute;
    inset: 0;
    display: block;
    background: linear-gradient(100deg, #09090c 18%, #19191e 42%, #09090c 66%);
    background-size: 220% 100%;
    animation: preview-loading-sweep 1.1s linear infinite;
  }

  .render-bottom-bar {
    display: flex;
    align-items: stretch;
    gap: 12px;
    width: 100%;
    margin-top: auto;
  }

  .render-progress-card {
    flex: 1;
    min-width: 0;
    background: #08080a;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 6px;
    padding: 12px 18px;
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 9px;
  }

  .render-progress-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    line-height: 1;
  }

  .render-stage-label {
    font-family: 'Space Grotesk', system-ui, sans-serif;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: #e4e4e7;
    text-transform: uppercase;
  }

  .render-progress-stats {
    display: flex;
    align-items: baseline;
    gap: 12px;
  }

  .render-percent {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 13px;
    font-weight: 800;
    color: #ffffff;
  }

  .render-eta {
    display: inline-block;
    min-width: 5ch;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: #a1a1aa;
    white-space: nowrap;
    cursor: default;
    transition: color 0.15s ease;
  }

  .render-eta:hover {
    color: #ffffff;
  }

  .render-eta:hover .eta-time { display: none; }
  .render-eta:hover .elapsed-time { display: inline; }

  .render-bottom-bar .pro-track {
    height: 4px;
    background: #050507;
    border: 1px solid rgba(255, 255, 255, 0.06);
    border-radius: 2px;
  }

  .render-bottom-bar .render-control-row {
    display: flex;
    align-items: stretch;
    gap: 10px;
    margin: 0;
    padding: 0;
  }

  .btn-render-action {
    display: grid;
    place-items: center;
    width: 62px;
    min-width: 62px;
    height: 100%;
    min-height: 52px;
    padding: 0;
    background: #0c0c0f;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 6px;
    color: #ffffff;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, transform 0.1s ease;
  }

  .btn-render-action svg {
    width: 20px;
    height: 20px;
  }

  .btn-render-action:hover:not(:disabled) {
    background: #18181c;
    border-color: rgba(255, 255, 255, 0.2);
    color: #ffffff;
  }

  .btn-render-action:focus-visible {
    outline: 1px solid #ffffff;
    outline-offset: 2px;
  }

  .btn-render-action:active:not(:disabled) {
    transform: translateY(1px);
  }

  .btn-render-action:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }

  @media (max-width: 680px) {
    .pro-render-card { padding: 16px; }
    .fps-value { font-size: 18px; }
    .render-processing-card { padding: 16px; }
    .render-preview-stage { padding-left: 0; margin: 4px 0; }
    .live-render-preview { width: 100%; max-height: 220px; }
    .btn-render-action { width: 50px; min-width: 50px; }
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

  /* Hero Progress Card */
  .pro-progress-card {
    background: #0c0c0f;
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 8px;
    padding: 14px 16px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .pro-progress-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 12px;
  }

  .progress-stage-name {
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: #a1a1aa;
    text-transform: uppercase;
  }

  .pro-percent-hero {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 14px;
    font-weight: 800;
    color: #ffffff;
  }

  .progress-summary {
    display: flex;
    flex: none;
    align-items: baseline;
    gap: 12px;
  }

  .progress-time {
    display: inline-block;
    min-width: 5ch;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    color: #a1a1aa;
    cursor: default;
    white-space: nowrap;
    transition: color 0.15s ease;
  }

  .elapsed-time { display: none; }
  .progress-time:hover .eta-time { display: none; }
  .progress-time:hover .elapsed-time { display: inline; }
  .progress-time:hover { color: #ffffff; }

  .pro-track {
    position: relative;
    width: 100%;
    height: 6px;
    background: #050507;
    border: 1px solid #27272a;
    border-radius: 4px;
    overflow: hidden;
  }

  .pro-indeterminate-sweep {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 40%;
    background: linear-gradient(90deg, transparent, #ffffff 50%, transparent);
    border-radius: 4px;
    animation: progress-sweep 1.5s cubic-bezier(0.4, 0, 0.2, 1) infinite;
  }

  .pro-track.paused .pro-indeterminate-sweep {
    animation-play-state: paused;
  }

  @keyframes progress-sweep {
    0% { transform: translateX(-100%); }
    100% { transform: translateX(350%); }
  }

  .render-control-row {
    display: flex;
    justify-content: flex-end;
    align-items: center;
    gap: 10px;
    margin-top: auto;
    padding-top: 6px;
  }

  /* Professional Complete Card */
  .pro-complete-card {
    background: #08080a;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 12px;
    padding: 28px;
    display: flex;
    flex-direction: column;
    align-items: stretch;
    justify-content: center;
    gap: 16px;
    height: 100%;
    min-height: 380px;
  }

  .completion-preview {
    position: relative;
    display: grid;
    place-items: center;
    width: min(100%, 460px);
    aspect-ratio: 16 / 9;
    margin: 0 auto;
    padding: 0;
    overflow: hidden;
    background: #050507;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 8px;
    color: #ffffff;
    cursor: pointer;
    transition: border-color 0.16s ease, transform 0.16s ease;
  }

  .completion-preview:hover {
    border-color: rgba(255, 255, 255, 0.42);
    transform: translateY(-1px);
  }

  .completion-preview img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .completion-preview-loading {
    color: #71717a;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.08em;
  }

  .completion-output-name {
    display: block;
    width: min(100%, 460px);
    margin: -6px auto 0;
    overflow: hidden;
    color: #a1a1aa;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    font-weight: 700;
    text-align: center;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .completion-error {
    width: 100%;
    max-width: 460px;
    margin: 0 auto;
    color: #d4d4d8;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    line-height: 1.45;
    text-align: left;
  }

  .complete-actions-row {
    display: flex;
    justify-content: center;
    flex-wrap: wrap;
    gap: 8px;
    width: 100%;
    max-width: 460px;
    margin: 2px auto 0;
  }

  .completion-action {
    min-height: 38px;
    padding: 9px 14px;
  }

  .btn-pro-primary {
    background: #ffffff;
    color: #000000;
    border: 0;
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
    border: 0;
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

  .btn-pro-icon {
    width: 74px;
    height: 67px;
    display: grid;
    place-items: center;
    padding: 0;
    background: #18181b;
    color: #ffffff;
    border: 0;
    border-radius: 4px;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, transform 0.15s ease;
  }

  .btn-pro-icon svg {
    width: 31px;
    height: 31px;
  }

  .btn-pro-icon:hover:not(:disabled) {
    background: #242429;
    color: #a1a1aa;
  }

  .btn-pro-icon:focus-visible {
    outline: 1px solid #ffffff;
    outline-offset: 2px;
  }

  .btn-pro-icon:active:not(:disabled) {
    transform: translateY(1px);
  }

  .btn-pro-icon:disabled {
    cursor: not-allowed;
    opacity: 0.45;
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

  .settings-card {
    width: 600px;
    background: #08080a;
    border-color: rgba(255, 255, 255, 0.12);
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.72);
  }

  .settings-card .modal-header {
    padding: 20px 20px 14px;
    background: transparent;
    border-bottom: 0;
  }

  .settings-title-group {
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .settings-kicker {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.1em;
    color: #71717a;
  }

  .settings-card .modal-header h2 {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 16px;
    font-weight: 700;
    letter-spacing: -0.03em;
  }

  .settings-card .btn-close-modal {
    width: 30px;
    height: 30px;
    display: grid;
    place-items: center;
    padding: 0;
    border: 1px solid transparent;
    border-radius: 4px;
  }

  .settings-card .btn-close-modal svg {
    width: 15px;
    height: 15px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
  }

  .settings-card .btn-close-modal:hover {
    background: #141417;
    border-color: #27272a;
  }

  .settings-body {
    display: grid;
    gap: 10px;
    padding: 0 20px 20px;
  }

  .settings-section {
    margin: 0;
    padding: 14px 16px;
    background: #0c0c0f;
    border: 1px solid rgba(255, 255, 255, 0.07);
    border-radius: 8px;
  }

  .settings-section h3 {
    margin: 0 0 8px;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.09em;
    color: #71717a;
  }

  .settings-section .setting-row,
  .settings-section .slider-row {
    margin: 0;
    padding: 10px 0;
  }

  .settings-section :is(.setting-row, .slider-row) + :is(.setting-row, .slider-row) {
    border-top: 1px solid rgba(255, 255, 255, 0.06);
  }

  .settings-section .setting-row {
    min-height: 42px;
  }

  .settings-section .setting-row label,
  .settings-section .slider-label {
    font-family: 'IBM Plex Mono', monospace;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.06em;
    color: #a1a1aa;
  }

  .settings-section .setting-row select,
  .settings-section .setting-row input[type="number"] {
    min-width: 174px;
    background: #08080a;
    border-color: #27272a;
    border-radius: 4px;
    color: #ffffff;
  }

  .settings-card .modal-footer {
    padding: 0 20px 20px;
    background: transparent;
    border-top: 0;
  }

  .settings-card .modal-footer .btn-secondary,
  .settings-card .modal-footer .btn-primary-modal {
    min-height: 38px;
    border-radius: 4px;
  }

  .settings-card .btn-primary-modal {
    background: #141417;
    border: 0;
    color: #ffffff;
  }

  .settings-card .btn-primary-modal:hover {
    background: #1c1c20;
    border-color: rgba(255, 255, 255, 0.4);
    color: #ffffff;
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
    border: 0;
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
    border: 0;
    border-radius: 6px;
    background: #000000;
    color: #ffffff;
    cursor: pointer;
    font-size: 11px;
    font-weight: 700;
    letter-spacing: 0.04em;
    transition: all 0.15s ease;
  }
  .btn-danger-modal:hover {
    background: #18181b;
    border-color: #ffffff;
    color: #ffffff;
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

  /* Toast Overlay */
  .toast {
    position: fixed;
    bottom: 14px;
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
    background: #000000;
    color: #ffffff;
    border: 1px solid #ffffff;
    font-family: 'IBM Plex Mono', monospace;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.05em;
    display: flex;
    align-items: center;
    gap: 5px;
    transition: background-color 150ms ease, border-color 150ms ease;
  }
  .titlebar-btn.update-badge:hover {
    background: #18181b;
    border-color: #ffffff;
    color: #ffffff;
  }
  .update-badge-dot {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    background: #ffffff;
    box-shadow: 0 0 4px rgba(255, 255, 255, 0.6);
  }

  .update-ready-text {
    display: flex;
    align-items: center;
    gap: 6px;
    color: #ffffff;
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
    background: #000000;
    border: 1px solid #ffffff;
    border-radius: 6px;
    color: #ffffff;
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
