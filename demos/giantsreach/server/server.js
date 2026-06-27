"use strict";
// Giantsreach game server. Dependency-free Node.js (http, crypto, fs).
// Authoritative, time-based, deterministic. Formulas grounded in the research
// (Travian/RoK economy + combat, F2P gem-to-time + retention curves).
const http = require("http");
const fs = require("fs");
const path = require("path");
const crypto = require("crypto");

const ROOT = path.join(__dirname, "..");
const WEB = path.join(ROOT, "web");
const DB_FILE = path.join(ROOT, "db", "db.json");
const PORT = process.env.PORT || 8787;
const NOW = () => Date.now() / 1000; // seconds

// ----------------------------------------------------------------- config
const RES = ["grain", "timber", "stone", "iron", "gold"];
const BUILD = {
  keep:      { name: "The Keep",     icon: "keep",       max: 30, base: { grain: 90,  timber: 110, stone: 80,  iron: 40 },  cmult: 1.28, time: 60,  desc: "Heart of your hold. Raises the level cap of every other building and speeds all construction. Your might flows from here." },
  granary:   { name: "Granary",      icon: "granary",    max: 30, prod: "grain", base: { grain: 0,  timber: 70,  stone: 40,  iron: 10 }, cmult: 1.28, time: 40, desc: "Tills the fields and grows grain to feed your people and your armies." },
  sawmill:   { name: "Sawmill",      icon: "sawmill",    max: 30, prod: "timber", base: { grain: 50, timber: 0,   stone: 40,  iron: 10 }, cmult: 1.28, time: 40, desc: "Fells the timber that raises every wall and roof." },
  quarry:    { name: "Quarry",       icon: "quarry",     max: 30, prod: "stone", base: { grain: 50, timber: 70,  stone: 0,   iron: 10 }, cmult: 1.28, time: 40, desc: "Cuts the stone for keeps, walls and roads." },
  mine:      { name: "Iron Mine",    icon: "mine",       max: 30, prod: "iron",  base: { grain: 60, timber: 80,  stone: 50,  iron: 0 },  cmult: 1.30, time: 45, desc: "Digs the iron for blades, plate and tools." },
  market:    { name: "Market",       icon: "market",     max: 30, prod: "gold",  base: { grain: 90, timber: 90,  stone: 90,  iron: 40 }, cmult: 1.30, time: 50, desc: "Trade caravans bring a steady flow of gold to the treasury." },
  barracks:  { name: "Barracks",     icon: "barracks",   max: 30, base: { grain: 120, timber: 150, stone: 90,  iron: 80 }, cmult: 1.30, time: 60, desc: "Drills soldiers for your host. Each level trains them faster." },
  wall:      { name: "Wall",         icon: "wall",       max: 30, base: { grain: 40,  timber: 100, stone: 180, iron: 40 }, cmult: 1.30, time: 55, desc: "Ramparts of cut stone that multiply your defense against any siege." },
  watchtower:{ name: "Watchtower",   icon: "watchtower", max: 30, base: { grain: 40,  timber: 110, stone: 90,  iron: 30 }, cmult: 1.30, time: 50, desc: "Spies approaching marches and wards your hold against enemy scouts." },
};
const UNITS = {
  spearman:  { name: "Spearman",  cost: { grain: 20, timber: 25, iron: 10 }, time: 18, atk: 8,  dinf: 35, dcav: 60, hp: 24, speed: 14, carry: 40 },
  swordsman: { name: "Swordsman", cost: { grain: 30, timber: 15, iron: 35 }, time: 28, atk: 65, dinf: 35, dcav: 20, hp: 30, speed: 12, carry: 45 },
  archer:    { name: "Archer",    cost: { grain: 25, timber: 40, iron: 20 }, time: 24, atk: 40, dinf: 20, dcav: 30, hp: 20, speed: 16, carry: 30 },
  knight:    { name: "Knight",    cost: { grain: 80, timber: 40, iron: 80 }, time: 55, atk: 70, dinf: 40, dcav: 25, hp: 70, speed: 30, carry: 60 },
};
// gem packs (simulated purchase: "buy" just grants the gems). Tiered value ladder from research.
const PACKS = [
  { id: "p1", price: "$0.99",  gems: 80,   label: "Pouch of Shards" },
  { id: "p2", price: "$4.99",  gems: 500,  label: "Coffer of Shards" },
  { id: "p3", price: "$9.99",  gems: 1100, label: "Hoard of Shards" },
  { id: "p4", price: "$19.99", gems: 2400, label: "Vault of Shards" },
  { id: "p5", price: "$49.99", gems: 6500, label: "Titan's Trove" },
  { id: "p6", price: "$99.99", gems: 14000, label: "Crown of the Realm" },
];
const STARTER = { id: "starter", price: "$0.99", gems: 300, res: { grain: 5000, timber: 5000, stone: 4000, iron: 2000 }, label: "Founder's Pack" };
// escalating 7-day login calendar (research-backed). r = resource bundle.
const DAILY = [
  { gems: 50,  res: {} },
  { gems: 30,  res: { grain: 800, timber: 800 } },
  { gems: 100, res: {} },
  { gems: 40,  res: { stone: 800, iron: 500 } },
  { gems: 150, res: {} },
  { gems: 60,  res: { grain: 1500, timber: 1500, stone: 1200, iron: 800 } },
  { gems: 300, res: { gold: 2000 } },
];
// daily task ladder (research: tasks -> points -> chests at thresholds; drives daily return)
const TASKS = [
  { id: "build1", label: "Order a construction", key: "build", goal: 1, pts: 10 },
  { id: "build3", label: "Order three constructions", key: "build", goal: 3, pts: 15 },
  { id: "train", label: "Train 20 soldiers", key: "train", goal: 20, pts: 15 },
  { id: "tribute", label: "Claim your daily tribute", key: "login", goal: 1, pts: 10 },
  { id: "hasten", label: "Hasten a build with shards", key: "speedup", goal: 1, pts: 10 },
  { id: "spend", label: "Spend 3000 resources", key: "spend", goal: 3000, pts: 10 },
  { id: "raid", label: "Win a raid", key: "raid", goal: 1, pts: 20 },
  { id: "loot", label: "Bring home camp spoils", key: "loot", goal: 1, pts: 10 },
];
const TASK_CHESTS = [
  { at: 20, gems: 20, res: { grain: 500, timber: 500 } },
  { at: 40, gems: 30, res: { stone: 600, iron: 400 } },
  { at: 60, gems: 45, res: { grain: 1200, timber: 1200, stone: 900 } },
  { at: 80, gems: 70, res: { iron: 900, gold: 600 } },
  { at: 100, gems: 130, res: { grain: 2500, timber: 2500, stone: 1800, iron: 1200 } },
];
const CHEST_COOLDOWN = 4 * 3600;   // a free chest every 4 hours (appointment mechanic)
const CHEST_REWARD = { gems: 25, res: { grain: 800, timber: 800, stone: 600, iron: 400 } };
const FREE_FINISH = 300; // builds under 5 min finish free (Travian rule)
const BUILD_SLOTS = 2;   // 1 free + 1 (VIP-style) for a good feel
const PROD_BASE = 130;   // per-hour scale per resource building
const START_RES = { grain: 1200, timber: 1200, stone: 800, iron: 400, gold: 200 };

// ----------------------------------------------------------------- formulas
const r5 = (n) => Math.max(5, Math.round(n / 5) * 5);
function buildCost(bid, level) {
  const b = BUILD[bid]; const out = {};
  for (const k of Object.keys(b.base)) out[k] = b.base[k] ? r5(b.base[k] * Math.pow(b.cmult, level)) : 0;
  return out;
}
function buildTime(bid, level, keepLevel) {
  let t = BUILD[bid].time * Math.pow(1.16, level);
  t *= Math.pow(0.964, Math.max(0, keepLevel - 1)); // keep speeds all construction
  return Math.max(3, Math.floor(t));
}
function prodPerHour(bid, level) {
  if (!BUILD[bid].prod || level <= 0) return 0;
  return Math.round(PROD_BASE * level * Math.pow(1.14, level - 1));
}
function capacity(p) {
  const lv = (p.b.keep || 1) + Math.floor(((p.b.granary || 0) + (p.b.quarry || 0)) / 2);
  return Math.round((21.2 * Math.pow(1.2, lv) - 13.2)) * 100;
}
function unitTime(unit, barracksLevel) {
  return Math.max(2, Math.floor(UNITS[unit].time * Math.pow(0.92, Math.max(0, barracksLevel - 1))));
}
// gems to instantly finish `sec` of remaining time (CoC-style 4-anchor interpolation)
function gemsForTime(sec) {
  if (sec <= 0) return 0;
  if (sec <= 60) return 1;
  if (sec <= 3600) return Math.max(1, Math.round((19 / 3540) * (sec - 60) + 1));
  if (sec <= 86400) return Math.round((240 / 82800) * (sec - 3600) + 20);
  return Math.round((740 / 518400) * (sec - 86400) + 260);
}
function might(p) {
  let m = 0;
  for (const k of Object.keys(BUILD)) m += (p.b[k] || 0) * 12;
  for (const u of Object.keys(UNITS)) m += (p.t[u] || 0) * Math.round((UNITS[u].atk + UNITS[u].dinf + UNITS[u].dcav) / 8);
  return m;
}
function ratePerSec(p) {
  const r = { grain: 0, timber: 0, stone: 0, iron: 0, gold: 0 };
  for (const k of Object.keys(BUILD)) { const pr = BUILD[k].prod; if (pr) r[pr] += prodPerHour(k, p.b[k] || 0) / 3600; }
  const m = 1 + vipPerks(p).prod / 100 + allyProdBonus(p) / 100; // VIP + alliance production buffs
  for (const k in r) r[k] *= m;
  return r;
}

// ---- world map + marches + deterministic combat ----
const CAV = { knight: true };
const LOOTABLE = ["grain", "timber", "stone", "iron"];
function unitsCount(t) { let n = 0; for (const k in t) n += t[k] || 0; return n; }
// sanitize a client troop selection: only known units, non-negative integers (blocks negative-count duplication)
function cleanTroops(t) { const out = {}; if (t && typeof t === "object") for (const u in UNITS) { const n = Math.floor(Number(t[u]) || 0); if (n > 0) out[u] = n; } return out; }
function ihash(x, y) { let h = ((x * 73856093) ^ (y * 19349663)) >>> 0; h = (h ^ (h >>> 13)) >>> 0; h = (h * 1274126177) >>> 0; return h; }
function tileAt(x, y) {
  const h = ihash(x, y);
  if (h % 11 === 0) return { type: "camp", x, y, level: 1 + ((h >>> 5) % 6) };
  if (h % 37 === 3) return { type: "ruin", x, y };
  return null;
}
function campGarrison(level) { return { spearman: 5 * level, archer: 3 * level, knight: level >= 4 ? level : 0 }; }
function campLoot(level) { return { grain: level * 450, timber: level * 450, stone: level * 350, iron: level * 220 }; }

