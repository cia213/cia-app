import assert from 'node:assert/strict';
import fs from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import { parse } from 'svelte/compiler';

// Execute the actual handlers from the component, with the Tauri boundary mocked.
// This makes the race tests exercise production assignments and request guards.
const source = fs.readFileSync(new URL('./App.svelte', import.meta.url), 'utf8');
const functions = parse(source).instance.content.body
  .filter(node => node.type === 'FunctionDeclaration')
  .map(node => source.slice(node.start, node.end)).join('\n');
const derivedExpressions = Object.fromEntries(parse(source).instance.content.body
  .filter(node => node.type === 'VariableDeclaration')
  .flatMap(node => node.declarations)
  .filter(node => node.init?.callee?.name === '$derived')
  .map(node => [node.id.name, source.slice(node.init.arguments[0].start, node.init.arguments[0].end)]));
const media = { width: 1680, height: 1050, fps: 120, duration: 2 };
const rife = { mode: 'boost', factor: 2, crf: 18, preset: 'medium', precision: 'fp32', encoder: 'libx264' };
const smoothie = { fps: 30, speed: 1, blendIntensity: 1, brightness: 1.1, saturation: 1.1, contrast: 1, lutEnabled: 'no', lutOpacity: 0.67, borderless: 'no', encoder: 'libx264' };

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function makeContext(overrides = {}) {
  let serial = 0;
  const calls = [];
  const notices = [];
  const ctx = {
    console, Object, Number, Math, JSON,
    crypto: { randomUUID: () => `job-${++serial}` },
    setTimeout: () => ++serial, clearTimeout: () => {},
    setInterval: () => ++serial, clearInterval: () => {},
    activePage: 'dashboard', activeJob: null, activeRenderJobId: '',
    isProcessing: false, isSmoothieProcessing: false,
    isLoading: false, isSmoothieLoading: false, isInstallingRifeEnvironment: false,
    isRuntimeLoading: false, isRecoveringRuntime: false, isSavingRuntime: false,
    isCancellingInstall: false, isPausePending: false,
    runtimeError: '', preferencesWarning: '', rifeSettingsError: '', smoothieSettingsError: '',
    rifeInstallError: '', rifeFailureLogs: '', smoothieFailureLogs: '', installFailureLogs: '',
    rifeRecoveryNote: '', smoothieRecoveryNote: '',
    updateState: 'idle', availableUpdate: null, updateErrorMessage: '',
    installStep: 0, installTotal: 0, installLabel: '', setupDraft: null,
    installDownloadedBytes: 0, installContentLength: 0, installDownloadLabel: '',
    isComplete: false, isSmoothieComplete: false,
    isRenderPaused: false, isCancellingRender: false,
    videoPath: '', smoothiePath: '', videoInfo: null, smoothieInfo: null,
    rifeOutputPath: '', rifeOutputInfo: null, lastOutputPath: '', smoothieOutputPath: '',
    rifeSelectionRequest: 0, smoothieSelectionRequest: 0, historyNavigationRequest: 0,
    rifePreviewRequest: 0, smoothiePreviewRequest: 0,
    rifeOutputPreviewRequest: 0, smoothieOutputPreviewRequest: 0,
    rifePreviewFramesLoading: false, smoothiePreviewFramesLoading: false,
    rifePreviewHovered: false, smoothiePreviewHovered: false,
    rifePreviewTimer: null, smoothiePreviewTimer: null,
    rifePreviewFrameIndex: -1, smoothiePreviewFrameIndex: -1,
    rifePreviewSet: null, smoothiePreviewSet: null,
    rifeOutputPreview: '', smoothieOutputPreview: '',
    rifeOutputPreviewStatus: 'idle', smoothieOutputPreviewStatus: 'idle',
    historyStack: [{ page: 'dashboard', smoothiePath: '', videoPath: '' }], historyIndex: 0,
    logs: [], pendingLogs: [], logFlushTimer: null, shouldShowExecutionLogs: false,
    jobPhase: 'idle', jobError: '', smoothieJobError: '', progress: 0, generatedFrame: 0, pipelineTotalFrames: 0,
    elapsedTime: '00:00', remainingTime: '--:--', isEncodingPhase: false,
    encodingProgress: 0, encodingFrame: 0, encodingFps: '', encodingSpeed: '', encodingTime: '',
    autoRender: false, rifeSettings: { ...rife }, smoothieSettings: { ...smoothie },
    runtimeSnapshot: { config: { rife: {}, smoothie: { lutFile: '' }, ui: {} }, rifeReady: true, smoothieReady: true, mediaToolsReady: true },
    showRifeSettings: true, showSmoothieSettings: true,
    failureDialog: null, bugReportError: '', bugReportNotice: '', isOpeningBugReport: false,
    toast: { show: false, message: '', type: 'info' },
    notifiedRuntimeProblem: '', appVersion: '1.3.1',
    DEFAULT_RIFE: rife, DEFAULT_SMOOTHIE: smoothie,
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === 'generate_video_preview_frame') return `preview:${args.videoPath}`;
      return null;
    },
    ...overrides
  };
  Object.defineProperties(ctx, {
    anyProcessing: { get: () => vm.runInContext(derivedExpressions.anyProcessing, ctx) },
    renderRuntimeReady: { get: () => vm.runInContext(derivedExpressions.renderRuntimeReady, ctx) },
    hasConfiguredLut: { get: () => vm.runInContext(derivedExpressions.hasConfiguredLut, ctx) },
    canGoBack: { get: () => ctx.historyIndex > 0 },
    canGoForward: { get: () => ctx.historyIndex < ctx.historyStack.length - 1 },
    outputDuration: { get: () => ctx.activeJob?.kind === 'rife' ? ctx.activeJob.outputDuration : ctx.videoInfo?.duration || 0 }
  });
  vm.createContext(ctx);
  vm.runInContext(functions, ctx);
  ctx.showToast = (message, type) => notices.push({ message, type });
  ctx.playCompletionChime = () => {};
  return { ctx, calls, notices };
}

