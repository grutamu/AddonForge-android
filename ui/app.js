const T = window.__TAURI__;
const invoke = T.core.invoke;
const openUrl = (u) => T.opener.openUrl(u).catch((e) => toast(String(e), true));
// MainActivity bridge; undefined on desktop and in tests.
const A = window.AddonForgeAndroid;

const $ = (id) => document.getElementById(id);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

let packages = [];
let catalog = [];
let busy = false;
let settings = null;
let selfUpdate = null;
let activeFilter = "all";
let sortMode = "downloads";
const BROWSE_MAX = 200;
const icons = new Map(); // "p:<key>" | "c:<catalogue id>" -> data URL or null
const needsAttention = (p) => p.status === "error" || p.status === "no-key" || p.status === "no-source" || p.missing_deps.length > 0 || !p.supports_forever;

// ---------------------------------------------------------------- helpers
function iconTile(kind, id, name) {
  const src = icons.get(kind + ":" + id);
  const letter = esc(String(name || "?").replace(/^[^a-z0-9]+/i, "").charAt(0).toUpperCase() || "?");
  return `<span class="ico" data-ico="${esc(kind + ":" + id)}" aria-hidden="true">${src ? `<img src="${src}" alt="">` : letter}</span>`;
}
// Patches icons into the DOM in place, without a re-render.
async function loadIcons(kind, ids) {
  const want = ids.filter((id) => !icons.has(kind + ":" + id));
  if (!want.length) return;
  let got;
  try {
    got = kind === "c" ? await invoke("catalog_icons", { ids: want }) : await invoke("package_icons", { keys: want });
  } catch (_) {
    return;
  }
  if (!got) return;
  for (const id of want) icons.set(kind + ":" + id, got[id] || null);
  for (const [id, src] of Object.entries(got)) {
    document.querySelectorAll(`[data-ico="${CSS.escape(kind + ":" + id)}"]`).forEach((el) => { el.innerHTML = `<img src="${src}" alt="">`; });
  }
}
// Catalogue logos are fetched one row at a time, as the row scrolls into view,
// so cfwidget isn't hit for the whole list at once and icons pop in as you go.
let catIconObserver = null;
function observeCatalogIcons() {
  if (typeof IntersectionObserver === "undefined") return; // fake DOM in tests, non-browser
  if (!catIconObserver) {
    catIconObserver = new IntersectionObserver((entries) => {
      const want = [];
      for (const e of entries) {
        if (!e.isIntersecting) continue;
        catIconObserver.unobserve(e.target);
        const k = e.target.dataset.ico || "";
        if (k.startsWith("c:")) want.push(k.slice(2));
      }
      if (want.length) loadIcons("c", want);
    }, { rootMargin: "200px 0px" });
  } else {
    catIconObserver.disconnect();
  }
  document.querySelectorAll('#browse-list .ico[data-ico^="c:"]').forEach((el) => {
    if (!icons.has(el.dataset.ico)) catIconObserver.observe(el);
  });
}
function fmtCount(n) {
  if (n == null) return "";
  if (n >= 1e6) return (n / 1e6).toFixed(n >= 1e7 ? 0 : 1).replace(/\.0$/, "") + "M";
  if (n >= 1e3) return (n / 1e3).toFixed(n >= 1e4 ? 0 : 1).replace(/\.0$/, "") + "k";
  return String(n);
}
function ago(iso) {
  if (!iso) return "";
  const days = Math.floor((Date.now() - new Date(iso).getTime()) / 86400000);
  if (!Number.isFinite(days) || days < 0) return "";
  if (days === 0) return "updated today";
  if (days === 1) return "updated yesterday";
  if (days < 14) return `updated ${days} days ago`;
  if (days < 60) return `updated ${Math.floor(days / 7)} weeks ago`;
  if (days < 365) return `updated ${Math.floor(days / 30)} months ago`;
  return `updated ${Math.floor(days / 365)} year${days < 730 ? "" : "s"} ago`;
}

let toastTimer;
function toast(msg, bad = false) {
  if (!bad && /^(Installed|Imported|Updated|Removed)|is now /.test(msg)) A?.haptic();
  const t = $("toast");
  t.textContent = msg;
  t.className = "toast show" + (bad ? " bad" : "");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (t.className = "toast"), bad ? 6000 : 3000);
}

function confirmModal(html, yes = "Continue") {
  return new Promise((resolve) => {
    const previousFocus = document.activeElement;
    $("modal-text").innerHTML = html;
    $("modal-yes").textContent = yes;
    $("modal").classList.remove("hidden");
    $("modal-no").focus();
    const onKey = (e) => {
      if (e.key === "Escape") { e.preventDefault(); done(false); }
      if (e.key === "Tab") {
        e.preventDefault();
        (document.activeElement === $("modal-no") ? $("modal-yes") : $("modal-no")).focus();
      }
    };
    const done = (v) => {
      $("modal").classList.add("hidden");
      $("modal-yes").onclick = $("modal-no").onclick = null;
      document.removeEventListener("keydown", onKey);
      previousFocus?.focus();
      resolve(v);
    };
    document.addEventListener("keydown", onKey);
    $("modal-yes").onclick = () => done(true);
    $("modal-no").onclick = () => done(false);
  });
}

function setBusy(b) {
  if (b !== busy) A?.keepAwake(b);
  busy = b;
  document.querySelectorAll(".btn").forEach((el) => {
    if (el.id === "modal-yes" || el.id === "modal-no") return;
    el.disabled = b || (el.id === "btn-update-all" && !packages.some((p) => p.status === "update" && !p.pinned && !p.ignored));
  });
}

async function copyText(text) {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch (_) {
    const ta = document.createElement("textarea");
    ta.value = text;
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    ta.remove();
    return ok;
  }
}

document.addEventListener("click", (e) => {
  const navigation = e.target.closest("[data-navigate]");
  if (navigation) document.querySelector(`.tab[data-tab="${navigation.dataset.navigate}"]`)?.click();
  const a = e.target.closest("[data-url]");
  if (a && a.dataset.url) {
    e.preventDefault();
    openUrl(a.dataset.url);
  }
});

