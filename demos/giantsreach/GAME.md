# GIANTSREACH — master design, roadmap, and build status

A complete, production-quality, time-based browser strategy MMO in the lineage of
Travian / Clash of Kings / Rise of Kingdoms, set in a realm built among the ruins of
fallen stone giants. Purchases are SIMULATED (no real money; "buying" a pack just grants
shards). Runtime is deterministic and server-authoritative; AI is used OFFLINE only to bake
art / music / sfx, never at runtime.

This file is the single source of truth for the build loop. Each loop iteration: read the
Build Status, do the NEXT meaningful step, test it (server runs, endpoints work, UI renders
via a headless screenshot), fix issues, and update Build Status. Only `git`-commit a slice
once it is tested across iterations.

## Art direction (LOCKED, do not drift)
Hand-painted PAINTERLY game art (the chosen style, #2 of the style board). Warm golden-hour,
rich brushwork, soft light outlines, the city as the playfield with the fallen-giant ruins as
a SMALL distant landmark (never dominating the play space). UI = the locked carved-oak-and-gold
game HUD with one rationed oxblood accent and gilt for premium; functional color coding
(oxblood = act/alert/timer, gold = wealth/level, green = ready/confirm). Woodcut building
emblems on parchment tiles. Bake all art OFFLINE with Qwen-Image via ComfyUI (127.0.0.1:8188);
the slow high-quality pass (no Lightning LoRA, ~24-26 steps, cfg 3.5) for finals; commit to ONE
medium anchor; never the generic painterly-matte-painting slop default. Music/sfx baked offline
(ACE-Step / synth). Splash video via LTX if viable.

## Tech (LOCKED)
Dependency-free Node.js server (`server/server.js`, http+crypto+fs), JSON-file persistence
(`db/db.json`), static web client (`web/`). Lazy, deterministic time resolution (store
timestamps; resolve on read; survives restarts). Launch with `./launch.sh` (PORT 8787).

## Grounded systems spec (from research, with sources in the loop's notes)
- Resource cost growth: building `cost = round5(base · 1.28^level)`; resource fields 1.67.
- Storage cap: `round(21.2 · 1.2^lvl − 13.2) · 100`.
- Build time: `base · 1.16^level`, reduced by Keep `· 0.964^(keep−1)` (keep gates + speeds all).
- Production: scales ~1.4×/level early (table-like).
- Combat (deterministic, no RNG by default): winner_loss = (loser_pts/winner_pts)^K,
  K = clamp(2·(1.8592 − N^0.015), 1.2578, 1.5); defense weighted by attacker arm mix
  (def_eff = inf_share·def_inf + cav_share·def_cav); wall multiplier `wall_base^level`.
- March travel time = euclidean_distance / slowest_unit_speed.
- Gem-to-time (speedup) anchors: 60s→1, 1h→20, 1d→260, 1wk→1000 (interpolate); builds under
  5 min finish free (Travian rule). Alliance help shaves max(1%, 1min) per click.
- Power/might = Σ building weights + Σ troop tier weights (+ tech + hero later).
- Retention: 7-day escalating login calendar; daily task points (chests at 20/40/60/80/100);
  VIP (free builder queue at VIP6); battle/season pass (30 days, free+premium); starter pack
  ($0.99 overstuffed, once); value-tier gem ladder; returning-player win-back.
- Onboarding: emotional hook < 3 min; guided first builds with fast timers; persistent
  objective tracker that always points at the next power action; PvP delayed to ~session 4.

## DONE (tested foundation, this iteration)
- Server: register / login / guest auth (scrypt), per-player persistent state, lazy time
  resolution. Endpoints: state, build, speedup, cancel, train, buygems, daily, tutorial,
  leaderboard. Real formulas (cost 1.28^lvl, time 1.16^lvl·0.964^keep, cap 1.2^lvl,
  gem-to-time interpolation, free-finish<5min, prod curve).
- 9 buildings (keep/granary/sawmill/quarry/mine/market/barracks/wall/watchtower) with the
  woodcut icons; keep gates levels; 2 build slots.
- 4 unit types (spearman/swordsman/archer/knight) with Travian-style atk + split def, training
  queue at the barracks.
- Resources accrue over time (offline too), capped by storage; gold uncapped.
- Gems + SIMULATED shop (6-tier gem ladder + Founder's starter pack) that just grants shards.
- 7-day escalating daily-login tribute with streaks.
- Client: splash/auth (register/login/quick-play), the painterly city HUD wired LIVE, resources
  ticking smoothly each frame (client interpolation between 3.5s syncs), build queue with
  countdowns + gem/free speedup, training queue, building upgrade modal, shop modal, daily modal,
  army modal, leaderboard, settings, toasts, and a tutorial spotlight + objective tracker that
  always points at the next action.
- `launch.sh` boots the server and opens the browser.

## DONE (iteration 2: WORLD MAP + MARCHES + DETERMINISTIC COMBAT)
- Infinite deterministic world map (seeded by tile hash): barbarian camps (levels 1-6 with
  scaling garrisons + loot), fallen-giant ruins, and other players' cities; `/api/map` returns
  a window around the player's hold. Fixed a signed-shift bug that made negative camp levels.
- Marches: `/api/march` sends a chosen troop mix at a camp; travel time = distance / slowest
  unit speed; troops are deducted, the host marches out, then returns. Up to 5 marches out.
- Deterministic combat resolver (Travian mixed-arms, no RNG): A vs weighted D,
  winner_loss = (loser/winner)^K with K clamped by army size; loser wiped, winner keeps
  (1 - winner_loss); loot capped by surviving carry capacity; camp clears for 30 min then
  returns. Battle reports stored (`/api/reports`), win/lose + losses + loot.
- Client: a full world-map modal (grid of tiles, your gold hold at center, red camps by level,
  ally cities, ruins), a raid dialog (pick troops, see distance/garrison/spoils), an active
  "Marches" panel with live countdowns, a recent-raids report list, and a toast when a raid
  resolves. Verified END TO END: marched 40 knights at a Lv1 camp, won at 5% losses, looted
  ~1470 resources that returned home; map UI screenshot-verified.

## DONE (iteration 3: SOUND + MUSIC)
- `web/audio.js`: a fully procedural Web Audio engine (no files, no runtime AI, deterministic).
  A warm generative ambient bed (an A-minor modal pad progression with a bass drone, lowpass +
  feedback-delay space, and an occasional pentatonic bell shimmer, low volume, looping every
  ~7s), plus synthesized SFX: click, build, build-done, coin/loot, reward, level, march horn,
  victory fanfare, defeat. Started on the first user gesture (browser autoplay rule), with a
  mute toggle on the HUD persisted to localStorage.
- Wired SFX to actions: build, speedup, buy shards, daily claim, train, march out, and a
  victory/defeat sting when a raid resolves. Verified END TO END headless: GA loads, the mute
  icon toggles 🔊->🔇 and persists, all sfx code paths fire with no JS errors.
- NOTE: a richer COMPOSED track via ACE-Step (present at ~/Documents/ACE-Step-1.5) is a future
  upgrade; the procedural bed ships the feature reliably now.

## DONE (iteration 4: UX FIXES from user feedback)
- Tutorial spotlight no longer darkens/covers the screen: removed the full-screen box-shadow
  mask, replaced with a pulsing glow ring on the target (the bug where the dark mask covered
  everything and never revealed is gone).
- Hotbar level badges were clipped by the scroll container (overflow-x:auto forced overflow-y
  to clip): added top padding + overflow-y:hidden so the gold level badges and green upgrade
  arrows show fully. Verified 0 badges clipped.
- World map is now DRAGGABLE and ZOOMABLE: a #mapview viewport with a transformed #mapinner
  grid, pointer drag to pan, wheel + on-screen +/- buttons to zoom (0.5x..2.2x), and a center
  button; the map no longer auto-refreshes mid-interaction. Bigger world (R 11 -> 16). Verified
  transform changes on drag and zoom.
- Removed ALL emoji from the UI: built a gold-stroke SVG line-icon set (shield, gift, gem,
  sword, map, trophy, gear, sound on/off, hammer, scroll, horse, home, ruin, flag) and swapped
  every emoji in the HUD, rails, objective tracker, panel headers, modals, map cells, and toasts
  for SVG or text. Verified zero emoji remain in web/.
- Added hover states across interactive elements (rails, top icons, buttons, shard pill, shop
  packs, unit/daily cards, queue rows, report cards, map camps/cities), subtle lift + glow.
- Verified END TO END headless: no JS errors, all HUD icons render as SVG, mute icon is SVG,
  badges visible, map pans + zooms; screenshots gr_fix_game.png and gr_fix_map.png.

## DONE (iteration 5: INTERACTIVE TOWN + user-reported fixes)
- The TOWN itself is now the interactive map (per user: "town map not big world map"). The city
  is a pannable/zoomable view (#worldwrap viewport + transformed #worldinner, drag to pan, wheel
  to zoom, clamped to cover) with 9 CLICKABLE building markers placed over the painterly city
  (keep/market/barracks/granary/sawmill/quarry/mine/wall/watchtower), each a gold chip with the
  building name, roman level, a green upgrade arrow when affordable, and a live build timer;
  clicking a marker opens the upgrade modal. The "World" rail still opens the big raid map.
- Fixed "finish free not working": confirmed it works server-side (build under 5 min) AND
  client-side (clicking the queue speedup at <5 min levels the building); verified keep I -> II.
- Fixed the huge army icon (root cause): the resource icons in the unit COST line were raw
  `<svg>` with no size constraint (only `.ri`-wrapped icons were sized), so they blew up and
  broke the card layout ("ui issue when click"). Added a global `svg{18px}` default plus
  `.unitcard .st svg{14px}` / `.em svg{26px}`. Verified max svg in the army modal is 26px and
  all unit cards are a uniform 70px tall, no JS errors.
- Verified END TO END headless: 9 town markers, keep marker opens the modal, finish-free levels
  the keep, town pans, army icon correct, no JS errors. Screenshot gr_town.png.
## DONE (iteration 6: RETENTION, daily task ladder + free chest)
- Daily task ladder fully wired and tested. 8 daily tasks (order 1 / order 3 constructions,
  train 20, claim tribute, hasten a build, spend 3000 resources, win a raid, loot a camp) that
  reset each day; each completed task adds points toward a 100 cap. Hooks: build/train/speedup/
  daily/raid actions call bump(); points recomputed server-side.
- Five reward chests at 20/40/60/80/100 points, claimable once each (gems + resources); a points
  track UI with glowing claimable nodes and a fill bar.
- A FREE CHEST on a 4-hour cooldown (gems + resources), with a live countdown and claim.
- New "Tasks" rail with a claimable badge (lights when any task chest or the free chest is ready).
- Endpoints /api/taskchest and /api/chest; tasks + chest state in /api/state.
- Verified END TO END: build + daily earned 20 points, the 20-chest claimed (+20 gems), the free
  chest claimed (+25 gems) then correctly blocked on cooldown; tasks modal renders (8 rows, the
  points track, the free chest), badge lights, no JS errors, no oversized icons. Screenshot
  gr_tasks.png.

## DONE (iteration 7: EQUIPMENT + HEROES, the Forge with transparent pity)
- A champion/hero per player with a level and an XP bar. Each cleared barbarian camp grants XP
  (level * 8); levelling adds flat +2 attack and +2 defense per level. Resolved in the lazy raid
  resolver so it survives restarts.
- Four equipment slots (Warblade/atk, Aegis/def, War Banner/march-speed, Sigil/spoils), each affix
  bound to its slot. Equipped relics combine into a heroBonus {atk,def,speed,gold} percentage.
- The bonus is applied everywhere it should be: combat() takes an atkMult so infantry+cavalry power
  scales with atk%; raid loot scales with gold% (spoils); march travel time divides by (1+speed%).
- The Forge: a gacha draw for FORGE_COST (60) shards. Deterministic per player via a seeded PRNG
  (name hash XOR draw index), so a save reload never re-rolls the same draw differently. Four tiers
  Common/Rare/Epic/Legendary at weights 60/28/9/3, with affix value ranges per tier.
- TRANSPARENT PITY: a visible counter; reaching 9 guarantees the next draw is Epic or better, then
  resets. Verified end to end: pity climbed 0..9 over unlucky draws and the next draw was Epic.
- Endpoints /api/forge (spends shards, rolls, applies pity), /api/equip {seed} (swaps, old item
  returns to stash), /api/unequip {slot}. relics/equipped/hero/heroBonus/pity/forgeCost in /api/state.
- Client "Forge" rail + modal: hero bar with XP + combined bonus line, four tier-colored equipment
  slots (unequip inline), the Forge box with the pity readout and a Strike button, and a tier-colored
  Stash (Common gray / Rare blue / Epic purple / Legendary gold) sorted by power, click to equip.
- Verified END TO END via curl (12 draws + pity reset on Legendary, the 10-step pity guarantee,
  equip reflected in heroBonus) and a headless screenshot (shots/forge2.png): no JS errors, tiers
  colour-correctly, bonuses read +33% atk / +8% def / +17% march / +10% spoils on the test loadout.

## DONE (iteration 8: ACHIEVEMENTS / MILESTONES, permanent tiered goals)
- A permanent lifetime-stat layer (p.life: raidsWon, looted, trained, peakMight, logins) that, unlike
  the daily task counters, never resets. Hooked into the raid resolver (raidsWon + looted), training
  (trained), and the daily tribute (logins). Peak might is tracked in resolve() so it only ratchets up.
- Seven milestone tracks, each with five tiers and rising shard rewards: Master Builder (total building
  levels), Warlord (raids won), Plunderer (resources plundered), Drillmaster (soldiers trained),
  Forgemaster (relics forged, derived from drawN), Ascendant (peak might), Devotee (daily logins).
  Two stats are derived from live state (buildLevels = sum of building levels, forged = drawN) so they
  need no separate counter and can never desync.
- Claiming a milestone grabs EVERY newly-earned tier at once and sums the shards; claimed tiers persist
  in p.achv. Endpoint /api/achv {id}; achievements + an achvClaim banner flag in /api/state.
- Client "Honors" rail (with a claimable badge that lights when any tier is ready) and a wide modal:
  each track shows its icon with a tier counter, a progress bar to the next tier, five pips (filled =
  claimed, green-glow = ready), and either a shard Claim button, the live progress, or "Mastered".
- Verified END TO END: fresh player reads buildLevels 6 and peak might correctly; forging 12 relics made
  Forgemaster claimable, claiming grabbed 2 tiers for 60 shards (20+40) and advanced the tier, a second
  claim correctly returned "Nothing to claim yet"; training increments Drillmaster. Screenshot honors.png
  shows all seven tracks rendering in the carved-oak gold style with the Forgemaster claim lit, no JS errors.

## DONE (iteration 9: VIP TRACK, accumulating points -> permanent empire buffs)
- An 11-rank VIP ladder (0..10) at rising point thresholds (100, 300, 700, 1500, 3000, 6000, 12000,
  24000, 48000, 100000). Each rank grants cumulative passive buffs: faster construction (up to 33%),
  more empire-wide production (up to 30%), faster marches (up to 25%), and extra march slots (up to +3).
- Perks are applied at the real formulas, server-side and deterministic: buildSpeedMult() shrinks build
  time in both the build route and the displayed time; ratePerSec() multiplies all production by the VIP
  prod bonus (so it flows through resolve + the resource bar); marchCap() raises the 5-march limit; the
  march travel calc divides by hero-speed + VIP-march combined. Verified VIP 0 vs VIP 8: keep build time
  69s -> 52s (24% faster) and grain 0.036 -> 0.044/s (22% more), march cap 5 -> 7.
- Two point sources: a FREE daily Royal Audience (+60 VIP points + a resource crate that scales with rank,
  once per day) and shard packs (1 VIP point per shard, the monetization accelerator). Purchases stay
  simulated (no payment), so VIP is reachable free over time but far faster if you "buy".
- Endpoint /api/vipdaily (daily, idempotent); VIP points hooked into /api/buygems (packs + starter);
  vip block in /api/state (level, points, nextAt, perks, full ladder, dailyReady, marchCap).
- Client: a gold VIP crown crest in the top HUD showing the current rank with a daily-ready badge, opening
  a wide modal: the current rank with its active bonuses and a progress bar to the next rank, the daily
  audience button, and the full ladder with each rank's perks (current rank highlighted).
- Verified END TO END: fresh player VIP 0 / daily ready / cap 5; claiming the audience grants +60 and a
  crate then correctly blocks re-claim; buying packs jumped to VIP 8 (42060 pts) with cap 7 and live perks;
  build-speed and production deltas confirmed numerically. Screenshots vip.png (modal) + vipcrest.png (HUD),
  no JS errors.

## DONE (iteration 10: SEASON / BATTLE PASS, 30 days, free + gold tracks)
- A 50-level season pass on a deterministic 30-day cycle. Seasons tile forward from a fixed epoch, so the
  current season id (and its rotating themed name from a 6-name table) is computed from NOW(); when the
  cycle rolls over, a player's pass progress and premium flag reset automatically via seasonSync(). The
  season end time is sent to the client for a live countdown.
- Season XP comes from daily play, hooked into the real actions: login (+120), build (+25), train
  (+min(n,60)), raid win (+50), speedup (+15), VIP daily audience (+40). 300 XP per level.
- Two reward tracks per level. FREE: resources each level, with gem nodes every 5th/10th level. GOLD
  (premium): gems every level (scaling) plus a big gem + iron bonus at each 10th level. The premium track
  is unlocked by a SIMULATED purchase (/api/seasonbuy, no payment); unlocking makes all already-earned
  premium tiers claimable retroactively.
- Endpoints: /api/season claims a single {level, track} or every available reward with {all:true};
  /api/seasonbuy unlocks the gold track. A full season block (level, xp, the 50-level ladder with each
  reward + claim state, premium flag, endsAt, claimable) is in /api/state.
- Client: a season banner pinned at the top of the right column (themed name, level, XP bar, a claimable
  badge) that opens a wide modal: the level/XP header with a live "season ends in Nd Nh" countdown, the
  gold-pass unlock, a "Claim all available" action, and a horizontally-scrollable two-track ladder (FREE
  over GOLD) that auto-scrolls to the current level and marks it NOW.
- Verified END TO END (seeded a player to level 10 to exercise claims deterministically, since the live XP
  grind is storage-capped): claim-all free granted 10 levels (+55 shards), unlocking gold then claim-all
  granted the 10 premium tiers retroactively (+280 shards, matching the formula), re-claim correctly
  returned "Nothing to claim yet", and claiming an unreached level errored. Screenshots season.png (modal)
  + full.png (banner in the HUD), no JS errors.
- Also FIXED a latent bug: the auth login/register toggle used arguments.callee inside an arrow function
  (threw in strict mode), so switching to the login form errored. Replaced with a named toggleAuth().

## DONE (iteration 11: SPLASH / TITLE SCREEN, dedicated baked key-art)
- Baked a dedicated cinematic TITLE key-art OFFLINE with Qwen-Image via ComfyUI on the locked recipe
  (qwen_image_2512_fp8 UNet, qwen_2.5_vl_7b CLIP, qwen_image_vae, 1536x896, 26 steps, cfg 3.5, euler/
  simple, no Lightning LoRA). Three seeds were baked and art-directed; chose seed 43. It keeps the locked
  painterly medium anchor and palette but uses a title-specific composition: the player's banner-flying
  walled city on the left, the COLOSSAL FALLEN STONE TITAN as the emotional centerpiece on the right (a
  weathered CRACKED CARVED head and hand, mossy ruin, clearly not alive and not a skeleton, per the locked
  direction), a marching army column on the road connecting them, a winding river and arched bridge, under
  a luminous golden-hour sunset with open sky for the title. Saved to web/img/splash.png.
- Redesigned the splash screen around it: full-bleed key-art with a slow 38s Ken Burns drift for life, a
  bottom-weighted scrim (radial + linear) that reveals the vista up top and darkens toward the form, a
  round gold crest emblem above the wordmark, and an enlarged Pirata One "Giantsreach" wordmark with a
  layered dark halo for legibility over the bright sky. The form is anchored at the bottom.
- Made the wordmark responsive (clamp 46-76px) so it never clips; verified the whole screen on desktop
  (1280x800) and phone (390x844) widths.
- Verified END TO END: splash renders with the new key-art on both desktop and mobile, the title is fully
  legible, Quick Play still enters the game with no JS errors. Screenshots splash_v2.png (desktop) +
  splash_mob_v2.png (mobile). ComfyUI confirmed reachable at 127.0.0.1:8188; bake script saved at
  scratchpad/splash_bake.py.

## DONE (iteration 12: MOBILE / PORTRAIT LAYOUT pass)
- The whole HUD now reflows for phones via a `@media (max-width:760px)` pass (desktop layout untouched
  above that width). Verified the game at 390x844 and confirmed desktop at 1280x800 is unchanged.
- Top bar: two rows on mobile. Row 1 is a compact avatar + the VIP crest + the mute/ladder/steward icons;
  row 2 is a single horizontal-scroll resource strip with compact pills (the per-hour sub-line is hidden
  to save width). No more overflowing wrapped resource block.
- Left rail: shrunk to 44px and anchored above the bottom stack; still the familiar vertical side rail.
- The right column (season banner + Construction + The Host) becomes a SLIDE-UP DRAWER on mobile: it is
  pinned to the bottom, peeks a tappable "Empire & Queues" handle (with a grip and a claim badge), and
  slides up to ~82vh when tapped. This keeps the town clear while still giving full access to queues and
  the season pass. Toggle wired in JS ($("#drawerhandle")); badge reflects season.claimable.
- Bottom: the objective becomes a thin single bar stacked above the horizontally-scrolling hotbar (slots
  shrunk to 64px), clear of the drawer handle.
- Modals already used max-width:94vw; nudged to 97vw / 88vh on mobile with tighter padding. The Barracks,
  Shop, Forge, etc. all render full-width and usable.
- Verified END TO END on a 390x844 touch viewport: enters from Quick Play, the two-row top + compact rail
  + full-screen tappable town + thin objective + scroll hotbar all fit; the drawer opens to show the season
  banner and both queue panels; the Barracks modal renders cleanly; no JS errors. Desktop screenshot
  confirms zero regression. Screenshots game_mob.png, mob_drawer.png, mob_modal.png, desktop_check.png.

## DONE (iteration 13: ALLIANCES / banners, with timer-shaving help)
- A full alliance ("banner") system stored in db.alliances (keyed by a unique 2-4 char TAG). Players hold
  at most one banner (p.alliance). Founding costs 80 shards; up to 30 members; leader is the founder.
- Endpoints: GET /api/alliances (browse, sorted by total might), POST /api/alliancecreate {name, tag},
  /api/alliancejoin {tag}, /api/allianceleave (auto-transfers leadership, prunes the banner when empty),
  /api/alliancehelp {member, i}, /api/alliancechat {text}. A full alliance block is in /api/state.
- The signature ALLIANCE HELP (Travian/RoK style): a member can speed a fellow member's build order. Each
  aid shaves max(60s, 1% of the order's total time); an order accepts up to 20 helps; each member may aid a
  given order only once (tracked by q.helpedBy). This is the grounded timer-shaving social loop.
- A passive banner bonus: +1% production per member (capped +10%), wired into ratePerSec alongside the VIP
  buff, giving a real reason to band together.
- A simple shared War Table chat (last 40 messages, with system lines for found/join/leave) for life.
- Client: a new "Banners" rail (8 rails now; verified they still fit on both desktop and mobile). Two modal
  states: BROWSE (found-your-own form with tag + cost, plus a joinable banner list) and IN-BANNER (a roster
  of the sworn with might/keep, an Aid button on each fellow member's live orders showing helps N/20, the
  bonus readout, a Leave button, and the chat with an input). A rail badge lights when any member has an
  order you can still aid.
- Verified END TO END with two accounts via curl: A founds IRV (-80 shards, bonus +1%), B browses + joins
  (bonus +2%), A queues a keep (69s), B sees A's order and aids it (shaved to 9s), a second aid is blocked
  ("already sped"), chat shows system + member lines, the leader leaving transfers leadership, and the last
  member leaving prunes the banner. Client verified on a clean db: ally_in.png shows the Aid button on a live
  order + chat, ally_browse.png shows the found/join screen, no JS errors.

## DONE (iteration 14: THE VOICE OF THE REALM, baked flavor text)
- Authored a flavor-text corpus at BUILD TIME (the offline bake) and wired it deterministically. ZERO AI at
  runtime: it is a static FLAVOR const, served by seed, never an API call. No em-dashes.
- Four surfaces: (1) barbarian camp TAUNTS shown in the raid dialog, one per camp seeded by its coords so it
  is stable (pick(FLAVOR.taunts, ihash(x,y))); (2) battle-report NARRATION, a victory or defeat line appended
  to each raid report seeded by the camp + time; (3) the STEWARD'S COUNSEL, one atmospheric+useful line per
  player per day (seed = hstr(name) ^ curDay()), shown in the Steward modal; (4) a CHRONICLE OF THE FALLEN
  lore codex (5 entries on the giants, the Reach, the camps, the banners) opened from the Steward.
- All exposed cleanly: camp tiles carry `taunt` in /api/map, reports carry `flavor`, and /api/state carries
  `counsel` + `chronicle`. Client renders: an oxblood-accented italic taunt quote atop the raid dialog, an
  italic narration line under each report card, a bordered counsel panel + a "Chronicle of the Fallen" button
  in the Steward, and a gold-ruled lore reader. All in the locked carved-oak palette.
- Verified END TO END: /api/state returns a stable daily counsel + the 5 chronicle entries; /api/map returns
  a deterministic per-camp taunt (identical across repeated calls); screenshots flavor_taunt.png (raid jeer),
  flavor_chron.png (lore codex), flavor_counsel.png (steward counsel). Report narration is wired identically
  to the verified pick() pattern and the field is populated server-side. No JS errors.

## DONE (iteration 15: PRODUCTION HARDENING + README + first git checkpoint)
- Durable atomic saves: writeDbNow() writes to db.json.tmp, copies the live db.json to db.json.bak, then
  renames the temp over the real file, so a crash mid-write can never corrupt the database. load() now tries
  db.json then falls back to db.json.bak on a parse failure (verified: corrupting db.json recovered every
  player from the backup, with a logged warning).
- Graceful shutdown: SIGINT / SIGTERM / uncaughtException flush the pending debounced save before exit
  (verified: a daily claim made inside the 800ms debounce window survived a SIGTERM + restart, gems 120 to
  170 persisted). unhandledRejection is logged.
- A per-IP sliding-window rate limit on /api (240 requests / 10s -> 429 with Retry-After; verified a 280-call
  flood returned 238x200 + 42x429). Plus method allow-listing (PUT -> 405, verified), a 1KB URL-length cap,
  a clientError handler, and the existing body-size cap and static path-traversal guard.
- Routes already validate inputs (bounded numerics, name/tag regex, JSON body cap); the request handler now
  also guards against a route throwing after headers are sent.
- Wrote a real README.md (run instructions, feature list, architecture, the hardening notes, the offline
  asset-baking policy, and the simulated-purchase / zero-runtime-AI constraints).
- Verified END TO END: smoke (guest + state 200), the rate-limit + method + save + shutdown + corruption-
  recovery tests above, and a final UI screenshot (full.png) with no regression and no JS errors.
- FIRST GIT CHECKPOINT: git init + a local commit of the tested build (no push, no co-author trailer), since
  the whole roadmap is now built and tested across 15 iterations.

## DONE (iteration 16: PvP, attacking rival cities)
- Player-vs-player city attacks, the missing competitive layer. The map already showed rival cities; now you
  can march on them. Reuses the deterministic combat engine, extended to return the loser's losses and accept
  a defender (wall) multiplier.
- Endpoint /api/attack {x, y, troops}: validates a real rival hold stands there, both lords are out of
  beginner's peace, and you do not share a banner; then launches a city-kind march with the same travel/return
  mechanics as a camp raid. /api/map city tiles now carry might, keep, shielded, allied, and dist; the map
  payload carries the viewer's own shield state.
- resolveCityAttack(): on arrival it resolves the defender to current, applies a wall defense multiplier
  (+4%/level), runs combat with the attacker's hero bonus, then applies casualties to BOTH armies (the loser
  always loses more; a rout lets some escape, an even fight is mutually devastating). A victor carries off half
  the defender's stores, capped by surviving carry capacity, and the defender actually loses those resources.
  Both lords get a report: the attacker an attack report, the defender a defense report ("Atk stormed your
  hold ... RAIDED" / "... you threw them back ... HELD").
- Beginner's peace: a hold below Keep 3 cannot attack or be attacked, so newcomers grow safely; allies cannot
  hit each other. Verified all three guards reject correctly.
- Client: rival cities render RED and clickable on the world map (allies green, the shielded blue and
  inert); a siege dialog ("March on <name>", foe keep/might/distance, troop picker, "Sound the war horns");
  the battle log now renders camp raids, city victories/defeats, and incoming-defense outcomes distinctly;
  active PvP marches read "War on <name>".
- Verified END TO END with two seeded Keep-3 accounts: the attacker won, lost 20% of the host, looted 1,550
  of each resource (capped by storage + carry); the defender lost 76% of its troops with an exact breakdown
  and the looted resources left its stores; both reports rendered; the Keep-under-3 shield blocked attacks in
  both directions and self/empty-tile attacks errored. Screenshots pvp_attack.png (siege), pvp_map2.png (red
  foe + victory), pvp_def_report.png (the RAIDED defense report). No JS errors, no regression to camp raids.

## DONE (iteration 17: BUILDING UPGRADE ART, painterly portraits)
- Baked 9 painterly building portraits OFFLINE with Qwen-Image via ComfyUI on the locked recipe (qwen_image_
  2512_fp8, qwen_2.5_vl CLIP, 1024x576, 24 steps, cfg 3.5, euler/simple, no Lightning LoRA), one per building
  (keep, granary, sawmill, quarry, mine, market, barracks, wall, watchtower). Each is a single hero building in
  the locked golden-hour painterly style, matching city.png and splash.png. Saved to web/img/bld/.
- Redesigned the building UPGRADE MODAL around them: a full-width painterly portrait banner heads the modal
  with a soft inset vignette and the current "Level N -> N+1" tag overlaid, then the description, the
  production/build-time/cost lines, and the action button. Replaces the small woodcut tile that was there.
- Graceful fallback: if a portrait is ever missing the image swaps to the existing woodcut icon (centered on a
  parchment field via a .noart class), so the modal never breaks. The bake script is at scratchpad/bld_bake.py.
- Verified END TO END: the Keep, Barracks (correctly gated "Raise the Keep first"), and Market (Build, with the
  production delta) modals all render the new portraits in the carved-oak frame; verified on desktop (620w) and
  phone (390x844, the banner shrinks to 150px). Screenshots bld_keep.png, bld_barracks.png, bld_market.png,
  bld_mobile.png. No JS errors, no regression.

## DONE (iteration 18: COMPOSED MUSIC THEME, baked with ACE-Step)
- Baked a real main theme OFFLINE with ACE-Step 1.5 (~/Documents/ACE-Step-1.5, its own .venv, torch 2.10 on
  MPS) via its cli.py + a TOML config (scratchpad-saved gr_theme.toml). Art-directed caption: a warm, wistful,
  hopeful golden-hour orchestral piece (legato strings, a soft solo horn, harp/lute arpeggios, distant choir
  pad, no drums), instrumental, D major, ~72 bpm, 95s. Two takes were generated; analyzed both programmatically
  (peak/RMS/silence) and chose the fuller one.
- Encoded it for the web with ffmpeg: loudness-normalized to a gentle background level (loudnorm I=-19) with a
  2s fade-in and a soft fade-out, to a 1.3MB 112kbps mp3 (web/audio/theme.mp3). mp3 for universal browser
  support (the server already serves audio/mpeg).
- Wired audio.js: the baked theme is now the primary MUSIC layer, looped, created on the first user gesture
  (still muted-until-first-click), routed through the WebAudio master gain so the existing mute toggle controls
  it (and it pauses on mute). The procedural Web Audio bed remains as an automatic FALLBACK if the file ever
  fails to load; the synthesized SFX (clicks, build, victory, etc.) are kept as-is.
- Verified END TO END (audio is not screenshot-testable, so checked the wiring): theme.mp3 serves 200 as
  audio/mpeg (1.33MB); after a gesture the GA Audio element has src theme.mp3, is NOT paused, and currentTime
  advances (2.49s) over a 95s duration; toggling mute pauses it and flips isMuted; zero JS errors. Bake config
  and analysis in scratchpad; the ACE-Step run log captured.

## DONE (iteration 22: BATTLE MUSIC CUE, the fighting scene gets its own score)
- The battle cinematic played under the calm main theme. Baked a dramatic BATTLE CUE OFFLINE with ACE-Step 1.5
  (gr_battle.toml: pounding war drums + taiko, brass stabs, driving string ostinato, a fierce low choir, D
  minor, 142 bpm, 28s; instrumental). Two takes generated; chose the fuller one by peak/RMS/silence analysis,
  loudness-normalized hotter than the theme (I=-15) and encoded to a 448KB mp3 (web/audio/battle.mp3).
- Wired audio.js: a new cue layer (a second looped Audio routed through its own gain into the master). GA.cue()
  ducks the main theme to 0.10 and swings the battle cue up; GA.cueStop() fades the cue and restores the theme.
  The battle cinematic calls GA.cue() on open and GA.cueStop() when dismissed, so the fighting scene swaps to
  the war score and the calm theme returns after. Respects mute (no-op while muted; the cue pauses on mute).
- Verified END TO END (audio is not screenshot-testable, so instrumented the Audio elements): battle.mp3 serves
  200 as audio/mpeg (448KB); after the first gesture the theme plays; during the cinematic the battle cue is
  playing (currentTime advances) while the theme is ducked; on dismiss the cue pauses and the theme resumes.
  No JS errors.

## DONE (iteration 21: THE INFIRMARY, wounded soldiers are recoverable)
- Softened the one harsh part of combat: every casualty used to be gone forever. Now a share of the slain are
  WOUNDED (recoverable), not lost. 30% of casualties become wounded; the rest die. The keep shelters up to
  keep*60 wounded (overflow is lost).
- Wired into every combat site deterministically: camp raids (on a win the winnerLoss casualties, and even on a
  loss a share of the wiped host limps home wounded), and PvP both ways (the attacker's wounded come home with
  the survivors on the return leg; the defender's are sheltered immediately). The injured are added at the
  return barrier, so it survives restarts like the rest of the lazy resolver.
- The INFIRMARY in the Army modal: shows the wounded per unit and the shelter usage (N / cap), and a "Tend all"
  button that heals every wounded back into the host for half their original training cost in resources
  (instant). Endpoint /api/heal; wounded + woundCap + healCost in /api/state. An Army rail badge lights while
  wounded await tending, and the battle cinematic now reads "N wounded" alongside the casualties and spoils.
- Verified END TO END: a seeded keep-5 raid that lost generated 18 wounded (30% of the fallen) with the rest
  lost; healCost computed as half the training cost (grain 195 / timber 270 / iron 120); "Tend all" returned
  them to the host and spent exactly that; a second heal correctly returned "No wounded to tend." Screenshots
  infirm.png (the infirmary panel + lit rail badge) and cin_wounded.png (the cinematic wounded line). No JS errors.

## DONE (iteration 20: ONBOARDING, a real first-session tutorial)
- The tutorial was just a pulsing ring + a bottom objective line. Built a proper first-session ONBOARDING.
- A warm steward WELCOME modal on the very first entry (gated by p.tutorial < 1): a gold crest, a personalized
  greeting that sets the fallen-giants premise, what you will do (grow, wall, drill, march), and a "Take up the
  banner" button that records tutorial step 1 server-side so it never shows again.
- Coach BUBBLES: the objective spotlight now renders a tooltip anchored to the highlighted element (not just
  the far-off bottom bar), with the step title + instruction and a gold pointer arrow that picks the roomy side
  (left/right/above/below) and clamps to the viewport. It rides the existing objective ladder (raise the Keep,
  grow grain, claim the daily gift, build a barracks, train a host, ...) through the early game, and hides once
  a modal is open or the player passes the early game.
- Verified END TO END: a fresh guest gets the welcome (tutorial 0); "Take up the banner" advances to tutorial 1
  and the coach bubble appears anchored over the Keep ("Raise the Keep"); reloading as a returning player does
  NOT show the welcome again (persisted). Screenshots tut_welcome.png + tut_bubble.png. No JS errors.

## DONE (iteration 19: BATTLE CINEMATIC + UI fixes + relocation into the project)
- Project RELOCATED into the realdb project at demos/giantsreach (isolated alongside the other sample demos),
  per the user. The server is path-independent (paths are __dirname-relative) and runs unchanged from the new
  location; the separate inner git repo was dropped so the game now lives inside the realdb repository.
- THE FIGHTING SCENE: baked a painterly battle backdrop OFFLINE with Qwen (two hosts clashing, crimson banners,
  a charging knight, golden-hour god rays, the fallen giant's stone head watching over the field; 1344x768, 26
  steps cfg 3.5; web/img/battle.png). Built a battle CINEMATIC that auto-plays when your own raid or attack
  resolves: your host vs the foe with army counts slide in, a clash with screen-shake and spark flashes, a wax
  seal stamps down VICTORY (green) or DEFEAT (oxblood) with the win/lose sfx, then the aftermath reveals the
  flavor line, casualties percent, and animated spoils, with a To-the-spoils / Onward button. Skippable by a
  tap. Incoming raids (defense reports) stay as a toast so the cinematic is reserved for your own actions.
- FIX (user report) the left rail was cut off vertically on short screens: it now flex-wraps into additional
  columns with a max-height, so all 8 rails stay visible (verified at 600px height: previously the last items
  sat at y=636 off a 600px viewport; now they wrap and the rail bottom is 210, nothing hidden).
- FIX (user report) "finish free not clickable": the speedup control was a tiny 56x19 unstyled text span. It is
  now a proper button pill (92x23, gold for shards / green for free), AND the building modal now shows a clear
  "Finish free" / "Hasten" button while a build is in progress (the discoverable place), both wired to the
  speedup endpoint. Verified the server free-finish path and both client affordances complete the build.
- Verified END TO END from the new location: server serves index/battle.png/theme.mp3 (200); the rail no longer
  cuts off; the queue pill and the modal Finish button both hasten a build; the battle cinematic plays for a
  win and a loss. Screenshots fix_rail.png, fix_battle_win.png, fix_battle_loss.png, fix_modal_finish.png. No
  JS errors.

## ROADMAP COMPLETE
All planned pillars are built and tested: foundation economy + timers, building art + interactive town,
procedural sound/music, the big world map + marches + deterministic combat, equipment/heroes with a
transparent-pity Forge, the full retention suite (login calendar, daily task ladder + chests, achievements,
VIP, 30-day season pass), the splash/title key-art, the mobile layout, alliances with timer-shaving help,
the baked flavor-text voice of the realm, and production hardening + README. Further polish ideas live in the
NEXT STEPS list below; none are blocking.

## NEXT STEPS (roadmap — pick the next meaningful one each loop, keep it tested)
1. POLISH + TEST the foundation hard: balance the early curve so the first 3 builds feel fast
   and rewarding (research: early dopamine), fix any UI overflow at common resolutions, make the
   tutorial bullet-proof, screenshot-verify each screen.
2. ART: [DONE] dedicated splash/title key-art (iteration 11); [DONE] painterly building portraits in the
   upgrade modal (iteration 17). Remaining: a few visual TIERS per building (humble/grown/grand) swapped by
   level, a city image that reflects keep level, a Founder/lord portrait. All painterly, offline, high-quality.
3. SOUND + MUSIC: bake an ambient town theme + UI clicks/build-complete/coin/level sfx (offline);
   muted until first click; a mute toggle. Splash video via LTX if viable.
4. [DONE] WORLD MAP + MARCHES + COMBAT (iteration 2); [DONE] PvP city attacks with a beginner shield
   (iteration 16). Future: resource-gather tiles, scouting, a wounded/hospital system, reinforcements.
5. [DONE] SOUND + MUSIC (iteration 3, procedural SFX); [DONE] composed ACE-Step main theme (iteration 18).
   Future: a distinct tense map/battle theme to swap in during marches; splash video via LTX.
6. RETENTION: [DONE] daily task ladder + free 4h chest (iteration 6); [DONE] permanent achievements/
   milestones (iteration 8); [DONE] VIP track (iteration 9); [DONE] 30-day season/battle pass with
   free + gold tracks (iteration 10). Remaining: returning-player rewards. The retention suite is now
   broad; next best step is likely ALLIANCES (item 8) or a full MOBILE LAYOUT pass (item 10).
7. [DONE] EQUIPMENT + HEROES (iteration 7): a hero with a gear loadout, deterministic Forge gacha
   with a transparent pity counter, hero buffs to combat/loot/march. Future: relic salvage/fusion
   to upgrade tiers, a second hero slot, set bonuses, hero skills.
8. [DONE] ALLIANCES (banners) (iteration 13): create/join/leave/browse, timer-shaving help (max(1%,60s),
   20 per order), +1%/member production bonus, War Table chat. Future: a shared map territory, rallies
   (joint marches), alliance tech/treasury, member ranks.
9. [DONE] A LITTLE AI / FLAVOR (iteration 14): camp taunts, battle narration, steward counsel, a lore
   codex, all baked offline to a static corpus, deterministic, never an API call at runtime. Future:
   per-building flavor, named barbarian warlords, event log narration.
10. [DONE] MOBILE LAYOUT pass (iteration 12). Remaining: SERVER SELECT (one realm) screen, settings,
    accessibility, push-style in-game alerts ("a host marches on you"). Next best step is likely
    ALLIANCES (item 8), the offline AI flavor text (item 9), or PRODUCTION HARDENING + README (item 11).
11. PRODUCTION HARDENING: rate-limiting, input validation, save integrity, error states,
    a real README, and only THEN a git checkpoint.

## Working rules
- Test before commit. Keep the server runnable at all times. No em-dashes anywhere. Never call
  an AI API at runtime. Bake assets offline. Keep the locked art direction. Do not stop while
  anything can still be improved.
