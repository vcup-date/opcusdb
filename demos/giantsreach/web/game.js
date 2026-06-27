"use strict";
// Giantsreach client. Talks to the authoritative server, interpolates resources
// smoothly between syncs, renders the city HUD, modals, tutorial and feedback.
const $ = (s, r = document) => r.querySelector(s);
const $$ = (s, r = document) => [...r.querySelectorAll(s)];
const el = (h) => { const d = document.createElement("div"); d.innerHTML = h.trim(); return d.firstElementChild; };
let TOKEN = localStorage.getItem("gr_token") || null;
let S = null;            // last server snapshot
let local = null;        // interpolated resources {grain,...}
let lastFrame = 0;
let modalOpen = null;

// ---- resource icons (engraving-line SVG, gold stroke) ----
const ICON = {
  grain: '<svg viewBox="0 0 24 24" fill="none" stroke="#e7c061" stroke-width="1.7"><path d="M12 22V8M12 8Q7 9 6 4Q11 5 12 8M12 8Q17 9 18 4Q13 5 12 8M12 14Q7 15 6 10M12 14Q17 15 18 10"/></svg>',
  timber: '<svg viewBox="0 0 24 24" fill="none" stroke="#e7c061" stroke-width="1.7"><ellipse cx="7" cy="8" rx="2.3" ry="3.2"/><ellipse cx="7" cy="15" rx="2.3" ry="3.2"/><path d="M7 8H17M7 15H17"/><ellipse cx="17" cy="8" rx="2.3" ry="3.2"/><ellipse cx="17" cy="15" rx="2.3" ry="3.2"/></svg>',
  stone: '<svg viewBox="0 0 24 24" fill="none" stroke="#e7c061" stroke-width="1.7"><path d="M4 9 12 5 20 9 20 16 12 20 4 16Z M4 9 12 13 20 9 M12 13V20"/></svg>',
  iron: '<svg viewBox="0 0 24 24" fill="none" stroke="#e7c061" stroke-width="1.7"><path d="M3 9H21V11Q21 16 15 16H14L13 20H11L10 16H9Q3 16 3 11Z M7 9V7H17V9"/></svg>',
  gold: '<svg viewBox="0 0 24 24" fill="none" stroke="#e7c061" stroke-width="1.7"><circle cx="12" cy="12" r="8"/><circle cx="12" cy="12" r="4"/></svg>',
  gem: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.8"><path d="M12 3 20 9 17 19 7 19 4 9Z M4 9H20 M9 9 11 19 M15 9 13 19"/></svg>',
};
// ---- UI icons (gold-stroke line icons, no emoji) ----
const SI = {
  gift: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.7"><rect x="4" y="10" width="16" height="10" rx="1"/><path d="M3 10h18M12 10v10M12 10c-2-4-7-4-6 0M12 10c2-4 7-4 6 0"/></svg>',
  sword: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.7"><path d="M14 3 21 4 20 11 9 22 7 20 5 18 16 7Z M5 18l-2 2 1 1 2-2M14 9l1 1"/></svg>',
  map: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M9 4 3 6v14l6-2 6 2 6-2V4l-6 2-6-2Z M9 4v14M15 6v14"/></svg>',
  trophy: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M7 4h10v4a5 5 0 0 1-10 0Z M7 6H4v2a3 3 0 0 0 3 3M17 6h3v2a3 3 0 0 1-3 3M9.5 13.5h5L14 18h-4Z M8 21h8"/></svg>',
  gear: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.5"><circle cx="12" cy="12" r="3.2"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M19 5l-2 2M7 17l-2 2"/></svg>',
  soundOn: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.7"><path d="M4 9v6h4l5 4V5L8 9Z M16 9.5a3 3 0 0 1 0 5M18.5 7a6 6 0 0 1 0 10"/></svg>',
  soundOff: '<svg viewBox="0 0 24 24" fill="none" stroke="#caa86a" stroke-width="1.7"><path d="M4 9v6h4l5 4V5L8 9Z M16 9.5l5 5M21 9.5l-5 5"/></svg>',
  hammer: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M13.5 6.5l4 4-2.2 2.2-4-4ZM11.3 8.7 4 16l2 2 7.3-7.3M14.5 5.5l4 4"/></svg>',
  scroll: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M6 4h11v13a3 3 0 0 0 3 3H8a3 3 0 0 1-3-3V6M6 4a2 2 0 0 0-2 2v1h3M9 9h6M9 13h5"/></svg>',
  horse: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M5 20c0-6 3-9 7-9l2-3 2 1-1 3c2 1 3 3 3 6M5 20h13M8 11 5 10l1-2"/></svg>',
  shield: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M12 3 19 6v5c0 5-3 7-7 9-4-2-7-4-7-9V6Z"/></svg>',
  home: '<svg viewBox="0 0 24 24" fill="none" stroke="#bfe39f" stroke-width="1.7"><path d="M4 11 12 4l8 7M6 10v9h12v-9"/></svg>',
  ruin: '<svg viewBox="0 0 24 24" fill="none" stroke="#caa86a" stroke-width="1.6"><path d="M5 21h14M7 21V9l3-2v14M14 21V8l4 2v11"/></svg>',
  flag: '<svg viewBox="0 0 24 24" fill="none" stroke="#bfe39f" stroke-width="1.7"><path d="M6 21V4M6 5h11l-2 3 2 3H6"/></svg>',
  plus: '<svg viewBox="0 0 24 24" fill="none" stroke="#11250a" stroke-width="3"><path d="M12 6v12M6 12h12"/></svg>',
  gem: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.7"><path d="M12 3 20 9 17 19 7 19 4 9Z M4 9H20 M9 9 11 19 M15 9 13 19"/></svg>',
  tasks: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><rect x="4" y="3" width="16" height="18" rx="2"/><path d="M7.5 8l1.4 1.4L11.5 7M7.5 14l1.4 1.4L11.5 13M14 8h3M14 14h3"/></svg>',
  anvil: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M3 8h9l3 3c2 0 4-1 5-3l1 1c-1 3-3 5-6 5v2h2v3H7v-3h2v-2.5L5 11H3ZM8 8V6h3"/></svg>',
  weapon: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M14 3 21 4 20 11 9 22 7 20 5 18 16 7Z M5 18l-2 2 1 1 2-2"/></svg>',
  armor: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M12 3 19 6v5c0 5-3 7-7 9-4-2-7-4-7-9V6Z M12 3v18"/></svg>',
  banner: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M6 21V4M6 5h11l-2 3 2 3H6"/></svg>',
  charm: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M12 3 14.5 9 21 9.5 16 13.5 17.5 20 12 16.5 6.5 20 8 13.5 3 9.5 9.5 9Z"/></svg>',
  medal: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.6"><path d="M8 3 10.5 9M16 3 13.5 9M12 9a6 6 0 1 0 0 12 6 6 0 0 0 0-12Z M12 12.5 13 14.7l2.4.2-1.8 1.6.6 2.3-2.2-1.3-2.2 1.3.6-2.3-1.8-1.6 2.4-.2Z"/></svg>',
  crown: '<svg viewBox="0 0 24 24" fill="none" stroke="#1c1206" stroke-width="1.4"><path d="M4 18h16l1-10-5 4-4-7-4 7-5-4Z M4 18v2h16v-2"/></svg>',
  pass: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.5"><path d="M5 3h11l3 3v15H5Z M16 3v3h3M8 11h8M8 15h6M8 7h4"/></svg>',
  ally: '<svg viewBox="0 0 24 24" fill="none" stroke="#f6e2a0" stroke-width="1.5"><path d="M12 3 19 5v6c0 4-3 6-7 8-4-2-7-4-7-8V5Z M12 3v16M8.5 8h7M8.5 11.5h7"/></svg>',
};
function initIcons(root) { (root || document).querySelectorAll("[data-svg]").forEach((e) => { if (e.dataset.done !== "1") { e.innerHTML = SI[e.dataset.svg] || ""; e.dataset.done = "1"; } }); }
const ic = (name) => `<span class="ri">${SI[name] || ""}</span>`;
const RESES = ["grain", "timber", "stone", "iron", "gold"];
const ROMAN = ["", "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII", "XIV", "XV", "XVI", "XVII", "XVIII", "XIX", "XX"];
const roman = (n) => ROMAN[n] || ("" + n);
const fmt = (n) => { n = Math.floor(n); if (n >= 1e6) return (n / 1e6).toFixed(2) + "M"; if (n >= 1e4) return (n / 1e3).toFixed(1) + "k"; return n.toLocaleString(); };
const hms = (s) => { s = Math.max(0, Math.floor(s)); const h = (s / 3600) | 0, m = ((s % 3600) / 60) | 0, x = s % 60; return (h ? h + ":" + String(m).padStart(2, "0") : m) + ":" + String(x).padStart(2, "0"); };

// ---- api ----
async function api(path, body) {
  const r = await fetch("/api/" + path, {
    method: body ? "POST" : "GET",
    headers: Object.assign({ "Content-Type": "application/json" }, TOKEN ? { "x-token": TOKEN } : {}),
    body: body ? JSON.stringify(body) : undefined,
  });
  const j = await r.json().catch(() => ({}));
  if (!r.ok) throw new Error(j.err || "error");
  return j;
}
function toast(msg, bad) { const t = el(`<div class="toast ${bad ? "bad" : ""}">${msg}</div>`); $("#toasts").appendChild(t); setTimeout(() => t.remove(), 2600); }
function spark(x, y, ch) { const s = el(`<div class="spark" style="left:${x}px;top:${y}px">${ch}</div>`); $("#app").appendChild(s); setTimeout(() => s.remove(), 1000); }

// ---- auth ----
let authMode = "register";
function toggleAuth() {
  authMode = authMode === "register" ? "login" : "register";
  $("#au-go").textContent = authMode === "register" ? "Found your city" : "Return to your city";
  $(".swap").innerHTML = authMode === "register"
    ? 'Already hold a city? <a id="au-toggle">Return to it</a>'
    : 'New to the realm? <a id="au-toggle">Found a city</a>';
  $("#au-toggle").onclick = toggleAuth;
}
$("#au-toggle").onclick = toggleAuth;
async function doAuth(fn) {
  $("#au-err").textContent = "";
  try {
    const r = await api(fn, { name: $("#au-name").value, pass: $("#au-pass").value });
    TOKEN = r.token; localStorage.setItem("gr_token", TOKEN); enterGame();
  } catch (e) { $("#au-err").textContent = e.message; }
}
$("#au-go").onclick = () => doAuth(authMode);
$("#au-pass").addEventListener("keydown", (e) => { if (e.key === "Enter") $("#au-go").click(); });
$("#au-guest").onclick = async () => {
  try { const r = await api("guest", {}); TOKEN = r.token; localStorage.setItem("gr_token", TOKEN); enterGame(); }
  catch (e) { $("#au-err").textContent = e.message; }
};