for (const kind of ['rife', 'smoothie']) {
  test(`history navigation preserves the ${kind} job and blocks another selection`, async () => {
    const job = Object.freeze({ id: 'active', kind, path: 'A.mp4' });
    const { ctx, calls } = makeContext({
      activeJob: job, activeRenderJobId: job.id,
      isProcessing: kind === 'rife', isSmoothieProcessing: kind === 'smoothie',
      videoPath: 'A.mp4', smoothiePath: 'A.mp4', videoInfo: media, smoothieInfo: media
    });
    await ctx.applyHistoryEntry({ page: 'about', smoothiePath: '', videoPath: '' });
    await ctx.loadVideo('B.mp4');
    await ctx.loadSmoothie('B.mp4');
    await ctx.pickFile();
    await ctx.pickSmoothieFile();
    ctx.clearVideoSelection();
    ctx.clearSmoothieSelection();
    ctx.resetInterpolation();
    assert.equal(ctx.activePage, 'about');
    assert.equal(ctx.activeJob, job);
    assert.equal(ctx.activeRenderJobId, 'active');
    assert.equal(ctx.videoPath, 'A.mp4');
    assert.equal(ctx.smoothiePath, 'A.mp4');
    assert.equal(ctx.isProcessing, kind === 'rife');
    assert.equal(ctx.isSmoothieProcessing, kind === 'smoothie');
    assert.equal(ctx.anyProcessing, true);
    assert.equal(calls.length, 0);
  });
}

for (const [kind, loader, pathKey, infoKey, loadingKey, clear] of [
  ['rife', 'loadVideo', 'videoPath', 'videoInfo', 'isLoading', 'clearVideoSelection'],
  ['smoothie', 'loadSmoothie', 'smoothiePath', 'smoothieInfo', 'isSmoothieLoading', 'clearSmoothieSelection']
]) {
  for (const staleFails of [false, true]) {
    test(`${kind} analysis ignores a stale ${staleFails ? 'failure' : 'success'} from an earlier selection`, async () => {
      const first = deferred();
      const latest = deferred();
      const infoB = { ...media, width: 1920 };
      const { ctx, notices } = makeContext({ invoke: (command, args) => {
        if (command === 'analyze_video') return args.videoPath === 'A.mp4' ? first.promise : latest.promise;
        return Promise.resolve('preview');
      } });
      const promiseA = ctx[loader]('A.mp4');
      const promiseB = ctx[loader]('B.mp4');
      latest.resolve(infoB);
      await promiseB;
      if (staleFails) first.reject(new Error('obsolete analysis failed'));
      else first.resolve({ ...media, width: 640 });
      await promiseA;
      assert.equal(ctx[pathKey], 'B.mp4');
      assert.equal(ctx[infoKey], infoB);
      assert.equal(ctx[loadingKey], false);
      assert.equal(ctx.historyStack.at(-1)[pathKey], 'B.mp4');
      assert.equal(notices.length, 1);
      assert.equal(notices[0].type, 'success');
    });
  }

  test(`${kind} clearing a selection invalidates pending analysis`, async () => {
    const pending = deferred();
    const { ctx } = makeContext({ invoke: command => command === 'analyze_video' ? pending.promise : Promise.resolve() });
    const loading = ctx[loader]('A.mp4');
    ctx[clear]();
    pending.resolve(media);
    await loading;
    assert.equal(ctx[pathKey], '');
    assert.equal(ctx[infoKey], null);
    assert.equal(ctx[loadingKey], false);
  });
}

