// UI logic. All setup knowledge lives in the Rust core; this file only
// collects symptoms and renders the suggestions it gets back.

// Inside the app we talk to Rust through Tauri. In a plain browser (the
// ui_preview example) the same commands go over HTTP instead.
const invoke = window.__TAURI__
  ? window.__TAURI__.core.invoke
  : (cmd, args = {}) =>
      fetch(`/invoke/${cmd}`, { method: "POST", body: JSON.stringify(args) }).then((r) =>
        r.ok ? r.json() : r.text().then((t) => Promise.reject(t)),
      );
const $ = (sel) => document.querySelector(sel);

const TOP_COUNT = 4;
const SEVERITY_TEXT = { slight: "Slight", moderate: "Noticeable", severe: "Really bad" };

const state = {
  catalog: null,
  car: { drivetrain: "rwd", has_aero: true },
  draft: { behavior: null, phase: null, speed: "both", severity: "moderate" },
  symptoms: [], // { symptom, label }
  tried: new Map(), // "param:direction" -> AppliedChange (without run)
  // Earlier runs: changes made and problems they fixed. Sent with every suggest call.
  history: { applied: [], fixed: [] }, // fixed: [{ symptom, label }]
  run: 1,
};

// The session survives app restarts.
const STORE_KEY = "setup-assistant-session-v1";

function save() {
  try {
    localStorage.setItem(
      STORE_KEY,
      JSON.stringify({
        car: state.car,
        symptoms: state.symptoms,
        tried: [...state.tried.entries()],
        history: state.history,
        run: state.run,
      }),
    );
  } catch (_) {}
}

function load() {
  try {
    const s = JSON.parse(localStorage.getItem(STORE_KEY) || "null");
    if (!s) return;
    state.car = s.car || state.car;
    state.symptoms = s.symptoms || [];
    state.tried = new Map(s.tried || []);
    state.history = s.history || state.history;
    state.run = s.run || 1;
  } catch (_) {}
}

function historyForCore() {
  return { applied: state.history.applied, fixed: state.history.fixed.map((f) => f.symptom) };
}

// ---------- small helpers ----------

function segmented(el, onChange) {
  el.addEventListener("click", (e) => {
    const btn = e.target.closest("button");
    if (!btn) return;
    el.querySelectorAll("button").forEach((b) => b.classList.toggle("active", b === btn));
    onChange(btn.dataset.value);
  });
}

function setSegmented(el, value) {
  el.querySelectorAll("button").forEach((b) => b.classList.toggle("active", b.dataset.value === value));
}

function currentBehavior() {
  return state.catalog.behaviors.find((b) => b.id === state.draft.behavior) || null;
}

// ---------- builder ----------

function renderBehaviors() {
  const wrap = $("#behaviors");
  wrap.innerHTML = "";
  for (const b of state.catalog.behaviors) {
    const btn = document.createElement("button");
    btn.className = "behavior" + (b.id === state.draft.behavior ? " active" : "");
    btn.innerHTML = `<span class="b-label"></span><span class="b-desc"></span>`;
    btn.querySelector(".b-label").textContent = b.label;
    btn.querySelector(".b-desc").textContent = b.description;
    btn.addEventListener("click", () => selectBehavior(b.id));
    wrap.appendChild(btn);
  }
}

function selectBehavior(id) {
  state.draft.behavior = id;
  const b = currentBehavior();
  // Pick the phase automatically when there is only one choice.
  state.draft.phase = b.phases.length === 1 ? b.phases[0] : b.phases.includes(state.draft.phase) ? state.draft.phase : null;
  renderBehaviors();
  renderSteps();
}

function renderSteps() {
  const b = currentBehavior();
  const showPhase = !!b && b.phases.length > 0;
  const showSpeed = !!b && b.speed_matters;
  $("#step-phase").hidden = !showPhase;
  $("#step-speed").hidden = !showSpeed;
  $("#severity-num").textContent = 2 + (showPhase ? 1 : 0) + (showSpeed ? 1 : 0);

  document.querySelectorAll("#corner-svg .seg").forEach((g) => {
    const p = g.dataset.phase;
    g.classList.toggle("disabled", !b || !b.phases.includes(p));
    g.classList.toggle("active", state.draft.phase === p);
  });
  document.querySelectorAll("#corner-svg .lbl[data-for]").forEach((t) => {
    t.classList.toggle("active", state.draft.phase === t.dataset.for);
  });

  const ready = !!b && (!showPhase || state.draft.phase);
  $("#add").disabled = !ready;
  $("#add").textContent = !b ? "Pick a problem" : ready ? "Add problem" : "Pick where it happens";
}

