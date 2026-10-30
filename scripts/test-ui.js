const assert = require('node:assert/strict');
const fs = require('node:fs');
const vm = require('node:vm');
// The fake DOM invents any id, so check ids against index.html directly.
{
  const html = fs.readFileSync('ui/index.html', 'utf8');
  const scripts = fs.readdirSync('ui').filter((f) => f.endsWith('.js')).map((f) => [f, fs.readFileSync(`ui/${f}`, 'utf8')]);
  const ids = (text) => [...text.matchAll(/\sid="([^"$]+)"/g)].map((m) => m[1]);
  const have = new Set([html, ...scripts.map(([, s]) => s)].flatMap(ids));
  for (const [f, s] of scripts) {
    const used = [...s.matchAll(/(?:\$|getElementById)\(\s*["']([^"']+)["']\s*\)/g)].map((m) => m[1]);
    const missing = [...new Set(used)].filter((id) => !have.has(id));
    assert.deepEqual(missing, [], `ui/${f} uses ids that are not in ui/index.html: ${missing.join(', ')}`);
  }
}
const nodes = new Map();
function element(id) {
  if (!nodes.has(id)) nodes.set(id, {
    value: '', checked: false, textContent: '', innerHTML: '', dataset: {},
    classList: { add() {}, remove() {}, toggle() {} },
    listeners: {}, addEventListener(type, fn) { this.listeners[type] = fn; }, setAttribute() {}, removeAttribute() {},
    focus() { document.activeElement = this; },
  });
  return nodes.get(id);
}
const document = {
  getElementById: element, querySelectorAll: () => [], querySelector: () => element('main'),
  addEventListener() {}, removeEventListener() {}, activeElement: element('previous'),
};
let detailReply;
// name -> (args) => value or rejected promise
const handlers = {};
const calls = [];
const context = vm.createContext({
  document, window: { __TAURI__: { core: { invoke(command, args) {
    calls.push([command, args]);
    if (handlers[command]) return handlers[command](args);
    if (command === "addon_details") return detailReply;
  } }, opener: {} } },
  setTimeout, clearTimeout, console,
});
const source = fs.readFileSync('ui/app.js', 'utf8').split('// ---------------------------------------------------------------- boot')[0];
vm.runInContext(source, context);
const run = (code) => vm.runInContext(code, context);
run(`packages = [
  {key:'a',name:'Alpha <addon>',folders:['Alpha'],author:'Ada',source_label:'GitHub',source:true,status:'update',supports_forever:true,missing_deps:[],installed_version:'1',remote_version:'2'},
  {key:'b',name:'Beta',folders:['Beta'],source_label:'Wago',status:'no-key',supports_forever:true,missing_deps:[]},
  {key:'c',name:'Gamma',folders:['Gamma'],source_label:'GitHub',status:'ok',supports_forever:true,missing_deps:[],ignored:true},
  {key:'d',name:'Delta',folders:['Delta'],source_label:'GitHub',source:true,status:'update',supports_forever:true,missing_deps:[],pinned:true}
]; renderInstalled();`);
assert.equal(String(element('stat-total').textContent), '4');
assert.equal(String(element('stat-updates').textContent), '2');
assert.equal(String(element('stat-attention').textContent), '1');
assert.equal(element('btn-update-all').textContent, 'Update all (1)');
assert.match(element('installed-list').innerHTML, /Alpha &lt;addon&gt;/);
run(`packages[0].notice = 'Beta workaround <b>'; renderInstalled();`);
assert.match(element('installed-list').innerHTML, /note warn">Beta workaround &lt;b&gt;/);
run(`delete packages[0].notice; renderInstalled();`);
assert.doesNotMatch(element('installed-list').innerHTML, /Gamma/);
assert.equal(run(`activeFilter = 'updates'; visiblePackages().length`), 2);
assert.equal(run(`activeFilter = 'attention'; visiblePackages()[0].key`), 'b');
element('installed-filter').value = 'Ada';
assert.equal(run(`activeFilter = 'all'; visiblePackages().length`), 1);
element('installed-filter').value = 'unmatched';
run('renderInstalled()');
assert.match(element('installed-list').innerHTML, /Show all addons/);
element('installed-filter').value = '';
element('show-ignored').checked = true;
assert.equal(run('visiblePackages().length'), 4);
run(`packages = []; renderInstalled()`);
assert.equal(String(element('stat-updates').textContent), '0');
assert.match(element('installed-list').innerHTML, /Discover addons/);
(async () => {
  const originalFocus = document.activeElement;
  const promise = run(`confirmModal('Confirm?', 'Remove')`);
  assert.equal(document.activeElement, element('modal-no'));
  element('modal-no').onclick();
  assert.equal(await promise, false);
  assert.equal(document.activeElement, originalFocus);
  run(`catalog = [{ id:'boss', name:'Boss', github:'example/boss', desc:'Boss timers', installed:true }];`);
  detailReply = Promise.resolve({summary:'Release description', image:'data:image/png;base64,test'});
  await run(`openDetail('boss', {key:'boss-local',name:'Boss',folders:['Boss'],installed_version:'2'})`);
  assert.equal(element('detail-desc').textContent, 'Boss timers');
  assert.match(element('detail-meta').innerHTML, /Installed: 2/);
  assert.match(element('detail-body').innerHTML, /Release description/);
  assert.match(element('detail-body').innerHTML, /class="shot"/);
  run('closeDetail()');
  assert.equal(document.activeElement, originalFocus);
  await run(`openDetail(null, {key:'local',name:'Local addon',folders:['Local<folder>'],notes:'Local description',source_url:'https://example.test/addon'})`);
  assert.equal(element('detail-desc').textContent, 'Local description');
  assert.match(element('detail-body').innerHTML, /Local&lt;folder&gt;/);
  assert.equal(run('entryUrl(detailEntry)'), 'https://example.test/addon');
  run('closeDetail()');
  // Details pane actions.
  run(`packages = [{key:'up',name:'Up',folders:['Up'],source:true,source_label:'GitHub',status:'update',supports_forever:true,missing_deps:[],installed_version:'1 <old>',remote_version:'2'}]`);
  await run('openDetail(null, packages[0])');
  const actions = element('detail-actions').innerHTML;
  for (const label of ['Update', 'Pin version', 'Ignore', 'Remove']) assert.match(actions, new RegExp(`>${label}<`), label);
  assert.match(actions, /Hold at 1 &lt;old&gt;/);
  run('packages[0].pinned = true; renderPackageActions()');
  assert.match(element('detail-actions').innerHTML, />Allow updates</);
  assert.match(element('detail-actions').innerHTML, /Pinned, so Update all skips it/);
  run('closeDetail(); packages = []');
  let finish;
  detailReply = new Promise(resolve => { finish = resolve; });
  const loading = run("openDetail('boss')");
  run('closeDetail()');
  const before = element('detail-body').innerHTML;
  finish({summary:'Late response'});
  await loading;
  assert.equal(element('detail-body').innerHTML, before, 'Closed dialogs ignore late responses');

  // Update all.
  const tick = () => new Promise((resolve) => setTimeout(resolve, 0));
  const answerModal = async (yes) => {
    for (let i = 0; i < 50 && !element('modal-yes').onclick; i++) await tick();
    assert.ok(element('modal-yes').onclick, 'the question was asked');
    const question = element('modal-text').innerHTML;
    element(yes ? 'modal-yes' : 'modal-no').onclick();
    return question;
  };
  const pkg = (key, extra = {}) => ({ key, name: key, folders: [key], source: { kind: 'github', id: 'a/' + key }, source_label: 'GitHub', status: 'update', supports_forever: true, missing_deps: [], installed_version: '1', remote_version: '2', ...extra });
  const batchPackages = () => run(`packages = ${JSON.stringify([pkg('ok1'), pkg('bad <x>'), pkg('nofv'), pkg('ok2'), pkg('held', { pinned: true })])}; batch = null;`);
  handlers.update_package = ({ key, allowNonForever }) => {
    if (key === 'bad <x>') return Promise.reject('download refused');
    if (key === 'nofv' && !allowNonForever) return Promise.reject('NOT_FOREVER');
    return { ...pkg(key), status: 'ok', installed_version: '2' };
  };
  batchPackages();
  calls.length = 0;
  let updating = run('updateAll()');
  assert.match(await answerModal(true), /<b>nofv<\/b>'s latest release does not list a Forever build/);
  await updating;
  assert.deepEqual(calls.filter(([c]) => c === 'update_package').map(([, a]) => `${a.key}${a.allowNonForever ? '!' : ''}`), ['ok1', 'bad <x>', 'nofv', 'ok2', 'nofv!'], 'one at a time, the pinned one skipped, the question answered at the end');
  assert.match(element('batch').innerHTML, /3 updated · 1 failed/);
  assert.match(element('batch').innerHTML, /<b>bad &lt;x&gt;<\/b>: download refused/);
  assert.match(element('batch').innerHTML, /Show what needs attention/);
  assert.equal(element('toast').textContent, 'Updated 3 of 4. 1 failed.');
  assert.equal(run('packages.find((p) => p.key === "bad <x>").status'), 'error');
  batchPackages();
  handlers.update_package = ({ key, allowNonForever }) => (key === 'ok2' || allowNonForever ? { ...pkg(key), status: 'ok' } : Promise.reject('NOT_FOREVER'));
  updating = run('updateAll()');
  assert.match(await answerModal(false), /The latest releases of <b>ok1<\/b>, <b>bad &lt;x&gt;<\/b>, <b>nofv<\/b> do not/);
  await updating;
  assert.match(element('batch').innerHTML, /1 updated · 3 skipped/);
  assert.match(element('batch').innerHTML, /Skipped, no Forever build: ok1, bad &lt;x&gt;, nofv/);
  assert.equal(element('toast').textContent, 'Updated 1 of 4, skipped 3');
  handlers.check_updates = () => [];
  await run('checkUpdates(true)');
  assert.equal(element('batch').innerHTML, '');

  // Release notes.
  handlers.release_notes = () => [{ version: 'v2', text: 'Fixed <script> tags' }, { version: 'v1.5', text: 'Older' }];
  run(`packages = [${JSON.stringify(pkg('notes'))}]`);
  await run('openDetail(null, packages[0])');
  await tick();
  assert.match(element('detail-changes').innerHTML, /What's new in 2/);
  assert.match(element('detail-changes').innerHTML, /<b>v2<\/b>\nFixed &lt;script&gt; tags/);
  run('closeDetail()');
  assert.equal(element('detail-changes').innerHTML, '');
  handlers.release_notes = () => [];
  run(`packages = [${JSON.stringify(pkg('cf', { source: { kind: 'curse', id: '1' } }))}]`);
  await run('openDetail(null, packages[0])');
  await tick();
  assert.match(element('detail-changes').innerHTML, /CurseForge doesn't share release notes/);
  run('closeDetail()');

  // Switching between CurseForge and GitHub. Source ids keep the backend's
  // types: a CurseForge id is a number, or install_source rejects it.
  handlers.install_source = ({ source }) => ({ ...pkg('sw'), source, status: 'ok' });
  handlers.scan = () => [];
  handlers.catalog_list = () => [];
  run(`packages = [${JSON.stringify(pkg('sw', { curse_id: 257550 }))}]`);
  await run('openDetail(null, packages[0])');
  assert.match(element('detail-actions').innerHTML, />Get it from CurseForge</);
  calls.length = 0;
  await run('packageAction("switch", packages[0], null)');
  assert.equal(JSON.stringify(calls.find(([c]) => c === 'install_source')[1].source), '{"kind":"curse","id":257550}');
  run(`packages = [${JSON.stringify(pkg('sw', { source: { kind: 'curse', id: 257550 }, github_repo: 'me/sw' }))}]; renderPackageActions()`);
  assert.match(element('detail-actions').innerHTML, />Get it from GitHub<span>Reinstall from me\/sw/);
  calls.length = 0;
  await run('packageAction("switch", packages[0], null)');
  assert.equal(JSON.stringify(calls.find(([c]) => c === 'install_source')[1]), '{"source":{"kind":"github","id":"me/sw"},"key":"sw","allowNonForever":false}');
  run('closeDetail(); packages = []');

  // Go back to the previous version.
  run(`packages = [${JSON.stringify(pkg('rb', { status: 'ok', installed_version: '2', backup: { version: '1 <old>', saved_at: 1 } }))}]`);
  await run('openDetail(null, packages[0])');
  assert.match(element('detail-actions').innerHTML, />Go back to 1 &lt;old&gt;</);
  handlers.restore_backup = () => ({ ...pkg('rb'), status: 'unknown', installed_version: '1', remote_version: null, backup: { version: '2', saved_at: 2 } });
  handlers.set_pin = () => undefined;
  handlers.scan = () => [{ ...pkg('rb'), pinned: false, installed_version: '2', backup: { version: '1', saved_at: 3 } }];
  calls.length = 0;
  await run('packageAction("restore", packages[0], null)');
  assert.equal(run('packages[0].status'), 'update', 'still knows 2 is the latest');
  assert.equal(run('packages[0].pinned'), true);
  assert.equal(JSON.stringify(calls.find(([c]) => c === 'set_pin')[1]), '{"key":"rb","pinned":true}');
  assert.equal(element('snackbar-text').textContent, 'rb is back to 1 and pinned');
  assert.match(element('detail-actions').innerHTML, />Go back to 2</);
  calls.length = 0;
  handlers.restore_backup = () => ({ ...pkg('rb'), status: 'unknown', installed_version: '2', pinned: true, remote_version: null });
  await element('snackbar-undo').onclick();
  assert.deepEqual(calls.filter(([c]) => c !== 'package_icons').map(([c, a]) => c === 'set_pin' ? `set_pin ${a.pinned}` : c), ['restore_backup', 'set_pin false', 'scan'], 'Undo restores again, then unpins');
  assert.equal(run('packages[0].pinned'), false);
  run('closeDetail(); packages = []');
  // Game settings (WTF) backups.
  handlers.settings_backups = () => [
    { id: 'wtf-200-update', saved_at: 200, reason: 'update', size: 3 * 1048576 },
    { id: 'wtf-100-manual', saved_at: 100, reason: 'manual', size: 2048 },
  ];
  await run('loadSettingsBackups()');
  assert.match(element('wtf-list').innerHTML, /Before Update all · 3\.0 MB/);
  assert.match(element('wtf-list').innerHTML, /data-wtf-restore="wtf-100-manual"/);
  handlers.restore_settings = ({ id }) => (id === 'wtf-100-manual' ? { id: 'wtf-300-restore', saved_at: 300, reason: 'restore', size: 1 } : null);
  calls.length = 0;
  const restoring = element('wtf-list').listeners.click({ target: { closest: () => ({ dataset: { wtfRestore: 'wtf-100-manual' } }) } });
  await new Promise((r) => setTimeout(r, 0));
  assert.match(element('modal-text').innerHTML, /Close WoW first/);
  element('modal-yes').onclick();
  await restoring;
  assert.equal(JSON.stringify(calls.find(([c]) => c === 'restore_settings')[1]), '{"id":"wtf-100-manual"}');
  assert.equal(element('snackbar-text').textContent, 'Game settings restored');
  calls.length = 0;
  await element('snackbar-undo').onclick();
  assert.equal(JSON.stringify(calls.find(([c]) => c === 'restore_settings')[1]), '{"id":"wtf-300-restore"}', 'Undo restores the backup made just before');
  handlers.settings_backups = () => [];
  await run('loadSettingsBackups()');
  assert.match(element('wtf-list').innerHTML, /No backups yet/);
  // Update all backs up the settings before the first update.
  handlers.backup_settings = () => Promise.reject('There\'s no WTF folder yet');
  handlers.update_package = () => ({ ...pkg('up'), status: 'ok', installed_version: '2' });
  handlers.scan = () => [];
  run(`packages = [${JSON.stringify(pkg('up', { status: 'update' }))}]`);
  calls.length = 0;
  await run('updateAll()');
  const order = calls.map(([c]) => c).filter((c) => c === 'backup_settings' || c === 'update_package');
  assert.deepEqual(order, ['backup_settings', 'update_package'], 'a failed backup (no WTF yet) doesn\'t stop the updates');
  assert.equal(JSON.stringify(calls.find(([c]) => c === 'backup_settings')[1]), '{"reason":"update"}');
  run('packages = []; batch = null');
  console.log('UI checks passed: counts, eligible updates, filters, search, escaping, empty states, modal focus and cancellation, addon actions in the details pane, Update all progress and summary, release notes, switching between CurseForge and GitHub, going back to the previous version, game settings backups.');
})().catch((error) => { console.error(error); process.exitCode = 1; });
