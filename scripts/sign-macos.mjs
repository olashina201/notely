import { execFileSync } from 'node:child_process';
import { resolve } from 'node:path';

if (process.platform !== 'darwin') throw new Error('This signing script is for macOS.');
const args = process.argv.slice(2);
const adhoc = args.includes('--adhoc');
const app = resolve(args.find((arg) => !arg.startsWith('--')) ?? 'src-tauri/target/debug/bundle/macos/Notely.app');
let identity = process.env.NOTELY_SIGNING_IDENTITY;
if (!identity && !adhoc) {
  const output = execFileSync('security', ['find-identity', '-v', '-p', 'codesigning'], { encoding: 'utf8' });
  const identities = [...output.matchAll(/\d+\) ([A-Fa-f0-9]{40}) "([^"]+)"/g)];
  const preferred = identities.find(([, , name]) => name.startsWith('Apple Development:'))
    ?? identities.find(([, , name]) => name.startsWith('Developer ID Application:') || name.startsWith('Mac Developer:'));
  identity = preferred?.[1];
}
if (!identity && !adhoc) {
  throw new Error('No usable Apple signing identity found. Set NOTELY_SIGNING_IDENTITY or use --adhoc explicitly. Ad-hoc signatures can lose macOS permissions after every rebuild.');
}
execFileSync('codesign', ['--force', '--sign', adhoc ? '-' : identity, '--timestamp=none',
  '--identifier', 'com.notely.desktop', '--entitlements', 'src-tauri/entitlements.mac.plist', app], { stdio: 'inherit' });
execFileSync('codesign', ['--verify', '--strict', app], { stdio: 'inherit' });
const requirement = execFileSync('codesign', ['-d', '-r-', app], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
if (!adhoc && requirement.includes('designated => cdhash')) throw new Error('Signing did not establish a certificate-based app identity.');
console.log(adhoc ? 'Ad-hoc signed Notely: permission continuity across rebuilds is not guaranteed.' : 'Notely signed with an existing Apple certificate; bundle identifier com.notely.desktop.');
