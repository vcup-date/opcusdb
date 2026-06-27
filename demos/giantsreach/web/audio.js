"use strict";
// Giantsreach audio. Fully procedural Web Audio (no files, no runtime AI, deterministic-ish).
// A warm generative ambient bed + synthesized SFX. Created on the first user gesture
// (browsers require it), low volume, with a mute toggle persisted in localStorage.
const GA = (() => {
  let ctx = null, master = null, musicGain = null, sfxGain = null, delay = null, lp = null;
  let started = false;
  let muted = localStorage.getItem("gr_muted") === "1";
  let chordTimer = null, chordI = 0;

  // A minor / modal warm progression (root midi, chord intervals). i - VI - III - VII.
  const PROG = [
    { root: 57, ch: [0, 3, 7] }, // Am
    { root: 53, ch: [0, 4, 7] }, // F
    { root: 60, ch: [0, 4, 7] }, // C
    { root: 55, ch: [0, 4, 7] }, // G
    { root: 57, ch: [0, 3, 7] }, // Am
    { root: 50, ch: [0, 3, 7] }, // Dm
    { root: 53, ch: [0, 4, 7] }, // F
    { root: 55, ch: [0, 4, 7] }, // G
  ];
  const PENTA = [0, 3, 5, 7, 10]; // for the bell shimmer
  const mtof = (m) => 440 * Math.pow(2, (m - 69) / 12);

  function build() {
    const AC = window.AudioContext || window.webkitAudioContext;
    ctx = new AC();
    master = ctx.createGain(); master.gain.value = muted ? 0 : 0.9; master.connect(ctx.destination);
    // shared space: a soft feedback delay
    delay = ctx.createDelay(); delay.delayTime.value = 0.42;
    const fb = ctx.createGain(); fb.gain.value = 0.32; delay.connect(fb); fb.connect(delay);
    const wet = ctx.createGain(); wet.gain.value = 0.5; delay.connect(wet); wet.connect(master);
    musicGain = ctx.createGain(); musicGain.gain.value = 0.18;
    lp = ctx.createBiquadFilter(); lp.type = "lowpass"; lp.frequency.value = 950; lp.Q.value = 0.6;
    musicGain.connect(lp); lp.connect(master); lp.connect(delay);
    sfxGain = ctx.createGain(); sfxGain.gain.value = 0.5; sfxGain.connect(master); sfxGain.connect(delay);
  }

  function playChord() {
    if (!ctx) return;
    const c = PROG[chordI % PROG.length]; chordI++;
    const t = ctx.currentTime, hold = 7.2;
    // bass drone
    voice(mtof(c.root - 12), t, hold, "sine", 0.5, musicGain);
    // pad notes (detuned pairs)
    c.ch.forEach((iv, k) => {
      const f = mtof(c.root + iv);
      voice(f, t, hold, "triangle", 0.32 - k * 0.04, musicGain, 0.0);
      voice(f * 1.005, t, hold, "sawtooth", 0.10, musicGain, 0.0);
    });
    // occasional high bell shimmer
    if (chordI % 2 === 0) {
      const n = PENTA[(chordI * 3) % PENTA.length];
      const f = mtof(c.root + 24 + n);
      voice(f, t + 1.5, 2.2, "triangle", 0.12, musicGain, 0.05);
    }
  }
  // a single enveloped note into a destination gain
  function voice(freq, when, dur, type, vol, dest, attack) {
    const o = ctx.createOscillator(); o.type = type; o.frequency.value = freq;
    const g = ctx.createGain(); const a = attack == null ? 1.4 : attack;
    g.gain.setValueAtTime(0.0001, when);
    g.gain.exponentialRampToValueAtTime(Math.max(0.0002, vol), when + a);
    g.gain.setValueAtTime(Math.max(0.0002, vol), when + dur - 2.0);
    g.gain.exponentialRampToValueAtTime(0.0001, when + dur);
    o.connect(g); g.connect(dest); o.start(when); o.stop(when + dur + 0.1);
  }

  // ---- sfx primitives ----
  function blip(f0, f1, dur, type, vol) {
    if (!ctx || muted) return;
    const t = ctx.currentTime; const o = ctx.createOscillator(); const g = ctx.createGain();
    o.type = type; o.frequency.setValueAtTime(f0, t); o.frequency.exponentialRampToValueAtTime(Math.max(1, f1), t + dur);
    g.gain.setValueAtTime(0.0001, t); g.gain.exponentialRampToValueAtTime(vol, t + 0.01); g.gain.exponentialRampToValueAtTime(0.0001, t + dur);
    o.connect(g); g.connect(sfxGain); o.start(t); o.stop(t + dur + 0.02);
  }
  function noise(dur, vol, hp) {
    if (!ctx || muted) return;
    const t = ctx.currentTime; const n = ctx.sampleRate * dur | 0; const b = ctx.createBuffer(1, n, ctx.sampleRate); const d = b.getChannelData(0);
    for (let i = 0; i < n; i++) d[i] = (Math.random() * 2 - 1) * (1 - i / n);
    const s = ctx.createBufferSource(); s.buffer = b; const f = ctx.createBiquadFilter(); f.type = "highpass"; f.frequency.value = hp || 400;
    const g = ctx.createGain(); g.gain.value = vol; s.connect(f); f.connect(g); g.connect(sfxGain); s.start(t);
  }
  function chord(notes, dur, type, vol, gap) {
    notes.forEach((f, i) => setTimeout(() => blip(f, f, dur, type || "triangle", vol || 0.18), i * (gap || 70)));
  }

  // ---- public sfx ----
  const SFX = {
    click: () => blip(620, 740, 0.05, "square", 0.06),
    build: () => { noise(0.18, 0.12, 250); blip(180, 90, 0.18, "sawtooth", 0.12); },
    done: () => chord([660, 880], 0.16, "triangle", 0.16, 110),
    coin: () => chord([1320, 1760, 2100], 0.09, "square", 0.07, 55),
    reward: () => chord([784, 1047, 1319, 1568], 0.14, "triangle", 0.12, 80),
    level: () => chord([523, 659, 784, 1047], 0.13, "triangle", 0.13, 75),
    march: () => { blip(160, 240, 0.5, "sawtooth", 0.14); setTimeout(() => blip(200, 300, 0.4, "sawtooth", 0.1), 120); },
    victory: () => chord([523, 659, 784, 1047, 1319], 0.18, "triangle", 0.15, 95),
    defeat: () => { blip(400, 120, 0.5, "sawtooth", 0.16); noise(0.5, 0.12, 200); },
  };

  // the baked composed theme (offline ACE-Step), looped; falls back to the procedural bed if it fails to load
  let musicEl = null, themeOk = false, themeGain = null;
  const THEME_VOL = 0.62;
  function startProcedural() { if (chordTimer) return; playChord(); chordTimer = setInterval(playChord, 7000); }
  function startTheme() {
    try {
      musicEl = new Audio("audio/theme.mp3"); musicEl.loop = true; musicEl.preload = "auto";
      try { const src = ctx.createMediaElementSource(musicEl); themeGain = ctx.createGain(); themeGain.gain.value = THEME_VOL; src.connect(themeGain); themeGain.connect(master); }
      catch (e) { musicEl.volume = muted ? 0 : 0.5; }
      musicEl.addEventListener("canplay", () => { themeOk = true; if (!muted) musicEl.play().catch(() => {}); }, { once: true });
      musicEl.addEventListener("error", () => { if (!themeOk) startProcedural(); }, { once: true });
      musicEl.load();
    } catch (e) { startProcedural(); }
  }
  // a dramatic battle cue that ducks the theme while a fighting scene plays
  let cueEl = null, cueGain = null, cueOk = false;
  function startCue() {
    if (cueEl) return;
    try {
      cueEl = new Audio("audio/battle.mp3"); cueEl.loop = true; cueEl.preload = "auto";
      try { const src = ctx.createMediaElementSource(cueEl); cueGain = ctx.createGain(); cueGain.gain.value = 0; src.connect(cueGain); cueGain.connect(master); }
      catch (e) {}
      cueEl.addEventListener("canplaythrough", () => { cueOk = true; }, { once: true });
      cueEl.load();
    } catch (e) {}
  }
  function cue() {
    if (muted || !ctx) return;
    if (!cueEl) startCue();
    if (themeGain) themeGain.gain.setTargetAtTime(0.10, ctx.currentTime, 0.15); // duck the theme
    if (cueEl) { try { cueEl.currentTime = 0; } catch (e) {} cueEl.play().catch(() => {}); if (cueGain) cueGain.gain.setTargetAtTime(0.85, ctx.currentTime, 0.05); }
  }
  function cueStop() {
    if (!ctx) return;
    if (cueGain) cueGain.gain.setTargetAtTime(0, ctx.currentTime, 0.4);
    if (cueEl) setTimeout(() => { try { cueEl.pause(); } catch (e) {} }, 700);
    if (themeGain) themeGain.gain.setTargetAtTime(muted ? 0 : THEME_VOL, ctx.currentTime, 0.6); // restore the theme
  }
  function start() {
    if (started) return; started = true;
    if (!ctx) build();
    if (ctx.state === "suspended") ctx.resume();
    startTheme();
  }
  function setMuted(m) {
    muted = m; localStorage.setItem("gr_muted", m ? "1" : "0");
    if (master && ctx) master.gain.setTargetAtTime(m ? 0 : 0.9, ctx.currentTime, 0.2);
    if (musicEl) { if (m) musicEl.pause(); else if (themeOk) musicEl.play().catch(() => {}); musicEl.volume = m ? 0 : musicEl.volume; }
    if (m && cueEl) { try { cueEl.pause(); } catch (e) {} }
  }
  return {
    start, isMuted: () => muted, setMuted,
    toggle: () => { setMuted(!muted); return muted; },
    sfx: (k) => { try { if (SFX[k]) SFX[k](); } catch (e) {} },
    cue: () => { try { cue(); } catch (e) {} },
    cueStop: () => { try { cueStop(); } catch (e) {} },
  };
})();
window.GA = GA;