// ---- the voice of the realm: flavor text BAKED at build time (authored offline, never an AI call at runtime) ----
const FLAVOR = {
  // barbarian war-camp jeers, shown when you scout a camp to raid (picked deterministically by camp coords)
  taunts: [
    "The crows already know your banners, little lord. They wait where you will fall.",
    "Come closer. My axe has gone dull from the waiting.",
    "You smell of fresh bread and soft hands. We will fix the one and break the other.",
    "Three holds burned under this moon. Yours will make a fine fourth.",
    "Bring more men next time. These ones barely warmed the blade.",
    "Tall walls, soft throats. We have climbed taller for less.",
    "Pray to the sleeping giants, hold-lord. They did not answer the last who knelt here.",
    "We keep no gold and no mercy. Come and find which is true.",
    "Your steward counts grain while we count the days until your gate.",
    "Ride out, then. The mud is hungry and your horses look well fed.",
    "Every lord thinks his is the host that breaks us. We bury them facing home.",
    "The wind carries your cookfires to us. Soon it will carry your ash to them.",
  ],
  // battle narration appended to each raid report, chosen by the report's own seed
  victory: [
    "Your spears held the line and the camp broke like rotten ice underfoot.",
    "They ran before the second charge. The field is yours, and so are their stores.",
    "Smoke marks where their tents stood. Your banner stands where they did not.",
    "The barbarians paid in full for every insult. Your host rides home heavy with spoil.",
    "A clean rout. The survivors fled into the hills with nothing but their fear.",
    "Steel answered steel, and yours rang the longer. The wains roll back laden.",
  ],
  defeat: [
    "The barbarians sang as your banners drew back through the churned mud.",
    "Their line would not break, and yours did. What men remain limp homeward.",
    "A bitter day. The drums followed your retreat further than you would like.",
    "They were dug in deeper than the scouts swore. Your host paid for the lie.",
    "The field stayed theirs. Count the empty saddles and learn from them.",
    "Wounded pride and a thinner host. The camp still smokes, and not for you.",
  ],
  // the steward's rotating counsel, one per player per day (atmospheric, lightly useful)
  counsel: [
    "Stone wins the long wars, my lord. Raise the wall before the wolves think to test it.",
    "A full granary is a quiet realm. Hunger has toppled more keeps than any siege.",
    "Train in peace so you need not beg it in war. The drill yard is never wasted coin.",
    "The sleeping giants gave their bones to this land. We owe them a hold worth the ground.",
    "Send the host to the camps while the roads are dry. Spoil spends the same as taxes.",
    "Band your banner with others, lord. A lone flag is only a target with manners.",
    "Hasten what you must, but let the rest grow in its own hour. Shards are not endless.",
    "Answer the daily tribute. Small gifts, faithfully taken, build great hoards.",
    "A hero with the right blade is worth a company of bare-handed men. Visit the Forge.",
    "Watch the far banners climb the ladder. Pride is a fine whip for a slow steward.",
  ],
  // a short lore codex, the Chronicle of the Fallen
  chronicle: [
    { t: "The Reach", b: "They call this land Giantsreach, for here the last of the stone titans lay down and did not rise. Their carved heads break the hillsides like buried moons. Farmers plow around the fingers of a hand the size of a barn and think nothing of it now." },
    { t: "The Age That Ended", b: "Once the giants walked the ridgelines and the small folk sheltered in the warmth of their shadows. No chronicle agrees on what felled them. Some say a grief, some a god, some only time. What is certain is that they sleep, and the world they held up has been ours to carry since." },
    { t: "Why We Build", b: "A hold raised among the fallen is a promise that the small folk endure. Every wall is a wager against the next long dusk. Every banner is a name the giants never had the chance to keep." },
    { t: "The Barbarian Camps", b: "Not all who wander the Reach kneel to a lord. The camps take what the land offers and what the holds cannot defend. They are not lawless so much as bound to an older, colder law. Clear them, and the roads breathe easier for a while." },
    { t: "The Banners", b: "No single hold outlasts the Reach alone. Lords swear their banners together, lend their hands to one another's walls, and march as one when the drums demand. Alone you are a flame. Banded, a hearth." },
  ],
};
function pick(arr, seed) { return arr[(seed >>> 0) % arr.length]; }
// deterministic battle resolver (Travian-style mixed-arms, no RNG)
function combat(att, def, atkMult, defMult) {
  let Ainf = 0, Acav = 0;
  for (const u in att) { const c = att[u] || 0; if (!c) continue; const off = UNITS[u].atk * c; if (CAV[u]) Acav += off; else Ainf += off; }
  Ainf *= (atkMult || 1); Acav *= (atkMult || 1);
  const A = Ainf + Acav;
  if (A <= 0) return { attWins: false, winnerLoss: 1, loserLoss: 0, A: 0, D: 0 };
  const infS = Ainf / A, cavS = Acav / A;
  let D = 0, dcount = 0;
  for (const u in def) { const c = def[u] || 0; if (!c) continue; D += c * (infS * UNITS[u].dinf + cavS * UNITS[u].dcav); dcount += c; }
  D = Math.max(1, (D + 10) * (defMult || 1)); // base defense + wall multiplier
  const N = unitsCount(att) + dcount;
  const K = Math.max(1.2578, Math.min(1.5, 2 * (1.8592 - Math.pow(N, 0.015))));
  const attWins = A >= D;
  const ratio = attWins ? D / A : A / D; // loser power / winner power, in (0,1]
  const winnerLoss = Math.min(1, Math.pow(ratio, K));
  // the loser always loses more than the winner; a rout lets some escape, an even fight wipes them
  const loserLoss = Math.min(1, winnerLoss + (1 - ratio) * 0.85);
  return { attWins, winnerLoss, loserLoss, A, D };
}
// ---- the infirmary: a share of the slain are recoverable wounded, not lost outright ----
const WOUND_RATE = 0.30;   // 30% of casualties become wounded (the rest are lost)
const HEAL_FRACTION = 0.5; // tending a wounded soldier costs half its training cost
function woundCap(p) { return (p.b.keep || 1) * 60; } // the keep limits how many wounded can be sheltered
function totalWounded(p) { let s = 0; for (const u in (p.wounded || {})) s += p.wounded[u] || 0; return s; }
function woundedFromLost(lost) { const w = {}; for (const u in lost) { const c = Math.floor((lost[u] || 0) * WOUND_RATE); if (c > 0) w[u] = c; } return w; }
function addWounded(p, w) { // add capped to the keep's shelter; overflow is lost
  if (!p.wounded) p.wounded = { spearman: 0, swordsman: 0, archer: 0, knight: 0 };
  let room = Math.max(0, woundCap(p) - totalWounded(p));
  for (const u in w) { const add = Math.min(w[u] || 0, room); if (add > 0) { p.wounded[u] = (p.wounded[u] || 0) + add; room -= add; } }
}
function healCostOf(p) { const w = p.wounded || {}; const cost = {}; for (const u in w) { const n = w[u] || 0; if (!n || !UNITS[u]) continue; for (const k in UNITS[u].cost) cost[k] = (cost[k] || 0) + Math.ceil(UNITS[u].cost[k] * n * HEAL_FRACTION); } return cost; }
// ---- player-vs-player: attacking rival cities ----
const SHIELD_KEEP = 3;   // a hold below this keep is under beginner's peace: it cannot attack or be attacked
const PVP_LOOT = 0.5;    // fraction of a beaten defender's resources the victor can carry off
const WALL_DEF = 0.04;   // each wall level adds 4% to the defender's strength
function shielded(p) { return (p.b.keep || 1) < SHIELD_KEEP; }
function cityAt(x, y, exceptName) { for (const n of Object.keys(db.players)) { const q = db.players[n]; if (q && q.x === x && q.y === y && n !== exceptName) return n; } return null; }

// ---- daily tasks + free chest (retention) ----
function curDay() { return Math.floor(NOW() / 86400); }
function resetDailyTasks(p) {
  if (!p.tasks) p.tasks = { day: 0, counts: {}, claimed: [] };
  if (p.tasks.day !== curDay()) { p.tasks.day = curDay(); p.tasks.counts = {}; p.tasks.claimed = []; }
}
function bump(p, key, amt) { resetDailyTasks(p); p.tasks.counts[key] = (p.tasks.counts[key] || 0) + (amt == null ? 1 : amt); }
function tasksPoints(p) { resetDailyTasks(p); let pts = 0; for (const t of TASKS) if ((p.tasks.counts[t.key] || 0) >= t.goal) pts += t.pts; return Math.min(100, pts); }
function tasksView(p) {
  resetDailyTasks(p); const pts = tasksPoints(p);
  return {
    points: pts,
    list: TASKS.map((t) => ({ id: t.id, label: t.label, goal: t.goal, pts: t.pts, have: Math.min(t.goal, p.tasks.counts[t.key] || 0), done: (p.tasks.counts[t.key] || 0) >= t.goal })),
    chests: TASK_CHESTS.map((c) => ({ at: c.at, gems: c.gems, res: c.res, ready: pts >= c.at, claimed: p.tasks.claimed.includes(c.at) })),
  };
}
function sumCost(cost, mult) { let s = 0; for (const k in cost) s += cost[k] * (mult || 1); return s; }
// lifetime stat (never resets) -- powers permanent achievements
function life(p, key, amt) { if (!p.life) p.life = { raidsWon: 0, looted: 0, trained: 0, peakMight: 0, logins: 0 }; p.life[key] = (p.life[key] || 0) + (amt == null ? 1 : amt); }