async function enterGame() {
  $("#auth").classList.add("hidden"); $("#game").classList.remove("hidden");
  initIcons(); updateMuteIcon();
  await sync(); setupTown(); buildHotbar(); loop(); setInterval(sync, 3500);
  maybeWelcome();
  maybeCouncil();
}
// returning-player digest: what happened while away + what awaits your word (shown once per session)
function maybeCouncil() {
  if (!S || (S.tutorial || 0) < 1) return;          // brand-new players get the welcome instead
  if (sessionStorage.getItem("gr_council")) return;  // once per browser session
  const key = "gr_lastvisit_" + (S.name || "");
  const lastVisit = +localStorage.getItem(key) || 0;
  const since = (S.reports || []).filter((r) => r.time > lastVisit && lastVisit > 0);
  // tally the battles
  let raids = 0, sieges = 0, defenses = 0, defLost = 0;
  since.forEach((r) => { if (r.kind === "defense") { defenses++; if (!r.win) defLost++; } else if (r.kind === "city") sieges++; else if (!r.kind || r.kind === "camp") raids++; });
  const today = Math.floor(S.now / 86400);
  const claim = [];
  if (!S.login || S.login.claimed !== today) claim.push(["gift", "Daily tribute", openDaily]);
  if (S.tasks && S.tasks.chests.some((c) => c.ready && !c.claimed)) claim.push(["tasks", "Task reward chests", openTasks]);
  if (S.chest && S.chest.ready) claim.push(["gift", "The free chest", openTasks]);
  if (S.vip && S.vip.dailyReady) claim.push(["crown", "VIP audience", openVip]);
  if (S.season && S.season.claimable) claim.push(["pass", "Season pass rewards", openSeason]);
  const wounded = Object.values(S.wounded || {}).reduce((a, c) => a + c, 0);
  if (wounded > 0) claim.push(["shield", fmt(wounded) + " wounded to tend", openArmy]);
  if (S.alliance && S.alliance.members.some((m) => (m.orders || []).some((o) => !o.helpedByYou && !o.maxed))) claim.push(["ally", "Banner aid to give", openAlliance]);
  localStorage.setItem(key, "" + Math.floor(S.now));
  if (!since.length && !claim.length) return;        // nothing worth interrupting for
  sessionStorage.setItem("gr_council", "1");
  const battleLines = [];
  if (raids) battleLines.push(`${raids} raid${raids > 1 ? "s" : ""} on the camps resolved`);
  if (sieges) battleLines.push(`${sieges} march${sieges > 1 ? "es" : ""} on rival holds returned`);
  if (defenses) battleLines.push(`your hold was assailed ${defenses} time${defenses > 1 ? "s" : ""}${defLost ? " (" + defLost + " broke through)" : " (all thrown back)"}`);
  const wereAway = since.length ? `<div class="cwsec"><div class="cwh">While you were away</div>${battleLines.map((l) => `<div class="cwline">${ic("sword")}<span>${l}</span></div>`).join("")}<button class="cmore" id="c-reports">See the battle reports</button></div>` : "";
  const claimRows = claim.length ? `<div class="cwsec"><div class="cwh">Awaiting your word</div>${claim.map(([icn, label], i) => `<div class="crow" data-ci="${i}">${ic(icn)}<span>${esc(label)}</span><span class="cgo">&#8594;</span></div>`).join("")}</div>` : "";
  showModal(`<div class="ph">${ic("scroll")} The Council &middot; ${esc(S.name)} <span class="x">&times;</span></div>
    <div class="bd council">
      <p class="csub">Welcome back, my lord. Here is the realm's account.</p>
      ${wereAway}${claimRows}
      <div class="modal-actions"><button class="gbtn grn" id="c-done">To your hold</button></div>
    </div>`);
  modalOpen = null;
  $("#c-done").onclick = closeModal;
  const rb = $("#c-reports"); if (rb) rb.onclick = () => { closeModal(); openMap(); };
  $$("#modal .crow").forEach((row) => row.onclick = () => { const fn = claim[+row.dataset.ci][2]; closeModal(); fn(); });
}
// first-session onboarding: a warm steward welcome, then the coached objective bubbles take over
const LORD_NAMES = ["The Veteran", "The Young Lord", "The Lady Commander", "The Old King"];
function portraitTiles(sel) {
  return [0, 1, 2, 3].map((i) => `<div class="ptile ${i === sel ? "sel" : ""}" data-por="${i}"><img src="img/lord/lord${i}.png" alt="${LORD_NAMES[i]}"/><div class="pnm">${LORD_NAMES[i]}</div></div>`).join("");
}
function maybeWelcome() {
  if (!S || (S.tutorial || 0) >= 1) return;
  showModal(`<div class="ph">${ic("scroll")} A Steward's Welcome <span class="x">&times;</span></div>
    <div class="bd welcome">
      <h2 class="wh">Welcome, my lord ${esc(S.name)}.</h2>
      <p>This is your hold, raised among the bones of the fallen giants. Their carved heads break the far hills like buried moons, and the world they once carried is ours to keep now.</p>
      <p style="margin-bottom:8px">First, my lord, show the realm your face.</p>
      <div class="pgrid" id="w-portraits">${portraitTiles(S.portrait || 0)}</div>
      <div class="modal-actions" style="margin-top:8px"><button class="gbtn grn" id="w-begin">Take up the banner</button></div>
    </div>`);
  modalOpen = null;
  bindPortraitTiles();
  const done = async () => { closeModal(); try { const v = await api("tutorial", { step: 1 }); S.tutorial = v.tutorial; renderObjective(); } catch (e) {} };
  $("#w-begin").onclick = done;
  const x = $("#modal .x"); if (x) x.onclick = done;
}
function bindPortraitTiles() {
  $$("#modal .ptile").forEach((t) => t.onclick = async () => {
    const i = +t.dataset.por; $$("#modal .ptile").forEach((o) => o.classList.toggle("sel", o === t));
    try { const v = await api("portrait", { i }); S.portrait = v.portrait; renderTop(); sfx("click"); } catch (e) {}
  });
}
// change your likeness later from the avatar
$("#t-por-wrap").onclick = () => {
  showModal(`<div class="ph">${ic("shield")} Your Likeness <span class="x">&times;</span></div>
    <div class="bd"><p style="color:#caa86a;text-align:center;margin-bottom:12px">Choose the face the realm will know you by.</p>
    <div class="pgrid">${portraitTiles(S.portrait || 0)}</div></div>`);
  modalOpen = null; bindPortraitTiles();
};

// ---- sync + interpolation ----
async function sync() {
  try { const v = await api("state"); applyState(v); }
  catch (e) { if (("" + e.message).includes("auth")) { localStorage.removeItem("gr_token"); location.reload(); } }
}
function applyState(v) {
  S = v;
  // reconcile local resource estimate with server truth (snap, but keep smooth)
  if (!local) local = Object.assign({}, v.res);
  for (const k of RESES) {
    const diff = v.res[k] - (local[k] || 0);
    if (Math.abs(diff) > Math.max(50, v.rate[k] * 6)) local[k] = v.res[k]; // big drift (spend/build) -> snap
  }
  renderTop(); renderHot(); renderQueue(); renderTrain(); renderMarches(); renderTownSpots(); renderObjective(); setCityTier();
  $("#rl-daily-bdg").classList.toggle("hidden", v.login && v.login.claimed === Math.floor(v.now / 86400));
  const taskClaim = (v.tasks && v.tasks.chests.some((c) => c.ready && !c.claimed)) || (v.chest && v.chest.ready);
  const tb = $("#rl-tasks-bdg"); if (tb) tb.classList.toggle("hidden", !taskClaim);
  const hb = $("#rl-honors-bdg"); if (hb) hb.classList.toggle("hidden", !v.achvClaim);
  if (v.vip) { const vl = $("#vip-lv"); if (vl) vl.textContent = v.vip.level; const vb = $("#vip-bdg"); if (vb) vb.classList.toggle("hidden", !v.vip.dailyReady); }
  renderSeasonBar(); renderIncoming(v);
  const drb = $("#draw-bdg"); if (drb) drb.classList.toggle("hidden", !(v.season && v.season.claimable));
  const canHelp = !!(v.alliance && v.alliance.members.some((m) => (m.orders || []).some((o) => !o.helpedByYou && !o.maxed)));
  const ab = $("#rl-ally-bdg"); if (ab) ab.classList.toggle("hidden", !canHelp);
  const woundedN = Object.values(v.wounded || {}).reduce((a, c) => a + c, 0);
  const wb = $("#rl-army-bdg"); if (wb) wb.classList.toggle("hidden", woundedN <= 0);
  // a freshly resolved battle: play the cinematic for your own attacks, toast incoming raids
  if (v.reports && v.reports.length) {
    const r = v.reports[0];
    if (lastReport && r.time > lastReport) {
      if (r.kind === "defense") {
        const loot = Object.entries(r.looted || {}).filter(([k, x]) => x).map(([k, x]) => fmt(x) + " " + k).join(", ");
        sfx(r.win ? "victory" : "defeat");
        toast(r.win ? (esc(r.attacker) + "'s raid was thrown back") : (esc(r.attacker) + " raided you" + (loot ? ", taking " + loot : "")), !r.win);
      } else {
        playBattle(r);
      }
    }
    lastReport = r.time;
  }
  if (modalOpen) refreshModal();
}
let lastReport = 0;
// incoming enemy attacks: warn the lord with a pulsing banner + sound on a fresh threat
let incomingAlerted = new Set();
function renderIncoming(v) {
  const box = $("#incoming"); if (!box) return;
  const inc = v.incoming || []; const keys = new Set(inc.map((i) => i.from + "|" + Math.round(i.arrive)));
  for (const i of inc) { const k = i.from + "|" + Math.round(i.arrive); if (!incomingAlerted.has(k)) { incomingAlerted.add(k); sfx("defeat"); toast("A host marches on you! " + esc(i.from) + " approaches.", true); } }
  for (const k of [...incomingAlerted]) if (!keys.has(k)) incomingAlerted.delete(k);
  if (!inc.length) { box.classList.add("hidden"); box.innerHTML = ""; return; }
  const soon = inc[0];
  box.classList.remove("hidden");
  box.innerHTML = `<span class="incico">${ic("sword")}</span><div class="inctxt"><div class="inct1">A host marches on you</div>
    <div class="inct2"><b>${esc(soon.from)}</b> &middot; ~${fmt(soon.total)} strong &middot; <span class="inccd" data-arr="${soon.arrive}">--</span>${inc.length > 1 ? ` &middot; +${inc.length - 1} more` : ""}</div></div>`;
  box.onclick = openMap;
}

// ---- the battle cinematic: a fighting scene with win/lose, casualties and spoils ----
let battleBusy = false;
function playBattle(r) {
  if (battleBusy) return; battleBusy = true;
  const foe = r.kind === "city" ? esc(r.target) : ("a level " + r.level + " camp");
  const sentN = Object.values(r.sent || {}).reduce((a, c) => a + c, 0);
  const woundedN = Object.values(r.wounded || {}).reduce((a, c) => a + c, 0);
  const lossPct = Math.round((r.attLoss || 0) * 100);
  const lootRows = Object.entries(r.loot || {}).filter(([k, x]) => x).map(([k, x]) => `<span class="bl"><span class="ic">${ICON[k]}</span>+${fmt(x)}</span>`).join("");
  const ov = el(`<div id="battle" class="phase-in">
    <div class="bscene">
      <div class="bvs">
        <div class="bside me"><div class="bbnr">${ic("flag")}</div><div class="bn">Your host</div><div class="bc">${fmt(sentN)}</div></div>
        <div class="bclash"><div class="spark s1"></div><div class="spark s2"></div><div class="cross">${ic("sword")}</div></div>
        <div class="bside foe"><div class="bbnr">${ic(r.kind === "city" ? "flag" : "ruin")}</div><div class="bn">${foe}</div><div class="bc">&nbsp;</div></div>
      </div>
      <div class="bseal ${r.win ? "win" : "loss"}"><span>${r.win ? "VICTORY" : "DEFEAT"}</span></div>
      <div class="baftermath">
        ${r.flavor ? `<div class="bflav">&ldquo;${esc(r.flavor)}&rdquo;</div>` : ""}
        <div class="bstats"><span class="bs">Host lost <b>${lossPct}%</b></span>${woundedN ? `<span class="bs">${ic("shield")} <b>${fmt(woundedN)}</b> wounded</span>` : ""}${r.win && lootRows ? `<span class="bs">Spoils ${lootRows}</span>` : ""}</div>
        <button class="gbtn ${r.win ? "grn" : "ox"}" id="b-done">${r.win ? "To the spoils" : "Onward"}</button>
      </div>
    </div></div>`);
  $("#app").appendChild(ov);
  initIcons(ov);
  if (window.GA) GA.cue(); // swap to the dramatic battle cue
  const finish = () => { if (window.GA) GA.cueStop(); ov.remove(); battleBusy = false; };
  // phase timeline: march in -> clash (shake) -> seal stamp -> aftermath
  setTimeout(() => ov.classList.add("clash"), 350);
  setTimeout(() => { ov.classList.add("sealed"); sfx(r.win ? "victory" : "defeat"); }, 1250);
  setTimeout(() => ov.classList.add("show-after"), 1700);
  const skip = (e) => { if (e.target.closest("#b-done") || e.target.closest(".baftermath")) return; ov.classList.add("clash", "sealed", "show-after"); };
  ov.addEventListener("click", skip);
  ov.querySelector("#b-done").onclick = finish;
}

// ---- main loop (smooth resource ticking + countdowns) ----
function loop(t) {
  requestAnimationFrame(loop);
  if (!S) return;
  const dt = lastFrame ? (performance.now() - lastFrame) / 1000 : 0; lastFrame = performance.now();
  for (const k of RESES) { local[k] = (local[k] || 0) + (S.rate[k] || 0) * dt; if (k !== "gold" && local[k] > S.cap) local[k] = S.cap; }
  // resource bar values
  $$("#res .rp").forEach((rp) => { const k = rp.dataset.k; if (k && k !== "gem") rp.querySelector(".v").textContent = fmt(local[k]); });
  // countdown timers
  const now = Date.now() / 1000 + (S.now - (S._recv || S.now));
  $$("#queue .qrow").forEach((row, i) => {
    const q = S.queue[i]; if (!q) return; const rem = q.finish - now;
    const bar = row.querySelector(".pbar i"); if (bar) bar.style.width = Math.min(100, 100 * (1 - rem / q.total)) + "%";
    const tEl = row.querySelector(".qt .left"); if (tEl) tEl.textContent = rem <= 0 ? "done" : hms(rem);
    const sp = row.querySelector(".qt .sp"); if (sp) { const free = rem <= 300; sp.classList.toggle("free", free); sp.textContent = free ? "finish free" : (gemsFor(rem) + " shards"); }
  });
  $$("#trainq .qrow").forEach((row, i) => { const tr = S.train[i]; if (!tr) return; const rem = tr.finish - now; const tEl = row.querySelector(".left"); if (tEl) tEl.textContent = rem <= 0 ? "done" : hms(rem); });
  $$("#marchpanel .mleft").forEach((e) => { const m = S.marches[+e.dataset.mi]; if (!m) return; const rem = (m.resolved ? m.ret : m.arrive) - now; e.textContent = rem <= 0 ? "arriving" : hms(rem); });
  $$("#townspots .tmr").forEach((e) => { const q = S.queue[+e.dataset.q]; if (!q) { e.textContent = ""; return; } const rem = q.finish - now; e.textContent = rem <= 0 ? "done" : hms(rem); });
  updateMapMarches(now);
  const cd = $("#incoming .inccd"); if (cd) { const rem = +cd.dataset.arr - now; cd.textContent = rem <= 0 ? "arriving" : hms(rem); }
}
function gemsFor(sec) { if (sec <= 60) return 1; if (sec <= 3600) return Math.max(1, Math.round((19 / 3540) * (sec - 60) + 1)); if (sec <= 86400) return Math.round((240 / 82800) * (sec - 3600) + 20); return Math.round((740 / 518400) * (sec - 86400) + 260); }