async function addSymptom() {
  const b = currentBehavior();
  if (!b) return;
  const symptom = {
    behavior: b.id,
    phase: b.phases.length ? state.draft.phase : null,
    speed: b.speed_matters ? state.draft.speed : "both",
    severity: state.draft.severity,
  };
  await addToProblems(symptom);
  // Reset the draft for the next problem.
  state.draft.behavior = null;
  state.draft.phase = null;
  renderBehaviors();
  renderSteps();
}

const sameProblem = (a, b) => a.behavior === b.behavior && a.phase === b.phase && a.speed === b.speed;

/// Adds a symptom to "Your problems". Re-adding the same problem just updates its severity.
async function addToProblems(symptom, label) {
  // A problem that was marked fixed but shows up again isn't fixed.
  state.history.fixed = state.history.fixed.filter((f) => !sameProblem(f.symptom, symptom));
  const same = state.symptoms.find((s) => sameProblem(s.symptom, symptom));
  if (same) {
    same.symptom.severity = symptom.severity;
  } else {
    label = label || (await invoke("symptom_label", { symptom }));
    state.symptoms.push({ symptom: { ...symptom }, label });
  }
  renderProblems();
  refresh();
}

function renderProblems() {
  const list = $("#problem-list");
  list.innerHTML = "";
  state.symptoms.forEach((s, i) => {
    const li = document.createElement("li");
    li.innerHTML = `<span class="sev ${s.symptom.severity}"></span><span class="p-label"></span><span class="p-sev"></span><button title="Remove">×</button>`;
    li.querySelector(".p-label").textContent = s.label;
    li.querySelector(".p-sev").textContent = SEVERITY_TEXT[s.symptom.severity];
    li.querySelector("button").addEventListener("click", () => {
      state.symptoms.splice(i, 1);
      renderProblems();
      refresh();
    });
    list.appendChild(li);
  });
  $("#problem-empty").hidden = state.symptoms.length > 0;
  $("#clear").hidden = state.symptoms.length === 0;
  renderHistory();
  save();
}

// ---------- runs and history ----------

const sign = (dir) => (dir === "increase" ? "+" : "−");

function renderHistory() {
  const h = state.history;
  $("#run-label").textContent = `Run ${state.run}`;
  $("#history").hidden = h.applied.length === 0 && h.fixed.length === 0;
  $("#next-run").textContent = state.tried.size ? `Next run · ${state.tried.size} change${state.tried.size === 1 ? "" : "s"} made →` : "Next run →";

  const fixed = $("#fixed-list");
  fixed.replaceChildren(
    ...h.fixed.map((f, i) => {
      const li = document.createElement("li");
      li.innerHTML = `<span class="tick">✓</span><span class="h-label"></span><button title="Not fixed after all">×</button>`;
      li.querySelector(".h-label").textContent = f.label;
      li.querySelector("button").addEventListener("click", () => {
        h.fixed.splice(i, 1);
        renderProblems();
        refresh();
      });
      return li;
    }),
  );
  $("#fixed-wrap").hidden = h.fixed.length === 0;

  const applied = $("#applied-list");
  applied.replaceChildren(
    ...h.applied.map((a, i) => {
      const info = state.catalog.params[a.param];
      const li = document.createElement("li");
      li.innerHTML = `<span class="run-tag"></span><span class="h-label"><b></b> <span class="h-dir"></span><span class="h-for"></span></span><button title="Remove">×</button>`;
      li.querySelector(".run-tag").textContent = `R${a.run}`;
      li.querySelector("b").textContent = info ? info.label : a.param;
      li.querySelector(".h-dir").textContent = `${sign(a.direction)}${a.clicks} · ${info ? (a.direction === "increase" ? info.increase : info.decrease) : ""}`;
      li.querySelector(".h-for").textContent = a.for_symptoms.length ? `for ${a.for_symptoms.join(", ")}` : "";
      li.querySelector("button").addEventListener("click", () => {
        h.applied.splice(i, 1);
        renderProblems();
        refresh();
      });
      return li;
    }),
  );
  $("#applied-wrap").hidden = h.applied.length === 0;
}

