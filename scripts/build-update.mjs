import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';

const config = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8'));
const version = config.version;
const installer = process.argv[2] || path.join('src-tauri', 'target', 'release', 'bundle', 'nsis', `cia render_${version}_x64-setup.exe`);
const key = path.join(os.homedir(), '.tauri', 'cia-app.key');
if (!fs.existsSync(installer)) throw new Error(`Installer missing: ${installer}`);
if (fs.readFileSync(`${key}.pub`, 'utf8').trim() !== config.plugins.updater.pubkey) throw new Error('Signing key does not match updater public key');
execFileSync(process.execPath, ['node_modules/@tauri-apps/cli/tauri.js', 'signer', 'sign', '--private-key-path', key, installer], {
  stdio: ['pipe', 'inherit', 'inherit'], input: '\n',
  env: { ...process.env, TAURI_SIGNING_PRIVATE_KEY_PASSWORD: process.env.TAURI_SIGNING_PRIVATE_KEY_PASSWORD || '', CI: 'true' }
});
const manifest = {
  version, notes: fs.readFileSync(`docs/RELEASE-v${version}.md`, 'utf8'), pub_date: new Date().toISOString(),
  platforms: { 'windows-x86_64': {
    signature: fs.readFileSync(`${installer}.sig`, 'utf8').trim(),
    url: `https://github.com/cia213/cia-app/releases/download/v${version}/${path.basename(installer).replaceAll(' ', '.')}`
  } }
};
fs.mkdirSync('dist', { recursive: true });
fs.writeFileSync('dist/latest.json', JSON.stringify(manifest, null, 2) + '\n');
console.log(`Signed installer and generated updater manifest for v${version}`);
