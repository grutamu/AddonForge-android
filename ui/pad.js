// Uses app.js globals, so it must load after it.
(() => {
  const A = window.AddonForgeAndroid;
  const body = document.body;

  function prepare(root) {
    root.querySelectorAll(".item").forEach((el) => { el.tabIndex = 0; el.setAttribute("role", "button"); });
    root.querySelectorAll(".addon-detail, .c-src a").forEach((el) => { el.tabIndex = -1; });
  }
  // The full-screen details hide the focused list row.
  new MutationObserver(() => {
    if (fullDetail() && body.classList.contains("pad-used") && !current()) setTimeout(focusFirst, 0);
  }).observe($("detail"), { attributes: true, attributeFilter: ["class"] });
  for (const id of ["installed-list", "browse-list"]) {
    const list = $(id);
    new MutationObserver(() => {
      prepare(list);
      if (id === "installed-list") highlightOnLaunch();
      restoreFocus();
    }).observe(list, { childList: true });
    prepare(list);
  }
  let launched = false;
  function highlightOnLaunch() {
    if (launched || lastPointer || open("onboard") || body.dataset.tab !== "installed" || !$("installed-list").querySelector(".item")) return;
    launched = true;
    body.classList.add("pad-used");
    padStarted = true;
    focusFirst();
  }

  // ------------------------------------------------------------ gamepad
  const ROLE = { A: "confirm", B: "back", X: "primary", Y: "search" };

  const FOCUSABLE = 'button, a[href], a[data-url], input:not([type=hidden]), select, summary, [tabindex="0"]';
  const TEXT = "input[type=text], input[type=search], input[type=password]";
  const open = (id) => !$(id).classList.contains("hidden");
  const scope = () => (open("modal") ? $("modal") : open("onboard") ? $("onboard") : body);
  const shown = (el) => el.getClientRects().length > 0 && getComputedStyle(el).visibility !== "hidden";
  const candidates = () => [...scope().querySelectorAll(FOCUSABLE)].filter((el) => !el.disabled && el.tabIndex >= 0 && shown(el));
  // Must match the breakpoint in style.css.
  const NARROW = matchMedia("(max-width: 700px)");
  const fullDetail = () => NARROW.matches && open("detail");
  const current = () => {
    const el = document.activeElement;
    return el && el !== body && scope().contains(el) && shown(el) ? el : null;
  };

  let wanted = null;
  function focus(el, dir) {
    if (!el) return;
    wanted = el;
    // Only A (typeIn) should open the keyboard.
    if (el.matches(TEXT)) el.inputMode = "none";
    el.focus({ preventScroll: true });
    el.scrollIntoView({ block: "nearest", inline: "nearest" });
    // At the last item, scroll to the page edge.
    const box = scroller(el);
    if (box && (dir === "UP" || dir === "DOWN") && !nearest(el, dir, candidates().filter((c) => box.contains(c)))) reveal(box, el, dir === "DOWN");
    remember(el);
  }
  function scroller(el) {
    for (let p = el.parentElement; p && p !== body; p = p.parentElement) {
      const o = getComputedStyle(p).overflowY;
      if ((o === "auto" || o === "scroll") && p.scrollHeight > p.clientHeight + 1) return p;
    }
    return null;
  }
  function reveal(box, el, down) {
    const b = box.getBoundingClientRect(), r = el.getBoundingClientRect();
    const room = down ? r.top - b.top - 8 : b.bottom - r.bottom - 8;
    if (room > 0) box.scrollTop += down ? room : -room;
  }
  const inSightOf = (box, el) => {
    const b = box.getBoundingClientRect(), r = el.getBoundingClientRect();
    return r.bottom > b.top && r.top < b.bottom;
  };
  // Leaving touch mode, Android swallows the first D-pad press and the
  // WebView focuses the first tab instead. Treat that as a pad press.
  let lastPointer = 0;
  document.addEventListener("focusin", (e) => {
    if (!e.target.matches(".tab") || e.target === wanted || Date.now() - lastPointer < 800) return;
    body.classList.add("pad-used");
    padStarted = true;
    setTimeout(() => (wanted && document.contains(wanted) && shown(wanted) ? wanted.focus({ preventScroll: true }) : focusFirst()), 0);
  });
  function focusFirst() {
    const s = scope();
    if (s !== body) return focus(candidates()[0]);
    if (fullDetail()) {
      const main = [$("detail-install"), $("detail-actions").querySelector(".btn")].find((el) => el && shown(el));
      return focus(main || candidates().find((el) => $("pane").contains(el)));
    }
    const panel = document.querySelector(".panel.active");
    // Skip text boxes: focusing one opens the keyboard.
    const pick = panel.querySelector(".item.selected") || panel.querySelector(".item[tabindex]") || candidates().find((el) => panel.contains(el) && !el.matches(TEXT));
    focus(pick || candidates()[0]);
  }

  let padStarted = false;
  function move(dir) {
    // The WebView focuses the first tab at launch.
    const cur = padStarted && current();
    padStarted = true;
    if (!cur) return focusFirst();
    const vertical = dir === "UP" || dir === "DOWN";
    // Up/down stay inside the details pane so its text can be scrolled.
    if (vertical && scope() === body && $("pane").contains(cur) && (!NARROW.matches || fullDetail())) return paneMove(dir === "DOWN", cur);
    const box = scroller(cur);
    if (vertical && box && !inSightOf(box, cur)) return focus(cur);
    const best = nearest(cur, dir, candidates());
    if (best) return focus(best, dir);
    if (vertical && box) box.scrollBy({ top: (dir === "DOWN" ? 1 : -1) * Math.round(box.clientHeight * 0.6), behavior: "smooth" });
  }
  function nearest(cur, dir, pool) {
    const r = cur.getBoundingClientRect();
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    const gap = (a1, a2, b1, b2) => Math.max(0, a1 - b2, b1 - a2);
    const vertical = dir === "UP" || dir === "DOWN";
    let best = null, bestScore = Infinity;
    for (const el of pool) {
      if (el === cur) continue;
      const c = el.getBoundingClientRect();
      const ex = c.left + c.width / 2, ey = c.top + c.height / 2;
      let along;
      if (cur.contains(el) || el.contains(cur)) {
        along = { UP: cy - ey, DOWN: ey - cy, LEFT: cx - ex, RIGHT: ex - cx }[dir];
        if (along <= 2) continue;
      } else {
        along = { UP: r.top - c.bottom, DOWN: c.top - r.bottom, LEFT: r.left - c.right, RIGHT: c.left - r.right }[dir];
        if (along < -6) continue;
        along = Math.max(0, along);
      }
      const side = vertical ? gap(c.left, c.right, r.left, r.right) : gap(c.top, c.bottom, r.top, r.bottom);
      const score = along + side * 4 + (vertical ? Math.abs(ex - cx) * 0.05 : Math.abs(ey - cy) * 0.05);
      if (score < bestScore) { best = el; bestScore = score; }
    }
    return best;
  }

  // Scrolls the pane a step per press until the next button is in sight. A
  // focused button scrolled out of sight hands focus to the pinned bar.
  const PINNED = ".row.right, .pane-actions";
  let paneTimer = 0;
  function paneMove(down, cur) {
    const pane = $("pane");
    const els = candidates().filter((el) => pane.contains(el));
    const flow = els.filter((el) => !el.closest(PINNED));
    const pinned = els.filter((el) => el.closest(PINNED));
    const inSight = (el) => {
      const pr = pane.getBoundingClientRect();
      const bars = pinned.map((p) => p.closest(PINNED).getBoundingClientRect().top).filter((t) => t < pr.bottom);
      const bottom = bars.length ? Math.min(...bars) : pr.bottom;
      const c = el.getBoundingClientRect();
      return c.top >= pr.top - 1 && c.bottom <= bottom + 1;
    };
    const ahead = (from) => {
      const i = flow.indexOf(from);
      if (down) return i < 0 ? flow : flow.slice(i + 1);
      return (i < 0 ? flow : flow.slice(0, i)).reverse();
    };
    const near = cur.closest(PINNED) ? (down ? null : ahead(cur).find(inSight)) : ahead(cur)[0];
    if (near && inSight(near)) return focus(near);
    const more = down ? pane.scrollTop + pane.clientHeight < pane.scrollHeight - 1 : pane.scrollTop > 0;
    if (!more) {
      if (down && cur.closest(PINNED)) return;
      const n = ahead(cur).find(inSight) || (!cur.closest(PINNED) && ahead(cur)[0]);
      if (n) return focus(n);
      if (down && pinned[0]) focus(pinned[0]);
      return;
    }
    const before = new Set(flow.filter(inSight));
    const step = Math.round(pane.clientHeight * 0.6);
    pane.scrollBy({ top: down ? step : -step, behavior: "smooth" });
    clearTimeout(paneTimer);
    paneTimer = setTimeout(() => {
      const at = current();
      if (!at || !pane.contains(at)) return;
      const fromBar = !!at.closest(PINNED);
      const n = ahead(at).find((el) => inSight(el) && (!fromBar || !before.has(el)));
      if (n) focus(n);
      else if (!fromBar && !inSight(at) && pinned[0]) focus(pinned[0]);
    }, 350);
  }

  // Re-renders replace the focused element.
  let lastFocus = null;
  function remember(el) {
    const row = el.closest("[data-key], [data-id]");
    lastFocus = el.id ? { id: el.id } : row ? { row: row.dataset.key || row.dataset.id, act: el.dataset.act || el.dataset.detail || "", self: el === row } : null;
  }
  document.addEventListener("focusin", (e) => remember(e.target));
  document.addEventListener("pointerdown", () => { wanted = null; lastPointer = Date.now(); }, true);
  function restoreFocus() {
    if (!body.classList.contains("pad-used") || current() || !lastFocus) return;
    let el = lastFocus.id && $(lastFocus.id);
    if (!el && lastFocus.row) {
      const row = [...document.querySelectorAll(".panel.active [data-key], .panel.active [data-id]")].find((r) => (r.dataset.key || r.dataset.id) === lastFocus.row);
      if (row) el = (!lastFocus.self && lastFocus.act && row.querySelector(`[data-act="${lastFocus.act}"], [data-detail="${lastFocus.act}"]`)) || row;
    }
    if (el && shown(el) && !el.disabled) el.focus({ preventScroll: true });
  }

  function confirm() {
    const el = current();
    if (!el) return focusFirst();
    if (el.matches(TEXT)) return typeIn(el);
    el.click();
  }

  function typeIn(el) {
    el.removeAttribute("inputmode");
    A?.showKeyboard();
  }
  document.addEventListener("pointerdown", (e) => { if (e.target.matches?.(TEXT)) e.target.removeAttribute("inputmode"); }, true);

  function tabs(step) {
    const all = [...document.querySelectorAll(".tab")];
    const i = all.findIndex((t) => t.classList.contains("active"));
    all[(i + step + all.length) % all.length].click();
    setTimeout(focusFirst, 0);
  }

  function primary() {
    if (open("modal")) return $("modal-yes").click();
    const tab = body.dataset.tab;
    if (tab === "installed" && !$("btn-update-all").disabled) return $("btn-update-all").click();
    if (tab === "browse" && detailId && shown($("detail-install"))) return $("detail-install").click();
  }

  function search() {
    const input = { installed: "installed-filter", browse: "browse-filter" }[body.dataset.tab];
    if (!input) return;
    focus($(input));
    typeIn($(input));
  }

  function back() {
    if (open("modal")) return $("modal-no").classList.contains("hidden") ? $("modal-yes").click() : $("modal-no").click();
    if (open("onboard")) return A?.exit();
    const el = current();
    if (el?.matches(TEXT)) {
      el.blur();
      return focusFirst();
    }
    if (detailId) {
      closeDetail();
      return current() || focusFirst();
    }
    if (body.dataset.tab !== "installed") {
      document.querySelector('.tab[data-tab="installed"]').click();
      return setTimeout(focusFirst, 0);
    }
    A?.exit();
  }

  window.afPad = (name) => {
    if (name !== "BACK") body.classList.add("pad-used");
    const r = ROLE[name];
    if (["UP", "DOWN", "LEFT", "RIGHT"].includes(name)) return move(name);
    if (name === "BACK" || r === "back") return back();
    if (r === "confirm") return confirm();
    if (r === "primary") return primary();
    if (r === "search") return search();
    if (name === "L1" || name === "L2") return tabs(-1);
    if (name === "R1" || name === "R2") return tabs(1);
    if (name === "START" && body.dataset.tab === "installed" && !busy) return $("btn-check").click();
  };
  document.addEventListener("keydown", (e) => {
    const typing = e.target.matches?.(TEXT);
    const dir = { ArrowUp: "UP", ArrowDown: "DOWN", ArrowLeft: "LEFT", ArrowRight: "RIGHT" }[e.key];
    if (dir && !(typing && (dir === "LEFT" || dir === "RIGHT"))) {
      e.preventDefault();
      body.classList.add("pad-used");
      move(dir);
    }
  });
  document.addEventListener("pointerdown", () => body.classList.remove("pad-used"), true);

  // ------------------------------------------------------------ first run
  const ob = {
    show(step) {
      $("onboard").classList.remove("hidden");
      $("onboard-access").classList.toggle("hidden", step !== "access");
      $("onboard-find").classList.toggle("hidden", step !== "find");
      setTimeout(focusFirst, 0);
    },
    hide() { $("onboard").classList.add("hidden"); },
  };
  async function waitFor(test, ms) {
    for (let t = 0; t < ms && !test(); t += 150) await new Promise((r) => setTimeout(r, 150));
    return test();
  }
  let finding = false;
  async function onboard() {
    if (!A || finding) return;
    if (!A.hasFileAccess()) return ob.show("access");
    if (!(await waitFor(() => settings, 4000))) return;
    // app.js retries the search on return from the permission screen.
    if (await waitFor(() => settings?.install_path, 2500)) return ob.hide();
    finding = true;
    ob.show("find");
    $("onboard-find-text").innerHTML = `<span class="spinner"></span> Looking for your game folder\u2026`;
    $("onboard-found").innerHTML = "";
    $("onboard-manual").classList.add("hidden");
    let found = [];
    try { found = await invoke("find_installs"); } catch (_) {}
    finding = false;
    await loadSettings();
    if (settings?.install_path) return ob.hide();
    if (found.length) {
      $("onboard-find-text").textContent = found.length === 1 ? "Found your game:" : "Found these. Pick your WoW: Forever install:";
      $("onboard-found").innerHTML = found.map((i) => `<button class="btn found ${i.flavor === "forever" ? "primary" : ""}" data-path="${esc(i.path)}">${esc(i.label)}<span>${esc(i.path)}</span></button>`).join("") +
        `<button class="btn" data-other>Use a different folder</button>`;
    } else {
      $("onboard-find-text").textContent = "Couldn't find WoW: Forever in the usual places.";
      $("onboard-manual").classList.remove("hidden");
    }
    setTimeout(focusFirst, 0);
  }
  async function usePath(path) {
    if (!path) return toast("Enter a folder first", true);
    try {
      const info = await setGameFolder(path);
      toast(`Managing ${info.label}`);
      ob.hide();
      await loadSettings();
      document.querySelector('.tab[data-tab="installed"]').click();
      await rescan();
      checkUpdates();
    } catch (err) {
      toast(String(err), true);
    }
  }
  $("onboard-allow").onclick = () => A?.requestFileAccess();
  $("onboard-found").addEventListener("click", (e) => {
    const b = e.target.closest("button[data-path]");
    if (b) return usePath(b.dataset.path);
    if (e.target.closest("[data-other]")) {
      $("onboard-manual").classList.remove("hidden");
      focus($("onboard-pick"));
    }
  });
  $("onboard-use").onclick = () => usePath($("onboard-path").value.trim());
  $("onboard-pick").onclick = async () => { const path = await ask("pickFolder"); if (path) usePath(path); };
  $("onboard-retry").onclick = onboard;
  const RECHECK_MS = 10 * 60 * 1000;
  let lastCheck = Date.now();
  let checkedOnce = false;
  new MutationObserver(() => { lastCheck = Date.now(); checkedOnce = true; }).observe($("last-checked"), { childList: true });
  document.addEventListener("visibilitychange", async () => {
    if (document.visibilityState !== "visible") return;
    onboard();
    if (!busy && settings?.install_path && A?.hasFileAccess() && Date.now() - lastCheck > RECHECK_MS) {
      lastCheck = Date.now();
      await rescan(true);
      checkUpdates(true);
    }
  });

  // From the launcher's long-press shortcut.
  async function launchAction() {
    if (A?.takeLaunchAction() !== "update-all") return;
    if (!(await waitFor(() => settings?.install_path, 5000))) return;
    if (!navigator.onLine) return toast("You're offline. Connect to Wi-Fi to update addons.", true);
    document.querySelector('.tab[data-tab="installed"]').click();
    if (checkedOnce) {
      await rescan(true);
      await checkUpdates(true);
    } else await waitFor(() => checkedOnce && !busy, 120000);
    if (!$("btn-update-all").disabled) $("btn-update-all").click();
    else toast("Everything is up to date");
  }
  window.afLaunch = launchAction;
  launchAction();

  onboard();
})();