// ---- render: top bar ----
function renderTop() {
  $("#t-name").textContent = S.name; $("#t-might").textContent = fmt(S.might);
  $("#t-keep").textContent = roman((S.buildings.find((b) => b.id === "keep") || {}).level || 1);
  const por = $("#t-por"); if (por) { const src = "img/lord/lord" + (S.portrait || 0) + ".png"; if (por.getAttribute("src") !== src) { por.style.display = ""; por.src = src; } }
  const res = $("#res"); res.innerHTML = "";
  for (const k of RESES) {
    res.appendChild(el(`<div class="rp ${k === "gold" ? "gild" : ""}" data-k="${k}"><div class="ic">${ICON[k]}</div><div><div class="v">${fmt(local[k])}</div><div class="r">+${fmt(S.rate[k] * 3600)}/h</div></div></div>`));
  }
  const gem = el(`<div class="rp gem gild" data-k="gem"><div class="ic">${ICON.gem}</div><div><div class="v">${fmt(S.gems)}</div><div class="r" style="color:#d9b86a">shards</div></div><div class="plus">+</div></div>`);
  gem.onclick = openShop; res.appendChild(gem);
}

// ---- hotbar ----
function buildHotbar() { renderHot(); }
function renderHot() {
  if (!S) return; const hot = $("#hot"); const open = hot.childElementCount;
  hot.innerHTML = "";
  for (const b of S.buildings) {
    const inQ = S.queue.some((q) => q.b === b.id);
    const can = canAfford(b.cost) && b.level < b.max && !inQ && (b.id === "keep" || b.level + 1 <= keepLv()) && S.queue.length < S.buildSlots;
    const slot = el(`<div class="slot ${can ? "canup" : ""}" data-b="${b.id}">
      <div class="lv num">${b.level || 0}</div>${can ? '<div class="up">↑</div>' : ""}
      <div class="pl"><img src="img/ico/${b.icon}.png" onerror="this.style.opacity=0"/></div>
      <div class="nm">${b.name.replace("The ", "")}</div></div>`);
    slot.onclick = () => openBuilding(b.id);
    hot.appendChild(slot);
  }
}
const keepLv = () => (S.buildings.find((b) => b.id === "keep") || {}).level || 1;
// the home city grows grander as the Keep rises (img2img tiers that preserve the layout, so the markers still align)
let cityTier = 0;
function setCityTier() {
  const lv = keepLv(); const tier = lv >= 11 ? 3 : lv >= 5 ? 2 : 1;
  if (tier === cityTier) return; cityTier = tier;
  const inner = $("#worldinner"); if (!inner) return;
  const img = tier === 3 ? "city3.png" : tier === 2 ? "city2.png" : "city.png";
  inner.style.backgroundImage = `url('img/${img}')`;
}
function canAfford(cost) { for (const k in cost) if ((local[k] || 0) < cost[k]) return false; return true; }

// ---- the interactive town: clickable building markers + pan/zoom ----
const TOWN_SPOTS = { keep: [50, 37], market: [50, 57], barracks: [37, 55], granary: [62, 49], sawmill: [25, 47], quarry: [31, 64], mine: [69, 62], wall: [51, 80], watchtower: [73, 41] };
let townV = { z: 1, px: 0, py: 0, _init: false }; let townDragged = false;
function renderTownSpots() {
  const host = $("#townspots"); if (!host || !S) return; host.innerHTML = "";
  for (const b of S.buildings) {
    const pos = TOWN_SPOTS[b.id]; if (!pos) continue;
    const qi = S.queue.findIndex((q) => q.b === b.id);
    const can = canAfford(b.cost) && b.level < b.max && qi < 0 && (b.id === "keep" || b.level + 1 <= keepLv()) && S.queue.length < S.buildSlots;
    const sp = el(`<div class="tspot ${can ? "canup" : ""}" style="left:${pos[0]}%;top:${pos[1]}%">
      <div class="chip">${b.name.replace("The ", "")} <span class="lv">${roman(b.level || 0)}</span>${can ? ' <span class="up">&#9650;</span>' : ""}</div>
      ${qi >= 0 ? `<div class="tmr" data-q="${qi}">--</div>` : ""}</div>`);
    sp.onclick = (e) => { e.stopPropagation(); if (townDragged) return; openBuilding(b.id); };
    host.appendChild(sp);
  }
}
function setupTown() {
  const wrap = $("#worldwrap"), inner = $("#worldinner"); if (!wrap || !inner) return;
  const IW = 1536, IH = 896;
  const minZoom = () => Math.max(wrap.clientWidth / IW, wrap.clientHeight / IH);
  const clamp = () => { townV.px = Math.min(0, Math.max(wrap.clientWidth - IW * townV.z, townV.px)); townV.py = Math.min(0, Math.max(wrap.clientHeight - IH * townV.z, townV.py)); };
  const center = () => { townV.z = minZoom() * 1.04; townV.px = (wrap.clientWidth - IW * townV.z) / 2; townV.py = (wrap.clientHeight - IH * townV.z) / 2; };
  const apply = () => { clamp(); inner.style.transform = `translate(${townV.px}px,${townV.py}px) scale(${townV.z})`; };
  const zoomAt = (ox, oy, f) => { const nz = Math.max(minZoom(), Math.min(minZoom() * 2.6, townV.z * f)); const r = nz / townV.z; townV.px = ox - (ox - townV.px) * r; townV.py = oy - (oy - townV.py) * r; townV.z = nz; apply(); };
  if (!townV._init) { center(); townV._init = true; }
  apply();
  if (wrap._bound) return; wrap._bound = true;
  let drag = null;
  wrap.addEventListener("pointerdown", (e) => { if (e.target.closest(".tspot")) return; townDragged = false; drag = { x: e.clientX, y: e.clientY, px: townV.px, py: townV.py }; wrap.classList.add("drag"); try { wrap.setPointerCapture(e.pointerId); } catch (x) {} });
  wrap.addEventListener("pointermove", (e) => { if (!drag) return; const dx = e.clientX - drag.x, dy = e.clientY - drag.y; if (Math.abs(dx) + Math.abs(dy) > 5) townDragged = true; townV.px = drag.px + dx; townV.py = drag.py + dy; apply(); });
  wrap.addEventListener("pointerup", () => { drag = null; wrap.classList.remove("drag"); });
  wrap.addEventListener("pointercancel", () => { drag = null; wrap.classList.remove("drag"); });
  wrap.addEventListener("wheel", (e) => { e.preventDefault(); zoomAt(e.offsetX, e.offsetY, e.deltaY < 0 ? 1.12 : 0.89); }, { passive: false });
  window.addEventListener("resize", () => { center(); apply(); });
}

// ---- construction & training queues ----
function renderQueue() {
  const q = $("#queue"); q.innerHTML = "";
  if (!S.queue.length) { q.appendChild(el(`<div class="empty">No work underway. Choose a building to raise.</div>`)); return; }
  S.queue.forEach((it, i) => {
    const row = el(`<div class="qrow">
      <div class="qem"><img src="img/ico/${it.icon}.png" onerror="this.style.opacity=0"/></div>
      <div class="qmid"><div class="qnm"><span>${it.name}</span><span class="lv">to ${roman(it.to)}</span></div>
        <div class="pbar"><i></i></div>
        <div class="qt"><span class="left">--</span><span class="sp">rush</span></div></div></div>`);
    row.querySelector(".sp").onclick = () => speedup(i);
    q.appendChild(row);
  });
}
function renderTrain() {
  const q = $("#trainq"); q.innerHTML = "";
  if (!S.train.length) { q.appendChild(el(`<div class="empty">No soldiers drilling. Open the Army to train.</div>`)); return; }
  S.train.forEach((tr) => {
    q.appendChild(el(`<div class="qrow"><div class="qem">${ic("sword")}</div>
      <div class="qmid"><div class="qnm"><span>${tr.name}</span><span class="lv">${tr.done}/${tr.n}</span></div>
      <div class="qt"><span class="left">--</span><span>training</span></div></div></div>`));
  });
}
async function speedup(i) {
  try { const v = await api("speedup", { i }); applyState(v); sfx("done"); toast("Construction hastened"); }
  catch (e) { toast(e.message, true); }
}

// ---- objective tracker (the "do this next" pointer) ----
function objective() {
  const b = (id) => S.buildings.find((x) => x.id === id) || {};
  if (b("keep").level < 2) return { t: "Raise the Keep", d: "Tap the Keep below and raise it to level II. It speeds every other build.", sel: '.slot[data-b="keep"]' };
  if (b("granary").level < 2) return { t: "Grow more grain", d: "Raise your Granary so your stores fill faster.", sel: '.slot[data-b="granary"]' };
  if (!S.login || S.login.claimed !== Math.floor(S.now / 86400)) return { t: "Claim your daily gift", d: "Open the Daily chest on the left for free shards.", sel: "#rl-daily" };
  const troops = Object.values(S.troops || {}).reduce((a, c) => a + c, 0);
  if (b("barracks").level < 1) return { t: "Build a Barracks", d: "You will need soldiers. Raise a Barracks.", sel: '.slot[data-b="barracks"]' };
  if (troops < 10) return { t: "Train your first host", d: "Open the Army and train at least 10 soldiers.", sel: "#rl-army" };
  if (b("keep").level < 5) return { t: "Raise the Keep to V", d: "A greater Keep unlocks higher buildings and more might.", sel: '.slot[data-b="keep"]' };
  if (b("market").level < 1) return { t: "Open a Market", d: "Build a Market for a steady flow of gold.", sel: '.slot[data-b="market"]' };
  if (b("wall").level < 3) return { t: "Raise your Wall", d: "Stone ramparts multiply your defense. Raise the Wall to III.", sel: '.slot[data-b="wall"]' };
  return { t: "Grow your might", d: "Keep raising buildings and training your host. Climb the realm ladder.", sel: "#ic-board" };
}
function renderObjective() {
  const o = objective();
  $("#obj-title").textContent = o.t; $("#obj-desc").textContent = o.d;
  // tutorial spotlight + coach bubble only during early game (and not while a modal or the welcome is up)
  const tut = $("#tutorial"); const early = (keepLv() < 5 || (S.tutorial || 0) < 6) && (S.tutorial || 0) >= 1;
  const target = o.sel && $(o.sel);
  if (early && target && !modalOpen && $("#modal").classList.contains("hidden")) {
    tut.classList.remove("hidden");
    const r = target.getBoundingClientRect();
    const vw = innerWidth, vh = innerHeight, bw = Math.min(260, vw - 24);
    // place the bubble on the roomy side of the highlighted element, clamped to the viewport
    let left, top, arrow;
    if (r.left > vw * 0.55) { left = r.left - bw - 16; top = r.top; arrow = "r"; }        // element on the right -> bubble left
    else if (r.right < vw * 0.45) { left = r.right + 16; top = r.top; arrow = "l"; }       // element on the left -> bubble right
    else if (r.top > vh * 0.5) { left = r.left + r.width / 2 - bw / 2; top = r.top - 120; arrow = "d"; } // bottom -> bubble above
    else { left = r.left + r.width / 2 - bw / 2; top = r.bottom + 14; arrow = "u"; }
    left = Math.max(12, Math.min(vw - bw - 12, left)); top = Math.max(70, Math.min(vh - 150, top));
    tut.innerHTML = `<div class="ring" style="left:${r.left - 6}px;top:${r.top - 6}px;width:${r.width + 12}px;height:${r.height + 12}px"></div>
      <div class="bubble a-${arrow}" style="left:${left}px;top:${top}px;width:${bw}px"><h4>${esc(o.t)}</h4><p>${esc(o.d)}</p></div>`;
  } else tut.classList.add("hidden");
}

// ---- modal framework ----
function showModal(html) { const m = $("#modal"); m.classList.remove("hidden"); m.innerHTML = `<div class="sheet panel">${html}</div>`; const x = m.querySelector(".x"); if (x) x.onclick = closeModal; m.onclick = (e) => { if (e.target === m) closeModal(); }; }
function closeModal() { $("#modal").classList.add("hidden"); $("#modal").innerHTML = ""; modalOpen = null; }
function refreshModal() { if (modalOpen) modalOpen(); }