test('RIFE auto-chain uses its launch snapshot after settings and page change', async () => {
  const rendering = deferred();
  const invocations = [];
  let chainedRates;
  let chainedDuration;
  const { ctx } = makeContext({
    videoPath: 'A.mp4', videoInfo: media, autoRender: true,
    smoothieSettings: { ...smoothie, speed: 1.07 },
    invoke: (command, args) => {
      invocations.push({ command, args });
      if (command === 'run_time_remap') return rendering.promise;
      if (command === 'run_smoothie') {
        chainedRates = ctx.interpolationFrameRates();
        chainedDuration = ctx.activeJob.outputDuration;
        return Promise.resolve('A-final.mp4');
      }
      return Promise.resolve('preview');
    }
  });
  const pending = ctx.startProcessing();
  const job = ctx.activeJob;
  assert.deepEqual({ ...ctx.interpolationFrameRates() }, { input: 120, output: 240 });
  assert.equal(Object.isFrozen(job), true);
  assert.equal(Object.isFrozen(job.smoothieSettings), true);
  ctx.autoRender = false;
  ctx.rifeSettings.factor = 8;
  ctx.rifeSettings.encoder = 'h264_nvenc';
  ctx.smoothieSettings.fps = 60;
  ctx.smoothieSettings.speed = 2;
  ctx.smoothieSettings.brightness = 0;
  await ctx.applyHistoryEntry({ page: 'smoothie', videoPath: '', smoothiePath: '' });
  await ctx.startSmoothie();
  rendering.resolve('A-rife.mp4');
  await pending;
  const remap = invocations.find(call => call.command === 'run_time_remap');
  const final = invocations.find(call => call.command === 'run_smoothie');
  assert.equal(remap.args.factor, 2);
  assert.equal(remap.args.encoder, 'libx264');
  assert.equal(remap.args.precision, 'fp32');
  assert.equal(Object.hasOwn(remap.args, 'sceneThreshold'), false);
  assert.equal(Object.hasOwn(remap.args, 'blendCuts'), false);
  assert.deepEqual({ ...chainedRates }, { input: 240, output: 30 });
  assert.equal(final.args.outputFps, 30);
  assert.equal(final.args.speed, 1.07);
  assert.equal(chainedDuration, media.duration / 1.07);
  assert.equal(final.args.encoder, 'libx264');
  assert.equal(final.args.jobId, job.id);
  assert.ok(final.args.overrides.includes('color grading;brightness;1.1'));
  assert.ok(final.args.overrides.includes('color grading;enabled;yes'));
  assert.ok(final.args.overrides.includes('lut;enabled;no'));
  assert.equal(ctx.lastOutputPath, 'A-final.mp4');
  assert.equal(ctx.activeJob, null);
  assert.equal(ctx.anyProcessing, false);
});

test('automatic Smoothie failure preserves the verified RIFE output and does not announce success', async () => {
  const { ctx, notices } = makeContext({
    videoPath: 'A.mp4', videoInfo: media, autoRender: true,
    invoke: async command => {
      if (command === 'run_time_remap') return 'A-rife.mp4';
      if (command === 'run_smoothie') throw new Error('encoder unavailable');
      return 'preview';
    }
  });
  await ctx.startProcessing();
  assert.equal(ctx.jobPhase, 'failed');
  assert.equal(ctx.lastOutputPath, 'A-rife.mp4');
  assert.equal(ctx.isComplete, true);
  assert.match(ctx.jobError, /encoder unavailable/);
  assert.equal(notices.some(notice => notice.type === 'success'), false);
  assert.equal(ctx.anyProcessing, false);
});