// ---- achievements / milestones (permanent tiered goals, retention) ----
const ACHIEVEMENTS = [
  { id: "builder", name: "Master Builder", icon: "hammer", desc: "Raise the levels of your buildings", stat: "buildLevels", unit: "levels", tiers: [10, 25, 50, 100, 180], gems: [20, 40, 80, 150, 300] },
  { id: "warlord", name: "Warlord", icon: "sword", desc: "Win raids against the barbarian camps", stat: "raidsWon", unit: "raids won", tiers: [1, 10, 30, 75, 150], gems: [15, 40, 80, 160, 320] },
  { id: "plunder", name: "Plunderer", icon: "gem", desc: "Plunder resources from your enemies", stat: "looted", unit: "plundered", tiers: [5000, 50000, 250000, 1000000, 5000000], gems: [15, 40, 80, 160, 320] },
  { id: "drill", name: "Drillmaster", icon: "shield", desc: "Train soldiers for your host", stat: "trained", unit: "trained", tiers: [50, 250, 1000, 3000, 8000], gems: [15, 40, 80, 160, 320] },
  { id: "forge", name: "Forgemaster", icon: "anvil", desc: "Strike relics at the Forge", stat: "forged", unit: "forged", tiers: [1, 10, 30, 75, 150], gems: [20, 40, 80, 160, 320] },
  { id: "might", name: "Ascendant", icon: "trophy", desc: "Grow your peak might", stat: "peakMight", unit: "might", tiers: [500, 2500, 10000, 40000, 120000], gems: [20, 50, 100, 200, 400] },
  { id: "devotee", name: "Devotee", icon: "gift", desc: "Answer the daily tribute across many days", stat: "logins", unit: "days", tiers: [3, 7, 15, 30, 60], gems: [20, 40, 80, 160, 320] },
];
function achvStat(p, stat) {
  if (stat === "buildLevels") return Object.values(p.b || {}).reduce((a, c) => a + c, 0);
  if (stat === "forged") return p.drawN || 0;
  return (p.life && p.life[stat]) || 0;
}
function achvView(p) {
  return ACHIEVEMENTS.map((a) => {
    const have = achvStat(p, a.stat); const claimed = (p.achv && p.achv[a.id]) || 0;
    let nextIdx = claimed; // index of the next unclaimed tier
    const claimable = nextIdx < a.tiers.length && have >= a.tiers[nextIdx];
    const goalIdx = Math.min(nextIdx, a.tiers.length - 1);
    return {
      id: a.id, name: a.name, icon: a.icon, desc: a.desc, unit: a.unit,
      have, tiers: a.tiers, gems: a.gems, claimedTiers: claimed,
      maxed: claimed >= a.tiers.length, claimable,
      goal: a.tiers[goalIdx], reward: a.gems[goalIdx], tier: claimed,
    };
  });
}
function achvClaimable(p) { return achvView(p).some((a) => a.claimable); }

// ---- VIP track (accumulating points -> permanent empire buffs) ----
// points come from a free daily audience and (much faster) from buying shard packs.
const VIP_LEVELS = [
  { pts: 0, build: 0, prod: 0, slots: 0, march: 0 },
  { pts: 100, build: 3, prod: 3, slots: 0, march: 0 },
  { pts: 300, build: 5, prod: 5, slots: 0, march: 5 },
  { pts: 700, build: 8, prod: 8, slots: 1, march: 5 },
  { pts: 1500, build: 11, prod: 10, slots: 1, march: 8 },
  { pts: 3000, build: 14, prod: 13, slots: 1, march: 10 },
  { pts: 6000, build: 17, prod: 16, slots: 1, march: 12 },
  { pts: 12000, build: 20, prod: 19, slots: 2, march: 15 },
  { pts: 24000, build: 24, prod: 22, slots: 2, march: 18 },
  { pts: 48000, build: 28, prod: 26, slots: 2, march: 20 },
  { pts: 100000, build: 33, prod: 30, slots: 3, march: 25 },
];
const VIP_DAILY_PTS = 60; // the free daily VIP audience
function vipLevel(p) { const pts = (p.vip && p.vip.points) || 0; let lv = 0; for (let i = 0; i < VIP_LEVELS.length; i++) if (pts >= VIP_LEVELS[i].pts) lv = i; return lv; }
function vipPerks(p) { return VIP_LEVELS[vipLevel(p)]; }
function vipGain(p, amt) { if (!p.vip) p.vip = { points: 0, lastDaily: 0 }; p.vip.points += amt; }
function buildSpeedMult(p) { return 1 - vipPerks(p).build / 100; }
function vipDailyReady(p) { return curDay() > ((p.vip && p.vip.lastDaily) || 0); }
function marchCap(p) { return 5 + vipPerks(p).slots; }
function vipView(p) {
  const lv = vipLevel(p); const cur = VIP_LEVELS[lv]; const next = VIP_LEVELS[lv + 1] || null;
  return {
    points: (p.vip && p.vip.points) || 0, level: lv, max: VIP_LEVELS.length - 1,
    perks: cur, levels: VIP_LEVELS, nextAt: next ? next.pts : null, dailyReady: vipDailyReady(p),
    dailyPts: VIP_DAILY_PTS, marchCap: marchCap(p),
  };
}

// ---- season / battle pass (30 days, free + premium tracks, earned by daily play) ----
const SEASON_EPOCH = 1717200000;      // a fixed anchor; seasons tile forward from here
const SEASON_LEN = 30 * 86400;        // 30 days
const SEASON_LEVELS = 50;
const SEASON_XP_PER = 300;            // xp per pass level
const SEASON_NAMES = ["The Ashen Pact", "Banners of the Long Dusk", "The Gilded March", "Embers of the Giant-Kings", "The Frostbound Accord", "Crowns of the Reach"];
function seasonId() { return Math.floor((NOW() - SEASON_EPOCH) / SEASON_LEN); }
function seasonName(id) { const n = SEASON_NAMES.length; return SEASON_NAMES[((id % n) + n) % n]; }
function seasonSync(p) { const id = seasonId(); if (!p.season || p.season.id !== id) p.season = { id, xp: 0, level: 0, claimed: [], claimedP: [], premium: false }; }
function seasonGain(p, amt) { seasonSync(p); p.season.xp += amt; p.season.level = Math.min(SEASON_LEVELS, Math.floor(p.season.xp / SEASON_XP_PER)); }
function seasonFree(lv) { if (lv % 10 === 0) return { gems: 40 }; if (lv % 5 === 0) return { gems: 15 }; const a = 200 + lv * 40; return { res: { grain: a, timber: a, stone: a, iron: a } }; }
function seasonPrem(lv) { const r = { gems: 12 + Math.floor(lv / 4) * 4 }; if (lv % 10 === 0) r.gems += 120; if (lv % 5 === 0) r.res = { iron: 1500 + lv * 100 }; return r; }
function seasonClaimable(p) {
  seasonSync(p); const s = p.season;
  for (let l = 1; l <= s.level; l++) { if (!s.claimed.includes(l)) return true; if (s.premium && !s.claimedP.includes(l)) return true; }
  return false;
}
function grantReward(p, rw) { if (!rw) return 0; if (rw.gems) p.gems += rw.gems; for (const k in (rw.res || {})) p.r[k] = (p.r[k] || 0) + rw.res[k]; return rw.gems || 0; }
function seasonView(p) {
  seasonSync(p); const s = p.season; const endsAt = SEASON_EPOCH + (s.id + 1) * SEASON_LEN;
  const levels = [];
  for (let l = 1; l <= SEASON_LEVELS; l++) levels.push({ level: l, free: seasonFree(l), prem: seasonPrem(l), unlocked: l <= s.level, freeClaimed: s.claimed.includes(l), premClaimed: s.claimedP.includes(l) });
  return {
    id: s.id, name: seasonName(s.id), xp: s.xp, level: s.level, max: SEASON_LEVELS, xpPer: SEASON_XP_PER,
    nextXp: s.level < SEASON_LEVELS ? (s.level + 1) * SEASON_XP_PER : null, levelXp: s.level * SEASON_XP_PER,
    premium: s.premium, endsAt, levels, claimable: seasonClaimable(p),
  };
}

// ---- alliances (banners): create / join, alliance help (timer shaving), bonus, chat ----
const ALLY_CREATE_COST = 80;     // shards to found a banner
const ALLY_MAX = 30;             // members per banner
const HELP_MAX = 20;             // helps an order can receive
const HELP_FRACTION = 0.01;      // each help shaves 1% of total build time
const HELP_MIN = 60;             // ...but at least 60s
function allyOf(p) { return p.alliance ? db.alliances[p.alliance] : null; }
function allyProdBonus(p) { const a = allyOf(p); if (!a) return 0; return Math.min(10, (a.members || []).length); } // +1%/member up to +10%
function allyHelpShave(total) { return Math.max(HELP_MIN, Math.round(total * HELP_FRACTION)); }
function pruneAlliance(tag) {
  const a = db.alliances[tag]; if (!a) return;
  a.members = (a.members || []).filter((m) => db.players[m] && db.players[m].alliance === tag);
  if (!a.members.length) { delete db.alliances[tag]; return; }
  if (!a.members.includes(a.leader)) a.leader = a.members[0];
}
function allyChatPush(a, from, text) {
  a.chat = a.chat || []; a.chat.push({ from, text: String(text).slice(0, 160), t: NOW() });
  if (a.chat.length > 40) a.chat = a.chat.slice(-40);
}
// a member's open build orders that can still receive aid (for the roster help list)
function memberOrders(m, viewer) {
  const q = db.players[m] && db.players[m].queue || [];
  return q.map((it, i) => ({
    i, b: it.b, name: BUILD[it.b] ? BUILD[it.b].name : it.b, to: it.to, finish: it.finish, total: it.finish - it.start,
    helps: (it.helpedBy || []).length, maxed: (it.helpedBy || []).length >= HELP_MAX,
    helpedByYou: (it.helpedBy || []).includes(viewer),
  })).filter((o) => o.finish > NOW());
}
function allianceView(p) {
  const a = allyOf(p); if (!a) return null;
  return {
    tag: a.tag, name: a.name, leader: a.leader, created: a.created, bonus: allyProdBonus(p),
    members: (a.members || []).map((m) => {
      const q = db.players[m]; if (q) resolve(q);
      const reinf = (q && q.reinforcements) || {};
      let garrison = 0; for (const f in reinf) for (const u in reinf[f]) garrison += reinf[f][u] || 0;
      let yourReinf = 0; const mine = reinf[p.name]; if (mine) for (const u in mine) yourReinf += mine[u] || 0;
      return { name: m, might: q ? might(q) : 0, keep: q ? (q.b.keep || 1) : 1, leader: m === a.leader, orders: m === p.name ? [] : memberOrders(m, p.name), garrison, yourReinf };
    }).sort((x, y) => y.might - x.might),
    chat: (a.chat || []).slice(-30),
    helpMax: HELP_MAX,
  };
}
function alliancesList() {
  return Object.values(db.alliances).map((a) => {
    let mt = 0; for (const m of (a.members || [])) { const q = db.players[m]; if (q) mt += might(q); }
    return { tag: a.tag, name: a.name, members: (a.members || []).length, might: mt, leader: a.leader };
  }).sort((x, y) => y.might - x.might).slice(0, 40);
}

