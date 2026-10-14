import assert from 'node:assert/strict';
import { createHash, createPublicKey, verify } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

// Match minisign-verify's modern Ed25519/Blake2b format used by Tauri's updater.
// Only public configuration, the artifact and its detached signature are read.
const [installer, manifestFile] = process.argv.slice(2);
assert(installer && manifestFile, 'Usage: node scripts/verify-update.mjs INSTALLER latest.json');
const config = JSON.parse(fs.readFileSync('src-tauri/tauri.conf.json', 'utf8'));
const manifest = JSON.parse(fs.readFileSync(manifestFile, 'utf8'));
const platform = manifest.platforms['windows-x86_64'];
assert.equal(manifest.version, config.version, 'Updater version differs from the application');
assert.equal(platform.url, `https://github.com/cia213/cia-app/releases/download/v${config.version}/${path.basename(installer)}`);
assert.equal(platform.signature, fs.readFileSync(`${installer}.sig`, 'utf8').trim(), 'Updater signature differs from the artifact');
const publicLines = Buffer.from(config.plugins.updater.pubkey, 'base64').toString('utf8').trim().split(/\r?\n/);
const publicBytes = Buffer.from(publicLines[1], 'base64');
assert.equal(publicBytes.length, 42);
assert.equal(publicBytes.subarray(0, 2).toString(), 'Ed');
const publicKey = createPublicKey({ key: Buffer.concat([Buffer.from('302a300506032b6570032100', 'hex'), publicBytes.subarray(10)]), format: 'der', type: 'spki' });
const signatureLines = Buffer.from(platform.signature, 'base64').toString('utf8').trim().split(/\r?\n/);
assert.equal(signatureLines.length, 4);
const signature = Buffer.from(signatureLines[1], 'base64');
assert.equal(signature.length, 74);
assert.equal(signature.subarray(0, 2).toString(), 'ED', 'Legacy non-prehashed signatures are not accepted');
assert.deepEqual(signature.subarray(2, 10), publicBytes.subarray(2, 10), 'Signing key identity differs');
assert(signatureLines[2].startsWith('trusted comment: '));
const digest = createHash('blake2b512');
const sha256 = createHash('sha256');
for await (const chunk of fs.createReadStream(installer)) { digest.update(chunk); sha256.update(chunk); }
assert(verify(null, digest.digest(), publicKey, signature.subarray(10)), 'Installer signature verification failed');
const comment = Buffer.from(signatureLines[2].slice('trusted comment: '.length), 'utf8');
assert(verify(null, Buffer.concat([signature.subarray(10), comment]), publicKey, Buffer.from(signatureLines[3], 'base64')), 'Trusted comment verification failed');
console.log(JSON.stringify({ version: config.version, installer: path.basename(installer), bytes: fs.statSync(installer).size, sha256: sha256.digest('hex'), updaterSignatureVerified: true }, null, 2));