test('LUT is enabled only with a configured path and neutral color settings disable grading', () => {
  const { ctx } = makeContext();
  ctx.smoothieSettings.lutEnabled = 'yes';
  ctx.smoothieSettings.brightness = 1;
  ctx.smoothieSettings.saturation = 1;
  const job = ctx.createJobSnapshot('smoothie', 'A.mp4', media);
  assert.ok(ctx.smoothieOverrides(job.smoothieSettings).includes('lut;enabled;no'));
  assert.ok(ctx.smoothieOverrides(job.smoothieSettings).includes('color grading;enabled;no'));
  ctx.runtimeSnapshot.config.smoothie.lutFile = 'grade.cube';
  const configured = ctx.createJobSnapshot('smoothie', 'A.mp4', media);
  assert.ok(ctx.smoothieOverrides(configured.smoothieSettings).includes('lut;enabled;yes'));
});

test('manual render displays verified interpolated FPS and keeps its duration after settings change', async () => {
  const running = deferred();
  const { ctx } = makeContext({
    videoInfo: media, rifeOutputPath: 'A-rife.mp4',
    rifeOutputInfo: { ...media, fps: 360, duration: 8 },
    smoothieSettings: { ...smoothie, fps: 25 },
    rifeSettings: { ...rife, mode: 'boost', factor: 10 },
    invoke: command => command === 'run_smoothie' ? running.promise : Promise.resolve('preview')
  });
  const pending = ctx.renderRifeWithSmoothie();
  assert.equal(ctx.activeJob.outputDuration, 8);
  assert.deepEqual({ ...ctx.interpolationFrameRates() }, { input: 360, output: 25 });
  ctx.smoothieSettings.fps = 60;
  assert.deepEqual({ ...ctx.interpolationFrameRates() }, { input: 360, output: 25 });
  running.resolve('A-final.mp4');
  await pending;
  assert.equal(ctx.lastOutputPath, 'A-final.mp4');
});

test('standalone Smoothie failure remains visible without overwriting the RIFE error', async () => {
  const { ctx, notices } = makeContext({
    smoothiePath: 'A.mp4', smoothieInfo: media, jobError: 'Earlier interpolation error',
    invoke: async command => {
      if (command === 'run_smoothie') throw new Error('encoder unavailable');
      return 'preview';
    }
  });
  await ctx.startSmoothie();
  assert.match(ctx.smoothieJobError, /encoder unavailable/);
  assert.equal(ctx.jobError, 'Earlier interpolation error');
  assert.equal(ctx.isSmoothieComplete, false);
  assert.equal(ctx.anyProcessing, false);
  assert.equal(notices.some(notice => notice.type === 'success'), false);
});

test('output preview reads the final file and ignores an obsolete thumbnail', async () => {
  const old = deferred();
  const current = deferred();
  const calls = [];
  const { ctx } = makeContext({ lastOutputPath: 'old.mp4', invoke: (command, args) => {
    calls.push({ command, args });
    return args.videoPath === 'old.mp4' ? old.promise : current.promise;
  } });
  const first = ctx.loadOutputPreview('rife', 'old.mp4', 2);
  ctx.lastOutputPath = 'final.mp4';
  const latest = ctx.loadOutputPreview('rife', 'final.mp4', 2);
  current.resolve('final-pixels');
  await latest;
  old.resolve('old-pixels');
  await first;
  assert.equal(ctx.rifeOutputPreview, 'final-pixels');
  assert.equal(ctx.rifeOutputPreviewStatus, 'ready');
  assert.equal(calls.at(-1).command, 'generate_video_preview_frame');
  assert.equal(calls.at(-1).args.videoPath, 'final.mp4');
});

test('optional preview scans are lazy and cannot start after their selection becomes stale', async () => {
  const { ctx, calls } = makeContext({ videoPath: 'A.mp4', videoInfo: media });
  ctx.loadSourcePreview('rife', 'A.mp4', 2);
  await Promise.resolve();
  assert.equal(calls.filter(call => call.command === 'generate_video_preview_set').length, 0);
  const oldRequest = ctx.rifePreviewRequest;
  ctx.invalidateSourcePreview('rife');
  await ctx.loadSourcePreviewFrames('rife', 'A.mp4', 2, oldRequest);
  assert.equal(calls.filter(call => call.command === 'generate_video_preview_set').length, 0);
  assert.ok(calls.some(call => call.command === 'cancel_video_previews'));
});