// ---- equipment (relics) + hero (a deterministic gacha with transparent pity) ----
const SLOTS = ["weapon", "armor", "banner", "charm"];
const SLOT_NAME = { weapon: "Warblade", armor: "Aegis", banner: "War Banner", charm: "Sigil" };
const SLOT_AFFIX = { weapon: "atk", armor: "def", banner: "speed", charm: "gold" };
const AFFIX_NAME = { atk: "Attack", def: "Defense", speed: "March Speed", gold: "Spoils" };
const TIERS = ["Common", "Rare", "Epic", "Legendary"];
const TIER_WEIGHT = [60, 28, 9, 3];
const AFFIX_RANGE = { atk: [[3, 6], [7, 12], [13, 20], [22, 34]], def: [[3, 6], [7, 12], [13, 20], [22, 34]], speed: [[4, 8], [9, 15], [16, 24], [26, 40]], gold: [[5, 10], [12, 20], [22, 34], [36, 55]] };
const FORGE_COST = 60;   // shards per draw
const PITY = 10;         // a guaranteed Epic or better every 10 draws (shown to the player)
function prng(seed) { let s = seed >>> 0; return () => { s = (s + 0x6D2B79F5) >>> 0; let t = Math.imul(s ^ (s >>> 15), 1 | s); t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t; return ((t ^ (t >>> 14)) >>> 0) / 4294967296; }; }
function hstr(s) { let h = 2166136261 >>> 0; for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); } return h >>> 0; }
function rollRelic(seed, minTier) {
  const r = prng(seed); const slot = SLOTS[Math.floor(r() * SLOTS.length)];
  let tier = 0; const tot = TIER_WEIGHT.reduce((a, c) => a + c, 0); let x = r() * tot;
  for (let i = 0; i < TIER_WEIGHT.length; i++) { if (x < TIER_WEIGHT[i]) { tier = i; break; } x -= TIER_WEIGHT[i]; }
  if (minTier != null) tier = Math.max(tier, minTier);
  const aff = SLOT_AFFIX[slot]; const [lo, hi] = AFFIX_RANGE[aff][tier];
  return { seed, slot, tier, aff, val: lo + Math.floor(r() * (hi - lo + 1)) };
}
function heroBonusOf(p) {
  const b = { atk: 0, def: 0, speed: 0, gold: 0 };
  for (const s of SLOTS) { const it = p.equipped && p.equipped[s]; if (it) b[it.aff] += it.val; }
  const lvl = (p.hero && p.hero.level) || 1; b.atk += (lvl - 1) * 2; b.def += (lvl - 1) * 2;
  return b;
}

// ----------------------------------------------------------------- state
let db = { accounts: {}, players: {}, alliances: {}, meta: { created: NOW() } };
function load() {
  let loaded = null;
  for (const f of [DB_FILE, DB_FILE + ".bak"]) { // fall back to the backup if the main file is corrupt
    try { loaded = JSON.parse(fs.readFileSync(f, "utf8")); if (loaded && typeof loaded === "object") break; }
    catch (e) { if (fs.existsSync(f)) console.error("db read failed (" + path.basename(f) + "):", e.message); }
  }
  if (loaded && typeof loaded === "object") db = loaded;
  if (!db.accounts) db.accounts = {}; if (!db.players) db.players = {}; if (!db.alliances) db.alliances = {}; if (!db.meta) db.meta = { created: NOW() };
}
// durable, atomic write: temp file -> backup the old -> rename over. A crash mid-write never corrupts db.json.
function writeDbNow() {
  try { fs.mkdirSync(path.dirname(DB_FILE), { recursive: true }); } catch (e) {}
  const tmp = DB_FILE + ".tmp";
  fs.writeFileSync(tmp, JSON.stringify(db));
  try { if (fs.existsSync(DB_FILE)) fs.copyFileSync(DB_FILE, DB_FILE + ".bak"); } catch (e) {}
  fs.renameSync(tmp, DB_FILE);
}
let saveT = null;
function save() { if (saveT) return; saveT = setTimeout(() => { saveT = null; try { writeDbNow(); } catch (e) { console.error("save failed:", e.message); } }, 800); }
function flushSave() { if (saveT) { clearTimeout(saveT); saveT = null; } try { writeDbNow(); } catch (e) { console.error("flush failed:", e.message); } }