/// The driver made the ticked changes: they go into history, current problems
/// count as fixed (telemetry or the driver adds them back if they're not), and
/// live detection starts fresh for the changed car.
function nextRunClicked() {
  $("#toast").hidden = true;
  if (state.tried.size > 0) return nextRun();
  // Nothing ticked: most likely the driver forgot, so ask before moving on.
  const problems = state.symptoms.length + live.detected.filter((d) => !state.symptoms.some((s) => sameProblem(s.symptom, d.symptom))).length;
  $("#run-confirm-text").textContent =
    problems > 0
      ? `No changes are ticked as "Tried" yet. Tick the changes you made on the cards below, so later suggestions know about them and don't undo them.`
      : `Nothing to carry over yet: no problems and no ticked changes in this run. Add problems (or drive with Live telemetry), tick the changes you make, then start the next run.`;
  $("#run-confirm").hidden = false;
}

function toast(text, warn = false) {
  const t = $("#toast");
  t.textContent = text;
  t.className = "toast" + (warn ? " warn" : "");
  t.hidden = false;
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => (t.hidden = true), 9000);
}

async function nextRun() {
  $("#run-confirm").hidden = true;
  const changes = state.tried.size;
  for (const a of state.tried.values()) state.history.applied.push({ ...a, run: state.run });
  // Problems of the finished run: the ones in the list plus anything telemetry detected.
  const finished = [...state.symptoms];
  for (const d of live.detected) {
    if (!finished.some((s) => sameProblem(s.symptom, d.symptom))) finished.push({ symptom: d.symptom, label: d.label });
  }
  let fixedCount = 0;
  for (const s of finished) {
    if (!state.history.fixed.some((f) => sameProblem(f.symptom, s.symptom))) {
      state.history.fixed.push(s);
      fixedCount += 1;
    }
  }
  state.tried.clear();
  state.symptoms = [];
  state.run += 1;
  live.detected = [];
  live.runSeq += 1;
  await invoke("live_next_run", { run: state.run });
  live.runSeq += 1;
  renderProblems();
  refresh();
  pollLive();
  const parts = [`Run ${state.run} started`];
  parts.push(`${changes} change${changes === 1 ? "" : "s"} saved`);
  if (fixedCount) parts.push(`${fixedCount} problem${fixedCount === 1 ? "" : "s"} marked as fixed`);
  toast(`${parts.join(" · ")}. Drive a few laps: anything still wrong shows up again under Live telemetry.`);
}

/// A problem detected in this run can't be fixed: take it off the fixed list.
function unfixDetected(detected) {
  const back = state.history.fixed.filter((f) => detected.some((d) => sameProblem(d.symptom, f.symptom)));
  if (!back.length) return;
  state.history.fixed = state.history.fixed.filter((f) => !back.includes(f));
  renderProblems();
  refresh();
  toast(`Back again, no longer counted as fixed: ${back.map((b) => b.label).join(", ")}.`, true);
}

async function newSession() {
  state.tried.clear();
  state.symptoms = [];
  state.history = { applied: [], fixed: [] };
  state.run = 1;
  await invoke("live_new_session");
  renderProblems();
  refresh();
  pollLive();
}

// ---------- results ----------

async function refresh() {
  const symptoms = state.symptoms.map((s) => s.symptom);
  const suggestions = symptoms.length ? await invoke("suggest", { car: state.car, symptoms, history: historyForCore() }) : [];
  renderSuggestions(suggestions);
}

function renderSuggestions(list) {
  const has = list.length > 0;
  // When every option risks an earlier fix, the car is probably near its balance point.
  const risky = (x) => x.undoes.length > 0 || x.would_worsen.length > 0;
  $("#balance-point").hidden = !(list.length >= 2 && list.every(risky));
  $("#empty").hidden = has;
  $("#tip").hidden = !has;
  $("#result-count").textContent = has ? `${list.length} possible changes` : "";

  const top = list.slice(0, TOP_COUNT);
  const more = list.slice(TOP_COUNT);
  $("#top").replaceChildren(...top.map((s, i) => card(s, i)));
  $("#more").replaceChildren(...more.map((s, i) => card(s, i + TOP_COUNT)));
  $("#more-wrap").hidden = more.length === 0;
  $("#more-count").textContent = more.length ? `(${more.length})` : "";
}