test('preference failure propagates, keeps the dialog open and cannot emit a success toast', async () => {
  const { ctx, notices } = makeContext({ invoke: async () => { throw new Error('config write failed'); } });
  await assert.rejects(ctx.persistUiPreferences(), /config write failed/);
  notices.length = 0;
  await ctx.saveRifeSettings();
  assert.equal(ctx.showRifeSettings, true);
  assert.equal(notices.length, 1);
  assert.equal(notices[0].type, 'error');
  notices.length = 0;
  await ctx.saveSmoothieSettings();
  assert.equal(ctx.showSmoothieSettings, true);
  assert.equal(notices.length, 1);
  assert.equal(notices[0].type, 'error');
});

test('concurrent RIFE and encoder progress does not reset or reach 100 before verification', () => {
  const { ctx } = makeContext({ activeJob: { outputDuration: 2 }, jobPhase: 'rife', activePage: 'about' });
  ctx.parseLogLine('[cia render] ENCODING frame=0 total_frames=95 fps=0 time=00:00:00 pct=0%');
  ctx.parseLogLine('[cia render] RIFE frame=47 input_frames=24 total_frames=95 elapsed=4.0');
  assert.equal(ctx.progress, 49);
  assert.equal(ctx.elapsedTime, '00:04');
  ctx.parseLogLine('[cia render] Finalizing output');
  assert.equal(ctx.progress, 49);
  ctx.parseLogLine('[cia render] ENCODING frame=10 total_frames=95 fps=20 time=00:00:01 pct=11%');
  assert.equal(ctx.progress, 49);
  assert.equal(ctx.elapsedTime, '00:04');
  ctx.parseLogLine('[cia render] RIFE frame=95 input_frames=48 total_frames=95 elapsed=8.0');
  ctx.parseLogLine('[cia render] ENCODING frame=95 total_frames=95 fps=20 time=00:00:02 pct=100%');
  assert.equal(ctx.progress, 99);
});

test('render telemetry accepts only the active job', () => {
  const { ctx } = makeContext({ activeRenderJobId: 'current', jobPhase: 'rife' });
  ctx.handleRenderLog({ payload: { jobId: 'old', line: '[cia render] RIFE frame=95 total_frames=95 elapsed=8.0' } });
  assert.equal(ctx.progress, 0);
  assert.equal(ctx.pendingLogs.length, 0);
  ctx.handleRenderLog({ payload: { jobId: 'current', line: '[cia render] RIFE frame=47 total_frames=95 elapsed=4.0' } });
  assert.equal(ctx.progress, 49);
  assert.equal(ctx.pendingLogs.length, 1);
  ctx.activeRenderJobId = '';
  ctx.handleRenderLog({ payload: { jobId: 'current', line: '[cia render] RIFE frame=95 total_frames=95 elapsed=8.0' } });
  assert.equal(ctx.progress, 49);
});

test('slider animation stops requesting frames once its targets are settled', () => {
  const sliderSource = fs.readFileSync(new URL('./GlowSlider.svelte', import.meta.url), 'utf8');
  const node = parse(sliderSource).instance.content.body.find(node => node.type === 'FunctionDeclaration' && node.id.name === 'updateAnim');
  const pending = [];
  const ctx = {
    Math, lastTime: 0, animFrame: null,
    targetRatio: 0.5, targetHover: 0, vizRatio: 0, hoverAnim: 0,
    requestAnimationFrame: callback => { pending.push(callback); return pending.length; }
  };
  vm.createContext(ctx);
  vm.runInContext(sliderSource.slice(node.start, node.end), ctx);
  ctx.updateAnim(16);
  for (let frame = 1; pending.length && frame < 300; frame += 1) pending.shift()(16 * (frame + 1));
  assert.equal(pending.length, 0);
  assert.equal(ctx.animFrame, null);
  assert.equal(ctx.vizRatio, 0.5);
  ctx.updateAnim(5000);
  assert.equal(pending.length, 0);
});

