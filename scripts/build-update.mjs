import { execSync } from 'child_process';
import fs from 'fs';
import path from 'path';
import os from 'os';

const tauriConf = JSON.parse(fs.readFileSync(path.join('src-tauri', 'tauri.conf.json'), 'utf8'));
const version = tauriConf.version;
const keyPath = path.join(os.homedir(), '.tauri', 'cia-app.key');
const installerPath = path.join('src-tauri', 'target', 'release', 'bundle', 'nsis', `cia app_${version}_x64-setup.exe`);
const sigPath = `${installerPath}.sig`;

if (!fs.existsSync(installerPath)) {
  console.error(`Installer not found at ${installerPath}`);
  process.exit(1);
}

console.log(`Signing installer for v${version}...`);
execSync(`npx tauri signer sign --private-key-path "${keyPath}" "${installerPath}"`, {
  encoding: 'utf8',
  input: '\n',
  env: {
    ...process.env,
    TAURI_SIGNING_PRIVATE_KEY_PASSWORD: '',
    CI: 'true'
  }
});

if (!fs.existsSync(sigPath)) {
  console.error(`Signature file not found at ${sigPath}`);
  process.exit(1);
}

const signature = fs.readFileSync(sigPath, 'utf8').trim();

const manifest = {
  version: version,
  notes: `cia app v${version}\n- Integrated automatic delta update\n- Step-by-step progress during RIFE installation\n- Fail-fast GPU/CUDA validation\n- Updated application branding & icons`,
  pub_date: new Date().toISOString(),
  platforms: {
    'windows-x86_64': {
      signature: signature,
      url: `https://github.com/cia213/cia-app/releases/download/v${version}/cia.app_${version}_x64-setup.exe`
    }
  }
};

const manifestPath = path.join('dist', 'latest.json');
fs.mkdirSync('dist', { recursive: true });
fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2), 'utf8');
console.log(`\nUpdate manifest successfully written to ${manifestPath}:`);
console.log(JSON.stringify(manifest, null, 2));