function newPlayer(name) {
  return {
    name, created: NOW(), lastSeen: NOW(),
    r: Object.assign({}, START_RES), gems: 120, resTick: NOW(),
    b: { keep: 1, granary: 1, sawmill: 1, quarry: 1, mine: 1, market: 0, barracks: 1, wall: 0, watchtower: 0 },
    queue: [], t: { spearman: 0, swordsman: 0, archer: 0, knight: 0 }, train: [], wounded: { spearman: 0, swordsman: 0, archer: 0, knight: 0 },
    tutorial: 0, portrait: 0, login: { streak: 0, lastDay: 0, claimed: -1 }, boughtStarter: false,
    x: 400 + Math.floor(Math.random() * 80) - 40, y: 400 + Math.floor(Math.random() * 80) - 40,
    marches: [], reports: [], cleared: {}, intel: {},
    tasks: { day: 0, counts: {}, claimed: [] }, chest: { last: 0 },
    relics: [], equipped: { weapon: null, armor: null, banner: null, charm: null }, hero: { level: 1, xp: 0 }, pity: 0, drawN: 0,
    life: { raidsWon: 0, looted: 0, trained: 0, peakMight: 0, logins: 0 }, achv: {},
    vip: { points: 0, lastDaily: 0 },
    season: { id: -1, xp: 0, level: 0, claimed: [], claimedP: [], premium: false },
    alliance: null, reinforcements: {},
  };
}
// resolve a player-vs-player city attack on arrival (deterministic; mutates both attacker and defender)
function resolveCityAttack(p, m, now) {
  const d = db.players[m.target];
  const sent = Object.assign({}, m.troops);
  if (!d) { // the target city is gone: the host wheels around and comes home untouched
    m.surv = sent; m.loot = {};
    p.reports.unshift({ time: now, kind: "city", target: m.target, win: false, gone: true, attLoss: 0, sent });
    p.reports = p.reports.slice(0, 25); m.resolved = true; return;
  }
  resolve(d); // bring the defender current before the blow lands
  const hb = heroBonusOf(p);
  const defMult = 1 + (d.b.wall || 0) * WALL_DEF;
  // the defending host is the lord's own troops plus every allied reinforcement garrisoned with them
  const reinf = d.reinforcements || {};
  const defArmy = {}; for (const u in d.t) defArmy[u] = d.t[u] || 0;
  for (const from in reinf) for (const u in reinf[from]) defArmy[u] = (defArmy[u] || 0) + (reinf[from][u] || 0);
  const r = combat(m.troops, defArmy, 1 + hb.atk / 100, defMult);
  const attLossF = r.attWins ? r.winnerLoss : r.loserLoss;
  const defLossF = r.attWins ? r.loserLoss : r.winnerLoss;
  // attacker survivors + carry capacity
  const surv = {}; let carry = 0;
  for (const u in m.troops) { const k = Math.max(0, Math.round(m.troops[u] * (1 - attLossF))); surv[u] = k; carry += k * UNITS[u].carry; }
  // defender casualties (a share become wounded, sheltered immediately at home)
  const defLost = {};
  for (const u in d.t) { const k = Math.max(0, Math.round((d.t[u] || 0) * defLossF)); if (k) { defLost[u] = k; d.t[u] -= k; } }
  addWounded(d, woundedFromLost(defLost));
  // reinforcements take their share of the losses; each ally is told how their host fared
  for (const from in reinf) {
    const lost = {}; let any = false;
    for (const u in reinf[from]) { const k = Math.max(0, Math.round((reinf[from][u] || 0) * defLossF)); if (k) { lost[u] = k; reinf[from][u] -= k; any = true; } }
    if (Object.values(reinf[from]).every((v) => !v)) delete reinf[from];
    const fp = db.players[from];
    if (fp && any) { fp.reports.unshift({ time: now, kind: "reinf", ally: m.target, attacker: p.name, win: !r.attWins, lost }); fp.reports = fp.reports.slice(0, 25); }
  }
  // spoils: a victor carries off part of the defender's stores, capped by surviving carry
  const loot = {}; let looted = 0;
  if (r.attWins) {
    let want = {}, sum = 0;
    for (const k of LOOTABLE) { want[k] = Math.floor((d.r[k] || 0) * PVP_LOOT); sum += want[k]; }
    const f = (sum > carry && sum > 0) ? carry / sum : 1;
    for (const k of LOOTABLE) { const take = Math.floor(want[k] * f); loot[k] = take; d.r[k] = Math.max(0, (d.r[k] || 0) - take); looted += take; }
  }
  m.surv = surv; m.loot = loot;
  const attLost = {}; for (const u in m.troops) attLost[u] = (m.troops[u] || 0) - (surv[u] || 0);
  m.wounded = woundedFromLost(attLost); // the attacker's injured come home with the survivors
  if (r.attWins) { bump(p, "raid"); if (looted > 0) bump(p, "loot"); life(p, "looted", looted); seasonGain(p, 60); life(p, "raidsWon"); }
  // reports for both lords
  const flav = pick(r.attWins ? FLAVOR.victory : FLAVOR.defeat, (hstr(m.target) ^ (now >>> 4)) >>> 0);
  p.reports.unshift({ time: now, kind: "city", target: m.target, tx: m.tx, ty: m.ty, win: r.attWins, attLoss: attLossF, sent, loot, surv, wounded: m.wounded, flavor: flav });
  p.reports = p.reports.slice(0, 25);
  d.reports.unshift({ time: now, kind: "defense", attacker: p.name, win: !r.attWins, defLoss: defLossF, lost: defLost, looted: loot, raided: r.attWins });
  d.reports = d.reports.slice(0, 25);
  d.lastSeen = d.lastSeen || now;
  m.resolved = true;
}
// resolve a scout's arrival: reveal the target's strength, unless their watchtower outranks your own
function resolveScout(p, m, now) {
  const d = db.players[m.target];
  if (!d) { p.reports.unshift({ time: now, kind: "scout", target: m.target, gone: true }); p.reports = p.reports.slice(0, 25); m.resolved = true; return; }
  resolve(d);
  const myW = p.b.watchtower || 0, tgtW = d.b.watchtower || 0;
  if (tgtW > myW) { // their watchtower turns the scout back; the lord is warned
    p.reports.unshift({ time: now, kind: "scout", target: m.target, caught: true, tgtW });
    d.reports.unshift({ time: now, kind: "spotted", scout: p.name });
    d.reports = d.reports.slice(0, 25);
  } else {
    const intel = { time: now, troops: Object.assign({}, d.t), wall: d.b.wall || 0, watchtower: tgtW, keep: d.b.keep || 1, might: might(d), res: { grain: Math.floor(d.r.grain), timber: Math.floor(d.r.timber), stone: Math.floor(d.r.stone), iron: Math.floor(d.r.iron) }, wounded: Object.assign({}, d.wounded || {}) };
    if (!p.intel) p.intel = {}; p.intel[m.target] = intel;
    p.reports.unshift({ time: now, kind: "scout", target: m.target, caught: false, intel });
  }
  p.reports = p.reports.slice(0, 25);
  m.resolved = true;
}
// reinforcements arrive and garrison with an allied lord (they stay until recalled or slain)
function resolveReinforce(p, m, now) {
  const d = db.players[m.target];
  if (!d || d.alliance !== p.alliance || !p.alliance) { m.surv = Object.assign({}, m.troops); m.resolved = true; m.ret = now; return; } // ally gone or unbanded -> troops come home
  if (!d.reinforcements) d.reinforcements = {};
  const g = d.reinforcements[p.name] || {};
  for (const u in m.troops) g[u] = (g[u] || 0) + (m.troops[u] || 0);
  d.reinforcements[p.name] = g;
  p.reports.unshift({ time: now, kind: "reinfsent", ally: m.target, troops: Object.assign({}, m.troops) }); p.reports = p.reports.slice(0, 25);
  m.resolved = true; m.ret = now; // garrisoned, no return leg
}
// resolve all time-based state up to now (lazy, deterministic, survives restarts)
const resolving = new Set(); // re-entrancy guard: a scout/attack resolves its target, which must not recurse back
function resolve(p) {
  if (resolving.has(p.name)) return; // already mid-resolution (mutual scout/attack) -> use current state
  resolving.add(p.name);
  try { resolveInner(p); } finally { resolving.delete(p.name); }
}
function resolveInner(p) {
  const now = NOW();
  // builds
  if (p.queue && p.queue.length) {
    const keep = () => p.b.keep || 1;
    let again = true;
    while (again) {
      again = false;
      for (let i = 0; i < p.queue.length; i++) {
        if (p.queue[i].finish <= now) { const q = p.queue.splice(i, 1)[0]; p.b[q.b] = q.to; again = true; break; }
      }
    }
  }
  // training
  if (p.train && p.train.length) {
    for (let i = p.train.length - 1; i >= 0; i--) {
      const tr = p.train[i];
      const done = Math.min(tr.n, Math.floor((now - tr.start) / tr.per));
      if (done > tr.done) { p.t[tr.unit] = (p.t[tr.unit] || 0) + (done - tr.done); tr.done = done; }
      if (tr.done >= tr.n) p.train.splice(i, 1);
    }
  }
  // marches: resolve combat on arrival, return survivors + loot on the way back
  if (p.marches && p.marches.length) {
    for (let i = p.marches.length - 1; i >= 0; i--) {
      const m = p.marches[i];
      if (!m.resolved && now >= m.arrive && m.kind === "scout") resolveScout(p, m, now);
      if (!m.resolved && now >= m.arrive && m.kind === "reinforce") resolveReinforce(p, m, now);
      if (!m.resolved && now >= m.arrive && m.kind === "city") resolveCityAttack(p, m, now);
      if (!m.resolved && now >= m.arrive) {
        const def = campGarrison(m.level);
        const hb = heroBonusOf(p);
        const r = combat(m.troops, def, 1 + hb.atk / 100);
        const rep = { time: now, tx: m.tx, ty: m.ty, level: m.level, win: r.attWins, attLoss: r.attWins ? r.winnerLoss : 1, sent: Object.assign({}, m.troops) };
        rep.flavor = pick(r.attWins ? FLAVOR.victory : FLAVOR.defeat, ihash(m.tx, m.ty) ^ (now >>> 4));
        if (r.attWins) {
          const surv = {}; let carry = 0;
          for (const u in m.troops) { const keep = Math.max(0, Math.round(m.troops[u] * (1 - r.winnerLoss))); surv[u] = keep; carry += keep * UNITS[u].carry; }
          const src = campLoot(m.level); const loot = {}; let sum = 0;
          for (const k of LOOTABLE) { loot[k] = Math.round(src[k] * (1 + hb.gold / 100)); sum += loot[k]; }
          if (sum > carry && sum > 0) { const f = carry / sum; for (const k in loot) loot[k] = Math.floor(loot[k] * f); }
          m.surv = surv; m.loot = loot; rep.loot = loot; rep.surv = surv;
          const lostW = {}; for (const u in m.troops) lostW[u] = (m.troops[u] || 0) - (surv[u] || 0);
          m.wounded = woundedFromLost(lostW); rep.wounded = m.wounded;
          // hero earns experience from each cleared camp; level grants flat atk/def
          const gain = m.level * 8; p.hero.xp += gain; rep.heroXp = gain;
          while (p.hero.xp >= p.hero.level * 100) { p.hero.xp -= p.hero.level * 100; p.hero.level++; rep.heroLevel = p.hero.level; }
          bump(p, "raid"); if (Object.values(loot).some((v) => v > 0)) bump(p, "loot");
          life(p, "raidsWon"); life(p, "looted", Object.values(loot).reduce((a, c) => a + c, 0)); seasonGain(p, 50);
          p.cleared[m.tx + "," + m.ty] = now + 1800; // camp returns after 30 min
        } else { m.surv = {}; m.loot = {}; rep.loot = {}; rep.surv = {}; m.wounded = woundedFromLost(m.troops); rep.wounded = m.wounded; }
        p.reports.unshift(rep); p.reports = p.reports.slice(0, 25);
        m.resolved = true;
      }
      if (m.resolved && now >= m.ret) {
        for (const u in (m.surv || {})) p.t[u] = (p.t[u] || 0) + m.surv[u];
        for (const k in (m.loot || {})) p.r[k] = (p.r[k] || 0) + m.loot[k];
        if (m.wounded) addWounded(p, m.wounded); // the injured limp home to the infirmary
        p.marches.splice(i, 1);
      }
    }
  }
  // resources
  const dt = Math.max(0, now - (p.resTick || now));
  if (dt > 0) {
    const rate = ratePerSec(p); const cap = capacity(p);
    for (const k of RES) {
      const isGold = k === "gold";
      p.r[k] = (p.r[k] || 0) + rate[k] * dt;
      if (!isGold && p.r[k] > cap) p.r[k] = cap; // gold uncapped
    }
    p.resTick = now;
  }
  p.lastSeen = now;
  if (!p.life) p.life = { raidsWon: 0, looted: 0, trained: 0, peakMight: 0, logins: 0 };
  const mt = might(p); if (mt > (p.life.peakMight || 0)) p.life.peakMight = mt;
}
// hosts marching on this lord right now (so the defender is warned and can prepare)
function incomingFor(p) {
  const now = NOW(); const out = [];
  for (const on of Object.keys(db.players)) {
    if (on === p.name) continue;
    const q = db.players[on]; if (!q || !q.marches) continue;
    for (const m of q.marches) {
      if (m.kind === "city" && m.target === p.name && !m.resolved && m.arrive > now) {
        out.push({ from: on, fx: q.x, fy: q.y, depart: m.depart, arrive: m.arrive, total: unitsCount(m.troops) });
      }
    }
  }
  return out.sort((a, b) => a.arrive - b.arrive);
}
// snapshot for the client
function view(p) {
  resolve(p);
  const now = NOW(); const keep = p.b.keep || 1;
  const buildings = Object.keys(BUILD).map((bid) => {
    const lv = p.b[bid] || 0;
    return {
      id: bid, name: BUILD[bid].name, icon: BUILD[bid].icon, desc: BUILD[bid].desc,
      level: lv, max: BUILD[bid].max, prod: BUILD[bid].prod || null,
      cost: buildCost(bid, lv), time: Math.max(3, Math.floor(buildTime(bid, lv, keep) * buildSpeedMult(p))),
      prodNow: prodPerHour(bid, lv), prodNext: prodPerHour(bid, lv + 1),
    };
  });
  const queue = (p.queue || []).map((q) => ({ b: q.b, name: BUILD[q.b].name, icon: BUILD[q.b].icon, to: q.to, finish: q.finish, total: q.finish - q.start }));
  const train = (p.train || []).map((t) => ({ unit: t.unit, name: UNITS[t.unit].name, n: t.n, done: t.done, finish: t.start + t.per * t.n, per: t.per }));
  return {
    name: p.name, now, gems: Math.floor(p.gems), might: might(p), portrait: p.portrait || 0,
    res: { grain: Math.floor(p.r.grain), timber: Math.floor(p.r.timber), stone: Math.floor(p.r.stone), iron: Math.floor(p.r.iron), gold: Math.floor(p.r.gold) },
    rate: ratePerSec(p), cap: capacity(p), buildSlots: BUILD_SLOTS,
    buildings, queue, troops: p.t, train, units: UNITS,
    wounded: p.wounded || { spearman: 0, swordsman: 0, archer: 0, knight: 0 }, woundCap: woundCap(p), healCost: healCostOf(p),
    tutorial: p.tutorial, login: p.login, daily: DAILY, packs: PACKS, starter: STARTER, boughtStarter: !!p.boughtStarter,
    coords: { x: p.x, y: p.y },
    marches: (p.marches || []).map((m) => ({ tx: m.tx, ty: m.ty, level: m.level, depart: m.depart, arrive: m.arrive, ret: m.ret, resolved: m.resolved, troops: m.troops, kind: m.kind || "camp", target: m.target || null })),
    incoming: incomingFor(p),
    reports: (p.reports || []).slice(0, 12),
    tasks: tasksView(p),
    chest: { ready: NOW() - ((p.chest && p.chest.last) || 0) >= CHEST_COOLDOWN, nextAt: ((p.chest && p.chest.last) || 0) + CHEST_COOLDOWN, reward: CHEST_REWARD },
    relics: (p.relics || []).map((it) => ({ seed: it.seed, slot: it.slot, slotName: SLOT_NAME[it.slot], tier: it.tier, tierName: TIERS[it.tier], aff: it.aff, affName: AFFIX_NAME[it.aff], val: it.val })),
    equipped: Object.fromEntries(SLOTS.map((s) => { const it = p.equipped && p.equipped[s]; return [s, it ? { seed: it.seed, slot: it.slot, slotName: SLOT_NAME[it.slot], tier: it.tier, tierName: TIERS[it.tier], aff: it.aff, affName: AFFIX_NAME[it.aff], val: it.val } : null]; })),
    slots: SLOTS, slotNames: SLOT_NAME, affNames: AFFIX_NAME, tierNames: TIERS,
    hero: { level: p.hero.level, xp: p.hero.xp, xpNeed: p.hero.level * 100 },
    heroBonus: heroBonusOf(p), pity: p.pity, pityMax: PITY, forgeCost: FORGE_COST,
    achievements: achvView(p), achvClaim: achvClaimable(p),
    vip: vipView(p), season: seasonView(p),
    alliance: allianceView(p), allyTag: p.alliance || null,
    counsel: pick(FLAVOR.counsel, hstr(p.name) ^ curDay()), chronicle: FLAVOR.chronicle,
  };
}