// ---------------------------------------------------------------- tabs
document.querySelectorAll(".tab").forEach((btn) =>
  btn.addEventListener("click", () => {
    if (detailId) closeDetail();
    document.body.dataset.tab = btn.dataset.tab;
    document.querySelectorAll(".tab").forEach((b) => {
      b.classList.toggle("active", b === btn);
      if (b === btn) b.setAttribute("aria-current", "page");
      else b.removeAttribute("aria-current");
    });
    document.querySelectorAll(".panel").forEach((p) => p.classList.toggle("active", p.id === "tab-" + btn.dataset.tab));
    document.querySelector("main").scrollTop = 0;
    if (btn.dataset.tab === "browse" && !catalog.length) loadCatalog(false);
    if (btn.dataset.tab === "settings") loadSettings();
  })
);

// ---------------------------------------------------------------- installed
function badge(p) {
  const map = {
    ok: "Up to date",
    update: "Update available",
    error: "Error",
    "no-source": "No source",
    "no-key": "Needs Wago key",
    unknown: "Not checked",
    checking: "Checking",
  };
  return `<span class="badge ${p.status}">${p.status === "checking" ? '<span class="spinner"></span> ' : ""}${map[p.status] || p.status}</span>`;
}

function visiblePackages() {
  const q = $("installed-filter").value.trim().toLowerCase();
  const showIgnored = $("show-ignored").checked;
  return packages.filter((p) => (showIgnored || !p.ignored) && (activeFilter === "all" || (activeFilter === "updates" ? p.status === "update" : needsAttention(p))) && (!q || `${p.name} ${p.folders.join(" ")} ${p.author || ""} ${p.source_label}`.toLowerCase().includes(q)));
}

function renderInstalled() {
  const list = $("installed-list");
  const rows = visiblePackages();
  $("stat-total").textContent = packages.length;
  const updatesDue = packages.filter((p) => p.status === "update" && !p.ignored).length;
  $("stat-updates").textContent = updatesDue;
  const attention = packages.filter((p) => !p.ignored && needsAttention(p)).length;
  $("stat-attention").textContent = attention;
  $("rail-badge").textContent = updatesDue;
  $("rail-badge").classList.toggle("hidden", !updatesDue);
  const counts = { all: packages.filter((p) => $("show-ignored").checked || !p.ignored).length, updates: updatesDue, attention };
  document.querySelectorAll("#view-chips .fchip").forEach((b) => { b.querySelector("b").textContent = counts[b.dataset.value] ?? ""; });
  $("pane-folder").textContent = settings?.install_path ? `Game folder: ${settings.install_path.replace(/^\/storage\/emulated\/0\//, "")}` : "No game folder yet";
  const eligible = packages.filter((p) => p.status === "update" && !p.pinned && !p.ignored).length;
  $("btn-update-all").textContent = batch?.running && !batch.waiting ? `Updating ${batch.at} of ${batch.total}…` : eligible ? `Update all (${eligible})` : "Update all";
  if (!packages.length) {
    list.innerHTML = `<div class="empty"><strong>No addons found</strong>Nothing in this installation's AddOns folder yet.<br><button class="btn" data-navigate="browse">Discover addons</button></div>`;
    $("summary").textContent = "";
    setBusy(busy);
    return;
  }
  list.innerHTML = rows.length
    ? rows
        .map((p) => {
          const ver = p.installed_version ? `<b>${esc(p.installed_version)}</b>` : `<span class="muted">unknown</span>`;
          const remote = p.remote_version ? `${esc(p.remote_version)}${p.remote_prerelease ? ' <span class="badge pre">beta</span>' : ""}` : "—";
          const flags = [
            p.supports_forever ? "" : `<span class="badge not-forever" title="No 16001 interface or _Forever toc found">not for Forever</span>`,
            p.pinned ? `<span class="badge pinned" title="Updates held back">pinned</span>` : "",
            p.ignored ? `<span class="badge managed">ignored</span>` : "",
          ].filter(Boolean).join(" ");
          const src = p.source_url
            ? `<a href="#" data-url="${esc(p.source_url)}">${esc((p.source_label || (p.curse_id ? "CurseForge" : "link")).split(" · ")[0])}</a>`
            : `<span class="muted">${esc(p.source_label || "—")}</span>`;
          const canUpdate = p.source && p.status !== "no-key";
          const noteCls = p.status === "error" ? "note err" : "note";
          const notice = p.notice ? `<div class="note warn">${esc(p.notice)}</div>` : "";
          const deps = p.missing_deps.length
            ? `<div class="note warn">Needs: ${p.missing_deps
                .map((d) => (d.catalog_id ? `<b>${esc(d.name || d.folder)}</b> <button class="mini" data-act="dep" data-id="${esc(d.catalog_id)}">install</button>` : `<b>${esc(d.folder)}</b> <i>(not in catalogue)</i>`))
                .join(", ")}</div>`
            : "";
          const entry = catalog.find((c) => c.id === p.catalog_id);
          const sub = esc(entry?.category || p.author || `${p.folders.length} folder${p.folders.length === 1 ? "" : "s"}`);
          return `<div class="item ${p.ignored ? "ignored" : ""} ${p.status === "no-source" ? "dim" : ""} ${p.key === detailPackageKey ? "selected" : ""}" data-key="${esc(p.key)}">
        <div class="c-name">${iconTile("p", p.key, p.name)}<div class="nm-wrap"><button class="nm addon-detail" data-package-detail="${esc(p.key)}" aria-haspopup="dialog" title="View addon details">${esc(p.name)}</button>${flags ? `<span class="flags">${flags}</span>` : ""}${sub ? `<div class="sub-line">${sub}</div>` : ""}</div></div>
        <div class="c-ver" title="Installed version">${ver}</div>
        <div class="c-ver available" title="Available version">${remote}</div>
        <div class="c-src">${src}</div>
        <div class="c-status">${badge(p)}</div>
        <div class="c-act">
          ${canUpdate && p.status === "update" && !p.pinned ? `<button class="btn small primary" data-act="update">Update</button>` : ""}
        </div>
        ${p.note ? `<div class="${noteCls}">${esc(p.note)}</div>` : ""}
        ${notice}
        ${deps}
      </div>`;
        })
        .join("")
    : `<div class="empty"><strong>Nothing in this view</strong>Try another filter or clear the search.<br><button class="btn" data-reset-filters>Show all addons</button></div>`;
  const updates = packages.filter((p) => p.status === "update" && !p.ignored).length;
  const errors = packages.filter((p) => p.status === "error" && !p.ignored).length;
  const ignored = packages.filter((p) => p.ignored).length;
  $("summary").textContent =
    `Showing ${rows.length} of ${packages.length} addon${packages.length === 1 ? "" : "s"}` +
    (updates ? ` · ${updates} update${updates === 1 ? "" : "s"}` : "") +
    (errors ? ` · ${errors} error${errors === 1 ? "" : "s"}` : "") +
    (ignored ? ` · ${ignored} ignored` : "");
  renderPackageActions();
  setBusy(busy);
}

