import { readFileSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const problems = [];

// ---- Tauri window / webview ----
const confPath = resolve(ROOT, 'src-tauri/tauri.conf.json');
const conf = JSON.parse(readFileSync(confPath, 'utf8'));
const FORBIDDEN_FLAGS = [
  /msSmartScreenProtection/i,
  /disable-client-side-phishing-detection/i,
  /disable-component-update/i,
  /disable-web-security/i,
  /allow-running-insecure-content/i,
  /ignore-certificate-errors/i,
  /no-sandbox/i,
  /disable-site-isolation/i,
  /allow-file-access-from-files/i,
];
for (const w of conf.app?.windows || []) {
  const args = w.additionalBrowserArgs || '';
  for (const re of FORBIDDEN_FLAGS) if (re.test(args)) problems.push(`tauri.conf.json window "${w.label}": additionalBrowserArgs contains ${re.source.replace(/\\/g, '')}`);
  if (/--disable-features=/.test(args)) problems.push(`tauri.conf.json window "${w.label}": additionalBrowserArgs disables browser features (${args.match(/--disable-features=\S+/)[0]})`);
  if (w.devtools === true) problems.push(`tauri.conf.json window "${w.label}": devtools forced on`);
}

// ---- Content Security Policy ----
const csp = conf.app?.security?.csp;
const cspText = csp == null ? '' : typeof csp === 'string' ? csp : Object.entries(csp).map(([k, v]) => `${k} ${v}`).join('; ');
if (!cspText) problems.push('tauri.conf.json: security.csp is missing or null (every shipped app needs a Content Security Policy)');
else {
  if (!/default-src\s+'self'/.test(cspText)) problems.push("csp: default-src must be 'self'");
  if (/script-src[^;]*('unsafe-inline'|'unsafe-eval'|\*)/.test(cspText)) problems.push("csp: script-src must not allow 'unsafe-inline', 'unsafe-eval' or *");
  if (/connect-src[^;]*(\shttps?:(\s|;|$)|\*)/.test(cspText)) problems.push('csp: connect-src must list the actual servers, not every http/https destination');
  if (!/object-src\s+'none'/.test(cspText)) problems.push("csp: object-src should be 'none'");
  if (!/frame-ancestors\s+'none'/.test(cspText)) problems.push("csp: frame-ancestors should be 'none'");
}
if (conf.app?.security?.dangerousDisableAssetCspModification) problems.push('tauri.conf.json: dangerousDisableAssetCspModification is set');
if (conf.app?.security?.freezePrototype === false) problems.push('tauri.conf.json: freezePrototype disabled');

// ---- Capabilities ----
const capDir = resolve(ROOT, 'src-tauri/capabilities');
if (existsSync(capDir)) {
  const cap = JSON.parse(readFileSync(resolve(capDir, 'default.json'), 'utf8'));
  const risky = (cap.permissions || []).map(p => (typeof p === 'string' ? p : p.identifier)).filter(p => /^(shell:|fs:|http:|process:)/.test(p) && !/deny/.test(p));
  if (risky.length) problems.push(`capabilities/default.json grants ${risky.join(', ')} — the app does not need shell, filesystem, process or raw http access`);
}

// ---- No inline scripts or styles in ui/ ----
for (const f of readdirSafe(resolve(ROOT, 'ui')).filter((n) => /\.(html|js)$/.test(n))) {
  const p = resolve(ROOT, 'ui', f);
  const s = readFileSync(p, 'utf8');
  if (/\sstyle="/.test(s)) problems.push(`ui/${f}: inline style="" attribute (move it to style.css)`);
  if (f.endsWith('.html') && /<script(?![^>]*\ssrc=)/i.test(s)) problems.push(`ui/${f}: inline <script> block`);
  if (/\son[a-z]+="/i.test(s)) problems.push(`ui/${f}: inline event handler attribute`);
  if (/\beval\(|new Function\(/.test(s)) problems.push(`ui/${f}: eval / new Function`);
}

// ---- Android ----
const android = resolve(ROOT, 'src-tauri/gen/android/app');
if (existsSync(android)) {
  const sources = [];
  const collect = (dir) => {
    for (const name of readdirSafe(dir)) {
      const p = resolve(dir, name);
      if (isDir(p)) collect(p);
      else if (/\.(kt|java)$/.test(name)) sources.push(p);
    }
  };
  collect(resolve(android, 'src'));
  for (const p of sources) {
    if (/setWebContentsDebuggingEnabled\s*\(\s*true\s*\)/.test(readFileSync(p, 'utf8'))) problems.push(`${p.replace(ROOT, '.')}: turns on WebView remote debugging (local testing only, never commit it)`);
  }
  const manifest = resolve(android, 'src/main/AndroidManifest.xml');
  if (existsSync(manifest)) {
    const m = readFileSync(manifest, 'utf8');
    if (/android:debuggable\s*=\s*"true"/.test(m)) problems.push('AndroidManifest.xml: android:debuggable="true"');
    if (/android:usesCleartextTraffic\s*=\s*"true"/.test(m)) problems.push('AndroidManifest.xml: android:usesCleartextTraffic="true"');
  }
  const gradle = resolve(android, 'build.gradle.kts');
  if (existsSync(gradle)) {
    const g = readFileSync(gradle, 'utf8');
    if (!/manifestPlaceholders\["usesCleartextTraffic"\]\s*=\s*"false"/.test(g)) problems.push('app/build.gradle.kts: the default usesCleartextTraffic placeholder must be "false"');
    const release = g.slice(g.indexOf('getByName("release")'));
    const block = release.slice(0, release.indexOf('\n        }'));
    if (/isDebuggable\s*=\s*true/.test(block)) problems.push('app/build.gradle.kts: the release build is debuggable');
    if (/usesCleartextTraffic"\]\s*=\s*"true"/.test(block)) problems.push('app/build.gradle.kts: the release build allows cleartext traffic');
  }
}

// ---- Antivirus / Windows security settings ----
const SEC = /Set-MpPreference|Add-MpPreference|Remove-MpPreference|MpCmdRun|-ExclusionPath|DisableRealtimeMonitoring|PUAProtection|SmartScreenEnabled|EnableLUA|netsh\s+advfirewall\s+set|Set-NetFirewallProfile|Set-ExecutionPolicy\s+Unrestricted/i;
const scan = dir => {
  for (const name of readdirSafe(dir)) {
    const p = resolve(dir, name);
    if (/node_modules|target|\.git$|data$|logs$/.test(p)) continue;
    if (isDir(p)) scan(p);
    else if (/\.(ps1|js|mjs|cjs|cmd|bat|sh|json)$/.test(name) && !/security-check.m?js$/.test(name)) {
      const s = readFileSync(p, 'utf8');
      const m = s.match(SEC);
      if (m) problems.push(`${p.replace(ROOT, '.')}: touches a Windows security setting (${m[0]})`);
    }
  }
};
import { readdirSync, statSync } from 'node:fs';
function readdirSafe(d) {
  try {
    return readdirSync(d);
  } catch {
    return [];
  }
}
function isDir(p) {
  try {
    return statSync(p).isDirectory();
  } catch {
    return false;
  }
}
scan(ROOT);

if (problems.length) {
  console.error('SECURITY CHECK FAILED:\n - ' + problems.join('\n - '));
  process.exit(1);
}
console.log(`security check OK — webview protections on, CSP set (${cspText.length} chars), no inline scripts/styles in ui/, Android release not debuggable or cleartext, nothing touches Windows security settings`);