// ----------------------------------------------------------------- auth
function hashPass(pass, salt) { return crypto.scryptSync(pass, salt, 32).toString("hex"); }
function tokenFor(name) { const t = crypto.randomBytes(18).toString("hex"); db.accounts[name].token = t; return t; }
function authName(req) {
  const t = req.headers["x-token"]; if (!t) return null;
  for (const n of Object.keys(db.accounts)) if (db.accounts[n].token === t) return n;
  return null;
}

// ----------------------------------------------------------------- http
function send(res, code, obj) { const s = JSON.stringify(obj); res.writeHead(code, { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(s) }); res.end(s); }
function body(req) { return new Promise((resolve) => { let b = ""; req.on("data", (c) => { b += c; if (b.length > 1e6) req.destroy(); }); req.on("end", () => { try { resolve(b ? JSON.parse(b) : {}); } catch (e) { resolve({}); } }); }); }
const MIME = { ".html": "text/html; charset=utf-8", ".js": "application/javascript; charset=utf-8", ".css": "text/css; charset=utf-8", ".png": "image/png", ".jpg": "image/jpeg", ".ogg": "audio/ogg", ".mp3": "audio/mpeg", ".webp": "image/webp", ".mp4": "video/mp4", ".svg": "image/svg+xml" };
function serveStatic(req, res) {
  let p = decodeURIComponent(req.url.split("?")[0]); if (p === "/") p = "/index.html";
  const fp = path.normalize(path.join(WEB, p));
  if (!fp.startsWith(WEB)) { res.writeHead(403); return res.end(); }
  fs.readFile(fp, (err, data) => {
    if (err) { res.writeHead(404); return res.end("not found"); }
    res.writeHead(200, { "Content-Type": MIME[path.extname(fp)] || "application/octet-stream" });
    res.end(data);
  });
}