// building upgrade modal
function openBuilding(id) { modalOpen = () => renderBuilding(id); modalOpen(); }
function renderBuilding(id) {
  const b = S.buildings.find((x) => x.id === id); if (!b) return;
  const inQ = S.queue.some((q) => q.b === id);
  const locked = id !== "keep" && b.level + 1 > keepLv();
  const max = b.level >= b.max;
  const costHtml = RESES.filter((k) => b.cost[k]).map((k) => `<div class="cost ${(local[k] < b.cost[k]) ? "bad" : ""}">${ICON[k]}${fmt(b.cost[k])}</div>`).join("");
  const prodLine = b.prod ? `<div class="statline"><span class="k">Production</span><span class="v">${fmt(b.prodNow * 1)}/h <span class="up">&#8594; ${fmt(b.prodNext)}/h</span></span></div>` : "";
  const qi = S.queue.findIndex((q) => q.b === id);
  let btn;
  if (max) btn = `<button class="gbtn" disabled>Maximum level</button>`;
  else if (inQ) {
    const q = S.queue[qi]; const now = Date.now() / 1000 + (S.now - (S._recv || S.now)); const rem = q ? q.finish - now : 0;
    const free = rem <= 300;
    btn = `<button class="gbtn ${free ? "grn" : ""}" id="do-finish">${free ? "Finish free" : "Hasten &middot; " + ic("gem") + " " + gemsFor(rem)}</button>`;
  }
  else if (locked) btn = `<button class="gbtn" disabled>Raise the Keep to level ${roman(b.level + 1)} first</button>`;
  else if (S.queue.length >= S.buildSlots) btn = `<button class="gbtn" disabled>Build queues are full</button>`;
  else if (!canAfford(b.cost)) btn = `<button class="gbtn" disabled>Not enough resources</button>`;
  else btn = `<button class="gbtn grn" id="do-build">${b.level ? "Upgrade to " + roman(b.level + 1) : "Build"}</button>`;
  const lvTag = b.level < b.max ? `Level ${roman(b.level)} <span class="to">&#8594; ${roman(b.level + 1)}</span>` : `Level ${roman(b.level)} <span class="to">(max)</span>`;
  showModal(`
    <div class="ph">${b.name} <span class="x">&times;</span></div>
    <div class="bd bldbd">
      <div class="bportrait">
        <img src="img/bld/${b.icon}.png" alt="" onerror="this.onerror=null;this.src='img/ico/${b.icon}.png';this.parentElement.classList.add('noart')"/>
        <div class="bpover"><div class="bplv">${lvTag}</div></div>
      </div>
      <p class="bdesc">${b.desc}</p>
      ${prodLine}
      <div class="statline"><span class="k">Build time</span><span class="v">${hms(b.time)}</span></div>
      <div class="costrow">${costHtml || '<span class="k">free</span>'}</div>
      <div class="modal-actions">${btn}</div>
    </div>`);
  const db = $("#do-build"); if (db) db.onclick = async () => { try { const v = await api("build", { b: id }); applyState(v); sfx("build"); toast(b.name + " raising"); closeModal(); } catch (e) { toast(e.message, true); } };
  const fb = $("#do-finish"); if (fb) fb.onclick = async () => { const j = S.queue.findIndex((q) => q.b === id); if (j < 0) return; try { const v = await api("speedup", { i: j }); applyState(v); sfx("done"); toast(b.name + " hastened"); closeModal(); } catch (e) { toast(e.message, true); } };
}

// shop modal (simulated purchases -> grant shards)
function openShop() { modalOpen = renderShop; renderShop(); }
function renderShop() {
  const packs = S.packs.map((p, i) => `<div class="pack ${i === 3 ? "feat" : ""}"><div class="g">${ic("gem")} ${fmt(p.gems)}</div><div class="lab">${p.label}</div><button class="gbtn grn" data-pack="${p.id}">${p.price}</button></div>`).join("");
  const starter = S.boughtStarter ? "" : `<div class="pack feat" style="grid-column:1/-1;display:flex;align-items:center;gap:14px;text-align:left"><div class="g">${ic("gift")}</div><div style="flex:1"><div class="un cin" style="color:var(--gold2);font-weight:700">${S.starter.label}</div><div class="lab" style="margin:0">${fmt(S.starter.gems)} shards + a wagon of resources, once only</div></div><button class="gbtn ox" data-pack="starter">${S.starter.price}</button></div>`;
  showModal(`<div class="ph">${ic("gem")} The Vault of Shards <span class="x">&times;</span></div>
    <div class="bd"><p style="color:#caa86a;font-size:13px;margin-bottom:12px;text-align:center">Purchases are free in this realm. Tap a pack and the shards are yours.</p>
    <div class="grid">${starter}${packs}</div></div>`);
  $$("#modal [data-pack]").forEach((b) => b.onclick = async () => {
    try { const v = await api("buygems", { pack: b.dataset.pack }); applyState(v); sfx("coin"); toast("+" + fmt(v.bought) + " shards"); renderShop(); }
    catch (e) { toast(e.message, true); }
  });
}

// daily reward modal
$("#rl-daily").onclick = openDaily;
function openDaily() {
  modalOpen = renderDaily; renderDaily();
}
function renderDaily() {
  const today = Math.floor(S.now / 86400); const claimedToday = S.login && S.login.claimed === today;
  const streak = (S.login && S.login.streak) || 0; const curIdx = claimedToday ? ((streak - 1) % 7) : (streak % 7);
  const cells = S.daily.map((d, i) => {
    const done = i < (claimedToday ? ((streak - 1) % 7) + 1 : (streak % 7));
    return `<div class="dcell ${i === curIdx && !claimedToday ? "cur" : ""} ${done ? "done" : ""}"><div class="d">Day ${i + 1}</div><div class="g">${ic("gem")}${d.gems}</div></div>`;
  }).join("");
  const btn = claimedToday ? `<button class="gbtn" disabled>Claimed. Return tomorrow.</button>` : `<button class="gbtn grn" id="do-daily">Claim day ${(streak % 7) + 1}</button>`;
  showModal(`<div class="ph">${ic("gift")} Daily Tribute <span class="x">&times;</span></div>
    <div class="bd"><p style="color:#caa86a;text-align:center;margin-bottom:12px">Return each day for a greater gift. Streak: <b style="color:var(--gold2)">${streak}</b></p>
    <div class="dgrid">${cells}</div><div class="modal-actions" style="margin-top:16px">${btn}</div></div>`);
  const d = $("#do-daily"); if (d) d.onclick = async () => { try { const v = await api("daily", {}); applyState(v); sfx("reward"); toast("+" + v.reward.gems + " shards claimed"); renderDaily(); } catch (e) { toast(e.message, true); } };
}

// daily tasks ladder + free chest
$("#rl-tasks").onclick = openTasks;
function openTasks() { modalOpen = renderTasks; renderTasks(); }
function renderTasks() {
  if (!S || !S.tasks) return; const T = S.tasks; const now = S.now;
  const chestIn = Math.max(0, (S.chest.nextAt || 0) - now);
  const freeChest = `<div class="freechest"><div class="cg">${ic("gift")}</div><div class="ct"><b>Free Chest</b><div class="tg">${fmt(S.chest.reward.gems)} shards plus resources, every 4 hours</div></div>${S.chest.ready ? `<button class="gbtn grn" id="claim-free">Claim</button>` : `<button class="gbtn" disabled>${hms(chestIn)}</button>`}</div>`;
  const nodes = T.chests.map((c) => `<div class="node ${c.claimed ? "claimed" : (c.ready ? "ready" : "")}" data-at="${c.at}" style="left:${c.at}%" title="${c.gems} shards">${c.claimed ? "&#10003;" : ic("gem")}<small>${c.at}</small></div>`).join("");
  const tasks = T.list.map((t) => `<div class="taskrow ${t.done ? "done" : ""}"><div class="chk">${t.done ? "&#10003;" : ""}</div><div class="tk">${t.label}<div class="tg">${Math.min(t.have, t.goal)} / ${t.goal}</div></div><div class="pp">+${t.pts}</div></div>`).join("");
  showModal(`<div class="ph">${ic("tasks")} Daily Tasks <span class="x">&times;</span></div><div class="bd">
    ${freeChest}
    <p style="color:#caa86a;text-align:center;font-size:13px">Complete tasks to fill the bar and open chests. They reset each day. Earned <b style="color:var(--gold2)">${T.points}/100</b></p>
    <div class="ptrack"><div class="fill" style="width:${T.points}%"></div>${nodes}</div>
    ${tasks}</div>`);
  const fc = $("#claim-free"); if (fc) fc.onclick = async () => { try { const v = await api("chest", {}); applyState(v); sfx("reward"); toast("Free chest claimed"); renderTasks(); } catch (e) { toast(e.message, true); } };
  $$("#modal .node.ready").forEach((nd) => nd.onclick = async () => { try { const v = await api("taskchest", { at: +nd.dataset.at }); applyState(v); sfx("reward"); toast("Chest opened"); renderTasks(); } catch (e) { toast(e.message, true); } });
}

// army / training modal
$("#rl-army").onclick = openArmy;
function openArmy() { modalOpen = renderArmy; renderArmy(); }
function renderArmy() {
  const u = S.units; const cards = Object.keys(u).map((k) => {
    const un = u[k]; const cost = RESES.filter((r) => un.cost[r]).map((r) => `${ICON[r]}<span style="vertical-align:middle">${un.cost[r]}</span>`).join(" ");
    return `<div class="unitcard"><div class="em">${ic("sword")}</div><div class="mid"><div class="un">${un.name} <span style="color:#caa86a;font-weight:400">&times;${S.troops[k] || 0}</span></div>
      <div class="st">atk ${un.atk} &middot; def ${un.dinf}/${un.dcav} &middot; ${cost}</div></div>
      <input type="number" min="1" value="10" data-u="${k}"/><button class="gbtn grn" data-train="${k}" style="padding:9px 12px">Train</button></div>`;
  }).join("");
  const woundedTot = Object.values(S.wounded || {}).reduce((a, c) => a + c, 0);
  let infirm = "";
  if (woundedTot > 0) {
    const rows = Object.keys(u).filter((k) => (S.wounded[k] || 0) > 0).map((k) => `<span class="wcount">${u[k].name} <b>${fmt(S.wounded[k])}</b></span>`).join("");
    const costHtml = RESES.filter((r) => S.healCost[r]).map((r) => `<span class="rwc">${ICON[r]}${fmt(S.healCost[r])}</span>`).join(" ");
    const canHeal = RESES.every((r) => (local[r] || 0) >= (S.healCost[r] || 0));
    infirm = `<div class="infirm">
      <div class="ph" style="border:0;padding:4px 0">${ic("shield")} The Infirmary</div>
      <p style="color:#caa86a;font-size:12px;margin-bottom:8px">A share of your fallen are carried home wounded. Tend them and they rejoin the host. Sheltered ${fmt(woundedTot)} / ${fmt(S.woundCap)}.</p>
      <div class="wlist">${rows}</div>
      <div class="healrow"><div class="healcost">Cost ${costHtml || "free"}</div>${canHeal ? `<button class="gbtn grn" id="do-heal">Tend all (${fmt(woundedTot)})</button>` : `<button class="gbtn" disabled>Not enough resources</button>`}</div>
    </div>`;
  }
  showModal(`<div class="ph">${ic("sword")} The Barracks <span class="x">&times;</span></div>
    <div class="bd"><p style="color:#caa86a;text-align:center;margin-bottom:12px">${S.buildings.find((b) => b.id === "barracks").level ? "Drill soldiers for your host." : "Build a Barracks first to train soldiers."}</p>${infirm}${cards}</div>`);
  $$("#modal [data-train]").forEach((b) => b.onclick = async () => {
    const k = b.dataset.train; const n = +$(`#modal input[data-u="${k}"]`).value || 1;
    try { const v = await api("train", { unit: k, n }); applyState(v); sfx("build"); toast(`Training ${n} ${u[k].name}`); renderArmy(); } catch (e) { toast(e.message, true); }
  });
  const hb = $("#do-heal"); if (hb) hb.onclick = async () => { try { const v = await api("heal", {}); applyState(v); sfx("reward"); toast(`${fmt(v.healed)} soldiers tended back to the host`); renderArmy(); } catch (e) { toast(e.message, true); } };
}