$("installed-filter").addEventListener("input", renderInstalled);
$("show-ignored").addEventListener("change", renderInstalled);
// A gamepad can't open a <select> picker.
function chipRow(id, pick) {
  $(id).addEventListener("click", (e) => {
    const b = e.target.closest(".fchip");
    if (!b) return;
    setChip(id, b.dataset.value);
    pick(b.dataset.value);
  });
}
function setChip(id, value) {
  document.querySelectorAll(`#${id} .fchip`).forEach((b) => {
    b.classList.toggle("active", b.dataset.value === value);
    b.setAttribute("aria-checked", b.dataset.value === value);
  });
}
chipRow("view-chips", (v) => {
  activeFilter = v;
  renderInstalled();
});

$("installed-list").addEventListener("click", async (e) => {
  if (e.target.closest("[data-reset-filters]")) {
    $("installed-filter").value = "";
    $("show-ignored").checked = true;
    activeFilter = "all";
    setChip("view-chips", "all");
    renderInstalled();
    return;
  }
  const detailButton = e.target.closest("[data-package-detail]");
  const row = e.target.closest(".item[data-key]");
  if (detailButton || (row && !e.target.closest("button,a"))) {
    const p = packages.find((p) => p.key === row.dataset.key);
    if (p) await openDetail(p.catalog_id, p);
    return;
  }
  const btn = e.target.closest("button[data-act]");
  if (!btn || busy) return;
  const p = packages.find((x) => x.key === btn.closest(".item").dataset.key);
  if (!p) return;
  await packageAction(btn.dataset.act, p, btn);
});

async function packageAction(act, p, btn, undoing = false) {
  const key = p.key;
  if (act === "update") await updateOne(p);
  if (act === "dep") {
    const entry = catalog.find((c) => c.id === btn.dataset.id) || { id: btn.dataset.id, name: btn.dataset.id };
    await installCatalog(entry, false);
  }
  if (act === "pin" || act === "ignore") {
    try {
      if (act === "pin") await invoke("set_pin", { key, pinned: !p.pinned });
      else await invoke("set_ignore", { key, ignored: !p.ignored });
      await rescan(true);
    } catch (err) {
      toast(String(err), true);
    }
    const now = packages.find((x) => x.key === key);
    if (!undoing && now && (now.pinned !== p.pinned || now.ignored !== p.ignored)) {
      const done = act === "pin" ? (now.pinned ? "pinned" : "can update again") : now.ignored ? "ignored" : "shown again";
      snackbar(`${now.name} ${done}`, () => packageAction(act, now, btn, true));
    }
  }
  if (act === "restore") {
    setBusy(true);
    try {
      const fresh = await invoke("restore_backup", { key });
      Object.assign(fresh, { remote_version: p.remote_version, remote_forever: p.remote_forever, remote_prerelease: p.remote_prerelease });
      if (p.remote_version && fresh.source) {
        const same = sameVersion(fresh.installed_version, p.remote_version);
        Object.assign(fresh, { status: same ? "ok" : "update", update_available: !same, note: "" });
      }
      // Pin, or Update all would undo the roll back.
      const pin = !undoing && fresh.status === "update" && !fresh.pinned;
      if (pin) {
        await invoke("set_pin", { key, pinned: true });
        fresh.pinned = true;
      }
      const i = packages.findIndex((x) => x.key === key);
      if (i >= 0) packages[i] = fresh;
      renderInstalled();
      const now = `${fresh.name} is back to ${fresh.installed_version || "its previous version"}`;
      if (undoing) toast(now);
      else
        snackbar(pin ? `${now} and pinned` : now, async () => {
          await packageAction("restore", fresh, btn, true);
          if (pin) await packageAction("pin", packages.find((x) => x.key === key) || fresh, btn, true);
        });
    } catch (err) {
      toast(explainError(err), true);
    } finally {
      setBusy(false);
    }
  }
  if (act === "remove") {
    const ok = await confirmModal(`<p>Remove <b>${esc(p.name)}</b>?</p><p class="muted">Deletes: ${esc(p.folders.join(", "))}.<br>Your saved settings for it (in WTF) are left alone.</p>`, "Remove");
    if (!ok) return;
    setBusy(true);
    try {
      await invoke("remove_package", { key });
      toast(`Removed ${p.name}`);
      if (detailPackageKey === key) closeDetail();
      await rescan();
    } catch (err) {
      toast(String(err), true);
    } finally {
      setBusy(false);
    }
  }
}

let snackTimer;
function snackbar(text, undo) {
  $("snackbar-text").textContent = text;
  $("snackbar-undo").onclick = async () => {
    $("snackbar").classList.add("hidden");
    await undo();
  };
  $("snackbar").classList.remove("hidden");
  clearTimeout(snackTimer);
  snackTimer = setTimeout(() => $("snackbar").classList.add("hidden"), 6000);
}

