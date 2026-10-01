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
const media = { width: 1680, height: 1050, fps: 120, duration: 2 };
const rife = { mode: 'boost', factor: 2, crf: 18, preset: 'medium', precision: 'fp32', encoder: 'libx264' };
const smoothie = { fps: 30, blendIntensity: 1, brightness: 1.1, saturation: 1.1, contrast: 1, lutEnabled: 'no', lutOpacity: 0.67, borderless: 'no', encoder: 'libx264' };

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
    runtimeSnapshot: { config: { smoothie: { lutFile: '' } } },
    showRifeSettings: true, showSmoothieSettings: true,
    DEFAULT_RIFE: rife, DEFAULT_SMOOTHIE: smoothie,
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === 'generate_video_preview_frame') return `preview:${args.videoPath}`;
      return null;
    },
    ...overrides
  };
  Object.defineProperties(ctx, {
    anyProcessing: { get: () => Boolean(ctx.activeJob || ctx.activeRenderJobId || ctx.isProcessing || ctx.isSmoothieProcessing) },
    hasConfiguredLut: { get: () => Boolean(ctx.runtimeSnapshot?.config?.smoothie?.lutFile) },
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
  const { ctx } = makeContext({
    videoPath: 'A.mp4', videoInfo: media, autoRender: true,
    invoke: (command, args) => {
      invocations.push({ command, args });
      if (command === 'run_time_remap') return rendering.promise;
      if (command === 'run_smoothie') return Promise.resolve('A-final.mp4');
      return Promise.resolve('preview');
    }
  });
  const pending = ctx.startProcessing();
  const job = ctx.activeJob;
  assert.equal(Object.isFrozen(job), true);
  assert.equal(Object.isFrozen(job.smoothieSettings), true);
  ctx.autoRender = false;
  ctx.rifeSettings.factor = 8;
  ctx.rifeSettings.encoder = 'h264_nvenc';
  ctx.smoothieSettings.fps = 60;
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
  assert.equal(final.args.outputFps, 30);
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

test('manual render keeps the verified RIFE duration after interpolation settings change', async () => {
  const running = deferred();
  const { ctx } = makeContext({
    videoInfo: media, rifeOutputPath: 'A-rife.mp4',
    rifeOutputInfo: { ...media, fps: 120, duration: 8 },
    rifeSettings: { ...rife, mode: 'boost', factor: 10 },
    invoke: command => command === 'run_smoothie' ? running.promise : Promise.resolve('preview')
  });
  const pending = ctx.renderRifeWithSmoothie();
  assert.equal(ctx.activeJob.outputDuration, 8);
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