// forge / hero / relics modal (equipment with transparent pity gacha)
$("#rl-forge").onclick = openForge;
function openForge() { modalOpen = renderForge; renderForge(); }
const SLOT_ICON = { weapon: "weapon", armor: "armor", banner: "banner", charm: "charm" };
const AFF_SUFFIX = { atk: "% attack", def: "% defense", speed: "% march speed", gold: "% spoils" };
function relicChip(it, action) {
  return `<div class="relic t${it.tier}" ${action || ""}>
    <div class="rico">${ic(SLOT_ICON[it.slot])}</div>
    <div class="rmid"><div class="rn">${it.slotName} <span class="rt">${it.tierName}</span></div>
      <div class="rv">+${it.val}${AFF_SUFFIX[it.aff]}</div></div></div>`;
}
function renderForge() {
  if (!S || !S.hero) return;
  const h = S.hero, hb = S.heroBonus;
  const slots = S.slots.map((s) => {
    const it = S.equipped[s];
    return `<div class="eqslot ${it ? "t" + it.tier : "empty"}" data-slot="${s}">
      <div class="eqlabel">${S.slotNames[s]}</div>
      ${it ? `<div class="rico big">${ic(SLOT_ICON[s])}</div><div class="rv">+${it.val}${AFF_SUFFIX[it.aff]}</div><div class="eqx" data-uneq="${s}">unequip</div>`
            : `<div class="rico big dim">${ic(SLOT_ICON[s])}</div><div class="rv dim">empty</div>`}</div>`;
  }).join("");
  const bonusLine = [["atk", "Attack"], ["def", "Defense"], ["speed", "March"], ["gold", "Spoils"]]
    .map(([k, n]) => `<span class="hb">${n} <b>+${hb[k]}%</b></span>`).join("");
  const stash = (S.relics || []).slice().sort((a, b) => b.tier - a.tier || b.val - a.val)
    .map((it) => relicChip(it, `data-equip="${it.seed >>> 0}"`)).join("") || `<div class="empty">No relics in the stash. Work the Forge to draw one.</div>`;
  const canForge = S.gems >= S.forgeCost;
  showModalWide(`<div class="ph">${ic("anvil")} The Forge &amp; the Hero <span class="x">&times;</span></div>
    <div class="bd">
      <div class="herobar">
        <div class="hportrait">${ic("shield")}<div class="hlvl">${roman(h.level)}</div></div>
        <div class="hinfo"><div class="hname">Your Champion &middot; Level ${h.level}</div>
          <div class="xpbar"><i style="width:${Math.min(100, 100 * h.xp / h.xpNeed)}%"></i></div>
          <div class="hbonus">${bonusLine}</div></div>
      </div>
      <div class="eqrow">${slots}</div>
      <div class="forgebox">
        <div class="fdesc"><b>Work the Forge</b><div class="tg">Each strike yields a relic. Pity: <b style="color:var(--gold2)">${S.pity}/${S.pityMax}</b> until a guaranteed Epic or better.</div></div>
        ${canForge ? `<button class="gbtn grn" id="do-forge">${ic("gem")} ${S.forgeCost} &middot; Strike</button>`
                   : `<button class="gbtn" disabled>${ic("gem")} ${S.forgeCost} needed</button>`}
      </div>
      <div class="ph" style="border:0;padding:8px 0 4px">The Stash</div>
      <div class="stash">${stash}</div>
    </div>`);
  const f = $("#do-forge"); if (f) f.onclick = async () => {
    try { const v = await api("forge", {}); const d = v.drew; sfx(d.tier >= 2 ? "level" : "coin");
      toast(`Forged a ${d.tierName} ${d.slotName}: +${d.val}${AFF_SUFFIX[d.aff]}`); applyState(v); renderForge();
    } catch (e) { toast(e.message, true); }
  };
  $$("#modal [data-equip]").forEach((c) => c.onclick = async () => {
    try { const v = await api("equip", { seed: +c.dataset.equip >>> 0 }); applyState(v); sfx("done"); renderForge(); } catch (e) { toast(e.message, true); }
  });
  $$("#modal [data-uneq]").forEach((c) => c.onclick = async (e) => {
    e.stopPropagation();
    try { const v = await api("unequip", { slot: c.dataset.uneq }); applyState(v); sfx("click"); renderForge(); } catch (e) { toast(e.message, true); }
  });
}

// VIP track (accumulating points -> permanent empire buffs)
$("#ic-vip").onclick = openVip;
function openVip() { modalOpen = renderVip; renderVip(); }
const VIP_PERK_LABELS = [["build", "Faster construction", "%"], ["prod", "More production", "%"], ["march", "Faster marches", "%"], ["slots", "Extra march", ""]];
function vipPerkLine(perks) {
  return VIP_PERK_LABELS.filter(([k]) => perks[k]).map(([k, label, unit]) => `<span class="vp">${label} <b>+${perks[k]}${unit}</b></span>`).join("") || `<span class="vp" style="color:#8a7448">No bonuses yet</span>`;
}
function renderVip() {
  if (!S || !S.vip) return; const V = S.vip;
  const next = V.nextAt; const cur = V.levels[V.level].pts;
  const pct = next == null ? 100 : Math.min(100, 100 * (V.points - cur) / (next - cur));
  const ladder = V.levels.map((lv, i) => {
    const reached = i <= V.level;
    return `<div class="viprow ${reached ? "got" : ""} ${i === V.level ? "cur" : ""}">
      <div class="vlv">${ic("crown")}<span>${i}</span></div>
      <div class="vpts">${fmt(lv.pts)} pts</div>
      <div class="vperks">${vipPerkLine(lv)}</div></div>`;
  }).join("");
  const daily = V.dailyReady
    ? `<button class="gbtn grn" id="vip-claim">Hold audience &middot; +${V.dailyPts} VIP</button>`
    : `<button class="gbtn" disabled>Audience held today</button>`;
  showModalWide(`<div class="ph">${ic("crown")} VIP &middot; the Royal Audience <span class="x">&times;</span></div>
    <div class="bd">
      <div class="vipnow">
        <div class="vbig">${ic("crown")}<div class="vbl">VIP ${V.level}</div></div>
        <div class="vinfo">
          <div class="vline">Active bonuses: ${vipPerkLine(V.perks)}</div>
          <div class="xpbar" style="margin-top:8px"><i style="width:${pct}%"></i></div>
          <div class="tg" style="margin-top:5px">${next == null ? "Maximum VIP rank reached." : `${fmt(V.points)} / ${fmt(next)} points to VIP ${V.level + 1}`}</div>
        </div>
      </div>
      <div class="vipdaily">${daily}<div class="tg" style="flex:1">A free daily audience grants VIP points and a resource crate. Shard packs grant VIP points one-for-one.</div></div>
      <div class="ph" style="border:0;padding:8px 0 4px">The VIP ladder</div>
      <div class="vipladder">${ladder}</div>
    </div>`);
  const c = $("#vip-claim"); if (c) c.onclick = async () => {
    try { const v = await api("vipdaily", {}); sfx(v.levelled ? "level" : "reward"); toast(v.levelled ? `VIP rank up! Now VIP ${v.vip.level}` : `+${v.pts} VIP points and a resource crate`); applyState(v); renderVip(); }
    catch (e) { toast(e.message, true); }
  };
}

// season / battle pass (30 days, free + premium tracks earned by daily play)
function renderSeasonBar() {
  if (!S || !S.season) return; const S2 = S.season;
  const nm = $("#s-name"); if (nm) nm.textContent = S2.name;
  const lv = $("#s-lvl"); if (lv) lv.textContent = "Level " + S2.level + (S2.premium ? " &middot; Gold" : "");
  if (lv) lv.innerHTML = "Level " + S2.level + (S2.premium ? ' <span class="goldtag">GOLD</span>' : "");
  const fill = $("#s-fill"); if (fill) { const span = (S2.nextXp || S2.xp) - S2.levelXp; const into = S2.xp - S2.levelXp; fill.style.width = (S2.nextXp == null ? 100 : Math.max(3, Math.min(100, 100 * into / span))) + "%"; }
  const bd = $("#s-bdg"); if (bd) bd.classList.toggle("hidden", !S2.claimable);
}
function rewardChips(rw) {
  if (!rw) return "";
  let out = "";
  if (rw.gems) out += `<span class="rwc">${ICON.gem}${rw.gems}</span>`;
  for (const k in (rw.res || {})) out += `<span class="rwc">${ICON[k]}${fmt(rw.res[k])}</span>`;
  return out;
}
$("#seasonbar").onclick = openSeason;
function openSeason() { modalOpen = renderSeason; renderSeason(); }
function renderSeason() {
  if (!S || !S.season) return; const S2 = S.season;
  const remain = Math.max(0, S2.endsAt - (Date.now() / 1000 + (S.now - (S._recv || S.now))));
  const days = Math.floor(remain / 86400), hrs = Math.floor((remain % 86400) / 3600);
  const into = S2.xp - S2.levelXp; const span = (S2.nextXp || S2.xp) - S2.levelXp;
  const cols = S2.levels.map((L) => {
    const cur = L.level === S2.level;
    const freeBtn = L.unlocked && !L.freeClaimed
      ? `<button class="ssclaim" data-lv="${L.level}" data-tr="free">claim</button>`
      : `<div class="sscell ${L.freeClaimed ? "claimed" : (L.unlocked ? "" : "locked")}">${L.freeClaimed ? "&#10003;" : rewardChips(L.free)}</div>`;
    const premBtn = !S2.premium
      ? `<div class="sscell premlock">${rewardChips(L.prem)}</div>`
      : (L.unlocked && !L.premClaimed
        ? `<button class="ssclaim gold" data-lv="${L.level}" data-tr="prem">claim</button>`
        : `<div class="sscell gold ${L.premClaimed ? "claimed" : (L.unlocked ? "" : "locked")}">${L.premClaimed ? "&#10003;" : rewardChips(L.prem)}</div>`);
    return `<div class="sscol ${cur ? "cur" : ""}"><div class="sslvl">${L.level}</div>${freeBtn}${premBtn}</div>`;
  }).join("");
  const buy = S2.premium ? `<div class="goldtag big">GOLD PASS ACTIVE</div>` : `<button class="gbtn ox" id="s-buy">Unlock the Gold Pass</button>`;
  showModalWide(`<div class="ph">${ic("pass")} ${esc(S2.name)} &middot; Season Pass <span class="x">&times;</span></div>
    <div class="bd">
      <div class="seashead">
        <div class="shl"><div class="slvbig">Lv ${S2.level}<span>/${S2.max}</span></div>
          <div class="xpbar" style="width:160px"><i style="width:${S2.nextXp == null ? 100 : Math.max(3, 100 * into / span)}%"></i></div>
          <div class="tg">${S2.nextXp == null ? "Pass complete" : `${fmt(into)} / ${fmt(span)} to next level`}</div></div>
        <div class="shr"><div class="tg">Season ends in</div><div class="scount">${days}d ${hrs}h</div>${buy}</div>
      </div>
      <p style="color:#caa86a;font-size:13px;margin:4px 0 10px;text-align:center">Earn season points from daily play: logging in, building, training, raiding. Rewards never expire mid-season. <button class="ministr" id="s-all">Claim all available</button></p>
      <div class="sslabels"><div class="ssfree">FREE</div><div class="ssgold">GOLD</div></div>
      <div class="sstrack">${cols}</div>
    </div>`);
  const buyB = $("#s-buy"); if (buyB) buyB.onclick = async () => { try { const v = await api("seasonbuy", {}); sfx("level"); toast("Gold Pass unlocked. Past levels are now claimable."); applyState(v); renderSeason(); } catch (e) { toast(e.message, true); } };
  const allB = $("#s-all"); if (allB) allB.onclick = async () => { try { const v = await api("season", { all: true }); sfx("reward"); toast(`Claimed ${v.count} reward${v.count > 1 ? "s" : ""}` + (v.gained ? `, +${v.gained} shards` : "")); applyState(v); renderSeason(); } catch (e) { toast(e.message, true); } };
  $$("#modal .ssclaim").forEach((btn) => btn.onclick = async () => {
    try { const v = await api("season", { level: +btn.dataset.lv, track: btn.dataset.tr }); sfx("reward"); applyState(v); renderSeason(); } catch (e) { toast(e.message, true); }
  });
  // scroll the track to the current level
  const tr = $("#modal .sstrack"); const curEl = $("#modal .sscol.cur"); if (tr && curEl) tr.scrollLeft = Math.max(0, curEl.offsetLeft - 120);
}

// honors / achievements (permanent tiered milestones)
$("#rl-honors").onclick = openHonors;
function openHonors() { modalOpen = renderHonors; renderHonors(); }
function renderHonors() {
  if (!S || !S.achievements) return;
  const rows = S.achievements.map((a) => {
    const tier = a.claimedTiers; const max = a.tiers.length;
    const pct = a.maxed ? 100 : Math.min(100, 100 * a.have / a.goal);
    const pips = a.tiers.map((t, i) => `<span class="pip ${i < a.claimedTiers ? "got" : ""} ${a.claimable && i === a.claimedTiers ? "rdy" : ""}"></span>`).join("");
    const right = a.maxed
      ? `<div class="amax">Mastered</div>`
      : a.claimable
        ? `<button class="gbtn grn" data-achv="${a.id}">${ic("gem")} ${a.reward}</button>`
        : `<div class="aprog">${fmt(a.have)} / ${fmt(a.goal)}</div>`;
    return `<div class="achv ${a.claimable ? "rdy" : ""} ${a.maxed ? "maxed" : ""}">
      <div class="aico">${ic(a.icon)}<div class="atier">${tier}/${max}</div></div>
      <div class="amid"><div class="an">${a.name}</div><div class="ad">${a.desc}</div>
        <div class="abar"><i style="width:${pct}%"></i></div><div class="apips">${pips}</div></div>
      <div class="aright">${right}</div></div>`;
  }).join("");
  showModalWide(`<div class="ph">${ic("medal")} Honors &amp; Milestones <span class="x">&times;</span></div>
    <div class="bd"><p style="color:#caa86a;text-align:center;font-size:13px;margin-bottom:12px">Permanent feats of your reign. Each tier earns shards. They never reset.</p>
    <div class="achvlist">${rows}</div></div>`);
  $$("#modal [data-achv]").forEach((btn) => btn.onclick = async () => {
    try { const v = await api("achv", { id: btn.dataset.achv }); sfx("reward"); toast(`+${v.gained} shards from ${v.tiers} tier${v.tiers > 1 ? "s" : ""}`); applyState(v); renderHonors(); }
    catch (e) { toast(e.message, true); }
  });
}