test('standalone render freezes playback speed and uses retimed duration for progress and preview', async () => {
  const running = deferred();
  const calls = [];
  const { ctx } = makeContext({
    smoothiePath: 'A.mp4', smoothieInfo: media,
    smoothieSettings: { ...smoothie, speed: 0.8 },
    invoke: (command, args) => {
      calls.push({ command, args });
      return command === 'run_smoothie' ? running.promise : Promise.resolve('preview');
    }
  });
  const pending = ctx.startSmoothie();
  assert.equal(ctx.activeJob.outputDuration, media.duration / 0.8);
  ctx.smoothieSettings.speed = 1.5;
  assert.equal(calls[0].args.speed, 0.8);
  running.resolve('A-final.mp4');
  await pending;
  const preview = calls.find(call => call.command === 'generate_video_preview_frame');
  assert.equal(preview.args.timestamp, media.duration / 0.8 * 0.12);
});

test('RIFE installation keeps the complete diagnostic and clears it when retry succeeds', async () => {
  const detail = 'RIFE environment installation failed: Python package is incomplete at C:/a long path/python/python.exe';
  let attempts = 0;
  const { ctx, notices } = makeContext({
    rifeInstallError: '',
    invoke: () => ++attempts === 1
      ? Promise.reject(detail)
      : Promise.resolve({ config: { rife: {} }, rifeReady: true })
  });
  await ctx.installRifeEnvironment();
  assert.equal(ctx.rifeInstallError, detail);
  assert.equal(notices.at(-1).message, detail);
  assert.equal(ctx.isInstallingRifeEnvironment, false);
  await ctx.installRifeEnvironment();
  assert.equal(ctx.rifeInstallError, '');
  assert.equal(ctx.runtimeSnapshot.rifeReady, true);
  assert.equal(notices.at(-1).type, 'success');
});

test('environment setup prevents update installation and rendering until it ends', async () => {
  const installing = deferred();
  let updateCalled = false;
  const { ctx, calls } = makeContext({
    smoothiePath: 'A.mp4', smoothieInfo: media,
    availableUpdate: { downloadAndInstall: async () => { updateCalled = true; } },
    invoke: async command => command === 'install_rife_environment' ? installing.promise : null
  });
  const pending = ctx.installRifeEnvironment();
  await ctx.installAppUpdate();
  await ctx.startSmoothie();
  assert.equal(updateCalled, false);
  assert.equal(ctx.activeJob, null);
  assert.equal(ctx.anyProcessing, true);
  installing.resolve({ config: { rife: {}, smoothie: {} }, rifeReady: true });
  await pending;
  assert.equal(ctx.anyProcessing, false);
  assert.equal(calls.length, 0);
});

test('native update admission blocks a new render and releases the gate on failure', async () => {
  const downloading = deferred();
  const commands = [];
  const { ctx } = makeContext({
    smoothiePath: 'A.mp4', smoothieInfo: media,
    availableUpdate: { downloadAndInstall: () => downloading.promise },
    invoke: async command => { commands.push(command); }
  });
  const pending = ctx.installAppUpdate();
  await Promise.resolve();
  await ctx.startSmoothie();
  await ctx.installRifeEnvironment();
  assert.equal(ctx.activeJob, null);
  assert.deepEqual(commands, ['begin_app_update']);
  downloading.reject(new Error('download interrupted'));
  await pending;
  assert.deepEqual(commands, ['begin_app_update', 'end_app_update']);
  assert.equal(ctx.updateState, 'error');
  assert.equal(ctx.anyProcessing, false);
});

test('duplicate pause clicks send one command and accept backend state', async () => {
  const acknowledgement = deferred();
  const commands = [];
  const { ctx } = makeContext({
    activeRenderJobId: 'job-1', jobPhase: 'rife',
    invoke: (command, args) => { commands.push({ command, args }); return acknowledgement.promise; }
  });
  const first = ctx.toggleRenderPause();
  await ctx.toggleRenderPause();
  assert.equal(commands.length, 1);
  assert.equal(ctx.isPausePending, true);
  acknowledgement.resolve({ jobId: 'job-1', paused: true });
  await first;
  assert.equal(ctx.isRenderPaused, true);
  assert.equal(ctx.isPausePending, false);
});

