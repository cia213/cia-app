import assert from 'node:assert/strict';
import fs from 'node:fs/promises';
import http from 'node:http';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { after, before, test } from 'node:test';
import { chromium } from 'playwright';

// Exercise the compiled component and real event/derived-state wiring. Only the
// Tauri command/event boundary is mocked; native installer/GPU qualification is
// a separate release gate. Windows CI supplies Edge, so no browser download is
// needed. Run after npm run build.
const dist = fileURLToPath(new URL('../dist/', import.meta.url));
const mime = { '.html': 'text/html', '.js': 'text/javascript', '.css': 'text/css', '.png': 'image/png', '.woff': 'font/woff', '.woff2': 'font/woff2' };
let server;
let browser;
let origin;

before(async () => {
  await fs.access(path.join(dist, 'index.html'));
  server = http.createServer(async (request, response) => {
    const pathname = decodeURIComponent(new URL(request.url, 'http://localhost').pathname);
    const filename = path.resolve(dist, `.${pathname === '/' ? '/index.html' : pathname}`);
    if (!filename.startsWith(`${path.resolve(dist)}${path.sep}`)) { response.writeHead(403).end(); return; }
    try {
      const content = await fs.readFile(filename);
      response.writeHead(200, { 'content-type': mime[path.extname(filename)] || 'application/octet-stream' });
      response.end(content);
    } catch { response.writeHead(404).end(); }
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  origin = `http://127.0.0.1:${server.address().port}`;
  browser = await chromium.launch({ headless: true, channel: process.env.PLAYWRIGHT_BROWSER_CHANNEL || 'msedge' });
});

after(async () => {
  await browser?.close();
  if (server) await new Promise(resolve => server.close(resolve));
});

async function fixture(t, options = {}) {
  const context = await browser.newContext({ viewport: options.viewport || { width: 920, height: 540 }, permissions: ['clipboard-read', 'clipboard-write'] });
  t.after(() => context.close());
  await context.addInitScript(settings => {
    const callbacks = new Map(), events = new Map(), pending = new Map();
    let id = 0;
    const config = {
      rife: {}, smoothie: { lutFile: null }, mediaTools: {},
      ui: { migrated: true, autoRender: false, rifeSettings: {}, smoothieSettings: settings.preferences || {} }
    };
    let snapshot = {
      config, rifeReady: Boolean(settings.rifeReady), smoothieReady: settings.renderReady !== false,
      mediaToolsReady: settings.renderReady !== false, loadError: settings.loadError || null,
      rifeHealth: { status: settings.healthStatus || (settings.rifeReady ? 'ready' : 'missing'), detail: settings.healthDetail || '' }, components: []
    };
    window.audit = {
      commands: [],
      resolve(command, value) { pending.get(command)?.resolve(value); pending.delete(command); },
      reject(command, message) { pending.get(command)?.reject(message); pending.delete(command); },
      emit(event, payload) { events.get(event)?.({ payload }); },
      setSnapshot(value) { snapshot = value; },
      getSnapshot() { return snapshot; }
    };
    const wait = command => new Promise((resolve, reject) => pending.set(command, { resolve, reject }));
    window.__TAURI_EVENT_PLUGIN_INTERNALS__ = { unregisterListener() {} };
    window.__TAURI_INTERNALS__ = {
      metadata: { currentWindow: { label: 'main' } },
      transformCallback(callback) { callbacks.set(++id, callback); return id; },
      invoke: async (command, args) => {
        window.audit.commands.push({ command, args: command.includes('updater') ? undefined : args });
        if (command === 'plugin:event|listen') { events.set(args.event, callbacks.get(args.handler)); return ++id; }
        if (command === 'get_runtime_snapshot') return snapshot;
        if (command === 'get_app_version') return '1.3.1';
        if (command === 'save_ui_preferences') { config.ui = { migrated: true, ...args }; return snapshot; }
        if (command === 'plugin:updater|check') return settings.updateAvailable ? { rid: 40, currentVersion: '1.3.1', version: '1.3.2', body: 'Test update' } : null;
        if (command === 'install_rife_environment' || command === 'run_time_remap' || command === 'run_smoothie' || command === 'pause_render') return wait(command);
        if (command === 'plugin:updater|download_and_install') return wait(command);
        if (command === 'open_file_dialog') return 'C:/fixture/input.mp4';
        if (command === 'analyze_video') return { width: 640, height: 360, fps: 120, duration: 2 };
        if (command === 'pick_runtime_path') return 'C:/fixture/test.cube';
        if (command === 'save_runtime_config') { snapshot = { ...snapshot, config: args.config }; return snapshot; }
        if (command === 'reset_runtime_config') { snapshot = { ...snapshot, loadError: null }; return snapshot; }
        return null;
      }
    };
  }, options);
  const page = await context.newPage();
  await page.goto(origin, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => window.audit.commands.some(item => item.command === 'get_app_version'));
  return { page, context };
}

async function selectVideo(page, tab) {
  await page.getByRole('button', { name: tab, exact: true }).click();
  await page.locator('.drop-zone').click();
  await page.getByRole('button', { name: tab === 'INTERPOLATION' ? 'START INTERPOLATION' : 'START RENDER', exact: true }).waitFor();
}

test('setup blocks visible update installation and exposes cancellation', { timeout: 20000 }, async t => {
  const { page } = await fixture(t, { updateAvailable: true });
  await page.getByRole('button', { name: 'INTERPOLATION', exact: true }).click();
  await page.getByRole('button', { name: 'INSTALL ENVIRONMENT', exact: true }).click();
  await page.getByRole('button', { name: 'CANCEL SETUP', exact: true }).waitFor();
  await page.evaluate(() => {
    window.audit.emit('install-progress', { step: 2, total: 7, label: 'Preparing Python 3.11' });
    window.audit.emit('install-progress', { bytesDownloaded: 1048576, bytesTotal: 4194304, downloadLabel: 'Private Python' });
  });
  await page.getByText('STEP 2 / 7', { exact: true }).waitFor();
  await page.getByText('1.0 MB / 4.0 MB', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'ABOUT', exact: true }).click();
  await page.getByRole('button', { name: 'Update available', exact: true }).click();
  assert.equal(await page.getByRole('button', { name: 'UPDATE & RELAUNCH', exact: true }).isDisabled(), true);
  assert.equal(await page.evaluate(() => window.audit.commands.some(item => item.command === 'begin_app_update')), false);
  await page.keyboard.press('Escape');
  await page.getByRole('button', { name: 'SHOW SETUP', exact: true }).click();
  await page.getByRole('button', { name: 'CANCEL SETUP', exact: true }).click();
  assert.equal(await page.evaluate(() => window.audit.commands.find(item => item.command === 'cancel_render').args.jobId), 'install-rife');
  await page.evaluate(() => window.audit.reject('install_rife_environment', 'CIA_RENDER_CANCELLED'));
  await page.getByRole('alert').getByText('Environment setup cancelled. You can retry safely.', { exact: true }).waitFor();
  assert.equal(await page.getByRole('dialog').count(), 0);
});

test('healthy interpolation has no environment toolbar or error dialog at both supported sizes', { timeout: 20000 }, async t => {
  for (const viewport of [{ width: 920, height: 540 }, { width: 640, height: 440 }]) {
    const { page } = await fixture(t, { rifeReady: true, viewport });
    await page.getByRole('button', { name: 'INTERPOLATION', exact: true }).click();
    await page.locator('.drop-zone').waitFor();
    assert.equal(await page.getByText('ENVIRONMENT READY', { exact: true }).count(), 0);
    assert.equal(await page.getByRole('button', { name: 'RECHECK', exact: true }).count(), 0);
    assert.equal(await page.getByRole('button', { name: 'REPAIR ENVIRONMENT', exact: true }).count(), 0);
    assert.equal(await page.getByRole('dialog').count(), 0);
    assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
    assert.equal(await page.evaluate(() => window.audit.commands.some(item => item.command === 'open_bug_report')), false);
  }
});

test('setup error popup stays usable at minimum size and shares only a redacted report', { timeout: 20000 }, async t => {
  const { page } = await fixture(t, { viewport: { width: 640, height: 440 } });
  await page.getByRole('button', { name: 'INTERPOLATION', exact: true }).click();
  assert.equal(await page.getByRole('dialog').count(), 0);
  await page.getByRole('button', { name: 'INSTALL ENVIRONMENT', exact: true }).click();
  await page.evaluate(() => window.audit.reject('install_rife_environment', 'Python launch failed at C:/Users/Private Person/runtime/python.exe'));
  const dialog = page.getByRole('dialog', { name: 'SOMETHING WENT WRONG' });
  await dialog.getByRole('alert').getByText('Python launch failed at C:/Users/Private Person/runtime/python.exe', { exact: true }).waitFor();
  await dialog.getByText('VIEW REPORT', { exact: true }).click();
  assert.doesNotMatch(await dialog.locator('pre').innerText(), /Private Person|python\.exe/);
  const report = dialog.getByRole('button', { name: 'REPORT BUG', exact: true });
  assert.equal(await report.isVisible(), true);
  const bounds = await report.boundingBox();
  assert.ok(bounds.y >= 0 && bounds.y + bounds.height <= 440);
  await report.focus();
  await page.keyboard.press('Tab');
  assert.equal(await dialog.getByRole('button', { name: 'Close error report' }).evaluate(node => document.activeElement === node), true);
  await report.click();
  assert.doesNotMatch(await page.evaluate(() => window.audit.commands.find(item => item.command === 'open_bug_report').args.report), /Private Person|python\.exe/);
  await page.keyboard.press('Escape');
  assert.equal(await page.getByRole('dialog').count(), 0);
  assert.equal(await page.getByRole('button', { name: 'INSTALL ENVIRONMENT', exact: true }).evaluate(node => document.activeElement === node), true);
});

test('a job failure replaces an open update dialog with a single error popup', { timeout: 20000 }, async t => {
  const { page } = await fixture(t, { rifeReady: true, updateAvailable: true });
  await selectVideo(page, 'INTERPOLATION');
  await page.getByRole('button', { name: 'START INTERPOLATION', exact: true }).click();
  await page.getByRole('button', { name: 'ABOUT', exact: true }).click();
  await page.getByRole('button', { name: 'Update available', exact: true }).click();
  await page.getByRole('dialog', { name: 'Application update' }).waitFor();
  await page.evaluate(() => window.audit.reject('run_time_remap', 'CUDA execution failed'));
  await page.getByRole('dialog', { name: 'SOMETHING WENT WRONG' }).waitFor();
  assert.equal(await page.getByRole('dialog').count(), 1);
  assert.equal(await page.getByRole('button', { name: 'Close error report' }).evaluate(node => document.activeElement === node), true);
});

test('interpolation failure and useful logs remain after toast, navigation and reload', { timeout: 20000 }, async t => {
  const { page } = await fixture(t, { rifeReady: true });
  await selectVideo(page, 'INTERPOLATION');
  await page.getByRole('button', { name: 'START INTERPOLATION', exact: true }).click();
  await page.evaluate(() => window.audit.reject('run_time_remap', 'CUDA unsupported fixture device'));
  const dialog = page.getByRole('dialog', { name: 'SOMETHING WENT WRONG' });
  await dialog.getByRole('alert').getByText('CUDA unsupported fixture device', { exact: true }).waitFor();
  await dialog.getByRole('button', { name: 'COPY REPORT', exact: true }).click();
  assert.match(await page.evaluate(() => navigator.clipboard.readText()), /cia render 1\.3\.1[\s\S]*CUDA unsupported fixture device/);
  assert.equal(await page.evaluate(() => window.audit.commands.some(item => item.command === 'open_bug_report')), false);
  await dialog.getByRole('button', { name: 'REPORT BUG', exact: true }).click();
  assert.match(await page.evaluate(() => window.audit.commands.find(item => item.command === 'open_bug_report').args.report), /CUDA unsupported fixture device/);
  await page.keyboard.press('Escape');
  assert.equal(await page.getByRole('dialog').count(), 0);
  const failure = page.locator('.failure-card');
  await failure.getByText('CUDA unsupported fixture device', { exact: true }).waitFor();
  await page.waitForTimeout(4200);
  assert.equal(await failure.getByText('CUDA unsupported fixture device', { exact: true }).isVisible(), true);
  await failure.getByRole('button', { name: 'COPY LOGS', exact: true }).click();
  assert.match(await page.evaluate(() => navigator.clipboard.readText()), /CUDA unsupported fixture device/);
  await page.getByRole('button', { name: 'ABOUT', exact: true }).click();
  assert.equal(await page.getByRole('button', { name: 'ADVANCED RUNTIME PATHS', exact: true }).count(), 0);
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.getByRole('button', { name: 'INTERPOLATION', exact: true }).click();
  assert.equal(await page.getByRole('dialog').count(), 0);
  await page.getByRole('alert').getByText('CUDA unsupported fixture device', { exact: true }).waitFor();
  await page.getByRole('alert').getByRole('button', { name: 'COPY LOGS', exact: true }).click();
  assert.match(await page.evaluate(() => navigator.clipboard.readText()), /CUDA unsupported fixture device/);
});

test('pause is disabled until acknowledged and then offers Resume', { timeout: 20000 }, async t => {
  const { page } = await fixture(t, { rifeReady: true });
  await selectVideo(page, 'INTERPOLATION');
  await page.getByRole('button', { name: 'START INTERPOLATION', exact: true }).click();
  await page.getByRole('button', { name: 'Pause interpolation', exact: true }).click();
  assert.equal(await page.getByRole('button', { name: 'Pause interpolation', exact: true }).isDisabled(), true);
  const jobId = await page.evaluate(() => window.audit.commands.find(item => item.command === 'run_time_remap').args.jobId);
  await page.evaluate(jobId => window.audit.resolve('pause_render', { jobId, paused: true }), jobId);
  await page.getByRole('button', { name: 'Resume interpolation', exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.audit.commands.filter(item => item.command === 'pause_render').length), 1);
});

test('invalid saved preferences recover and Details validates before saving with contextual LUT selection', { timeout: 20000 }, async t => {
  const { page } = await fixture(t, { preferences: { speed: 0, fps: '25' }, viewport: { width: 640, height: 440 } });
  await selectVideo(page, 'RENDER');
  await page.getByRole('button', { name: 'DETAILS', exact: true }).click();
  assert.equal(await page.locator('#mod-sm-speed').inputValue(), '1.07');
  assert.equal(await page.locator('#mod-sm-fps').inputValue(), '25');
  await page.locator('#mod-sm-speed').fill('0');
  await page.getByRole('button', { name: 'SAVE CONFIG', exact: true }).click();
  await page.getByRole('dialog').getByRole('alert').getByText('SPEED: enter a number from 0.5 to 2.', { exact: true }).waitFor();
  assert.equal(await page.evaluate(() => window.audit.commands.filter(item => item.command === 'save_ui_preferences').length), 0);
  await page.locator('#mod-sm-speed').fill('1.07');
  await page.getByRole('button', { name: 'SELECT .CUBE LUT', exact: true }).click();
  await page.getByText('test.cube', { exact: true }).waitFor();
  assert.equal(await page.locator('#mod-sm-lutenable').isEnabled(), true);
  const dialog = page.getByRole('dialog');
  const last = dialog.getByRole('button', { name: 'SAVE CONFIG', exact: true });
  await last.focus(); await page.keyboard.press('Tab');
  assert.equal(await page.getByRole('button', { name: 'Close render settings', exact: true }).evaluate(element => document.activeElement === element), true);
  await page.keyboard.press('Escape');
  assert.equal(await dialog.count(), 0);
  assert.equal(await page.getByRole('button', { name: 'DETAILS', exact: true }).evaluate(element => document.activeElement === element), true);
  assert.equal(await page.evaluate(() => document.documentElement.scrollWidth > innerWidth), false);
});

test('broken configuration exposes reset and unsupported CUDA keeps Render usable', { timeout: 20000 }, async t => {
  const { page } = await fixture(t, { loadError: 'Invalid config.json', healthStatus: 'unsupported', healthDetail: 'This fixture GPU is unsupported.' });
  const dialog = page.getByRole('dialog', { name: 'SOMETHING WENT WRONG' });
  await dialog.getByRole('alert').getByText('Invalid config.json', { exact: true }).waitFor();
  await dialog.getByRole('button', { name: 'Close error report', exact: true }).click();
  await page.getByRole('button', { name: 'RESET CONFIGURATION', exact: true }).waitFor();
  assert.equal(await page.locator('.drop-zone').count(), 0);
  await page.getByRole('button', { name: 'RESET CONFIGURATION', exact: true }).click();
  await page.locator('.drop-zone').waitFor();
  await page.getByRole('button', { name: 'INTERPOLATION', exact: true }).click();
  await page.getByText('This fixture GPU is unsupported.', { exact: true }).waitFor();
  assert.equal(await page.getByRole('button', { name: 'INSTALL ENVIRONMENT', exact: true }).count(), 0);
  await page.getByRole('button', { name: 'OPEN RENDER', exact: true }).click();
  await page.locator('.drop-zone').waitFor();
});

test('measured short-tail recovery stays visible on the completed output', { timeout: 20000 }, async t => {
  const { page } = await fixture(t);
  await selectVideo(page, 'RENDER');
  await page.getByRole('button', { name: 'START RENDER', exact: true }).click();
  const jobId = await page.evaluate(() => window.audit.commands.find(item => item.command === 'run_smoothie').args.jobId);
  await page.evaluate(jobId => {
    window.audit.emit('render-log', { jobId, line: '[cia render] RECOVERED_TAIL frames=8 seconds=0.320000' });
    window.audit.resolve('run_smoothie', 'C:/fixture/output.mp4');
  }, jobId);
  await page.getByText('Held the last valid frame for 0.32 s (8 frames).', { exact: true }).waitFor();
  await page.getByRole('button', { name: 'REVEAL IN EXPLORER', exact: true }).waitFor();
});