function card(s, index) {
  const node = $("#card-tpl").content.firstElementChild.cloneNode(true);
  const info = state.catalog.params[s.param];
  const key = `${s.param}:${s.direction}`;
  const inc = s.direction === "increase";

  node.style.animationDelay = `${Math.min(index, 8) * 30}ms`;
  if (index === 0) node.classList.add("first");
  node.classList.toggle("done", state.tried.has(key));

  const badge = node.querySelector(".badge");
  badge.classList.add(s.direction);
  badge.querySelector(".arrow").textContent = inc ? "▲" : "▼";
  badge.querySelector(".clicks").textContent = `${inc ? "+" : "−"}${s.clicks} click${s.clicks === 1 ? "" : "s"}`;

  node.querySelector(".group").textContent = s.group;
  node.querySelector(".label").textContent = s.label;
  const dir = node.querySelector(".dir");
  dir.innerHTML = "<b></b>";
  dir.querySelector("b").textContent = inc ? info.increase : info.decrease;

  const reasons = node.querySelector(".reasons");
  for (const r of s.reasons) {
    const li = document.createElement("li");
    li.innerHTML = `<span class="from"></span>`;
    li.querySelector(".from").textContent = r.symptom;
    li.append(r.why);
    reasons.appendChild(li);
  }

  if (s.conflicts.length) {
    node.querySelector(".conflict-tag").hidden = false;
    const c = node.querySelector(".conflicts");
    c.hidden = false;
    const names = [...new Set(s.conflicts.map((r) => r.symptom))].join(", ");
    c.textContent = `"${names}" asks for the opposite (${inc ? info.decrease.toLowerCase() : info.increase.toLowerCase()}). Fix the worse problem first and use small steps here.`;
  }

  // What earlier runs say about this change.
  const notes = [];
  if (s.undoes.length) {
    const what = s.undoes.map((a) => `${sign(a.direction)}${a.clicks} in run ${a.run}${a.for_symptoms.length ? ` for ${a.for_symptoms.join(", ")}` : ""}`).join("; ");
    notes.push(["undo", `↩ Partly undoes your earlier change (${what}). Try the other changes first.`]);
  }
  if (s.would_worsen.length) {
    const names = [...new Set(s.would_worsen.map((r) => r.symptom))].join(", ");
    notes.push(["undo", `Could bring back a problem you fixed: ${names}.`]);
  }
  if (s.already_clicks) {
    notes.push(["info", `You already changed this ${s.already_clicks} click${s.already_clicks === 1 ? "" : "s"} the same way.`]);
  }
  const hn = node.querySelector(".history-notes");
  for (const [kind, text] of notes) {
    const p = document.createElement("p");
    p.className = kind;
    p.textContent = text;
    hn.appendChild(p);
  }
  hn.hidden = notes.length === 0;
  if (s.undoes.length || s.would_worsen.length) {
    const tag = node.querySelector(".undo-tag");
    tag.textContent = s.undoes.length ? "Reverses a change" : "Risks a fixed problem";
    tag.hidden = false;
  }
  if (s.already_clicks || s.undoes.length) {
    const tag = node.querySelector(".info-tag");
    const net = s.undoes.length ? s.undoes.reduce((n, a) => n + a.clicks, 0) : s.already_clicks;
    tag.textContent = `Changed before (${net} click${net === 1 ? "" : "s"})`;
    tag.hidden = false;
  }

  const box = node.querySelector(".tried input");
  box.checked = state.tried.has(key);
  box.addEventListener("change", () => {
    if (box.checked) {
      state.tried.set(key, {
        param: s.param,
        direction: s.direction,
        clicks: s.clicks,
        for_symptoms: [...new Set(s.reasons.map((r) => r.symptom))],
      });
    } else {
      state.tried.delete(key);
    }
    node.classList.toggle("done", box.checked);
    renderHistory();
    save();
  });
  return node;
}

// ---------- wiring ----------

