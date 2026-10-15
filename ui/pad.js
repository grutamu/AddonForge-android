// AddonForge on the AYN Thor: gamepad navigation, focus, Back, first run and
// coming back from the launcher. Loaded after app.js and uses its globals.
// MainActivity forwards every pad button and Back as afPad("NAME"); without
// it (the Mac preview) the keyboard arrows move the focus.
(() => {
  const A = window.AddonForgeAndroid;
  const body = document.body;

  // Lists are rebuilt with innerHTML: make rows focusable, keep the focus.
  function prepare(root) {
    root.querySelectorAll(".item").forEach((el) => { el.tabIndex = 0; el.setAttribute("role", "button"); });
    root.querySelectorAll(".addon-detail, .c-src a").forEach((el) => { el.tabIndex = -1; });
  }
  for (const id of ["installed-list", "browse-list", "bundle-list"]) {
    const list = $(id);
    new MutationObserver(() => {
      prepare(list);
      if (id === "installed-list") highlightOnLaunch();
      restoreFocus();
    }).observe(list, { childList: true });
    prepare(list);
  }
  // The Thor is held with the controller: show where the focus is from the
  // start, unless the screen has already been touched.
  let launched = false;
  function highlightOnLaunch() {
    if (launched || lastPointer || open("onboard") || body.dataset.tab !== "installed" || !$("installed-list").querySelector(".item")) return;
    launched = true;
    body.classList.add("pad-used");
    padStarted = true;
    focusFirst();
  }

  // ------------------------------------------------------------ gamepad
  const LAYOUT_KEY = "af-pad-layout";
  const layout = () => localStorage.getItem(LAYOUT_KEY) || "nintendo";
  // Nintendo: A confirms, B goes back, X (top) = main action, Y = search.
  // Xbox swaps both pairs.
  function role(name) {
    const swap = layout() === "xbox";
    const map = { A: swap ? "back" : "confirm", B: swap ? "confirm" : "back", X: swap ? "search" : "primary", Y: swap ? "primary" : "search" };
    return map[name] || name;
  }
  function glyphs() {
    const swap = layout() === "xbox";
    body.style.setProperty("--g-confirm", `"${swap ? "B" : "A"}"`);
    body.style.setProperty("--g-back", `"${swap ? "A" : "B"}"`);
    body.style.setProperty("--g-primary", `"${swap ? "Y" : "X"}"`);
    document.querySelectorAll(".pad-layout").forEach((b) => {
      b.classList.toggle("active", b.dataset.layout === layout());
      b.setAttribute("aria-checked", b.dataset.layout === layout());
    });
  }
  document.querySelector(".pad-layouts").addEventListener("click", (e) => {
    const b = e.target.closest(".pad-layout");
    if (!b) return;
    localStorage.setItem(LAYOUT_KEY, b.dataset.layout);
    glyphs();
    toast(`${b.firstChild.textContent} layout: ${role("A") === "confirm" ? "A" : "B"} confirms`);
  });
  glyphs();

  const FOCUSABLE = 'button, a[href], a[data-url], input:not([type=hidden]), select, summary, [tabindex="0"]';
  const open = (id) => !$(id).classList.contains("hidden");
  const scope = () => (open("modal") ? $("modal") : open("onboard") ? $("onboard") : body);
  const shown = (el) => el.getClientRects().length > 0 && getComputedStyle(el).visibility !== "hidden";
  const candidates = () => [...scope().querySelectorAll(FOCUSABLE)].filter((el) => !el.disabled && el.tabIndex >= 0 && shown(el));
  const current = () => {
    const el = document.activeElement;
    return el && el !== body && scope().contains(el) && shown(el) ? el : null;
  };

  let wanted = null;
  function focus(el) {
    if (!el) return;
    wanted = el;
    el.focus({ preventScroll: true });
    el.scrollIntoView({ block: "nearest", inline: "nearest" });
    remember(el);
  }
  // The first D-pad press after touching the screen never reaches the app:
  // Android spends it leaving touch mode, and the WebView moves DOM focus to
  // the first tab (it does the same when the keyboard opens). A tab gaining
  // focus without a tap and without us asking is that: switch to pad mode
  // and put the focus where it belongs.
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
    const panel = document.querySelector(".panel.active");
    const pick = panel.querySelector(".item.selected") || panel.querySelector(".item[tabindex], .bundle .chip") || candidates().find((el) => panel.contains(el));
    focus(pick || candidates()[0]);
  }

  // Nearest focusable in a direction: the gap between edges along the axis,
  // plus a heavy penalty for being off to the side. A row and the buttons
  // inside it overlap, so between those two the centres decide instead.
  let padStarted = false;
  function move(dir) {
    // The WebView parks focus on the first tab at launch: start in the list instead.
    const cur = padStarted && current();
    padStarted = true;
    if (!cur) return focusFirst();
    const r = cur.getBoundingClientRect();
    const cx = r.left + r.width / 2, cy = r.top + r.height / 2;
    const gap = (a1, a2, b1, b2) => Math.max(0, a1 - b2, b1 - a2);
    const vertical = dir === "UP" || dir === "DOWN";
    // Up and down inside the side pane stay in it (its text is read by
    // scrolling, not by jumping to the list); Left and Back leave it.
    const pane = $("pane");
    const inPane = vertical && scope() === body && pane.contains(cur);
    let best = null, bestScore = Infinity;
    for (const el of candidates()) {
      if (el === cur || (inPane && !pane.contains(el))) continue;
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
    if (inPane) {
      // The next button is out of sight (or there is none): scroll the text
      // towards it first, a step at a time, while there is more to scroll.
      const pr = pane.getBoundingClientRect();
      const b = best?.getBoundingClientRect();
      const inView = b && b.bottom > pr.top && b.top < pr.bottom;
      const more = dir === "UP" ? pane.scrollTop > 0 : pane.scrollTop + pane.clientHeight < pane.scrollHeight - 1;
      if (!inView && more) return pane.scrollBy({ top: (dir === "UP" ? -1 : 1) * Math.round(pane.clientHeight * 0.6), behavior: "smooth" });
    }
    if (best) focus(best);
  }

  // Re-renders replace the focused row: find the same row / button again.
  let lastFocus = null;
  function remember(el) {
    const row = el.closest("[data-key], [data-id], [data-bundle]");
    lastFocus = el.id ? { id: el.id } : row ? { row: row.dataset.key || row.dataset.id || row.dataset.bundle, act: el.dataset.act || el.dataset.detail || "", self: el === row } : null;
  }
  document.addEventListener("focusin", (e) => remember(e.target));
  document.addEventListener("pointerdown", () => { wanted = null; lastPointer = Date.now(); }, true);
  function restoreFocus() {
    if (!body.classList.contains("pad-used") || current() || !lastFocus) return;
    let el = lastFocus.id && $(lastFocus.id);
    if (!el && lastFocus.row) {
      const row = [...document.querySelectorAll(".panel.active [data-key], .panel.active [data-id], .panel.active [data-bundle]")].find((r) => (r.dataset.key || r.dataset.id || r.dataset.bundle) === lastFocus.row);
      if (row) el = (!lastFocus.self && lastFocus.act && row.querySelector(`[data-act="${lastFocus.act}"], [data-detail="${lastFocus.act}"]`)) || row;
    }
    if (el && shown(el) && !el.disabled) el.focus({ preventScroll: true });
  }

  function confirm() {
    const el = current();
    if (!el) return focusFirst();
    if (el.matches("input[type=text], input[type=search], input[type=password]")) return A?.showKeyboard();
    el.click();
  }

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
    if ((tab === "browse" || tab === "start") && detailId && shown($("detail-install"))) return $("detail-install").click();
    if (tab === "start") {
      const b = current()?.closest(".bundle")?.querySelector("[data-act=bundle]:not([disabled])") || document.querySelector("#bundle-list [data-act=bundle]:not([disabled])");
      b?.click();
    }
  }

  function search() {
    const input = { installed: "installed-filter", browse: "browse-filter" }[body.dataset.tab];
    if (!input) return;
    focus($(input));
    A?.showKeyboard();
  }

  // One layer at a time: dialog, text box, details, other tab, then leave.
  function back() {
    if (open("modal")) return $("modal-no").classList.contains("hidden") ? $("modal-yes").click() : $("modal-no").click();
    if (open("onboard")) return A?.exit();
    const el = current();
    if (el?.matches("input[type=text], input[type=search], input[type=password]")) {
      el.blur();
      return focusFirst();
    }
    if (detailId) {
      closeDetail();
      return focusFirst();
    }
    if (body.dataset.tab !== "installed") {
      document.querySelector('.tab[data-tab="installed"]').click();
      return setTimeout(focusFirst, 0);
    }
    A?.exit();
  }

  const LABEL = { confirm: "Confirm", back: "Back", primary: "Main action", search: "Search", L1: "Previous tab", R1: "Next tab", START: "Check for updates" };
  window.afPad = (name) => {
    if (name !== "BACK") body.classList.add("pad-used");
    const r = role(name);
    if ($("pad-test") && body.dataset.tab === "settings") $("pad-test").textContent = `${name} \u2192 ${LABEL[r] || (name.length < 6 ? "Move " + name.toLowerCase() : "nothing")}`;
    if (["UP", "DOWN", "LEFT", "RIGHT"].includes(name)) return move(name);
    if (name === "BACK" || r === "back") return back();
    if (r === "confirm") return confirm();
    if (r === "primary") return primary();
    if (r === "search") return search();
    if (name === "L1" || name === "L2") return tabs(-1);
    if (name === "R1" || name === "R2") return tabs(1);
    if (name === "START" && body.dataset.tab === "installed" && !busy) return $("btn-check").click();
  };
  // Keyboards (and desktop preview): arrows move, Enter confirms, Escape backs out.
  document.addEventListener("keydown", (e) => {
    const typing = e.target.matches?.("input[type=text], input[type=search], input[type=password]");
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
    // app.js looks for the game itself when it comes back from the permission screen.
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
  // Coming back from the launcher: check again if the last check is stale.
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

  // Launcher shortcut (long-press the icon → "Update all"): check, then update.
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