// alliances (banners): create / join, roster, timer-shaving help, chat
$("#rl-ally").onclick = openAlliance;
let ALLYLIST = null;
async function openAlliance() {
  modalOpen = renderAlliance;
  if (!S.alliance) { try { ALLYLIST = (await api("alliances")).alliances; } catch (e) { ALLYLIST = []; } }
  renderAlliance();
}
function hmsShort(s) { s = Math.max(0, Math.floor(s)); const h = (s / 3600) | 0, m = ((s % 3600) / 60) | 0; return h ? h + "h " + m + "m" : (m ? m + "m " + (s % 60) + "s" : s + "s"); }
function renderAlliance() {
  if (!S) return;
  if (!S.alliance) return renderAllyBrowse();
  const A = S.alliance; const now = S.now;
  const roster = A.members.map((m) => {
    const orders = (m.orders || []).map((o) => {
      const rem = o.finish - now;
      if (o.helpedByYou) return `<div class="aord done">${esc(o.name)} ${roman(o.to)} &middot; aided</div>`;
      if (o.maxed) return `<div class="aord">${esc(o.name)} ${roman(o.to)} &middot; full</div>`;
      return `<div class="aord"><span>${esc(o.name)} ${roman(o.to)} &middot; ${hmsShort(rem)} <em>(${o.helps}/${A.helpMax})</em></span><button class="ahelp" data-m="${esc(m.name)}" data-i="${o.i}">Aid</button></div>`;
    }).join("");
    const isMe = m.name === S.name;
    const garr = m.garrison > 0 ? `<span class="garr">${ic("shield")}${fmt(m.garrison)} garrisoned</span>` : "";
    let reinfBtn = "";
    if (!isMe) {
      reinfBtn = `<button class="rbtn" data-reinf="${esc(m.name)}">Reinforce</button>`;
      if (m.yourReinf > 0) reinfBtn += `<button class="rbtn recall" data-recall="${esc(m.name)}">Recall ${fmt(m.yourReinf)}</button>`;
    }
    return `<div class="amem"><div class="amName">${m.leader ? ic("crown") : ""}${esc(m.name)}</div>
      <div class="amStat">${fmt(m.might)} might &middot; Keep ${roman(m.keep)} ${garr}</div>${orders ? `<div class="aords">${orders}</div>` : ""}${reinfBtn ? `<div class="areinf">${reinfBtn}</div>` : ""}</div>`;
  }).join("");
  const chat = (A.chat || []).map((c) => c.from ? `<div class="cmsg"><b>${esc(c.from)}</b> ${esc(c.text)}</div>` : `<div class="cmsg sys">${esc(c.text)}</div>`).join("") || `<div class="empty">No words yet. Hail your banner.</div>`;
  showModalWide(`<div class="ph">${ic("ally")} ${esc(A.name)} <span class="tagchip">${esc(A.tag)}</span> <span class="x">&times;</span></div>
    <div class="bd">
      <div class="allyhead"><div><b>${A.members.length}</b> sworn &middot; production <b style="color:var(--gold2)">+${A.bonus}%</b> while banded</div>
        <button class="gbtn ox" id="ally-leave" style="padding:8px 12px">Leave</button></div>
      <div class="allygrid">
        <div class="allyroster"><div class="ph" style="border:0;padding:6px 0">The sworn</div>${roster}</div>
        <div class="allychat"><div class="ph" style="border:0;padding:6px 0">War table</div>
          <div class="chatlog" id="chatlog">${chat}</div>
          <div class="chatin"><input id="chat-txt" maxlength="160" placeholder="rally your banner..."/><button class="gbtn grn" id="chat-send">Say</button></div>
        </div>
      </div>
    </div>`);
  $("#ally-leave").onclick = async () => { try { const v = await api("allianceleave", {}); applyState(v); toast("You left the banner"); renderAlliance(); } catch (e) { toast(e.message, true); } };
  $$("#modal .ahelp").forEach((btn) => btn.onclick = async () => {
    try { const v = await api("alliancehelp", { member: btn.dataset.m, i: +btn.dataset.i }); sfx("done"); toast("You sped " + esc(btn.dataset.m) + "'s work by " + hmsShort(v.shaved)); applyState(v); renderAlliance(); }
    catch (e) { toast(e.message, true); }
  });
  $$("#modal [data-reinf]").forEach((btn) => btn.onclick = () => reinforceDialog(btn.dataset.reinf));
  $$("#modal [data-recall]").forEach((btn) => btn.onclick = async () => {
    try { const v = await api("recall", { member: btn.dataset.recall }); sfx("march"); toast(fmt(v.recalled) + " soldiers marched home"); applyState(v); renderAlliance(); }
    catch (e) { toast(e.message, true); }
  });
  const send = async () => { const t = $("#chat-txt").value.trim(); if (!t) return; try { const v = await api("alliancechat", { text: t }); applyState(v); renderAlliance(); } catch (e) { toast(e.message, true); } };
  $("#chat-send").onclick = send; $("#chat-txt").addEventListener("keydown", (e) => { if (e.key === "Enter") send(); });
  const cl = $("#chatlog"); if (cl) cl.scrollTop = cl.scrollHeight;
}
function reinforceDialog(member) {
  const rows = Object.keys(S.units).map((u) => `<div class="unitcard"><div class="em">${ic("sword")}</div><div class="mid"><div class="un">${S.units[u].name}</div><div class="st">you have ${S.troops[u] || 0} &middot; speed ${S.units[u].speed}</div></div><input type="number" min="0" max="${S.troops[u] || 0}" value="0" data-ru="${u}"/></div>`).join("");
  showModal(`<div class="ph">${ic("ally")} Reinforce ${esc(member)} <span class="x">&times;</span></div><div class="bd">
    <p style="color:#caa86a;text-align:center;margin-bottom:10px;font-size:13px">Send troops to garrison ${esc(member)}'s hold. They will fight in every defense until you recall them or they fall.</p>
    ${rows}<div class="modal-actions"><button class="gbtn grn" id="do-reinf">Send to the walls</button></div></div>`);
  modalOpen = null;
  $("#do-reinf").onclick = async () => {
    const troops = {}; $$("#modal [data-ru]").forEach((i) => { const n = +i.value || 0; if (n > 0) troops[i.dataset.ru] = n; });
    if (!Object.keys(troops).length) return toast("Choose some soldiers to send.", true);
    try { const v = await api("reinforce", { member, troops }); applyState(v); sfx("march"); toast("Your host marches to " + esc(member) + "'s aid"); openAlliance(); }
    catch (e) { toast(e.message, true); }
  };
}
function renderAllyBrowse() {
  const rows = (ALLYLIST || []).map((a) => `<div class="allyrow"><div><div class="arn">${esc(a.name)} <span class="tagchip">${esc(a.tag)}</span></div><div class="ars">${a.members} sworn &middot; ${fmt(a.might)} might</div></div><button class="gbtn grn" data-join="${esc(a.tag)}" style="padding:8px 14px">Join</button></div>`).join("") || `<div class="empty">No banners yet. Found the first.</div>`;
  showModalWide(`<div class="ph">${ic("ally")} The Banners <span class="x">&times;</span></div>
    <div class="bd">
      <div class="allyfound">
        <div class="ph" style="border:0;padding:4px 0">Found your own banner</div>
        <div class="foundrow"><input id="ally-name" maxlength="24" placeholder="banner name"/><input id="ally-tag" maxlength="4" placeholder="TAG" style="width:90px;text-transform:uppercase"/>
        <button class="gbtn" id="ally-create">${ic("gem")} 80 &middot; Found</button></div>
        <div class="tg" style="color:#caa86a;font-size:12px;margin-top:4px">A banded host earns +1% production per member (up to +10%) and can speed each other's builds.</div>
      </div>
      <div class="ph" style="border:0;padding:8px 0 4px">Join a banner</div>
      <div class="allylist">${rows}</div>
    </div>`);
  $("#ally-create").onclick = async () => {
    try { const v = await api("alliancecreate", { name: $("#ally-name").value, tag: $("#ally-tag").value }); sfx("level"); toast("Your banner flies"); applyState(v); renderAlliance(); }
    catch (e) { toast(e.message, true); }
  };
  $$("#modal [data-join]").forEach((btn) => btn.onclick = async () => {
    try { const v = await api("alliancejoin", { tag: btn.dataset.join }); sfx("done"); toast("You joined " + esc(btn.dataset.join)); applyState(v); renderAlliance(); }
    catch (e) { toast(e.message, true); }
  });
}

// leaderboard
let LADDER = null, ladderTab = "lords";
$("#ic-board").onclick = async () => { try { LADDER = await api("leaderboard"); ladderTab = "lords"; renderLadder(); modalOpen = null; } catch (e) { toast(e.message, true); } };
function ladderRow(rank, portrait, name, tag, right, me) {
  return `<div class="lrow ${me ? "me" : ""}"><div class="lrank ${rank <= 3 ? "top" : ""}">${rank}</div>
    <div class="lpor"><img src="img/lord/lord${portrait || 0}.png" alt="" onerror="this.style.opacity=0"/></div>
    <div class="lname">${esc(name)}${tag ? ` <span class="tagchip">${esc(tag)}</span>` : ""}</div>
    <div class="lright">${right}</div></div>`;
}
function renderLadder() {
  if (!LADDER) return; const L = LADDER; const meName = S && S.name;
  const tabs = `<div class="ltabs">
    <button class="ltab ${ladderTab === "lords" ? "on" : ""}" data-tab="lords">Lords</button>
    <button class="ltab ${ladderTab === "raiders" ? "on" : ""}" data-tab="raiders">Warlords</button>
    <button class="ltab ${ladderTab === "banners" ? "on" : ""}" data-tab="banners">Banners</button></div>`;
  let body = "";
  if (ladderTab === "lords") {
    body = (L.lords || []).map((r) => ladderRow(r.rank, r.portrait, r.name, r.tag, `${fmt(r.might)} might &middot; Keep ${roman(r.keep)}`, r.name === meName)).join("") || `<div class="empty">No lords yet.</div>`;
    if (L.you && L.you.rank > 20) body += `<div class="lyou">Your rank &middot; ${ladderRow(L.you.rank, L.you.portrait, L.you.name, L.you.tag, `${fmt(L.you.might)} might &middot; Keep ${roman(L.you.keep)}`, true)}</div>`;
  } else if (ladderTab === "raiders") {
    body = (L.raiders || []).map((r) => ladderRow(r.rank, r.portrait, r.name, r.tag, `${fmt(r.raidsWon)} raids won`, r.name === meName)).join("") || `<div class="empty">No camps cleared yet. Be the first.</div>`;
  } else {
    body = (L.banners || []).map((r) => `<div class="lrow"><div class="lrank ${r.rank <= 3 ? "top" : ""}">${r.rank}</div><div class="lpor banner">${ic("ally")}</div><div class="lname">${esc(r.name)} <span class="tagchip">${esc(r.tag)}</span></div><div class="lright">${r.members} sworn &middot; ${fmt(r.might)} might</div></div>`).join("") || `<div class="empty">No banners raised yet.</div>`;
  }
  showModal(`<div class="ph">${ic("trophy")} The Realm Ladder <span class="sub2">${L.total} lords contend</span> <span class="x">&times;</span></div>
    <div class="bd">${tabs}<div class="lladder">${body}</div></div>`);
  modalOpen = null;
  $$("#modal .ltab").forEach((b) => b.onclick = () => { ladderTab = b.dataset.tab; renderLadder(); });
}
$("#ic-settings").onclick = () => {
  const mv = window.GA ? Math.round(GA.musicVol() * 100) : 70;
  const sv = window.GA ? Math.round(GA.sfxVol() * 100) : 100;
  const rm = localStorage.getItem("gr_reducemotion") === "1";
  const muted = window.GA && GA.isMuted();
  showModal(`<div class="ph">${ic("gear")} The Steward <span class="x">&times;</span></div><div class="bd">
    ${S.counsel ? `<div class="counsel">${ic("scroll")}<div><div class="ck">Your steward counsels</div>&ldquo;${esc(S.counsel)}&rdquo;</div></div>` : ""}
    <div class="setsec"><div class="seth">${ic("soundOn")} Sound</div>
      <div class="setrow"><span class="setk">Music</span><input type="range" min="0" max="100" value="${mv}" id="set-music" class="slider"/><span class="setv" id="set-music-v">${mv}</span></div>
      <div class="setrow"><span class="setk">Effects</span><input type="range" min="0" max="100" value="${sv}" id="set-sfx" class="slider"/><span class="setv" id="set-sfx-v">${sv}</span></div>
      <div class="setrow toggle" id="set-mute"><span class="setk">Mute all</span><span class="tog ${muted ? "on" : ""}"><span class="knob"></span></span></div>
    </div>
    <div class="setsec"><div class="seth">${ic("gear")} Display</div>
      <div class="setrow toggle" id="set-motion"><span class="setk">Reduce motion <small>fewer animations</small></span><span class="tog ${rm ? "on" : ""}"><span class="knob"></span></span></div>
    </div>
    <div class="setsec"><div class="seth">${ic("shield")} Your hold</div>
      <div class="statline"><span class="k">Lord</span><span class="v">${esc(S.name)}</span></div>
      <div class="statline"><span class="k">Coordinates</span><span class="v">(${S.coords.x} | ${S.coords.y})</span></div>
      ${S.allyTag ? `<div class="statline"><span class="k">Banner</span><span class="v">${esc(S.allyTag)}</span></div>` : ""}
    </div>
    <div class="modal-actions" style="gap:8px"><button class="gbtn" id="open-chron">Chronicle of the Fallen</button><button class="gbtn ox" id="logout">Leave the realm</button></div></div>`);
  modalOpen = null;
  const music = $("#set-music"), sfx = $("#set-sfx");
  music.oninput = () => { $("#set-music-v").textContent = music.value; if (window.GA) GA.setMusicVol(music.value / 100); };
  sfx.oninput = () => { $("#set-sfx-v").textContent = sfx.value; if (window.GA) GA.setSfxVol(sfx.value / 100); };
  sfx.onchange = () => { if (window.GA && !GA.isMuted()) GA.sfx("click"); }; // preview the new effects level
  $("#set-mute").onclick = () => { if (!audioStarted) kickAudio(); if (window.GA) { GA.toggle(); updateMuteIcon(); $("#set-mute .tog").classList.toggle("on", GA.isMuted()); } };
  $("#set-motion").onclick = () => { const on = !document.body.classList.contains("reduce-motion"); document.body.classList.toggle("reduce-motion", on); localStorage.setItem("gr_reducemotion", on ? "1" : "0"); $("#set-motion .tog").classList.toggle("on", on); };
  $("#open-chron").onclick = openChronicle;
  $("#logout").onclick = () => { localStorage.removeItem("gr_token"); location.reload(); };
};
// apply the saved reduce-motion preference at boot
if (localStorage.getItem("gr_reducemotion") === "1") document.body.classList.add("reduce-motion");
function openChronicle() {
  const entries = (S.chronicle || []).map((e) => `<div class="chron"><div class="cht">${esc(e.t)}</div><div class="chb">${esc(e.b)}</div></div>`).join("");
  showModal(`<div class="ph">${ic("ruin")} Chronicle of the Fallen <span class="x">&times;</span></div><div class="bd"><div class="chronwrap">${entries}</div></div>`);
  modalOpen = null;
}
$("#rl-shop").onclick = openShop;
// mobile: the right column is a slide-up drawer
{ const dh = $("#drawerhandle"); if (dh) dh.onclick = () => $("#right").classList.toggle("open"); }