async function init() {
  state.catalog = await invoke("catalog");
  load();
  setSegmented($("#drivetrain"), state.car.drivetrain);
  setSegmented($("#aero"), String(state.car.has_aero));
  renderBehaviors();
  renderSteps();
  renderProblems();

  segmented($("#drivetrain"), (v) => { state.car.drivetrain = v; save(); refresh(); });
  segmented($("#aero"), (v) => { state.car.has_aero = v === "true"; save(); refresh(); });
  $("#next-run").addEventListener("click", nextRunClicked);
  $("#run-confirm-yes").addEventListener("click", nextRun);
  $("#run-confirm-no").addEventListener("click", () => ($("#run-confirm").hidden = true));
  // The run count lives here (it survives restarts); tell the backend.
  invoke("live_set_run", { run: state.run });
  $("#new-session").addEventListener("click", newSession);
  refresh();
  segmented($("#speed"), (v) => { state.draft.speed = v; });
  segmented($("#severity"), (v) => { state.draft.severity = v; });

  document.querySelectorAll("#corner-svg .seg").forEach((g) =>
    g.addEventListener("click", () => {
      state.draft.phase = g.dataset.phase;
      renderSteps();
    }),
  );

  $("#add").addEventListener("click", addSymptom);
  $("#clear").addEventListener("click", () => {
    state.symptoms = [];
    state.tried.clear();
    renderProblems();
    refresh();
  });

  setSegmented($("#speed"), state.draft.speed);
  setSegmented($("#severity"), state.draft.severity);

  // Live telemetry
  $("#tabs").addEventListener("click", (e) => {
    const btn = e.target.closest("button");
    if (btn) showTab(btn.dataset.tab);
  });
  $("#demo-btn").addEventListener("click", async () => {
    await invoke("live_set_demo", { on: !live.demo });
    pollLive();
  });
  $("#reset-btn").addEventListener("click", async () => {
    await invoke("live_reset");
    pollLive();
  });
  $("#add-all").addEventListener("click", async () => {
    for (const d of live.detected) await addToProblems(d.symptom, d.label);
    renderDetected(live.detected);
  });

  // Overlay (desktop app only).
  if (window.__TAURI__) {
    $("#overlay-btn").addEventListener("click", async () => {
      live.overlay = !live.overlay;
      await invoke("overlay_show", { show: live.overlay });
      if (!live.overlay) live.overlayLocked = false;
      renderOverlayButtons();
    });
    $("#overlay-lock").addEventListener("click", async () => {
      live.overlayLocked = !live.overlayLocked;
      await invoke("overlay_lock", { lock: live.overlayLocked });
      renderOverlayButtons();
    });
  } else {
    $("#overlay-box").hidden = true;
  }
  livePollLoop();
}

// ---------- live telemetry ----------

const live = { tab: "describe", demo: false, detected: [], overlay: false, overlayLocked: false, runSeq: 0 };

function showTab(tab) {
  live.tab = tab;
  document.querySelectorAll("#tabs button").forEach((b) => b.classList.toggle("active", b.dataset.tab === tab));
  $("#tab-describe").hidden = tab !== "describe";
  $("#tab-live").hidden = tab !== "live";
  pollLive();
}

async function livePollLoop() {
  try {
    await pollLive();
  } finally {
    // Fast while the live tab is open, slow otherwise (only the tab dot needs it).
    setTimeout(livePollLoop, live.tab === "live" ? 150 : 1000);
  }
}

const CONN_TEXT = {
  waiting: "Waiting for RaceRoom…",
  connected: "RaceRoom connected, waiting for you to drive",
  driving: "Receiving telemetry",
};

async function pollLive() {
  const seq = live.runSeq;
  const st = await invoke("live_status");
  // A new run started while this request was out: its data belongs to the old run.
  if (seq !== live.runSeq) return;
  live.demo = st.demo;
  const r = st.report;

  const conn = $("#conn");
  conn.className = "conn " + (st.demo ? "demo" : st.connection);
  $("#conn-text").textContent = st.demo ? "Demo lap (synthetic data, 2× speed)" : CONN_TEXT[st.connection];
  $("#demo-btn").textContent = st.demo ? "Stop demo" : "Try demo lap";
  $("#tab-dot").className = "live-dot" + (st.demo ? " demo" : st.connection === "driving" ? " on" : "");

  // Needed even when the live tab is closed ("Next run" carries them over).
  live.detected = r.detected;
  unfixDetected(r.detected);

  if (live.tab !== "live") return;

  const f = r.live;
  $("#g-speed").textContent = f ? Math.round(f.speed_kmh) : "–";
  $("#g-gear").textContent = f ? (f.gear === 0 ? "N" : f.gear < 0 ? "R" : f.gear) : "–";
  $("#g-thr").style.height = `${(f ? f.throttle : 0) * 100}%`;
  $("#g-brk").style.height = `${(f ? f.brake : 0) * 100}%`;
  $("#g-phase").textContent = f && f.phase ? (f.phase === "mid" ? "mid-corner" : f.phase) : "–";
  $("#g-latg").textContent = f ? `${f.lat_g.toFixed(2)} g lateral` : "corner phase";

  // Balance: -1 = strong understeer (left), 0 = neutral, +1 = strong oversteer (right).
  const needle = $("#balance-needle");
  const b = f && f.balance;
  const pos = b == null ? 50 : 50 + Math.max(-0.96, Math.min(0.96, b)) * 50;
  needle.style.left = `${pos}%`;
  needle.classList.toggle("off", b == null);
  $("#balance-text").textContent = !r.calibrated
    ? "Balance: learning your car (needs a few gentle curves)…"
    : b == null
      ? "Balance: not cornering"
      : Math.abs(b) < 0.2
        ? "Balance: neutral"
        : b < 0
          ? "Balance: understeering"
          : "Balance: oversteering";

  const mins = Math.floor(r.driving_seconds / 60);
  const secs = Math.floor(r.driving_seconds % 60);
  $("#drive-time").textContent = r.driving_seconds > 0 ? `after ${mins}:${String(secs).padStart(2, "0")} of driving` : "";

  renderDetected(r.detected);
  renderPhaseBalance(r.balance_by_phase, st.previous_balance);
  renderTrack(st.track, st.car_at, r.detected);
  renderTyres(r.tyres);
}