test('a delayed pause acknowledgement cannot pause the next pipeline stage', async () => {
  const acknowledgement = deferred();
  const { ctx } = makeContext({ activeRenderJobId: 'job-1', jobPhase: 'rife', invoke: () => acknowledgement.promise });
  const pending = ctx.toggleRenderPause();
  ctx.jobPhase = 'smoothie';
  acknowledgement.resolve({ jobId: 'job-1', paused: true });
  await pending;
  assert.equal(ctx.isRenderPaused, false);
});

test('invalid speed is rejected before persistence and remains an inline field error', async () => {
  const { ctx, calls, notices } = makeContext({ smoothieSettings: { ...smoothie, speed: 0 } });
  await ctx.saveSmoothieSettings();
  assert.equal(calls.length, 0);
  assert.equal(ctx.showSmoothieSettings, true);
  assert.match(ctx.smoothieSettingsError, /SPEED.*0.5.*2/);
  assert.equal(notices.length, 0);
});

test('stored preferences migrate numeric strings and replace invalid fields safely', () => {
  const { ctx } = makeContext();
  const restored = ctx.sanitizeSettings('smoothie', { speed: 0, fps: '25', lutOpacity: null, encoder: 'unknown' });
  assert.equal(restored.speed, smoothie.speed);
  assert.equal(restored.fps, 25);
  assert.equal(restored.lutOpacity, smoothie.lutOpacity);
  assert.equal(restored.encoder, 'libx264');
  assert.match(ctx.preferencesWarning, /invalid/);
});

test('runtime load failure keeps configuration recovery available and version resolves independently', async () => {
  const loading = deferred();
  const { ctx } = makeContext({
    invoke: command => command === 'get_runtime_snapshot' ? loading.promise : Promise.resolve('1.3.0')
  });
  const initialization = ctx.initializeRuntime();
  await ctx.initializeAppVersion();
  assert.equal(ctx.appVersion, '1.3.0');
  assert.equal(ctx.isRuntimeLoading, true);
  loading.resolve({ config: { ui: { migrated: true } }, loadError: 'Invalid config.json' });
  await initialization;
  assert.equal(ctx.runtimeError, 'Invalid config.json');
  assert.equal(ctx.renderRuntimeReady, false);
  assert.equal(ctx.isRuntimeLoading, false);
});

test('setup cancellation targets the installer operation and waits for completion', async () => {
  const installing = deferred();
  const commands = [];
  const { ctx } = makeContext({ invoke: (command, args) => {
    commands.push({ command, args });
    return command === 'install_rife_environment' ? installing.promise : Promise.resolve();
  } });
  const pending = ctx.installRifeEnvironment(true);
  await ctx.cancelRifeInstallation();
  assert.equal(commands[0].args.repair, true);
  assert.equal(commands[1].command, 'cancel_render');
  assert.equal(commands[1].args.jobId, 'install-rife');
  assert.equal(ctx.isInstallingRifeEnvironment, true);
  assert.equal(ctx.isCancellingInstall, true);
  installing.reject('CIA_RENDER_CANCELLED');
  await pending;
  assert.equal(ctx.isInstallingRifeEnvironment, false);
  assert.match(ctx.rifeInstallError, /cancelled/);
  assert.equal(ctx.failureDialog, null);
});

test('new failures offer a frozen private report and contact GitHub only on request', async () => {
  const { ctx, calls, notices } = makeContext();
  ctx.appendLog('Input: C:\\Users\\Private User\\Videos\\secret clip.mp4');
  ctx.appendLog('Cache: \\\\private-server\\personal\\model.pt');
  ctx.appendLog('Source: /home/private-user/Videos/personal.mp4');
  ctx.appendLog('Authorization: Bearer private-access-value');
  ctx.appendLog('https://name:fake@example.test token=private-token github_pat_PRIVATEVALUE');
  ctx.appendLog('Authorization: Basic ZmFrZS11c2VyOmZha2Utc2VjcmV0');
  ctx.appendLog('password="fake private phrase"');
  ctx.appendLog('token = fake-private-token');
  ctx.appendLog("Path: C:\\Users\\O'Neil\\Videos\\private clip.mp4");
  ctx.recordFailure('rife', 'CUDA failed at C:/Users/Private User/runtime/python.exe');
  assert.equal(ctx.failureDialog.kind, 'rife');
  assert.match(ctx.failureDialog.error, /Private User/);
  assert.match(ctx.failureDialog.report, /cia render 1\.3\.1/);
  assert.match(ctx.failureDialog.report, /CUDA failed/);
  assert.match(ctx.failureDialog.report, /\[local path\]/);
  assert.doesNotMatch(ctx.failureDialog.report, /Private User|private-user|private-server|personal|private-access-value|private-token|PRIVATEVALUE|name:fake/);
  assert.doesNotMatch(ctx.failureDialog.report, /ZmFrZS11c2VyOmZha2Utc2VjcmV0|fake private phrase|fake-private-token|Neil|private clip/);
  assert.equal(calls.length, 0);
  const report = ctx.failureDialog.report;
  ctx.appendLog('A later unrelated job');
  await ctx.reportBug();
  assert.equal(calls.length, 1);
  assert.equal(calls[0].command, 'open_bug_report');
  assert.equal(calls[0].args.report, report);
  assert.doesNotMatch(calls[0].args.report, /later unrelated/);
  assert.match(ctx.bugReportNotice, /Review and submit/);
  assert.equal(notices.length, 0);
  ctx.closeFailureDialog();
  assert.equal(ctx.failureDialog, null);
});

