import { readFileSync, writeFileSync, mkdtempSync, copyFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { execFileSync } from 'node:child_process';

// One vector silhouette supplies the UI, app bundle, Windows installer, and tray.
const root = new URL('../', import.meta.url);
const ghost = readFileSync(new URL('src/assets/ghost.svg', root), 'utf8');
const body = ghost.match(/<path[\s\S]*\/>/)[0];
const temp = mkdtempSync(join(tmpdir(), 'notely-icons-'));
const cli = new URL('node_modules/@tauri-apps/cli/tauri.js', root);
const generate = (input, output, extra = []) => execFileSync(process.execPath, [cli.pathname, 'icon', input, '--output', output, ...extra], { stdio: 'inherit' });
try {
  const appSvg = `<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024"><defs><linearGradient id="tile" x2="0.8" y2="1"><stop stop-color="#344c43"/><stop offset="1" stop-color="#192923"/></linearGradient></defs><rect x="64" y="64" width="896" height="896" rx="206" fill="url(#tile)"/><rect x="65" y="65" width="894" height="894" rx="205" fill="none" stroke="#a3c9b5" stroke-opacity=".2" stroke-width="2"/><g transform="translate(176 144) scale(21)" fill="#e4eddf">${body}</g></svg>`;
  writeFileSync(join(temp, 'app.svg'), appSvg);
  generate(join(temp, 'app.svg'), join(temp, 'app'));
  for (const file of ['32x32.png', '128x128.png', '128x128@2x.png', 'icon.png', 'icon.ico', 'icon.icns']) {
    copyFileSync(join(temp, 'app', file), new URL(`src-tauri/icons/${file}`, root));
  }
  writeFileSync(join(temp, 'tray.svg'), ghost.replace('fill="currentColor"', 'fill="#000000"'));
  generate(join(temp, 'tray.svg'), join(temp, 'tray'), ['--png', '32']);
  copyFileSync(join(temp, 'tray', '32x32.png'), new URL('src-tauri/icons/tray-template.png', root));
} finally { rmSync(temp, { recursive: true, force: true }); }