const normVersion = (v) => String(v ?? "").trim().replace(/^[vV#]+/, "").trim().toLowerCase();
const sameVersion = (a, b) => !!normVersion(a) && normVersion(a) === normVersion(b);

function explainError(err) {
  const s = String(err);
  if (s.startsWith("SAFETY:")) return s.replace("SAFETY: refused, ", "Refused for safety: ");
  return s;
}

// -> { ok: package } | { notForever: true } | { error }
async function runUpdate(p, allowNonForever) {
  p.status = "checking";
  renderInstalled();
  try {
    const fresh = await invoke("update_package", { key: p.key, allowNonForever });
    const i = packages.findIndex((x) => x.key === p.key);
    if (i >= 0) packages[i] = fresh;
    return { ok: fresh };
  } catch (err) {
    if (String(err) === "NOT_FOREVER") {
      p.status = "update";
      return { notForever: true };
    }
    p.status = "error";
    p.note = explainError(err);
    return { error: p.note };
  } finally {
    renderInstalled();
  }
}

async function updateOne(p, allowNonForever = false) {
  setBusy(true);
  const r = await runUpdate(p, allowNonForever);
  setBusy(false);
  if (r.ok) toast(`${r.ok.name} is now ${r.ok.installed_version || "installed"}`);
  else if (r.error) toast(r.error, true);
  else if (await confirmModal(`<p><b>${esc(p.name)}</b>'s latest release does not list a Forever build.</p><p class="muted">It may still work, or it may error in game. Install it anyway?</p>`, "Install anyway")) return updateOne(p, true);
}

// keepStatus: keep known remote versions.
async function rescan(keepStatus = false) {
  setBusy(true);
  try {
    const fresh = await invoke("scan");
    if (keepStatus) {
      for (const f of fresh) {
        const old = packages.find((x) => x.key === f.key);
        if (old && old.remote_version) {
          f.remote_version = old.remote_version;
          f.remote_forever = old.remote_forever;
          f.remote_prerelease = old.remote_prerelease;
          f.update_available = old.update_available;
          f.status = old.status;
          f.note = old.note;
        }
      }
    }
    packages = fresh;
    renderInstalled();
    loadIcons("p", packages.map((p) => p.key));
  } catch (err) {
    packages = [];
    renderInstalled();
    $("installed-list").innerHTML = `<div class="empty"><strong>No install selected</strong>${esc(String(err))}<br><button class="btn primary" data-navigate="settings">Choose WoW folder</button></div>`;
  } finally {
    setBusy(false);
  }
}

// quiet: toast only when there are updates.
async function checkUpdates(quiet = false) {
  if (window.navigator?.onLine === false) {
    if (!quiet) toast("You're offline. Connect to Wi-Fi to check for updates.", true);
    return;
  }
  setBusy(true);
  batch = null;
  renderBatch();
  $("btn-check").innerHTML = '<span class="spinner"></span> Checking…';
  packages.forEach((p) => {
    if (p.source && p.status !== "no-key" && !p.ignored) p.status = "checking";
  });
  renderInstalled();
  try {
    packages = await invoke("check_updates");
    $("last-checked").textContent = `Checked at ${new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" })}`;
    renderInstalled();
    const n = packages.filter((p) => p.status === "update" && !p.ignored && !p.pinned).length;
    const unchecked = packages.filter((p) => !p.ignored && p.status !== "ok" && p.status !== "update").length;
    if (n || !quiet) toast(n ? `${n} update${n === 1 ? "" : "s"} available` : unchecked ? `Check complete. ${unchecked} addon${unchecked === 1 ? " could" : "s could"} not be verified.` : "All checked addons are up to date");
  } catch (err) {
    toast(String(err), true);
  } finally {
    $("btn-check").textContent = "Check for updates";
    setBusy(false);
  }
}

// Non-Forever updates are deferred to a single question at the end.
let batch = null; // { running, at, total, name, again, waiting } or { updated, failed, skipped }
const plural = (n, word) => `${n} ${word}${n === 1 ? "" : "s"}`;
function renderBatch() {
  const box = $("batch");
  box.classList.toggle("hidden", !batch);
  if (!batch) return (box.innerHTML = "");
  if (batch.running) {
    box.innerHTML = batch.waiting
      ? `<div>${plural(batch.waiting, "addon")} without a Forever build: install anyway?</div>`
      : `<div><span class="spinner"></span> ${batch.again ? "Installing" : "Updating"} ${batch.at} of ${batch.total}: <b>${esc(batch.name)}</b></div><progress max="${batch.total}" value="${batch.at - 1}"></progress>`;
    return;
  }
  const { updated, failed, skipped } = batch;
  const shown = failed.slice(0, 3);
  box.innerHTML =
    `<div class="batch-sum${failed.length ? " bad" : ""}">${[`${updated.length} updated`, failed.length && `${failed.length} failed`, skipped.length && `${skipped.length} skipped`].filter(Boolean).join(" · ")}</div>` +
    (shown.length ? `<ul>${shown.map((f) => `<li><b>${esc(f.name)}</b>: ${esc(f.error)}</li>`).join("")}${failed.length > shown.length ? `<li>and ${failed.length - shown.length} more</li>` : ""}</ul>` : "") +
    (skipped.length ? `<div class="muted">Skipped, no Forever build: ${skipped.map(esc).join(", ")}</div>` : "") +
    (failed.length ? `<button class="btn small" data-batch-attention>Show what needs attention</button>` : "");
}
$("batch").addEventListener("click", (e) => {
  if (!e.target.closest("[data-batch-attention]")) return;
  activeFilter = "attention";
  setChip("view-chips", "attention");
  renderInstalled();
});

async function updateAll() {
  const todo = packages.filter((p) => p.status === "update" && !p.pinned && !p.ignored);
  if (!todo.length || busy) return;
  const updated = [], failed = [], notForever = [];
  let skipped = [];
  const run = async (list, again) => {
    for (const [i, p] of list.entries()) {
      batch = { running: true, at: i + 1, total: list.length, name: p.name, again };
      renderBatch();
      const r = await runUpdate(p, again);
      if (r.ok) updated.push(r.ok.name);
      else if (r.notForever) notForever.push(p);
      else failed.push({ name: p.name, error: r.error });
    }
  };
  setBusy(true);
  try {
    await run(todo, false);
    if (notForever.length) {
      batch = { running: true, waiting: notForever.length };
      renderBatch();
      const names = notForever.map((p) => `<b>${esc(p.name)}</b>`).join(", ");
      const ok = await confirmModal(`<p>${notForever.length === 1 ? `${names}'s latest release does` : `The latest releases of ${names} do`} not list a Forever build.</p><p class="muted">${notForever.length === 1 ? "It" : "They"} may still work, or may error in game. Install anyway?</p>`, "Install anyway");
      if (ok) await run(notForever.slice(), true);
      else skipped = notForever.map((p) => p.name);
    }
  } finally {
    batch = { updated, failed, skipped };
    setBusy(false);
    renderInstalled();
    renderBatch();
  }
  const msg = `Updated ${updated.length === todo.length ? plural(updated.length, "addon") : `${updated.length} of ${todo.length}`}`;
  toast(failed.length ? `${msg}. ${failed.length} failed.` : skipped.length ? `${msg}, skipped ${skipped.length}` : msg, failed.length > 0);
}

$("btn-check").onclick = async () => {
  await rescan(true);
  await checkUpdates();
};
$("btn-update-all").onclick = updateAll;

// ---------------------------------------------------------------- browse
function sortedCatalog(rows) {
  const by = sortMode;
  const name = (a, b) => a.name.localeCompare(b.name, undefined, { sensitivity: "base" });
  if (by === "downloads") return rows.sort((a, b) => (b.downloads || 0) - (a.downloads || 0) || name(a, b));
  if (by === "updated") return rows.sort((a, b) => String(b.updated_at || "").localeCompare(String(a.updated_at || "")) || name(a, b));
  return rows.sort(name);
}

function renderCatalog() {
  const q = $("browse-filter").value.trim().toLowerCase();
  const all = sortedCatalog(catalog.filter((e) => !q || `${e.name} ${e.desc} ${e.category} ${e.github || ""}`.toLowerCase().includes(q)));
  const rows = all.slice(0, BROWSE_MAX);
  $("browse-list").innerHTML = rows.length
    ? rows
        .map((e) => {
          const src = e.github ? `GitHub · ${e.github}` : e.wago ? "Wago" : e.wowi ? "WoWInterface" : e.tukui ? "TukUI" : e.curse ? "CurseForge" : "";
          const fv = e.forever === true ? `<span class="badge forever">Forever</span>` : e.forever === false ? `<span class="badge not-forever">No Forever build</span>` : "";
          const canInstall = catalogInstallable(e);
          const needsKey = e.wago && !canInstall;
          return `<div class="item" data-id="${esc(e.id)}">
            <div class="c-name clickable" data-detail="${esc(e.id)}" title="Details">${iconTile("c", e.id, e.name)}<div class="nm-wrap"><span class="nm">${esc(e.name)}</span><span class="flags">${fv}${e.installed ? ' <span class="badge ok">installed</span>' : ""}</span><div class="sub-line" title="${esc(e.desc || "")}">${esc(e.desc || "")}</div></div></div>
            <div class="c-cat muted">${esc(e.category || "")}</div>
            <div class="c-dl" title="${e.downloads != null ? esc(e.downloads.toLocaleString() + " downloads, all time, counted on " + ({ wowi: "WoWInterface", curse: "CurseForge" }[e.stat_source] || "GitHub")) : "No download count for this source"}">${e.downloads != null ? `<b>${fmtCount(e.downloads)}</b>` : `<span class="muted">\u2014</span>`}${e.updated_at ? `<div class="sub-line">${ago(e.updated_at)}</div>` : ""}</div>
            <div class="c-src"><a href="#" data-url="${esc(e.url || (e.github ? "https://github.com/" + e.github : "#"))}">${esc(src)}</a></div>
            <div class="c-act">
              ${canInstall ? `<button class="btn small" data-act="install">${e.installed ? "Reinstall" : "Install"}</button>` : needsKey ? `<span class="badge no-key">needs Wago key</span>` : `<span class="badge no-source">link only</span>`}
            </div>
          </div>`;
        })
        .join("") + (all.length > rows.length ? `<div class="empty">Showing ${rows.length} of ${all.length}. Search to find the rest.</div>` : "")
    : `<div class="empty">Nothing matches.</div>`;
  setBusy(busy);
  observeCatalogIcons();
}

async function loadCatalog(refresh) {
  setBusy(true);
  try {
    catalog = await invoke("catalog_list", { refresh });
    renderCatalog();
  } catch (err) {
    toast(String(err), true);
  } finally {
    setBusy(false);
  }
}

$("browse-filter").addEventListener("input", renderCatalog);
chipRow("sort-chips", (v) => {
  sortMode = v;
  renderCatalog();
});

// ---------------------------------------------------------------- details window
let detailId = null;
let detailSeq = 0;
let detailEntry = null;
let detailFocus = null;
let detailPackageKey = null;
let listScroll = 0; // restored when the details close
// Mirrors pick_source in lib.rs.
function catalogInstallable(e) {
  return e.github || e.wowi || e.tukui || (e.wago && settings?.has_wago_key) || e.curse;
}
function sourceLabel(e) {
  return e.github ? `GitHub \u00b7 ${e.github}` : e.wago ? "Wago" : e.wowi ? "WoWInterface" : e.tukui ? "TukUI" : "CurseForge";
}
function entryUrl(e) {
  return e.url || (e.github ? "https://github.com/" + e.github : e.wago ? "https://addons.wago.io/addons/" + e.wago : e.wowi ? "https://www.wowinterface.com/downloads/info" + e.wowi : e.curse ? "https://www.curseforge.com/wow/addons/search?search=" + encodeURIComponent(e.name) : "");
}
async function openDetail(id, installedPackage = null) {
  const entry = catalog.find((x) => x.id === id);
  const e = installedPackage ? { ...entry, id: entry?.id, name: installedPackage.name,
    installed: true, desc: entry?.desc || installedPackage.notes || "",
    url: entryUrl(entry || {}) || installedPackage.source_url || "" } : entry;
  if (!e) return;
  if (!detailId) listScroll = document.querySelector("main").scrollTop;
  detailFocus = document.activeElement;
  detailEntry = e;
  detailId = id || `package:${installedPackage.key}`;
  detailPackageKey = installedPackage?.key || null;
  markSelected();
  const seq = ++detailSeq;
  const iconKey = installedPackage ? "p:" + installedPackage.key : "c:" + id;
  const src = icons.get(iconKey);
  $("detail-icon").innerHTML = src ? `<img src="${src}" alt="">` : esc(e.name.replace(/^[^a-z0-9]+/i, "").charAt(0).toUpperCase() || "?");
  $("detail-icon").dataset.ico = iconKey; // for loadIcons
  if (id && !installedPackage) loadIcons("c", [id]); // the catalogue logo for the detail tile
  $("detail-name").textContent = e.name;
  const bits = [e.category, installedPackage?.source_label || (entry ? sourceLabel(e) : "No update source")];
  if (installedPackage?.installed_version) bits.push(`Installed: ${installedPackage.installed_version}`);
  if (e.downloads != null) bits.push(`${fmtCount(e.downloads)} downloads`);
  if (e.updated_at) bits.push(ago(e.updated_at));
  $("detail-meta").innerHTML = bits.filter(Boolean).map(esc).join(" \u00b7 ") +
    (e.forever === true ? ' \u00b7 <span class="badge forever">Forever</span>' : e.forever === false ? ' \u00b7 <span class="badge not-forever">No Forever build</span>' : "") +
    (e.installed ? ' \u00b7 <span class="badge ok">installed</span>' : "");
  $("detail-desc").textContent = e.desc || "";
  const localInfo = installedPackage ? `<div class="muted">Folders: ${installedPackage.folders.map(esc).join(", ")}</div>` : "";
  const more = e.github ? "Reading its README" : e.curse ? "Reading its CurseForge page" : "";
  $("detail-body").innerHTML = localInfo + (more ? `<div class="muted"><span class="spinner"></span> ${more}\u2026</div>` : "");
  const canInstall = catalogInstallable(e);
  const needsKey = e.wago && !canInstall;
  $("detail-install").textContent = e.installed ? "Reinstall" : "Install";
  $("detail-install").classList.toggle("hidden", !canInstall);
  $("detail-note").textContent = needsKey ? "Needs a Wago key (Settings)." : canInstall || installedPackage ? "" : "Not installable from here: get it from its page.";
  $("detail-open").classList.toggle("hidden", !entryUrl(e));
  renderPackageActions();
  $("detail").classList.remove("hidden");
  if (!more) return;
  let d = null;
  try {
    d = await invoke("addon_details", { id });
  } catch (_) {}
  if (seq !== detailSeq || !detailId) return;
  const parts = [localInfo];
  if (d?.image) parts.push(`<img class="shot" src="${d.image}" alt="${e.github ? "Screenshot from the addon's README" : "The addon's CurseForge picture"}">`);
  if (d?.summary) parts.push(`<div class="readme">${esc(d.summary)}</div>`);
  if (!d?.image && !d?.summary) parts.push(`<div class="muted">${e.github ? "No README to show." : "Nothing to show."} Its page has the details.</div>`);
  $("detail-body").innerHTML = parts.join("");
}
function closeDetail() {
  $("detail").classList.add("hidden");
  loadNotes(null);
  detailId = null;
  detailEntry = null;
  detailPackageKey = null;
  markSelected();
  ++detailSeq;
  document.querySelector("main").scrollTop = listScroll;
  detailFocus?.focus({ preventScroll: true });
}
function markSelected() {
  document.querySelectorAll("#installed-list .item").forEach((row) => row.classList.toggle("selected", row.dataset.key === detailPackageKey));
}
function renderPackageActions() {
  const box = $("detail-actions");
  const p = detailPackageKey && packages.find((x) => x.key === detailPackageKey);
  loadNotes(p);
  if (!p) {
    box.innerHTML = "";
    return;
  }
  $("detail-install").classList.add("hidden");
  const canUpdate = p.source && p.status !== "no-key";
  const versions = `Installed <b>${esc(p.installed_version || "unknown")}</b>` + (p.remote_version ? ` · Available <b>${esc(p.remote_version)}</b>` : "");
  const notes = [
    p.note ? `<div class="note ${p.status === "error" ? "err" : ""}">${esc(p.note)}</div>` : "",
    p.notice ? `<div class="note warn">${esc(p.notice)}</div>` : "",
    p.supports_forever ? "" : `<div class="note warn">No Forever build found in its files. It may error in game.</div>`,
    ...p.missing_deps.map((d) => `<div class="note warn">Needs <b>${esc(d.name || d.folder)}</b> ${d.catalog_id ? `<button class="btn small" data-act="dep" data-id="${esc(d.catalog_id)}">Install it</button>` : "(not in the catalogue)"}</div>`),
  ].join("");
  const act = (name, label, why, cls = "") => `<button class="btn act ${cls}" data-act="${name}">${label}${why ? `<span>${why}</span>` : ""}</button>`;
  box.innerHTML = `<div class="pkg-status">${badge(p)} <span class="muted">${versions}</span></div>${notes}<div class="act-list">` +
    (canUpdate && p.status === "update" && !p.pinned ? act("update", "Update", `To ${esc(p.remote_version || "the latest")}`, "primary") : "") +
    (canUpdate && !(p.status === "update" && !p.pinned) ? act("update", p.status === "update" ? "Update" : p.managed ? "Reinstall" : "Install", p.status === "update" ? "Pinned, so Update all skips it" : "Download the current release again") : "") +
    (p.backup ? act("restore", `Go back to ${esc(p.backup.version || "the previous version")}`, "The version you had before the last update. Keeps this one, so you can switch again") : "") +
    act("pin", p.pinned ? "Allow updates" : "Pin version", p.pinned ? "Let Update all update it again" : `Hold at ${esc(p.installed_version || "this version")}; Update all skips it`) +
    act("ignore", p.ignored ? "Unignore" : "Ignore", p.ignored ? "Show it and check it for updates again" : "Hide it and stop checking for updates") +
    act("remove", "Remove", "Deletes its folders; settings in WTF are kept", "danger") +
    `</div>`;
}
let notesFor = null; // "key|remote version" shown or loading
async function loadNotes(p) {
  const want = p && p.status === "update" && p.source ? `${p.key}|${p.remote_version}` : null;
  if (want === notesFor) return;
  notesFor = want;
  const box = $("detail-changes");
  if (!want) return (box.innerHTML = "");
  box.innerHTML = `<div class="muted"><span class="spinner"></span> Reading what's new\u2026</div>`;
  let notes = null;
  let failed = false;
  try {
    notes = await invoke("release_notes", { key: p.key });
  } catch (_) {
    failed = true;
  }
  if (notesFor === want) box.innerHTML = renderNotes(p, notes, failed);
}
function renderNotes(p, notes, failed) {
  const head = `<div class="notes-head">What's new${p.remote_version ? ` in ${esc(p.remote_version)}` : ""}</div>`;
  if (!notes?.length) {
    const why = failed ? "Couldn't read the release notes." : p.source?.kind === "curse" ? "CurseForge doesn't share release notes with AddonForge." : "This release has no notes.";
    return `${head}<div class="muted">${why} Its page may have them.</div>`;
  }
  return head + notes.map((n) => `<div class="notes">${notes.length > 1 ? `<b>${esc(n.version)}</b>\n` : ""}${esc(n.text)}</div>`).join("");
}
$("detail-actions").addEventListener("click", async (e) => {
  const btn = e.target.closest("button[data-act]");
  const p = detailPackageKey && packages.find((x) => x.key === detailPackageKey);
  if (!btn || !p || busy) return;
  await packageAction(btn.dataset.act, p, btn);
});
$("detail-close").onclick = closeDetail;
$("detail").addEventListener("click", (e) => { if (e.target === $("detail")) closeDetail(); });
document.addEventListener("keydown", (e) => { if (e.key === "Escape" && detailId) closeDetail(); });
$("detail-open").onclick = () => { const e = detailEntry; if (e) openUrl(entryUrl(e)); };
$("detail-install").onclick = async () => {
  const e = detailEntry;
  if (!e || busy) return;
  closeDetail();
  await installCatalog(e, false);
};
$("btn-catalog-refresh").onclick = () => loadCatalog(true);
$("browse-list").addEventListener("click", async (e) => {
  const row = e.target.closest(".item[data-id]");
  if (row && !e.target.closest("button, a")) return openDetail(row.dataset.id);
  const btn = e.target.closest("button[data-act=install]");
  if (!btn || busy) return;
  const id = btn.closest(".item").dataset.id;
  const entry = catalog.find((x) => x.id === id);
  await installCatalog(entry, false);
});

async function installCatalog(entry, allowNonForever) {
  setBusy(true);
  btnState(entry.id, true);
  try {
    const p = await invoke("install_catalog", { id: entry.id, allowNonForever });
    toast(`Installed ${p.name} ${p.installed_version || ""}`);
    entry.installed = true;
    renderCatalog();
    await rescan(true);
  } catch (err) {
    if (String(err) === "NOT_FOREVER") {
      setBusy(false);
      btnState(entry.id, false);
      const ok = await confirmModal(`<p><b>${esc(entry.name)}</b>'s latest release does not list a Forever build.</p><p class="muted">Install it anyway?</p>`, "Install anyway");
      if (ok) return installCatalog(entry, true);
      return;
    }
    toast(explainError(err), true);
  } finally {
    setBusy(false);
    btnState(entry.id, false);
  }
}
function btnState(id, working) {
  const b = document.querySelector(`.item[data-id="${CSS.escape(id)}"] button[data-act=install]`);
  if (b) b.innerHTML = working ? '<span class="spinner"></span> Installing' : b.textContent.trim().replace(/^Installing$/, "Install");
}

async function installGithub(allowNonForever = false) {
  const repo = $("gh-repo").value.trim();
  if (!repo) return toast("Paste a GitHub repo first", true);
  setBusy(true);
  const btn = $("btn-gh-install");
  btn.innerHTML = '<span class="spinner"></span> Installing';
  try {
    const p = await invoke("install_github", { repo, allowNonForever });
    toast(`Installed ${p.name} ${p.installed_version || ""}`);
    $("gh-repo").value = "";
    await rescan(true);
    loadCatalog(false);
  } catch (err) {
    if (String(err) === "NOT_FOREVER") {
      setBusy(false);
      btn.textContent = "Install";
      const ok = await confirmModal(`<p>The latest release of <b>${esc(repo)}</b> does not list a Forever build.</p><p class="muted">Install it anyway?</p>`, "Install anyway");
      if (ok) return installGithub(true);
      return;
    }
    toast(explainError(err), true);
  } finally {
    setBusy(false);
    btn.textContent = "Install";
  }
}
$("btn-gh-install").onclick = () => installGithub(false);
$("gh-repo").addEventListener("keydown", (e) => {
  if (e.key === "Enter") installGithub(false);
});

// ---------------------------------------------------------------- settings
async function loadSettings() {
  try {
    settings = await invoke("get_settings");
  } catch (err) {
    toast(String(err), true);
    return;
  }
  $("version").textContent = settings.release || "(local build)";
  $("state-file").textContent = settings.state_file;
  $("install-path").value = settings.install_path || "";
  $("wago-link").dataset.url = settings.wago_key_url;
  $("wago-status").textContent = settings.has_wago_key ? "A key is saved." : "No key saved. Wago-only addons will show as \"Needs Wago key\".";
  $("pref-prerelease").checked = !!settings.allow_prerelease;
  $("catalog-info").textContent = settings.catalog_count
    ? `${settings.catalog_count} addons · ${{ remote: "downloaded from instawow's list", cached: "today's copy of instawow's list", "cached (old)": "older copy (instawow's list unreachable)" }[settings.catalog_source] || settings.catalog_source}${settings.catalog_updated ? " · " + settings.catalog_updated : ""}`
    : "not loaded yet";
}

async function detect() {
  $("detected").innerHTML = `<span class="muted"><span class="spinner"></span> Looking for your game folder…</span>`;
  try {
    const found = await invoke("find_installs");
    if (!found.length) {
      $("detected").innerHTML = `<span class="muted">Nothing found in the usual places. Use Choose folder… to pick the <code>_forever_</code> folder yourself.</span>`;
      return;
    }
    $("detected").innerHTML = found
      .map((i) => `<button class="btn ${i.flavor}" data-path="${esc(i.path)}" ${i.flavor !== "forever" ? 'title="Not supported yet: only WoW: Forever for now"' : ""}>${esc(i.label)} <code>${esc(i.path)}</code></button>`)
      .join("");
    await loadSettings();
    if (settings.install_path) rescan();
  } catch (err) {
    $("detected").innerHTML = `<span class="muted">${esc(String(err))}</span>`;
  }
}
$("detected").addEventListener("click", async (e) => {
  const b = e.target.closest("button[data-path]");
  if (!b) return;
  $("install-path").value = b.dataset.path;
  await savePath();
});

async function savePath() {
  const path = $("install-path").value.trim();
  if (!path) return toast("Enter a folder first", true);
  try {
    const info = await invoke("set_install_path", { path });
    toast(`Managing ${info.label}`);
    await loadSettings();
    await rescan();
  } catch (err) {
    toast(String(err), true);
  }
}

// ---------------------------------------------------------------- Android pickers and sharing
// MainActivity replies via afResult(id, value); null when cancelled.
const replies = new Map();
let replySeq = 0;
window.afResult = (id, value) => {
  if (id === "share-error") return toast(value, true);
  replies.get(id)?.(value);
  replies.delete(id);
};
function ask(method) {
  if (!A) return Promise.resolve(null);
  return new Promise((resolve) => {
    const id = `r${++replySeq}`;
    replies.set(id, resolve);
    A[method](id);
  });
}
// Accepts the game folder or its _forever_.
async function setGameFolder(path) {
  try {
    return await invoke("set_install_path", { path });
  } catch (err) {
    try {
      return await invoke("set_install_path", { path: `${path.replace(/\/$/, "")}/_forever_` });
    } catch (_) {
      throw err;
    }
  }
}

$("btn-pick").onclick = async () => {
  const path = await ask("pickFolder");
  if (!path) return;
  try {
    const info = await setGameFolder(path);
    toast(`Managing ${info.label}`);
    await loadSettings();
    await rescan();
    checkUpdates(true);
  } catch (err) {
    toast(String(err), true);
  }
};
$("btn-detect").onclick = detect;
$("btn-save-path").onclick = savePath;
$("btn-save-key").onclick = async () => {
  try {
    await invoke("set_wago_key", { key: $("wago-key").value });
    $("wago-key").value = "";
    toast("Wago key saved");
    await loadSettings();
    await rescan();
  } catch (err) {
    toast(String(err), true);
  }
};
$("btn-clear-key").onclick = async () => {
  await invoke("set_wago_key", { key: "" });
  toast("Wago key cleared");
  await loadSettings();
  await rescan();
};
$("pref-prerelease").addEventListener("change", async (e) => {
  try {
    await invoke("set_prefs", { allowPrerelease: e.target.checked });
    toast(e.target.checked ? "Pre-releases included. Check for updates to see them." : "Pre-releases excluded");
  } catch (err) {
    toast(String(err), true);
  }
});

// ---------------------------------------------------------------- export / import
$("btn-export").onclick = async () => {
  if (!A) return;
  try {
    const path = `${A.cacheDir()}/addonforge-list.json`;
    const n = await invoke("export_list", { path });
    A.share(path, "application/json", "My AddonForge addon list");
    toast(`${n} addon${n === 1 ? "" : "s"} in the list`);
  } catch (err) {
    toast(String(err), true);
  }
};
$("btn-import").onclick = async () => {
  const path = await ask("pickFile");
  if (!path) return;
  try {
    renderImportPlan(await invoke("import_list", { path }));
  } catch (err) {
    toast(String(err), true);
  }
};

function renderImportPlan(plan) {
  const box = $("import-plan");
  const todo = plan.filter((r) => r.installable && !r.installed);
  box.classList.remove("hidden");
  box.innerHTML =
    `<div class="muted">${plan.length} in the list · ${plan.filter((r) => r.installed).length} already installed · ${todo.length} to install · ${plan.filter((r) => !r.installable && !r.installed).length} can't be installed automatically</div>` +
    `<ul class="plan">` +
    plan
      .map((r) => `<li class="${r.installed ? "have" : r.installable ? "todo" : "skip"}">${esc(r.name)} <span class="muted">${r.installed ? "installed" : r.installable ? r.source.kind + " · " + r.source.id : r.why}</span></li>`)
      .join("") +
    `</ul>` +
    (todo.length ? `<button id="btn-import-go" class="btn primary">Install ${todo.length} missing</button>` : "");
  const go = $("btn-import-go");
  if (go)
    go.onclick = async () => {
      setBusy(true);
      let done = 0;
      for (const r of todo) {
        go.innerHTML = `<span class="spinner"></span> ${done + 1} / ${todo.length}: ${esc(r.name)}`;
        try {
          await invoke("install_source", { source: r.source, key: r.key, allowNonForever: true });
          done++;
        } catch (err) {
          toast(`${r.name}: ${explainError(err)}`, true);
        }
      }
      setBusy(false);
      toast(`Imported ${done} of ${todo.length}`);
      box.classList.add("hidden");
      await rescan();
      document.querySelector('.tab[data-tab="installed"]').click();
    };
}

// ---------------------------------------------------------------- report a problem
let diagText = "";
async function diag() {
  diagText = await invoke("diagnostics");
  $("diag").textContent = diagText;
  $("diag").classList.remove("hidden");
  return diagText;
}
$("btn-copy-diag").onclick = async () => {
  try {
    await diag();
    toast((await copyText(diagText)) ? "Diagnostics copied" : "Couldn't copy, select the text below instead");
  } catch (err) {
    toast(String(err), true);
  }
};
$("btn-share-log").onclick = () => settings?.state_file && A?.share(settings.state_file.replace(/[^/]+$/, "addonforge.log"), "text/plain", "AddonForge log");
$("btn-report-gh").onclick = async () => {
  try {
    const d = await diag();
    const body = "**What happened?**\n\n(describe it here)\n\n**Diagnostics**\n```\n" + d.slice(0, 5000) + "\n```";
    const url = `${settings.repo_url}/issues/new?title=${encodeURIComponent("Problem: ")}&body=${encodeURIComponent(body)}`;
    // GitHub's login redirect fails on long return_to URLs.
    if (url.length > 2000) {
      await copyText(d);
      toast("Diagnostics copied, paste them into the issue");
      openUrl(`${settings.repo_url}/issues/new?title=${encodeURIComponent("Problem: ")}`);
    } else openUrl(url);
  } catch (err) {
    toast(String(err), true);
  }
};

// ---------------------------------------------------------------- self-update
async function checkSelf(manual = false) {
  try {
    selfUpdate = await invoke("check_self_update");
  } catch (err) {
    if (manual) toast(String(err), true);
    return;
  }
  if (selfUpdate.available && selfUpdate.latest) {
    $("selfupdate-text").innerHTML = `AddonForge <b>${esc(selfUpdate.latest.version)}</b> is out.`;
    $("selfupdate").classList.remove("hidden");
  } else {
    $("selfupdate").classList.add("hidden");
    if (manual) toast(selfUpdate.error ? `Couldn't check: ${selfUpdate.error}` : "AddonForge is up to date");
  }
}
$("selfupdate-skip").onclick = async () => {
  await invoke("skip_self_update", { tag: selfUpdate.tag }).catch(() => {});
  $("selfupdate").classList.add("hidden");
};
$("selfupdate-go").onclick = () => selfUpdate?.latest?.url && openUrl(selfUpdate.latest.url);

// ---------------------------------------------------------------- boot
(async () => {
  await loadSettings();
  if (!settings || !settings.install_path) {
    await detect();
    await loadSettings();
  }
  if (settings && settings.install_path) {
    await rescan();
    checkUpdates();
  } else {
    document.querySelector('.tab[data-tab="settings"]').click();
  }
  loadCatalog(false);
  checkSelf(false);
  if (settings?.start_tab?.startsWith("detail:")) { await loadCatalog(false); document.querySelector(`.tab[data-tab="browse"]`)?.click(); openDetail(settings.start_tab.slice(7)); }
  else if (settings?.start_tab) document.querySelector(`.tab[data-tab="${settings.start_tab}"]`)?.click();
  invoke("whats_new")
    .then((w) => {
      if (!w) return;
      $("modal-no").classList.add("hidden");
      confirmModal(`<p><b>AddonForge ${esc(w.version)}</b> is installed. What changed:</p><div class="whatsnew">${esc(w.notes)}</div>`, "OK")
        .finally(() => $("modal-no").classList.remove("hidden"));
    })
    .catch(() => {});
})();

// Retry the folder search after returning from the storage permission screen.
document.addEventListener("visibilitychange", async () => {
  if (document.visibilityState !== "visible" || settings?.install_path) return;
  await detect();
  await loadSettings();
  if (settings?.install_path) { await rescan(); checkUpdates(); }
});

const showNet = () => $("offline").classList.toggle("hidden", navigator.onLine);
addEventListener("online", showNet);
addEventListener("offline", showNet);
showNet();