// ---- world map + marches + reports ----
function showModalWide(html) { const m = $("#modal"); m.classList.remove("hidden"); m.innerHTML = `<div class="sheet wide panel">${html}</div>`; const x = m.querySelector(".x"); if (x) x.onclick = closeModal; m.onclick = (e) => { if (e.target === m) closeModal(); }; }
let MAP = null;
let mapView = { z: 1, px: 0, py: 0, _init: false };
async function openMap() { try { MAP = await api("map"); } catch (e) { return toast(e.message, true); } mapView._init = false; renderMap(); modalOpen = null; }
function renderMap() {
  if (!MAP) return; const c = MAP.center, R = MAP.R, CELL = 30;
  const idx = {}; MAP.tiles.forEach((t) => idx[t.x + "," + t.y] = t);
  let cells = "";
  for (let dy = -R; dy <= R; dy++) for (let dx = -R; dx <= R; dx++) {
    const x = c.x + dx, y = c.y + dy;
    if (dx === 0 && dy === 0) { cells += `<div class="cell city me" title="Your hold (${x} | ${y})">&#9733;</div>`; continue; }
    const t = idx[x + "," + y];
    if (!t) cells += `<div class="cell"></div>`;
    else if (t.type === "camp") cells += `<div class="cell camp ${t.cleared ? "cleared" : ""}" data-x="${x}" data-y="${y}" title="Barbarian camp, level ${t.level}">${t.level}</div>`;
    else if (t.type === "ruin") cells += `<div class="cell ruin ${t.delved ? "delved" : ""}" ${t.delved ? "" : `data-rx="${x}" data-ry="${y}"`} title="${t.delved ? "A delved ruin" : "A fallen giant's ruin (delve it)"}">${ic("ruin")}</div>`;
    else if (t.type === "city") {
      const attackable = !t.shielded && !t.allied;
      const cls = t.allied ? "ally" : (t.shielded ? "shielded" : "foe");
      const tip = esc(t.name) + " &middot; Keep " + (t.keep || 1) + " &middot; " + fmt(t.might || 0) + " might" + (t.allied ? " (banner)" : t.shielded ? " (at peace)" : " (raidable)");
      cells += `<div class="cell city ${cls}" ${attackable ? `data-ax="${x}" data-ay="${y}"` : ""} title="${tip}">${ic(t.allied ? "ally" : "flag")}</div>`;
    }
  }
  const side = 2 * R + 1;
  showModalWide(`<div class="ph">${ic("map")} The Reach &#183; (${c.x} | ${c.y}) <span class="x">&times;</span></div>
    <div class="bd"><div id="mapview"><div class="maphint">drag to pan, scroll or +/- to zoom</div>
      <div id="mapinner" style="position:relative;display:grid;gap:2px;grid-template-columns:repeat(${side},${CELL}px);grid-auto-rows:${CELL}px">${cells}</div>
      <div class="mapctl"><button id="mz-in">+</button><button id="mz-out">&minus;</button><button id="mz-home" title="center on home">&#9733;</button></div>
    </div>${reportsHtml()}</div>`);
  $$("#mapinner .cell.camp:not(.cleared)").forEach((e) => e.onclick = (ev) => { ev.stopPropagation(); marchDialog(+e.dataset.x, +e.dataset.y); });
  $$("#mapinner .cell.ruin[data-rx]").forEach((e) => e.onclick = (ev) => { ev.stopPropagation(); delveDialog(+e.dataset.rx, +e.dataset.ry); });
  $$("#mapinner .cell.city.foe[data-ax]").forEach((e) => e.onclick = (ev) => { ev.stopPropagation(); attackDialog(+e.dataset.ax, +e.dataset.ay); });
  initIcons($("#mapinner"));
  buildMapMarchLayer();
  setupMapPanZoom(side, CELL);
}
// active marches drawn moving along their path on the world map
const MAP_CELL = 30, MAP_STEP = 32; // cell + 2px gap
function mapPos(dx, dy, R) { return [(dx + R) * MAP_STEP + MAP_CELL / 2, (dy + R) * MAP_STEP + MAP_CELL / 2]; }
function marchColor(m) { return m.kind === "scout" ? "#6fb4d6" : m.kind === "reinforce" ? "#7fc25a" : m.kind === "city" ? "#e2613f" : "#f6e2a0"; }
function buildMapMarchLayer() {
  const inner = $("#mapinner"); if (!inner || !MAP || !S) return;
  const old = $("#marchlayer"); if (old) old.remove();
  const c = MAP.center, R = MAP.R, side = 2 * R + 1, W = side * MAP_STEP;
  const [hx, hy] = mapPos(0, 0, R);
  let svg = `<svg id="marchlayer" width="${W}" height="${W}" style="position:absolute;left:0;top:0;pointer-events:none;overflow:visible;z-index:3">`;
  (S.marches || []).forEach((m, i) => {
    const [tx, ty] = mapPos(m.tx - c.x, m.ty - c.y, R); const col = marchColor(m);
    svg += `<line x1="${hx}" y1="${hy}" x2="${tx}" y2="${ty}" stroke="${col}" stroke-width="1.5" stroke-dasharray="3 4" opacity="0.45"/>`;
    svg += `<g class="mmark" data-mi="${i}"><circle r="11" fill="none" stroke="${col}" stroke-width="1.5" opacity="0.5"><animate attributeName="r" values="6;13;6" dur="1.5s" repeatCount="indefinite"/><animate attributeName="opacity" values="0.55;0;0.55" dur="1.5s" repeatCount="indefinite"/></circle><circle r="6.5" fill="${col}" stroke="#160d05" stroke-width="2"/></g>`;
  });
  (S.incoming || []).forEach((m, i) => {
    const [fxp, fyp] = mapPos(m.fx - c.x, m.fy - c.y, R); const col = "#ff4d2a";
    svg += `<line x1="${fxp}" y1="${fyp}" x2="${hx}" y2="${hy}" stroke="${col}" stroke-width="1.8" stroke-dasharray="2 4" opacity="0.55"/>`;
    svg += `<g class="imark" data-ii="${i}"><circle r="13" fill="none" stroke="${col}" stroke-width="2" opacity="0.6"><animate attributeName="r" values="7;15;7" dur="1.1s" repeatCount="indefinite"/><animate attributeName="opacity" values="0.7;0;0.7" dur="1.1s" repeatCount="indefinite"/></circle><circle r="7" fill="${col}" stroke="#160d05" stroke-width="2"/></g>`;
  });
  inner.insertAdjacentHTML("beforeend", svg + "</svg>");
  updateMapMarches(Date.now() / 1000 + (S.now - (S._recv || S.now)));
}
function updateMapMarches(now) {
  const layer = $("#marchlayer"); if (!layer || !MAP || !S) return;
  const marks = layer.querySelectorAll(".mmark"); const imarks = layer.querySelectorAll(".imark");
  if (marks.length !== (S.marches || []).length || imarks.length !== (S.incoming || []).length) { buildMapMarchLayer(); return; }
  const c = MAP.center, R = MAP.R, [hx, hy] = mapPos(0, 0, R);
  marks.forEach((g) => {
    const m = S.marches[+g.dataset.mi]; if (!m) return;
    const [tx, ty] = mapPos(m.tx - c.x, m.ty - c.y, R);
    let px, py;
    if (!m.resolved) { const t = Math.max(0, Math.min(1, (now - m.depart) / (m.arrive - m.depart))); px = hx + (tx - hx) * t; py = hy + (ty - hy) * t; }
    else { const t = Math.max(0, Math.min(1, (now - m.arrive) / (m.ret - m.arrive))); px = tx + (hx - tx) * t; py = ty + (hy - ty) * t; }
    g.setAttribute("transform", `translate(${px},${py})`);
  });
  imarks.forEach((g) => {
    const m = S.incoming[+g.dataset.ii]; if (!m) return;
    const [fxp, fyp] = mapPos(m.fx - c.x, m.fy - c.y, R);
    const t = Math.max(0, Math.min(1, (now - m.depart) / (m.arrive - m.depart)));
    g.setAttribute("transform", `translate(${fxp + (hx - fxp) * t},${fyp + (hy - fyp) * t})`);
  });
}
function setupMapPanZoom(side, CELL) {
  const view = $("#mapview"), inner = $("#mapinner"); if (!view || !inner) return;
  const gridPx = side * (CELL + 2);
  const center = () => { mapView.z = 1; mapView.px = view.clientWidth / 2 - gridPx / 2; mapView.py = view.clientHeight / 2 - gridPx / 2; };
  if (!mapView._init) { center(); mapView._init = true; }
  const apply = () => { inner.style.transform = `translate(${mapView.px}px,${mapView.py}px) scale(${mapView.z})`; };
  const zoomAt = (ox, oy, f) => { const nz = Math.max(0.5, Math.min(2.2, mapView.z * f)); const r = nz / mapView.z; mapView.px = ox - (ox - mapView.px) * r; mapView.py = oy - (oy - mapView.py) * r; mapView.z = nz; apply(); };
  apply();
  let drag = null;
  view.onpointerdown = (e) => { drag = { x: e.clientX, y: e.clientY, px: mapView.px, py: mapView.py, moved: false }; view.classList.add("drag"); view.setPointerCapture(e.pointerId); };
  view.onpointermove = (e) => { if (!drag) return; mapView.px = drag.px + (e.clientX - drag.x); mapView.py = drag.py + (e.clientY - drag.y); apply(); };
  view.onpointerup = view.onpointercancel = () => { drag = null; view.classList.remove("drag"); };
  view.onwheel = (e) => { e.preventDefault(); zoomAt(e.offsetX, e.offsetY, e.deltaY < 0 ? 1.12 : 0.89); };
  $("#mz-in").onclick = (e) => { e.stopPropagation(); zoomAt(view.clientWidth / 2, view.clientHeight / 2, 1.2); };
  $("#mz-out").onclick = (e) => { e.stopPropagation(); zoomAt(view.clientWidth / 2, view.clientHeight / 2, 0.83); };
  $("#mz-home").onclick = (e) => { e.stopPropagation(); center(); apply(); };
}
function esc(s) { return ("" + (s || "")).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" }[c])); }
const TIER_CLS = ["t0", "t1", "t2", "t3"];
function delveReveal(d) {
  const r = d.delved;
  let body;
  if (r.kind === "res") body = `<div class="delvereward">${RESES.filter((k) => r.res[k]).map((k) => `<span class="rwc">${ICON[k]}${fmt(r.res[k])}</span>`).join(" ")}</div><div class="delvelabel">A buried cache</div>`;
  else if (r.kind === "gems") body = `<div class="delvereward big">${ICON.gem} <b>${fmt(r.gems)}</b></div><div class="delvelabel">Shards of the old age</div>`;
  else { const it = r.relic; body = `<div class="delvereward"><span class="relic ${TIER_CLS[it.tier]}" style="display:inline-flex"><span class="rico">${ic(SLOT_ICON[it.slot])}</span><span class="rmid"><span class="rn">${it.slotName} <span class="rt">${it.tierName}</span></span><span class="rv">+${it.val}${AFF_SUFFIX[it.aff]}</span></span></span></div><div class="delvelabel">A relic, lost and found</div>`; }
  showModal(`<div class="ph">${ic("ruin")} The Ruin Gives Up Its Dead <span class="x">&times;</span></div><div class="bd" style="text-align:center">
    ${body}
    <p class="epitaph">&ldquo;${esc(d.epitaph)}&rdquo;</p>
    <div class="modal-actions"><button class="gbtn grn" id="delve-ok">Honor the fallen</button></div></div>`);
  modalOpen = null; $("#delve-ok").onclick = closeModal;
}
function delveDialog(x, y) {
  showModal(`<div class="ph">${ic("ruin")} A Fallen Giant <span class="x">&times;</span></div><div class="bd" style="text-align:center">
    <div class="delvegiant">${ic("ruin")}</div>
    <p style="color:#cbb88f;font-size:13.5px;line-height:1.5;margin:6px 0 12px">A carved titan lies here under the moss, older than any banner. Send a delving party to search the ruin. What sleeps in the stone may be a cache of stores, a hoard of shards, or a relic of the lost age.</p>
    <div class="statline"><span class="k">Provisions</span><span class="v">${ICON.grain}250 ${ICON.timber}250</span></div>
    <div class="modal-actions"><button class="gbtn grn" id="do-delve">Send a delving party</button></div></div>`);
  modalOpen = null;
  $("#do-delve").onclick = async () => {
    try { const v = await api("delve", { x, y }); sfx(v.delved.kind === "relic" ? "level" : "reward"); applyState(v); delveReveal(v); }
    catch (e) { toast(e.message, true); }
  };
}
function marchDialog(x, y) {
  const t = MAP.tiles.find((c) => c.x === x && c.y === y); if (!t) return;
  const garr = Object.entries(t.garrison).filter(([k, v]) => v > 0).map(([k, v]) => v + " " + MAP.units[k].name).join(", ");
  const loot = Object.entries(t.loot).map(([k, v]) => v + " " + k).join(", ");
  const rows = Object.keys(MAP.units).map((u) => `<div class="unitcard"><div class="em">${ic("sword")}</div><div class="mid"><div class="un">${MAP.units[u].name}</div><div class="st">you have ${MAP.troops[u] || 0} &middot; speed ${MAP.units[u].speed}</div></div><input type="number" min="0" max="${MAP.troops[u] || 0}" value="0" data-mu="${u}"/></div>`).join("");
  showModal(`<div class="ph">${ic("sword")} Raid camp &#183; level ${t.level} <span class="x">&times;</span></div><div class="bd">
    ${t.taunt ? `<div class="taunt">${ic("ruin")}<span>&ldquo;${esc(t.taunt)}&rdquo;</span></div>` : ""}
    <p style="color:#caa86a;text-align:center">Distance ${t.dist}. Defended by ${garr || "a few barbarians"}.</p>
    <p style="color:#caa86a;text-align:center;margin-bottom:10px">Spoils up to <b style="color:#f6e2a0">${loot}</b></p>
    ${rows}<div class="modal-actions"><button class="gbtn grn" id="do-march">Send the march</button></div></div>`);
  $("#do-march").onclick = async () => {
    const troops = {}; $$("#modal [data-mu]").forEach((i) => { const n = +i.value || 0; if (n > 0) troops[i.dataset.mu] = n; });
    if (!Object.keys(troops).length) return toast("Choose some soldiers to send.", true);
    try { const v = await api("march", { x, y, troops }); applyState(v); sfx("march"); toast("Your host marches out"); closeModal(); } catch (e) { toast(e.message, true); }
  };
}
// PvP: lay siege to a rival hold
function attackDialog(x, y) {
  const t = MAP.tiles.find((c) => c.x === x && c.y === y && c.type === "city"); if (!t) return;
  const rows = Object.keys(MAP.units).map((u) => `<div class="unitcard"><div class="em">${ic("sword")}</div><div class="mid"><div class="un">${MAP.units[u].name}</div><div class="st">you have ${MAP.troops[u] || 0} &middot; speed ${MAP.units[u].speed}</div></div><input type="number" min="0" max="${MAP.troops[u] || 0}" value="0" data-mu="${u}"/></div>`).join("");
  // intel from a prior scout (if any) and the scout action
  let intelHtml = "";
  if (t.intel) {
    const inT = t.intel; const ago = Math.max(0, Math.round(((Date.now() / 1000 + (S.now - (S._recv || S.now))) - inT.time)));
    const garr = Object.entries(inT.troops || {}).filter(([k, v]) => v).map(([k, v]) => fmt(v) + " " + MAP.units[k].name).join(", ");
    const stores = Object.entries(inT.res || {}).filter(([k, v]) => v).map(([k, v]) => `${ICON[k]}${fmt(v)}`).join(" ");
    intelHtml = `<div class="intel"><div class="intelh">${ic("map")} Scout report <span class="ago">${hms(ago)} ago</span></div>
      <div class="intelrow">Garrison: <b>${garr || "none seen"}</b></div>
      <div class="intelrow">Wall <b>${inT.wall}</b> &middot; Watchtower <b>${inT.watchtower}</b></div>
      <div class="intelrow">Stores: ${stores || "bare"}</div></div>`;
  }
  const canScout = (MAP.watchtower || 0) >= 1;
  const scoutBtn = canScout
    ? `<button class="gbtn" id="do-scout" style="background:linear-gradient(#9fc0d6,#5d7e9e);box-shadow:0 4px 0 #2e4858;color:#0e1a22">${ic("map")} Scout ${ICON.grain}300</button>`
    : `<button class="gbtn" disabled title="Raise a Watchtower to scout">Scout (needs Watchtower)</button>`;
  showModal(`<div class="ph">${ic("sword")} March on ${esc(t.name)} <span class="x">&times;</span></div><div class="bd">
    <div class="foehead"><div><div class="fn">${esc(t.name)}</div><div class="fs">Keep ${t.keep || 1} &middot; ${fmt(t.might || 0)} might &middot; distance ${t.dist}</div></div><div class="foesig">${ic("flag")}</div></div>
    ${intelHtml}
    <p style="color:#caa86a;text-align:center;margin:8px 0 10px;font-size:13px">${t.intel ? "Send enough to beat what your scouts found, and carry off a share of their stores." : "Scout first to learn their strength, or send your host and find out the hard way."}</p>
    ${rows}<div class="modal-actions" style="gap:8px">${scoutBtn}<button class="gbtn ox" id="do-attack">Sound the war horns</button></div></div>`);
  $("#do-attack").onclick = async () => {
    const troops = {}; $$("#modal [data-mu]").forEach((i) => { const n = +i.value || 0; if (n > 0) troops[i.dataset.mu] = n; });
    if (!Object.keys(troops).length) return toast("Choose some soldiers to send.", true);
    try { const v = await api("attack", { x, y, troops }); applyState(v); sfx("march"); toast("Your host marches to war"); closeModal(); } catch (e) { toast(e.message, true); }
  };
  const sb = $("#do-scout"); if (sb) sb.onclick = async () => {
    try { await api("scout", { x, y }); sfx("march"); toast("Your scout rides out for " + esc(t.name)); closeModal(); } catch (e) { toast(e.message, true); }
  };
}
function reportsHtml() {
  if (!S || !S.reports || !S.reports.length) return "";
  const rows = S.reports.slice(0, 6).map((r) => {
    const loot = Object.entries(r.loot || r.looted || {}).filter(([k, v]) => v).map(([k, v]) => fmt(v) + " " + k).join(", ");
    let line, win, label;
    if (r.kind === "scout") {
      const ok = !r.caught && !r.gone;
      return `<div class="repcard scout"><div class="rt">${r.gone ? `Scouted <b>${esc(r.target)}</b> &middot; the hold was gone` : r.caught ? `Your scout on <b>${esc(r.target)}</b> was turned back by their watchtower` : `Scouted <b>${esc(r.target)}</b> &middot; ${fmt(Object.values(r.intel.troops || {}).reduce((a, c) => a + c, 0))} soldiers, wall ${r.intel.wall}`}</div><div class="res scout">${ok ? "INTEL" : "CAUGHT"}</div></div>`;
    }
    if (r.kind === "spotted") {
      return `<div class="repcard def"><div class="rt">Your watchtower caught a scout from <b>${esc(r.scout)}</b>. They may march on you.</div><div class="res win">SPOTTED</div></div>`;
    }
    if (r.kind === "delve") {
      const rw = r.reward; const what = rw.kind === "res" ? "a buried cache" : rw.kind === "gems" ? fmt(rw.gems) + " shards" : "a " + (rw.relic.tierName || "") + " relic";
      return `<div class="repcard scout"><div class="rt">Delved a fallen giant's ruin &middot; found <b>${what}</b></div><div class="res scout">DELVED</div></div>`;
    }
    if (r.kind === "reinfsent") {
      const n = Object.values(r.troops || {}).reduce((a, c) => a + c, 0);
      return `<div class="repcard scout"><div class="rt">Sent <b>${fmt(n)}</b> soldiers to garrison <b>${esc(r.ally)}</b></div><div class="res scout">AID</div></div>`;
    }
    if (r.kind === "reinf") {
      const lost = Object.values(r.lost || {}).reduce((a, c) => a + c, 0);
      return `<div class="repcard ${r.win ? "win" : "loss"} def"><div class="rt">Your garrison at <b>${esc(r.ally)}</b> fought off <b>${esc(r.attacker)}</b> &middot; lost ${fmt(lost)} soldiers</div><div class="res ${r.win ? "win" : "loss"}">${r.win ? "HELD" : "FELL"}</div></div>`;
    }
    if (r.kind === "defense") {
      win = r.win; label = win ? "HELD" : "RAIDED";
      const lost = Object.values(r.lost || {}).reduce((a, c) => a + c, 0);
      line = `${esc(r.attacker)} stormed your hold &middot; you lost ${fmt(lost)} soldiers${r.raided && loot ? " &middot; they carried off <b>" + loot + "</b>" : win ? " &middot; you threw them back" : ""}`;
    } else if (r.kind === "city") {
      win = r.win; label = win ? "VICTORY" : "DEFEAT";
      line = `March on <b>${esc(r.target)}</b> &middot; lost ${Math.round(r.attLoss * 100)}% of the host${win && loot ? " &middot; looted <b>" + loot + "</b>" : ""}${r.flavor ? `<div class="repflav">${esc(r.flavor)}</div>` : ""}`;
    } else {
      win = r.win; label = win ? "VICTORY" : "DEFEAT";
      line = `Raid on a level <b>${r.level}</b> camp &middot; lost ${Math.round(r.attLoss * 100)}% of the host${win && loot ? " &middot; looted <b>" + loot + "</b>" : ""}${r.flavor ? `<div class="repflav">${esc(r.flavor)}</div>` : ""}`;
    }
    return `<div class="repcard ${win ? "win" : "loss"} ${r.kind === "defense" ? "def" : ""}"><div class="rt">${line}</div><div class="res ${win ? "win" : "loss"}">${label}</div></div>`;
  }).join("");
  return `<div style="margin-top:14px"><div class="ph" style="border:0;padding:6px 0">${ic("scroll")} Recent battles</div>${rows}</div>`;
}
$("#rl-map").onclick = openMap;

// active marches panel in the right column + countdowns
function renderMarches() {
  let panel = $("#marchpanel");
  if (!S.marches || !S.marches.length) { if (panel) panel.remove(); return; }
  if (!panel) { panel = el(`<div class="panel marches-panel" id="marchpanel" style="margin-bottom:12px"><div class="ph">${ic("horse")} Marches</div><div class="mbody"></div></div>`); $("#right").prepend(panel); }
  const body = panel.querySelector(".mbody"); body.innerHTML = "";
  S.marches.forEach((m, i) => {
    const returning = m.resolved;
    const scout = m.kind === "scout"; const reinf = m.kind === "reinforce";
    const dest = scout ? ("Scouting " + esc(m.target)) : reinf ? ("Aiding " + esc(m.target)) : m.kind === "city" ? ("War on " + esc(m.target)) : ("Raiding Lv " + m.level);
    body.appendChild(el(`<div class="qrow"><div class="em2">${scout ? ic("map") : reinf ? ic("ally") : returning ? ic("home") : ic("sword")}</div>
      <div class="qmid"><div class="qnm"><span>${returning && !scout ? "Returning home" : dest}</span><span class="lv">(${m.tx}|${m.ty})</span></div>
      <div class="qt"><span class="mleft" data-mi="${i}">--</span><span>${scout ? "riding out" : returning ? "with spoils" : "marching"}</span></div></div></div>`));
  });
}

window.addEventListener("resize", () => { if (S) renderObjective(); });

// ---- audio: start on first gesture, mute toggle ----
const sfx = (k) => { if (window.GA) GA.sfx(k); };
let audioStarted = false;
function kickAudio() { if (audioStarted) return; audioStarted = true; if (window.GA) GA.start(); updateMuteIcon(); }
document.addEventListener("pointerdown", kickAudio);
function updateMuteIcon() { const b = $("#ic-mute"); if (b) b.innerHTML = (window.GA && GA.isMuted()) ? SI.soundOff : SI.soundOn; }
$("#ic-mute").onclick = (e) => { e.stopPropagation(); if (!audioStarted) kickAudio(); if (window.GA) { GA.toggle(); updateMuteIcon(); } };
initIcons(); updateMuteIcon();

// auto-resume a session
if (TOKEN) enterGame();