// ---------- track map ----------

const SVG_NS = "http://www.w3.org/2000/svg";
function svgEl(tag, attrs, text) {
  const el = document.createElementNS(SVG_NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  if (text != null) el.textContent = text;
  return el;
}

/// Elevation profile with corners, crests, compressions, problem spots and the car.
function renderTrack(t, carAt, detected) {
  $("#track-card").hidden = !t || t.profile.length < 2;
  $("#track-empty").hidden = !!(t && t.profile.length >= 2);
  if (!t) {
    $("#track-status").textContent = "";
    return;
  }
  const pct = Math.round(t.coverage * 100);
  $("#track-status").textContent = t.ready
    ? `${t.name} · ${km(t.length_m)} · learned from ${t.laps} lap${t.laps === 1 ? "" : "s"}${t.runs ? ` · ${t.runs} earlier run${t.runs === 1 ? "" : "s"}` : ""}`
    : `${t.name} · learning the track (${pct}% of a lap seen)`;
  if (t.profile.length < 2) return;

  const W = 600, H = 160, top = 26, bottom = 128;
  const x = (d) => (d / t.length_m) * W;
  const elevs = t.profile.map((p) => p[1]);
  const lo = Math.min(...elevs), hi = Math.max(...elevs);
  const y = (e) => bottom - ((e - lo) / Math.max(hi - lo, 1)) * (bottom - top);
  const elevAt = (d) => {
    let best = t.profile[0];
    for (const p of t.profile) if (Math.abs(p[0] - d) < Math.abs(best[0] - d)) best = p;
    return best[1];
  };

  const key = JSON.stringify([t.profile.length, Math.round(lo), Math.round(hi), t.corners.map((c) => [c.number, c.name, Math.round(c.start_m)]), t.features.map((f) => [f.kind, Math.round(f.at_m)]), detected.map((d) => [d.label, d.places.map((p) => Math.round(p.distance_m))])]);
  const svg = $("#track-svg");
  if (key !== live.trackKey) {
    live.trackKey = key;
    const kids = [];
    // Corner bands with labels.
    for (const c of t.corners) {
      kids.push(svgEl("rect", { x: x(c.start_m), y: 14, width: Math.max(2, x(c.end_m) - x(c.start_m)), height: bottom - 14, class: "tk-corner" }));
      const label = svgEl("text", { x: x(c.apex_m), y: 11, class: "tk-label" }, `T${c.number}`);
      label.appendChild(svgEl("title", {}, `${c.name ? c.name + " · " : ""}min ${Math.round(c.min_speed_kmh)} km/h · ${c.peak_g.toFixed(1)} g`));
      kids.push(label);
    }
    // Elevation profile.
    const pts = t.profile.map((p) => `${x(p[0]).toFixed(1)},${y(p[1]).toFixed(1)}`);
    kids.push(svgEl("path", { d: `M${pts[0]} L${pts.join(" L")} L${x(t.profile[t.profile.length - 1][0])},${bottom} L${x(t.profile[0][0])},${bottom} Z`, class: "tk-fill" }));
    kids.push(svgEl("path", { d: `M${pts.join(" L")}`, class: "tk-line" }));
    // Crests and compressions.
    for (const f of t.features) {
      const fx = x(f.at_m), fy = y(elevAt(f.at_m));
      const comp = f.kind === "compression";
      const mark = svgEl("text", { x: fx, y: comp ? fy - 7 : fy - 7, class: comp ? "tk-comp" : "tk-crest" }, comp ? "▼" : "▲");
      mark.appendChild(svgEl("title", {}, `${comp ? "Compression" : "Crest"} at ${km(f.at_m)} · ${Math.abs(f.strength_g).toFixed(1)} g ${comp ? "extra" : "less"} load`));
      kids.push(mark);
    }
    // Problem spots along the bottom.
    for (const d of detected) {
      for (const p of d.places) {
        const dot = svgEl("circle", { cx: x(p.distance_m), cy: 142, r: Math.min(7, 3 + p.count / 2), class: `tk-prob sev-${d.symptom.severity}` });
        dot.appendChild(svgEl("title", {}, `${d.label} · ${p.label || km(p.distance_m)} · ${p.count}×`));
        kids.push(dot);
      }
    }
    kids.push(svgEl("line", { id: "tk-car", x1: -10, x2: -10, y1: 14, y2: 152, class: "tk-car" }));
    svg.replaceChildren(...kids);
    renderCornerInputs(t.corners);
  }
  const car = svg.querySelector("#tk-car");
  if (car) {
    const cx = carAt == null ? -10 : x(carAt);
    car.setAttribute("x1", cx);
    car.setAttribute("x2", cx);
  }
}

function renderCornerInputs(corners) {
  const wrap = $("#corner-inputs");
  // Don't rebuild while the driver is typing a name.
  if (wrap.contains(document.activeElement)) return;
  wrap.replaceChildren(
    ...corners.map((c) => {
      const row = document.createElement("label");
      row.innerHTML = `<span class="cn-num"></span><input type="text" placeholder="name" maxlength="40" /><span class="cn-info"></span>`;
      row.querySelector(".cn-num").textContent = `T${c.number}`;
      row.querySelector(".cn-info").textContent = `${km(c.apex_m)} · ${Math.round(c.min_speed_kmh)} km/h`;
      const input = row.querySelector("input");
      input.value = c.name || "";
      input.addEventListener("change", () => invoke("track_rename", { corner: c.number, name: input.value }));
      return row;
    }),
  );
}

function renderOverlayButtons() {
  $("#overlay-btn").textContent = live.overlay ? "Hide overlay" : "Show overlay";
  $("#overlay-lock").hidden = !live.overlay;
  $("#overlay-lock").textContent = live.overlayLocked ? "Unlock (move it)" : "Lock (click-through)";
}

const PHASE_NAMES = { braking: "Under braking", entry: "Entry", mid: "Mid-corner", exit: "Exit" };

/// Extra steering per g in each corner phase, this run vs. the previous one.
function renderPhaseBalance(now, prev) {
  const key = JSON.stringify([now.map((b) => [b.phase, b.speed, Math.round(b.deg_per_g)]), prev.length]);
  if (key === live.balanceKey) return;
  live.balanceKey = key;

  const cell = (list, phase, speed) => list.find((b) => b.phase === phase && b.speed === speed);
  const fmt = (b) => {
    const v = Math.round(b.deg_per_g);
    const kind = v > 15 ? "us" : v < -5 ? "os" : "neutral";
    return [`${v > 0 ? "+" : ""}${v}°/g`, kind];
  };
  const phases = ["entry", "mid", "exit"];
  if (now.some((b) => b.phase === "braking") || prev.some((b) => b.phase === "braking")) phases.unshift("braking");
  const rows = phases.map((phase) => {
    const tr = document.createElement("tr");
    const th = document.createElement("th");
    th.textContent = PHASE_NAMES[phase];
    if (phase === "braking") th.title = "Corners while the front tyres slip under braking. Some understeer here is normal.";
    tr.appendChild(th);
    for (const speed of ["slow", "fast"]) {
      const td = document.createElement("td");
      const b = cell(now, phase, speed);
      const p = cell(prev, phase, speed);
      if (b) {
        const [text, kind] = fmt(b);
        td.innerHTML = `<span class="pb ${kind}"></span>`;
        td.firstChild.textContent = text;
      } else {
        td.innerHTML = `<span class="pb none">–</span>`;
      }
      if (p) {
        const prevEl = document.createElement("span");
        prevEl.className = "pb-prev";
        prevEl.textContent = `was ${fmt(p)[0]}`;
        td.appendChild(prevEl);
      }
      tr.appendChild(td);
    }
    return tr;
  });
  $("#phase-balance tbody").replaceChildren(...rows);
  $("#phase-balance-empty").hidden = now.length > 0;
}

function renderDetected(list) {
  // Polling is fast: only rebuild when something visible changed.
  // (per_minute changes every frame, so it's left out; it updates with the count.)
  const key = JSON.stringify([list.map((d) => [d.label, d.count, d.symptom.severity, whereText(d)]), state.symptoms.map((s) => s.symptom)]);
  if (key === live.renderedKey) return;
  live.renderedKey = key;

  const ul = $("#detected");
  ul.replaceChildren(
    ...list.map((d) => {
      const li = document.createElement("li");
      li.innerHTML = `<span class="sev ${d.symptom.severity}"></span><span class="d-label"><span class="d-name"></span><span class="d-where"></span></span><span class="d-count"></span><button></button>`;
      li.querySelector(".d-name").textContent = d.label;
      li.querySelector(".d-where").textContent = whereText(d);
      if (d.one_place || d.persistent) li.classList.add("one-place");
      li.querySelector(".d-count").textContent = `${d.count}× · ${d.per_minute.toFixed(1)}/min`;
      const added = state.symptoms.find((s) => sameProblem(s.symptom, d.symptom));
      const btn = li.querySelector("button");
      btn.textContent = added ? (added.symptom.severity === d.symptom.severity ? "Added" : "Update") : "Add";
      btn.disabled = !!added && added.symptom.severity === d.symptom.severity;
      btn.addEventListener("click", async () => {
        await addToProblems(d.symptom, d.label);
        renderDetected(live.detected);
      });
      return li;
    }),
  );
  $("#detected-empty").hidden = list.length > 0;
  $("#add-all").hidden = list.length < 2;
}

const km = (m) => (m >= 1000 ? `${(m / 1000).toFixed(2)} km` : `${Math.round(m)} m`);

/// "mostly at 1.24 km (5x)" / "one place only: 1.24 km - maybe the track there".
function whereText(d) {
  if (!d.places || !d.places.length) return "";
  const top = d.places[0];
  const where = top.label || km(top.distance_m);
  // "at T7", but "before T4" on its own.
  const at = /^(before|after) /.test(where) ? where : `at ${where}`;
  let text;
  if (d.one_place) {
    text = top.feature
      ? `Only ${at} (${top.count} of ${d.count}): that's the track there`
      : `Only ${at} (${top.count} of ${d.count}): maybe that spot (crest, bump, kerb) rather than the setup`;
  } else if (d.places.length === 1) {
    text = at;
  } else {
    text = `${d.places.length} places · most ${at} (${top.count}×)`;
  }
  if (d.persistent) text += ` · same spot in each of your last ${d.runs_checked} runs, whatever you changed: likely the track`;
  return text;
}

function tempColor(t) {
  // 60 °C blue → 85 °C green → 110 °C red
  const hue = Math.max(0, Math.min(220, 220 - (t - 60) * 4.4));
  return `hsl(${hue} 75% 62%)`;
}

function renderTyres(tyres) {
  $("#tyres-empty").hidden = tyres.length > 0;
  const wrap = $("#tyres");
  wrap.replaceChildren(
    ...tyres.map((t) => {
      const el = document.createElement("div");
      el.className = "tyre";
      el.innerHTML = `<div class="tyre-head"><b></b><span class="p"></span></div><div class="tyre-temps"><span></span><span></span><span></span></div><ul class="tyre-hints"></ul>`;
      el.querySelector("b").textContent = t.name;
      el.querySelector(".p").textContent = t.pressure_kpa ? `${t.pressure_kpa.toFixed(0)} kPa · ${(t.pressure_kpa * 0.145038).toFixed(1)} psi` : "";
      // Show the outside edge on the outside of the car, like looking from above.
      const left = t.name.endsWith("L");
      const order = left ? [t.outer, t.middle, t.inner] : [t.inner, t.middle, t.outer];
      el.querySelectorAll(".tyre-temps span").forEach((s, i) => {
        s.textContent = Math.round(order[i]);
        s.style.background = tempColor(order[i]);
      });
      const hints = el.querySelector(".tyre-hints");
      for (const h of t.hints) {
        const li = document.createElement("li");
        li.textContent = h;
        hints.appendChild(li);
      }
      return el;
    }),
  );
}

init().catch((err) => {
  document.body.innerHTML = `<pre style="padding:24px;color:#ff8080">Failed to start: ${String(err)}</pre>`;
});
