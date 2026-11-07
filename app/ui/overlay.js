// In-game overlay: balance needle, corner phase and the latest detected event.
const invoke = window.__TAURI__
  ? window.__TAURI__.core.invoke
  : (cmd, args = {}) =>
      fetch(`/invoke/${cmd}`, { method: "POST", body: JSON.stringify(args) }).then((r) => r.json());
const $ = (id) => document.getElementById(id);

let lastSeq = null;
let hideTimer = null;

async function tick() {
  try {
    const st = await invoke("live_status");
    const f = st.report.live;
    $("dot").className = "dot " + (st.demo ? "demo" : st.connection === "driving" ? "driving" : "");
    $("phase").textContent =
      st.connection === "waiting" && !st.demo ? "Waiting for game" : f && f.phase ? (f.phase === "mid" ? "mid-corner" : f.phase) : "–";
    $("speed").textContent = f ? Math.round(f.speed_kmh) : "–";
    $("gear").textContent = f ? (f.gear === 0 ? "N" : f.gear < 0 ? "R" : f.gear) : "–";

    const b = f && f.balance;
    const needle = $("needle");
    needle.style.left = `${b == null ? 50 : 50 + Math.max(-0.96, Math.min(0.96, b)) * 50}%`;
    needle.classList.toggle("off", b == null);

    // Flash new events for a few seconds.
    const e = st.report.last_event;
    if (e && lastSeq !== null && e.seq !== lastSeq) {
      const ev = $("event");
      ev.textContent = e.label;
      ev.classList.add("show");
      clearTimeout(hideTimer);
      hideTimer = setTimeout(() => ev.classList.remove("show"), 3500);
    }
    lastSeq = e ? e.seq : 0;
  } catch (_) {
    // App is closing or not ready yet.
  }
  setTimeout(tick, 100);
}

tick();