test('report opener and clipboard errors remain actionable without claiming submission', async () => {
  const { ctx, notices } = makeContext({
    navigator: { clipboard: { writeText: async () => { throw new Error('Denied'); } } },
    invoke: async () => { throw new Error('Browser unavailable'); }
  });
  ctx.recordFailure('smoothie', 'Encoder failed');
  await ctx.copyBugReport();
  assert.match(ctx.bugReportError, /could not be copied/);
  await ctx.reportBug();
  assert.match(ctx.bugReportError, /could not be opened/);
  assert.equal(ctx.isOpeningBugReport, false);
  assert.equal(notices.length, 0);
});

test('failure diagnostics survive restart and redact credentials', async () => {
  const values = new Map();
  const localStorage = { getItem: key => values.get(key) ?? null, setItem: (key, value) => values.set(key, value) };
  const { ctx } = makeContext({ localStorage });
  ctx.appendLog('https://user:secret@example.test/private token=credential');
  ctx.recordFailure('rife', 'CUDA failed');
  const restored = makeContext({ localStorage }).ctx.loadFailure('rife');
  assert.equal(restored.error, 'CUDA failed');
  assert.match(restored.logs, /redacted/);
  assert.doesNotMatch(restored.logs, /secret|credential/);
});

test('measured tail recovery is attached to the correct completion workflow', () => {
  const { ctx } = makeContext({ activeJob: { kind: 'rife' } });
  ctx.parseLogLine('[cia render] RECOVERED_TAIL frames=8 seconds=0.320000');
  assert.equal(ctx.rifeRecoveryNote, 'Held the last valid frame for 0.32 s (8 frames).');
  assert.equal(ctx.smoothieRecoveryNote, '');
  ctx.activeJob = { kind: 'smoothie' };
  ctx.parseLogLine('[cia render] RECOVERED_TAIL frames=3 seconds=0.120000');
  assert.equal(ctx.smoothieRecoveryNote, 'Held the last valid frame for 0.12 s (3 frames).');
});

test('download events preserve setup phase and reset bytes only at a new stage', () => {
  const { ctx } = makeContext({ isInstallingRifeEnvironment: true });
  ctx.handleInstallProgress({ payload: { step: 2, total: 7, label: 'Preparing Python 3.11' } });
  ctx.handleInstallProgress({ payload: { bytesDownloaded: 1024, bytesTotal: 4096, downloadLabel: 'Private Python' } });
  assert.equal(ctx.installStep, 2);
  assert.equal(ctx.installTotal, 7);
  assert.equal(ctx.installLabel, 'Preparing Python 3.11');
  assert.equal(ctx.installDownloadedBytes, 1024);
  assert.equal(ctx.installContentLength, 4096);
  ctx.handleInstallProgress({ payload: { step: 3, total: 7, label: 'Creating virtual environment' } });
  assert.equal(ctx.installDownloadedBytes, 0);
  assert.equal(ctx.installContentLength, 0);
  assert.equal(ctx.installDownloadLabel, '');
  ctx.isInstallingRifeEnvironment = false;
  ctx.handleInstallProgress({ payload: { bytesDownloaded: 8192, bytesTotal: 16384 } });
  assert.equal(ctx.installDownloadedBytes, 0);
});