const ROUTES = {
  "POST /api/register": async (req, res, b) => {
    const name = (b.name || "").trim().slice(0, 16); const pass = (b.pass || "");
    if (!/^[A-Za-z0-9_ ]{2,16}$/.test(name)) return send(res, 400, { err: "Name must be 2-16 letters, numbers, spaces." });
    if (pass.length < 3) return send(res, 400, { err: "Password must be at least 3 characters." });
    if (db.accounts[name]) return send(res, 400, { err: "That name already holds a city." });
    const salt = crypto.randomBytes(8).toString("hex");
    db.accounts[name] = { salt, hash: hashPass(pass, salt), token: null };
    db.players[name] = newPlayer(name);
    const token = tokenFor(name); save();
    send(res, 200, { token, name });
  },
  "POST /api/login": async (req, res, b) => {
    const name = (b.name || "").trim(); const acc = db.accounts[name];
    if (!acc || acc.hash !== hashPass(b.pass || "", acc.salt)) return send(res, 400, { err: "Wrong name or password." });
    if (!db.players[name]) db.players[name] = newPlayer(name);
    const token = tokenFor(name); save();
    send(res, 200, { token, name });
  },
  "POST /api/guest": async (req, res) => {
    let name; let i = 0; do { name = "Lord" + Math.floor(1000 + Math.random() * 9000); i++; } while (db.accounts[name] && i < 50);
    const salt = crypto.randomBytes(8).toString("hex");
    db.accounts[name] = { salt, hash: hashPass(crypto.randomBytes(6).toString("hex"), salt), token: null };
    db.players[name] = newPlayer(name);
    const token = tokenFor(name); save();
    send(res, 200, { token, name, guest: true });
  },
  "GET /api/state": async (req, res) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; const v = view(p); save(); send(res, 200, v);
  },
  "POST /api/build": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const bid = b.b; if (!BUILD[bid]) return send(res, 400, { err: "no such building" });
    if ((p.queue || []).length >= BUILD_SLOTS) return send(res, 400, { err: "All build queues are busy." });
    const lv = p.b[bid] || 0;
    if (lv >= BUILD[bid].max) return send(res, 400, { err: "Already at maximum level." });
    if (bid !== "keep" && lv + 1 > (p.b.keep || 1)) return send(res, 400, { err: "Raise the Keep first to build higher." });
    const cost = buildCost(bid, lv);
    for (const k of Object.keys(cost)) if ((p.r[k] || 0) < cost[k]) return send(res, 400, { err: "Not enough resources." });
    for (const k of Object.keys(cost)) p.r[k] -= cost[k];
    const t = Math.max(3, Math.floor(buildTime(bid, lv, p.b.keep || 1) * buildSpeedMult(p))); const now = NOW();
    p.queue.push({ b: bid, to: lv + 1, start: now, finish: now + t });
    bump(p, "build"); bump(p, "spend", sumCost(cost)); seasonGain(p, 25);
    save(); send(res, 200, view(p));
  },
  "POST /api/speedup": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const i = b.i | 0; const q = p.queue[i]; if (!q) return send(res, 400, { err: "nothing there" });
    const remain = q.finish - NOW();
    if (remain <= FREE_FINISH) { q.finish = NOW(); bump(p, "speedup"); seasonGain(p, 15); resolve(p); save(); return send(res, 200, view(p)); }
    const cost = gemsForTime(remain);
    if (p.gems < cost) return send(res, 400, { err: "Not enough shards. Need " + cost + "." });
    p.gems -= cost; q.finish = NOW(); bump(p, "speedup"); seasonGain(p, 15); resolve(p); save(); send(res, 200, view(p));
  },
  "POST /api/cancel": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const i = b.i | 0; const q = p.queue[i]; if (!q) return send(res, 400, { err: "nothing" });
    const cost = buildCost(q.b, q.to - 1); for (const k of Object.keys(cost)) p.r[k] = (p.r[k] || 0) + Math.floor(cost[k] * 0.6);
    p.queue.splice(i, 1); save(); send(res, 200, view(p));
  },
  "POST /api/train": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const unit = b.unit; const num = Math.max(1, Math.min(500, b.n | 0));
    if (!UNITS[unit]) return send(res, 400, { err: "no such unit" });
    if ((p.b.barracks || 0) < 1) return send(res, 400, { err: "Build a Barracks first." });
    const c = UNITS[unit].cost;
    for (const k of Object.keys(c)) if ((p.r[k] || 0) < c[k] * num) return send(res, 400, { err: "Not enough resources for " + num + "." });
    for (const k of Object.keys(c)) p.r[k] -= c[k] * num;
    const per = unitTime(unit, p.b.barracks || 1);
    p.train.push({ unit, n: num, done: 0, start: NOW(), per });
    bump(p, "train", num); bump(p, "spend", sumCost(c, num)); life(p, "trained", num); seasonGain(p, Math.min(num, 60));
    save(); send(res, 200, view(p));
  },
  // ---- the infirmary: tend the wounded back into the host for a share of their cost ----
  "POST /api/heal": async (req, res) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    if (!p.wounded) p.wounded = { spearman: 0, swordsman: 0, archer: 0, knight: 0 };
    const tot = totalWounded(p); if (tot <= 0) return send(res, 400, { err: "No wounded to tend." });
    const cost = healCostOf(p);
    for (const k of Object.keys(cost)) if ((p.r[k] || 0) < cost[k]) return send(res, 400, { err: "Not enough resources to tend the wounded." });
    for (const k of Object.keys(cost)) p.r[k] -= cost[k];
    for (const u in p.wounded) { if (p.wounded[u]) p.t[u] = (p.t[u] || 0) + p.wounded[u]; }
    p.wounded = { spearman: 0, swordsman: 0, archer: 0, knight: 0 };
    save(); send(res, 200, Object.assign({ healed: tot }, view(p)));
  },
  "POST /api/buygems": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    if (b.pack === "starter") {
      if (p.boughtStarter) return send(res, 400, { err: "Founder's Pack already claimed." });
      p.boughtStarter = true; p.gems += STARTER.gems;
      for (const k of Object.keys(STARTER.res)) p.r[k] = (p.r[k] || 0) + STARTER.res[k];
      vipGain(p, STARTER.gems); // shard packs grant VIP points 1:1
      save(); return send(res, 200, Object.assign({ bought: STARTER.gems }, view(p)));
    }
    const pack = PACKS.find((x) => x.id === b.pack); if (!pack) return send(res, 400, { err: "no such pack" });
    p.gems += pack.gems; vipGain(p, pack.gems); save(); // simulated purchase: grant shards + VIP points
    send(res, 200, Object.assign({ bought: pack.gems }, view(p)));
  },
  "POST /api/daily": async (req, res) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const day = Math.floor(NOW() / 86400);
    if (p.login.lastDay === day && p.login.claimed === day) return send(res, 400, { err: "Already claimed today. Return tomorrow." });
    p.login.streak = (p.login.lastDay === day - 1) ? (p.login.streak + 1) : 1;
    p.login.lastDay = day; p.login.claimed = day;
    const idx = (p.login.streak - 1) % DAILY.length; const rw = DAILY[idx];
    p.gems += rw.gems; for (const k of Object.keys(rw.res || {})) p.r[k] = (p.r[k] || 0) + rw.res[k];
    bump(p, "login"); life(p, "logins"); seasonGain(p, 120);
    save(); send(res, 200, Object.assign({ reward: rw, streak: p.login.streak, idx }, view(p)));
  },
  "POST /api/tutorial": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; p.tutorial = Math.max(p.tutorial, b.step | 0); save(); send(res, 200, { tutorial: p.tutorial });
  },
  "POST /api/portrait": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; p.portrait = Math.max(0, Math.min(3, b.i | 0)); save(); send(res, 200, { portrait: p.portrait });
  },
  "GET /api/leaderboard": async (req, res) => {
    const me = authName(req);
    const all = Object.values(db.players).map((p) => { resolve(p); return { name: p.name, might: might(p), keep: p.b.keep || 1, portrait: p.portrait || 0, tag: p.alliance || null, raidsWon: (p.life && p.life.raidsWon) || 0 }; });
    const byMight = all.slice().sort((a, b) => b.might - a.might);
    const lords = byMight.slice(0, 20).map((r, i) => Object.assign({ rank: i + 1 }, r));
    const mi = byMight.findIndex((r) => r.name === me);
    const you = mi >= 0 ? { rank: mi + 1, might: byMight[mi].might, keep: byMight[mi].keep, portrait: byMight[mi].portrait, tag: byMight[mi].tag, name: me } : null;
    const raiders = all.filter((r) => r.raidsWon > 0).sort((a, b) => b.raidsWon - a.raidsWon).slice(0, 15).map((r, i) => ({ rank: i + 1, name: r.name, raidsWon: r.raidsWon, portrait: r.portrait, tag: r.tag }));
    const banners = Object.values(db.alliances).map((a) => { let mt = 0; for (const m of (a.members || [])) { const q = db.players[m]; if (q) mt += might(q); } return { tag: a.tag, name: a.name, members: (a.members || []).length, might: mt }; })
      .sort((a, b) => b.might - a.might).slice(0, 15).map((r, i) => Object.assign({ rank: i + 1 }, r));
    send(res, 200, { lords, you, raiders, banners, total: all.length });
  },
  "GET /api/map": async (req, res) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p); const R = 16; const now = NOW(); const tiles = [];
    for (let dx = -R; dx <= R; dx++) for (let dy = -R; dy <= R; dy++) {
      const x = p.x + dx, y = p.y + dy; if (x === p.x && y === p.y) continue;
      const t = tileAt(x, y); if (!t) continue;
      if (t.type === "camp") { const until = (p.cleared || {})[x + "," + y] || 0; tiles.push({ type: "camp", x, y, level: t.level, cleared: until > now, dist: Math.round(Math.hypot(dx, dy) * 10) / 10, garrison: campGarrison(t.level), loot: campLoot(t.level), taunt: pick(FLAVOR.taunts, ihash(x, y)) }); }
      else tiles.push({ type: t.type, x, y, dist: Math.round(Math.hypot(dx, dy) * 10) / 10 });
    }
    for (const m of Object.keys(db.players)) {
      const q = db.players[m]; if (!q || m === n) continue;
      if (Math.abs(q.x - p.x) <= R && Math.abs(q.y - p.y) <= R) {
        resolve(q);
        tiles.push({ type: "city", x: q.x, y: q.y, name: q.name, might: might(q), keep: q.b.keep || 1, shielded: shielded(q), allied: !!(p.alliance && q.alliance === p.alliance), dist: Math.round(Math.hypot(q.x - p.x, q.y - p.y) * 10) / 10, intel: (p.intel && p.intel[q.name]) || null });
      }
    }
    send(res, 200, { center: { x: p.x, y: p.y }, name: p.name, troops: p.t, units: UNITS, tiles, R, shielded: shielded(p), shieldKeep: SHIELD_KEEP, watchtower: p.b.watchtower || 0 });
  },
  "POST /api/march": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const tx = b.x | 0, ty = b.y | 0; const troops = cleanTroops(b.troops);
    const t = tileAt(tx, ty); if (!t || t.type !== "camp") return send(res, 400, { err: "No camp there to raid." });
    if (((p.cleared || {})[tx + "," + ty] || 0) > NOW()) return send(res, 400, { err: "That camp is cleared. It will return soon." });
    for (const u in troops) if ((troops[u] | 0) > (p.t[u] || 0)) return send(res, 400, { err: "You do not have that many " + (UNITS[u] ? UNITS[u].name : u) + "." });
    const total = unitsCount(troops); if (total <= 0) return send(res, 400, { err: "Send at least one soldier." });
    const cap = marchCap(p);
    if ((p.marches || []).filter((m) => m.kind !== "scout").length >= cap) return send(res, 400, { err: "All your marches are out (" + cap + " max)." });
    const dist = Math.hypot(tx - p.x, ty - p.y); let speed = Infinity;
    for (const u in troops) if (troops[u]) speed = Math.min(speed, UNITS[u].speed);
    const hb = heroBonusOf(p);
    const travel = Math.max(12, Math.round(dist / speed * 400 / (1 + (hb.speed + vipPerks(p).march) / 100)));
    for (const u in troops) p.t[u] -= troops[u];
    const now = NOW();
    p.marches.push({ tx, ty, level: t.level, troops: Object.assign({}, troops), depart: now, arrive: now + travel, ret: now + travel * 2, resolved: false });
    save(); send(res, 200, view(p));
  },
  "POST /api/attack": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const tx = b.x | 0, ty = b.y | 0; const troops = cleanTroops(b.troops);
    const target = cityAt(tx, ty, n);
    if (!target) return send(res, 400, { err: "No rival hold stands there." });
    const d = db.players[target]; resolve(d);
    if (shielded(p)) return send(res, 400, { err: "Raise your Keep to " + SHIELD_KEEP + " before you make war." });
    if (shielded(d)) return send(res, 400, { err: target + " is under beginner's peace and cannot be attacked." });
    if (p.alliance && d.alliance === p.alliance) return send(res, 400, { err: "You share a banner with " + target + "." });
    for (const u in troops) if ((troops[u] | 0) > (p.t[u] || 0)) return send(res, 400, { err: "You do not have that many " + (UNITS[u] ? UNITS[u].name : u) + "." });
    if (unitsCount(troops) <= 0) return send(res, 400, { err: "Send at least one soldier." });
    const cap = marchCap(p);
    if ((p.marches || []).filter((m) => m.kind !== "scout").length >= cap) return send(res, 400, { err: "All your marches are out (" + cap + " max)." });
    const dist = Math.hypot(tx - p.x, ty - p.y); let speed = Infinity;
    for (const u in troops) if (troops[u]) speed = Math.min(speed, UNITS[u].speed);
    const hb = heroBonusOf(p);
    const travel = Math.max(12, Math.round(dist / speed * 400 / (1 + (hb.speed + vipPerks(p).march) / 100)));
    for (const u in troops) p.t[u] -= troops[u];
    const now = NOW();
    p.marches.push({ kind: "city", target, tx, ty, troops: Object.assign({}, troops), depart: now, arrive: now + travel, ret: now + travel * 2, resolved: false });
    save(); send(res, 200, view(p));
  },
  "POST /api/scout": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const tx = b.x | 0, ty = b.y | 0;
    const target = cityAt(tx, ty, n);
    if (!target) return send(res, 400, { err: "No rival hold there to scout." });
    if ((p.b.watchtower || 0) < 1) return send(res, 400, { err: "Raise a Watchtower to send out scouts." });
    if ((p.marches || []).filter((m) => m.kind === "scout").length >= 3) return send(res, 400, { err: "Your scouts are all out (3 at a time)." });
    const cost = { grain: 300, iron: 150 };
    for (const k of Object.keys(cost)) if ((p.r[k] || 0) < cost[k]) return send(res, 400, { err: "A scout needs " + cost.grain + " grain and " + cost.iron + " iron in provisions." });
    for (const k of Object.keys(cost)) p.r[k] -= cost[k];
    const dist = Math.hypot(tx - p.x, ty - p.y); const SCOUT_SPEED = 22; // scouts ride light and fast
    const travel = Math.max(8, Math.round(dist / SCOUT_SPEED * 400 / (1 + vipPerks(p).march / 100)));
    const now = NOW();
    p.marches.push({ kind: "scout", target, tx, ty, depart: now, arrive: now + travel, ret: now + travel, resolved: false });
    save(); send(res, 200, view(p));
  },
  "GET /api/reports": async (req, res) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p); send(res, 200, { reports: (p.reports || []).slice(0, 25) });
  },
  "POST /api/taskchest": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p); resetDailyTasks(p);
    const at = b.at | 0; const ch = TASK_CHESTS.find((c) => c.at === at);
    if (!ch) return send(res, 400, { err: "no such chest" });
    if (tasksPoints(p) < at) return send(res, 400, { err: "Earn " + at + " task points first." });
    if (p.tasks.claimed.includes(at)) return send(res, 400, { err: "Already claimed." });
    p.tasks.claimed.push(at); p.gems += ch.gems;
    for (const k of Object.keys(ch.res || {})) p.r[k] = (p.r[k] || 0) + ch.res[k];
    save(); send(res, 200, Object.assign({ gained: ch.gems }, view(p)));
  },
  "POST /api/chest": async (req, res) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p); if (!p.chest) p.chest = { last: 0 };
    if (NOW() - p.chest.last < CHEST_COOLDOWN) return send(res, 400, { err: "The chest is not ready yet." });
    p.chest.last = NOW(); p.gems += CHEST_REWARD.gems;
    for (const k of Object.keys(CHEST_REWARD.res || {})) p.r[k] = (p.r[k] || 0) + CHEST_REWARD.res[k];
    save(); send(res, 200, Object.assign({ gained: CHEST_REWARD.gems }, view(p)));
  },
  // ---- equipment + heroes: the Forge (gacha with transparent pity) ----
  "POST /api/forge": async (req, res) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    if (p.gems < FORGE_COST) return send(res, 400, { err: "Not enough shards. The Forge costs " + FORGE_COST + "." });
    p.gems -= FORGE_COST; p.drawN = (p.drawN || 0) + 1;
    const seed = (hstr(p.name) ^ Math.imul(p.drawN, 0x9e3779b1)) >>> 0;
    const minTier = (p.pity || 0) >= PITY - 1 ? 2 : null; // pity guarantees Epic+ on the 10th
    const it = rollRelic(seed, minTier);
    if (it.tier >= 2) p.pity = 0; else p.pity = (p.pity || 0) + 1;
    p.relics.push(it);
    const out = { seed: it.seed, slot: it.slot, slotName: SLOT_NAME[it.slot], tier: it.tier, tierName: TIERS[it.tier], aff: it.aff, affName: AFFIX_NAME[it.aff], val: it.val };
    save(); send(res, 200, Object.assign({ drew: out }, view(p)));
  },
  "POST /api/equip": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const seed = b.seed >>> 0; const idx = (p.relics || []).findIndex((it) => it.seed === seed);
    if (idx < 0) return send(res, 400, { err: "no such relic" });
    const it = p.relics[idx];
    const cur = p.equipped[it.slot]; // swap: equipped item returns to the stash
    p.relics.splice(idx, 1); if (cur) p.relics.push(cur);
    p.equipped[it.slot] = it;
    save(); send(res, 200, view(p));
  },
  "POST /api/unequip": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const slot = b.slot; if (!SLOTS.includes(slot)) return send(res, 400, { err: "no such slot" });
    const cur = p.equipped[slot]; if (!cur) return send(res, 400, { err: "nothing equipped" });
    p.equipped[slot] = null; p.relics.push(cur);
    save(); send(res, 200, view(p));
  },
  // ---- achievements: claim every newly-earned tier of one milestone ----
  "POST /api/achv": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p); if (!p.achv) p.achv = {};
    const a = ACHIEVEMENTS.find((x) => x.id === b.id); if (!a) return send(res, 400, { err: "no such milestone" });
    const have = achvStat(p, a.stat); let claimed = p.achv[a.id] || 0; let gained = 0; let tiers = 0;
    while (claimed < a.tiers.length && have >= a.tiers[claimed]) { gained += a.gems[claimed]; claimed++; tiers++; }
    if (tiers === 0) return send(res, 400, { err: "Nothing to claim yet." });
    p.achv[a.id] = claimed; p.gems += gained;
    save(); send(res, 200, Object.assign({ gained, tiers }, view(p)));
  },
  // ---- VIP daily audience: free points + a resource crate scaled by VIP level ----
  "POST /api/vipdaily": async (req, res) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p); if (!p.vip) p.vip = { points: 0, lastDaily: 0 };
    if (!vipDailyReady(p)) return send(res, 400, { err: "The audience is spent for today. Return tomorrow." });
    const lvBefore = vipLevel(p);
    vipGain(p, VIP_DAILY_PTS); p.vip.lastDaily = curDay(); seasonGain(p, 40);
    const crate = 800 + lvBefore * 400; // a small resource gift that grows with VIP rank
    for (const k of ["grain", "timber", "stone", "iron"]) p.r[k] = (p.r[k] || 0) + crate;
    const levelled = vipLevel(p) > lvBefore;
    save(); send(res, 200, Object.assign({ pts: VIP_DAILY_PTS, crate, levelled }, view(p)));
  },
  // ---- season pass: unlock the premium track (simulated purchase) ----
  "POST /api/seasonbuy": async (req, res) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p); seasonSync(p);
    if (p.season.premium) return send(res, 400, { err: "The premium pass is already yours this season." });
    p.season.premium = true; // simulated: unlocks the premium track for this season, no payment
    save(); send(res, 200, view(p));
  },
  // ---- season pass: claim one level/track, or every available reward at once ----
  "POST /api/season": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p); seasonSync(p); const s = p.season;
    let gained = 0, count = 0;
    const claimOne = (lv, track) => {
      if (lv < 1 || lv > s.level) return;
      if (track === "free") { if (s.claimed.includes(lv)) return; gained += grantReward(p, seasonFree(lv)); s.claimed.push(lv); count++; }
      else { if (!s.premium || s.claimedP.includes(lv)) return; gained += grantReward(p, seasonPrem(lv)); s.claimedP.push(lv); count++; }
    };
    if (b.all) { for (let l = 1; l <= s.level; l++) { claimOne(l, "free"); claimOne(l, "prem"); } }
    else { const lv = b.level | 0; const track = b.track === "prem" ? "prem" : "free"; claimOne(lv, track); }
    if (count === 0) return send(res, 400, { err: "Nothing to claim yet." });
    save(); send(res, 200, Object.assign({ gained, count }, view(p)));
  },
  // ---- alliances ----
  "GET /api/alliances": async (req, res) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    send(res, 200, { alliances: alliancesList() });
  },
  "POST /api/alliancecreate": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    if (p.alliance) return send(res, 400, { err: "Leave your current banner first." });
    const name = (b.name || "").trim().slice(0, 24);
    const tag = (b.tag || "").trim().toUpperCase().slice(0, 4);
    if (!/^[A-Za-z0-9 ]{3,24}$/.test(name)) return send(res, 400, { err: "Banner name must be 3-24 letters or numbers." });
    if (!/^[A-Z0-9]{2,4}$/.test(tag)) return send(res, 400, { err: "Tag must be 2-4 letters or numbers." });
    if (db.alliances[tag]) return send(res, 400, { err: "That tag is taken." });
    if (p.gems < ALLY_CREATE_COST) return send(res, 400, { err: "Founding a banner costs " + ALLY_CREATE_COST + " shards." });
    p.gems -= ALLY_CREATE_COST;
    db.alliances[tag] = { tag, name, leader: n, members: [n], created: NOW(), chat: [], help: {} };
    p.alliance = tag; allyChatPush(db.alliances[tag], "", n + " founded the banner.");
    save(); send(res, 200, view(p));
  },
  "POST /api/alliancejoin": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    if (p.alliance) return send(res, 400, { err: "Leave your current banner first." });
    const tag = (b.tag || "").trim().toUpperCase(); const a = db.alliances[tag];
    if (!a) return send(res, 400, { err: "No such banner." });
    if ((a.members || []).length >= ALLY_MAX) return send(res, 400, { err: "That banner is full." });
    a.members.push(n); p.alliance = tag; allyChatPush(a, "", n + " joined the banner.");
    save(); send(res, 200, view(p));
  },
  "POST /api/allianceleave": async (req, res) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const a = allyOf(p); if (!a) return send(res, 400, { err: "You hold no banner." });
    const tag = p.alliance; p.alliance = null;
    a.members = (a.members || []).filter((m) => m !== n); allyChatPush(a, "", n + " left the banner.");
    pruneAlliance(tag); save(); send(res, 200, view(p));
  },
  "POST /api/alliancehelp": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const a = allyOf(p); if (!a) return send(res, 400, { err: "You hold no banner." });
    const member = b.member; if (member === n) return send(res, 400, { err: "Aid a fellow member, not yourself." });
    if (!a.members.includes(member)) return send(res, 400, { err: "Not in your banner." });
    const mp = db.players[member]; if (!mp) return send(res, 400, { err: "No such member." });
    resolve(mp); const q = mp.queue && mp.queue[b.i | 0];
    if (!q || q.finish <= NOW()) return send(res, 400, { err: "Nothing to aid there." });
    q.helpedBy = q.helpedBy || [];
    if (q.helpedBy.includes(n)) return send(res, 400, { err: "You already sped this order." });
    if (q.helpedBy.length >= HELP_MAX) return send(res, 400, { err: "This order has all the aid it can take." });
    q.helpedBy.push(n); const shave = allyHelpShave(q.finish - q.start);
    q.finish = Math.max(NOW(), q.finish - shave); resolve(mp);
    bump(p, "help"); save();
    send(res, 200, Object.assign({ shaved: shave, member }, view(p)));
  },
  "POST /api/alliancechat": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const a = allyOf(p); if (!a) return send(res, 400, { err: "You hold no banner." });
    const text = (b.text || "").trim(); if (!text) return send(res, 400, { err: "Say something." });
    allyChatPush(a, n, text); save(); send(res, 200, view(p));
  },
  // ---- send troops to garrison and defend a banded member's hold ----
  "POST /api/reinforce": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const a = allyOf(p); if (!a) return send(res, 400, { err: "You hold no banner." });
    const member = b.member; if (member === n) return send(res, 400, { err: "Reinforce a fellow member, not yourself." });
    if (!a.members.includes(member)) return send(res, 400, { err: "Not in your banner." });
    const d = db.players[member]; if (!d) return send(res, 400, { err: "No such member." });
    const troops = cleanTroops(b.troops);
    for (const u in troops) if ((troops[u] | 0) > (p.t[u] || 0)) return send(res, 400, { err: "You do not have that many " + (UNITS[u] ? UNITS[u].name : u) + "." });
    if (unitsCount(troops) <= 0) return send(res, 400, { err: "Send at least one soldier." });
    const cap = marchCap(p);
    if ((p.marches || []).filter((m) => m.kind !== "scout").length >= cap) return send(res, 400, { err: "All your marches are out (" + cap + " max)." });
    const dist = Math.hypot(d.x - p.x, d.y - p.y); let speed = Infinity;
    for (const u in troops) if (troops[u]) speed = Math.min(speed, UNITS[u].speed);
    const travel = Math.max(12, Math.round(dist / speed * 400 / (1 + vipPerks(p).march / 100)));
    for (const u in troops) p.t[u] -= troops[u] | 0;
    const now = NOW();
    p.marches.push({ kind: "reinforce", target: member, tx: d.x, ty: d.y, troops: Object.assign({}, troops), depart: now, arrive: now + travel, ret: now + travel, resolved: false });
    save(); send(res, 200, view(p));
  },
  "POST /api/recall": async (req, res, b) => {
    const n = authName(req); if (!n) return send(res, 401, { err: "auth" });
    const p = db.players[n]; resolve(p);
    const member = b.member; const d = db.players[member];
    if (!d || !d.reinforcements || !d.reinforcements[n]) return send(res, 400, { err: "You have no troops there." });
    const g = d.reinforcements[n]; let total = 0;
    for (const u in g) { p.t[u] = (p.t[u] || 0) + (g[u] || 0); total += g[u] || 0; }
    delete d.reinforcements[n];
    save(); send(res, 200, Object.assign({ recalled: total }, view(p)));
  },
};

// ---- per-IP sliding-window rate limit (abuse guard; generous for real play) ----
const RL = new Map(); const RL_WINDOW = 10000; const RL_MAX = 240;
function clientIp(req) { return (req.headers["x-forwarded-for"] || "").split(",")[0].trim() || (req.socket && req.socket.remoteAddress) || "?"; }
function rateLimited(ip) {
  const now = Date.now(); let arr = RL.get(ip);
  if (!arr) { arr = []; RL.set(ip, arr); }
  while (arr.length && arr[0] <= now - RL_WINDOW) arr.shift();
  if (arr.length >= RL_MAX) return true;
  arr.push(now); return false;
}
setInterval(() => { const cut = Date.now() - RL_WINDOW; for (const [ip, arr] of RL) { while (arr.length && arr[0] <= cut) arr.shift(); if (!arr.length) RL.delete(ip); } }, 30000).unref();

const server = http.createServer(async (req, res) => {
  if (!req.url || req.url.length > 1024) { res.writeHead(414); return res.end(); }
  const url = req.url.split("?")[0];
  if (url.startsWith("/api/")) {
    if (req.method !== "GET" && req.method !== "POST") return send(res, 405, { err: "method not allowed" });
    if (rateLimited(clientIp(req))) { res.setHeader("Retry-After", "5"); return send(res, 429, { err: "Too many requests. Slow down a moment." }); }
    const key = req.method + " " + url; const h = ROUTES[key];
    if (!h) return send(res, 404, { err: "no route" });
    let b = {};
    try { b = (req.method === "POST") ? await body(req) : {}; await h(req, res, b); }
    catch (e) { console.error("route error " + key + ":", e && e.message); if (!res.headersSent) send(res, 500, { err: "server error" }); }
    return;
  }
  serveStatic(req, res);
});
server.on("clientError", (err, socket) => { try { socket.end("HTTP/1.1 400 Bad Request\r\n\r\n"); } catch (e) {} });

load();
let shuttingDown = false;
function shutdown(code) { if (shuttingDown) return; shuttingDown = true; flushSave(); try { server.close(); } catch (e) {} process.exit(code || 0); }
process.on("SIGINT", () => shutdown(0));
process.on("SIGTERM", () => shutdown(0));
process.on("uncaughtException", (e) => { console.error("uncaught:", e && e.stack || e); shutdown(1); });
process.on("unhandledRejection", (e) => { console.error("unhandled rejection:", e && e.stack || e); });
server.listen(PORT, () => console.log("Giantsreach server on http://localhost:" + PORT));
