# GIANTSREACH: master design, roadmap, and build status

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

## DONE (iteration 58: FORT-ECONOMY BALANCE, a high stronghold is now a real fortress)
- A data-driven audit of the territory economy found a concrete imbalance: the stronghold build cost grows
  exponentially (6000 * 1.55^level, ~552k resources cumulative to reach level 10) but the NPC garrison floor grew only
  LINEARLY (85 troops/level), so an ungarrisoned level-10 fort fell to about 560 swordsmen (~45k resources), roughly a
  twelfth of its build cost. Tall forts were trivially cheap to chip down when undefended.
- Fix: the standing garrison now scales super-linearly so the defense tracks the investment. fortGarrison adds a
  quadratic term per arm (spearman 40L + 9L^2, archer 30L + 7L^2, swordsman 15L + 4L^2, and knights 3L^2 from level 5)
  and fortDefMult steepens from 1.35 + 0.05L to 1.4 + 0.07L. A level-10 fort now fields about 3,150 NPC defenders
  (was 850) and needs roughly 2,400 swordsmen to crack ungarrisoned, while low forts stay accessible (a level-2 fort
  still falls to about 140). The mixed garrison (knights have high anti-infantry defense, spearmen high anti-cavalry)
  holds against either attacking arm. Change is isolated to forts; camp, warlord, and PvP combat are untouched.
- Verified END TO END via simulation and LIVE assaults: a modest 900-knight host was REPELLED by a level-6 fort (87%
  losses, its spearmen crushing the cavalry), while a serious 4,000-swordsman host with a strong champion CRACKED a
  level-8 fort but bled 20% doing it, so forts are defensible yet still takeable by a real army. The assault dialog
  shows the heavier garrison (a level-9 fort lists 2,628 defenders). Screenshot fort-balance.png. Guest smoke clean
  (no regression), no em-dashes. (Note: fort marches take the full travel time to resolve, so reports lag the send.)

## DONE (iteration 57: THE SEASON TURNS, a recap when a season closes)
- The 30-day season pass rolled over SILENTLY (seasonSync just wiped the old season when its id changed), so the
  season cycle had no climax and a returning player never learned how their season ended. Added a one-time
  season-turn recap that closes the retention loop with a real moment.
- Server: seasonSync now, on a genuine rollover where the lord made progress, stashes a seasonEnded summary (the
  closed season's name and the pass level reached, the track, and the incoming season's name) before resetting. The
  state view enriches it with the lord's current realm standing by might (a cheap lordStanding sweep) and a baked
  season-turn flavor line, and a new /api/seasonack clears it so it shows exactly once. Deterministic, zero AI.
- Client: on entering, if a season has turned, a "The Season Turns" recap modal takes priority over the Council
  digest, showing the closed season, the flavor quote, a pass-level-reached card and a realm-standing card, and a note
  that the pass resets while the hold and standing endure; Onward acks it.
- Verified END TO END: seeding Hero's season to the previous id at level 15 (premium) triggered, on next load,
  seasonEnded "The Ashen Pact" -> "Banners of the Long Dusk" with level 15/50 and standing 3 of 28 lords; the modal
  auto-showed those exact values and the flavor, and Onward acked it so it did not reappear (seasonEnded cleared). A
  normal guest with no rollover saw nothing. Screenshot season-end.png. No JS errors, guest smoke clean, no em-dashes.

## DONE (iteration 56: YOUR BANNER'S STANDING on the Banners and Territory ladders)
- The Lords ladder showed your own rank when you fell below the visible top, but the Banners and Territory ladders
  did not, so a player could not see where their banner sat once it dropped out of the top 15. Surfaced own-banner
  standing on both, the way the Lords tab already does for the player.
- Server: the leaderboard now resolves the viewer's banner tag and returns youBanner (its rank in the full
  might-sorted list) and youTerritory (its rank among all forted banners), each carrying the full row stats, so the
  standing is known even far below the cut. Sends the viewer's tag too.
- Client: the player's own banner row is highlighted with the gold "me" treatment in both tabs; if it sits below the
  top 15, a "Your standing" row is appended (reusing the Lords-tab pattern). On the Territory tab, a banner with no
  stronghold instead gets a dashed hint pointing to the Banners panel to found one and enter the lists.
- Verified END TO END: the viewer's WLF banner read youBanner rank 3 and youTerritory rank 2; its row rendered
  highlighted at Territory rank 2; and with WLF's stronghold removed, youTerritory went null and the Territory tab
  showed the "found a stronghold" hint. Screenshots you-territory.png / territory-hint.png. No JS errors, guest smoke
  clean (no regression), no em-dashes.

## DONE (iteration 55: THE TERRITORY LADDER, a realm scoreboard for the stronghold war)
- The alliance stronghold war (found, pledge, garrison, assault, raze) generated real stakes but had no realm-wide
  scoreboard, so there was nothing to compete over. Added a Territory standing to the Realm Ladder.
- Server: the leaderboard now computes a banner-stats list once and derives two views from it. The Banners tab still
  ranks by total member might (now carrying each banner's stronghold level), and a new "territory" list ranks only
  the banners that hold a stronghold, by its level then by might, also reporting the total troops garrisoned in it.
- Client: a fourth Realm Ladder tab, Territory, lists the banners by stronghold with a gold fort marker, the
  stronghold level and might on the right, and the garrison strength as a subtitle (or "ungarrisoned"); the Banners
  tab now shows a small gold stronghold badge on each row.
- Verified END TO END: with four banners (forts at 8, 5, 3, and none), the territory list ranked Ravens(8) >
  Wolves(5) > Stags(3) and correctly excluded the fortless banner, reporting garrison counts (1300, 1200, 0); the
  Banners tab showed each fort level. The Territory tab rendered three ranked rows, top "Stronghold 8". Screenshot
  territory-ladder.png. No JS errors, guest smoke clean (no regression), no em-dashes.

## DONE (iteration 54: WAR-REPORT NARRATION, the realm's voice on the battle reports)
- Camp, warlord, and PvP-attacker reports carried a baked flavor line, but the DEFENDER's reports and the stronghold
  siege reports did not, so the emotional peak of the game (being raided, holding the walls, storming a fortress) read
  as dry stat lines. Extended the baked-voice corpus to the whole war layer.
- Server: four new FLAVOR pools, written from the right point of view, of five lines each: defended (you threw them
  back), overrun (your hold or stronghold fell), stormed (you took a rival fortress), and repulsed (you were thrown
  off the walls). The PvP defense report, the fort-assault report, and the per-member fort-defense report now each
  pick a line deterministically by seed, with per-report entropy (the defender's loss fraction; the fort level and a
  member's garrison losses) so adjacent reports do not echo the same line. Static corpus, picked by seed; zero AI at
  runtime, no em-dashes.
- Client: those three report cards now render the narration as an italic line beneath the result, matching the
  existing camp/warlord/city report styling.
- Verified END TO END: a live PvP raid and a live stronghold assault produced, for the attacker, "Your sappers earned
  their bread today. The walls are breached" on the fort report, and for the defender, "They were over the wall before
  the bell had finished ringing" on both the defense and fort-defense reports; the report panel rendered the italic
  narration lines. Screenshot war-flavor.png. Server compiles, guest smoke clean (no regression), no em-dashes.

## DONE (iteration 53: PER-BUILDING FLAVOR, the realm's voice in the upgrade modal)
- The most-repeated action in the game, raising a building, was mechanical: a stat line and a one-line description.
  Added a baked flavor voice so each building has character, the roadmap's "per-building flavor".
- Server: a BLD_FLAVOR corpus of three evocative lines per building (all nine), in the realm's medieval voice. A new
  buildFlavor(bid, level) picks one deterministically from the building id hashed against the level, so the note
  shifts as the building rises, and the building view now carries a flavor field. Baked static corpus, picked by
  seed; zero AI at runtime, no em-dashes.
- Client: the upgrade modal shows the flavor under the description as a gold-bordered italic quote with a scroll mark,
  in the locked carved-oak style.
- Verified END TO END: a guest's building view returned a flavor line per building; the keep's line cycled across
  levels 1/2/3/5/10/20 (deterministic, not static); the barracks modal rendered "An army is made in the quiet yard,
  long before the loud field." Screenshot bflavor.png. No JS errors, guest smoke clean (no regression).

## DONE (iteration 51: PRODUCTION QA SWEEP + favicon)
- A full new-player QA pass: a BRAND-NEW guest account (not a seeded one) was walked through every major screen with
  an automated JS-error collector and a horizontal-overflow detector, screenshotting each: the splash, the steward
  welcome and lord choice, the home town, and all panels (Daily, Shop, Army, Forge, Tasks, Honors, Banners, World map,
  Realm ladder, Steward settings, VIP). Every screen rendered cleanly, including the brand-new empty states (a Level-1
  champion already wearing the chosen lord's face, empty relic slots and stash with guidance, the found-a-banner form).
- The overflow detector flagged #worldinner / #townspots on every in-game screen, but this is a FALSE POSITIVE: that
  is the intentionally oversized pannable town world, clipped by its container; the document does not scroll
  horizontally (scrollWidth == clientWidth). No real layout overflow anywhere.
- The only genuine defect found was a favicon 404 (browsers auto-request /favicon.ico; a production site should answer
  it). Added an inline-SVG favicon, a carved-oak gold shield with an oxblood cross matching the locked HUD, as UI
  chrome via a data URI (no new file, no route, dependency-free).
- Verified END TO END: after the fix a fresh-guest load reported ZERO 404s and ZERO page errors across the walk, and
  the favicon link is present; guest smoke clean. Screenshots qa-*.png. No regression.

## DONE (iteration 50: THE MAJESTIC KEEP, a tier-3 art capstone for the heart of the hold)
- Buildings rose from a base to a grand (tier-2) portrait at level 10 (iteration 42), but there was no late-game art
  payoff. Gave the Keep, the heart of the hold and the building every player pushes hardest, a third majestic tier.
- Baked OFFLINE with Qwen-Image via ComfyUI (127.0.0.1:8188), the locked slow high-quality pass (26 steps, cfg 3.5,
  no Lightning LoRA) as img2img from the grand keep2 at 0.55 denoise, so the citadel keeps its composition while
  rising into a majestic royal castle: soaring gilded spires, a great golden dome, many proud towers, immense
  ramparts, and gold-and-crimson royal banners, in the locked golden-hour painterly style. Saved as web/img/bld/keep3.png.
- Client: the Keep modal loads its tier-3 portrait at level BLD_TIER3 (20) with a richer gilt-and-crimson frame and a
  gold "Majestic" tag; tier-3 is gated to the Keep only, and the existing tier3 -> tier1 -> icon fallback keeps any
  missing bake safe. Other buildings still cap at their grand tier-2. Runtime stays a static image swap; zero AI.
- Verified END TO END: a level-22 Keep loaded keep3.png with the majestic frame and Majestic tag; a guest smoke run
  (low-level buildings on base/grand art) was clean, confirming no other building reaches for a tier-3 it lacks.
  Screenshot keep3-modal.png. No JS errors, no regression.

## DONE (iteration 49: BALANCE FIX, the defender's champion now actually defends)
- A data-driven balance review found a real, concrete defect (not a vibe): the DEFENDER's hero defense bonus was
  computed and shown ("Defense +X%" from the Bulwark trait, armor relics, and the Panoply) but NEVER applied in any
  combat. Only the attacker's hero attack bonus fed the math, so PvP was structurally attacker-favored and every
  point a player spent on defense was dead. (The early curve and unit cost-efficiency were re-checked and are fine.)
- Server: resolveCityAttack now folds the defender's hero defense into the defense multiplier alongside the wall,
  defMult = (1 + wall*WALL_DEF) * (1 + defenderHeroDef/100), symmetric with the attacker's atkMult = 1 + atk/100.
  PvE defenders (camps, warlords, forts) stay NPC garrisons with no hero, which is correct. Scout intel now also
  carries the target's champion defense so an attacker can see and plan against it.
- Client: the scout report in the march dialog shows "Champion +N% def" beside wall and watchtower.
- Verified END TO END: a combat simulation showed a fixed attacker's losses climbing 39% -> 57% -> 85% as the
  defender's hero defense rose 0 -> 30 -> 70%; a live PvP attack by a 2000-swordsman host with a Warmonger champion
  on a defender holding a +110% defense champion behind wall 8 WON but lost 68% of the host (a pyrrhic result that
  before the fix would have been a cheap win); a scout returned heroDef 110 and the attack dialog displayed it.
  Screenshot scout-def.png. No JS errors, guest smoke clean (no regression).

## DONE (iteration 48: THE STRONGHOLD GETS A PORTRAIT, baked painterly art for the territory war)
- The Banner Stronghold, now a full found/pledge/buff/garrison/assault system, was represented only by a map glyph
  and a flag icon. Baked a painterly stronghold portrait so the central territory feature carries real visual weight.
- Baked OFFLINE with Qwen-Image via ComfyUI (127.0.0.1:8188), the locked slow high-quality pass (24 steps, cfg 3.5,
  no Lightning LoRA): a fortified war keep on a rocky hill with crenellated ramparts, corner bastion towers, an
  iron-gated barbican, crimson-and-gold alliance war banners, braziers, and a garrison drilling in the bailey, in the
  locked golden-hour painterly style and deliberately distinct from the home city/keep (this reads as a military
  fortress). Saved as web/img/fort.png. Pipeline: scratchpad/fort_bake.py (reusable).
- Client: the stronghold card in the Banners panel and the rival-fort Assault dialog both now lead with the portrait
  as a carved-oak framed header strip with a level/banner caption, with a graceful hide-on-error fallback. Runtime is
  a static image; zero AI at game time.
- Verified END TO END: the fort card portrait loaded over a level-4 stronghold, and the Assault dialog portrait loaded
  for a rival (Ravens, level 3); both framed in the carved-oak style with captions. Screenshots fortart-card.png /
  fortart-assault.png. No JS errors, guest smoke clean (no regression).

## DONE (iteration 47: FORT GARRISONING, members defend the stronghold with real troops)
- A stronghold's defense was a fixed garrison by level, so members could not actively defend it and an assault was a
  formality. Members can now station their own troops in the banner stronghold, making fort defense a real, contested
  asset that the banner stocks and risks together.
- Server: /api/fortgarrison marches a member's host to their own stronghold (kind "fortgarrison"); on arrival the
  troops join a.fort.garrison[name] and stay until recalled or slain. /api/fortrecall pulls a member's survivors home
  instantly. The assault resolver now combines the standing garrison AND every garrisoned troop into the defense, so
  a well-stocked fort genuinely repels attackers; the garrisoned troops take the defender's share of the casualties
  (and are lost with the fort if it is razed). allianceView reports the total and your own garrison; defense reports
  now show how many of your garrison fell.
- Client: the stronghold card gained a garrison line (total and yours) with Garrison Troops and Recall buttons, a
  garrison troop-picker dialog, and report cards for garrisoning and for garrison losses in a defense.
- Verified END TO END: a member garrisoned 2000 knights into the level-3 fort (deposited after travel; alliance view
  showed garrison 2000); a rival's 4000-knight assault still won but lost 14% to the bolstered defense (vs near zero
  against an empty fort), the garrison fell from 2000 to 453, the defense report read lostGarrison 1547, and the
  defender recalled the 453 survivors home. Screenshot fort-garrison.png. No JS errors, guest smoke clean (no regression).

## DONE (iteration 46: CONTESTABLE STRONGHOLDS, alliance-vs-alliance war on the map)
- The Banner Stronghold gave alliances a map presence but it was untouchable, so there were no territorial stakes.
  Made rival strongholds contestable: a host can now march on an enemy banner's fortress and batter it down.
- Server: /api/fortassault sends a host at a rival fort (blocked against your own banner, while under beginner's
  peace, or while the fort is rebuilding). It resolves as a deterministic march (kind "fort") on arrival against the
  fort's standing garrison (40 spearmen + 30 archers + 15 swordsmen per level) behind a fortified wall multiplier
  (1.35 + 0.05/level). A victory knocks the stronghold DOWN one level (razing it entirely at level 1), zeroes its
  pledge progress, and raises a 30-minute rebuild shield; the attacker takes shards + looted resources and the whole
  defending banner is warned on the War Table and with a defense report to every member. A loss leaves it standing.
- Client: rival fort tiles on the map are now clickable (gold allied, violet rival, greyed while rebuilding) and open
  an Assault dialog showing the garrison and the stakes; new battle-report cards for both the attacker (STORMED /
  RAZED / REPELLED) and each defender (HELD / BATTERED / RAZED).
- Verified END TO END: a rival (Rax of Ravens) stormed Wolves' level-3 stronghold, battering it to level 2 (not
  razed), banking 35 shards + 3,600 loot; the defending leader got a fortdef report and the War Table logged the
  storming; a re-assault while the rebuild shield stood was rejected; the assault dialog rendered the garrison and
  stakes. Screenshot fort-assault.png. No JS errors, guest smoke clean (no regression).

## DONE (iteration 45: THE BANNER STRONGHOLD, a shared alliance fortress on the map)
- Alliances had help, reinforcement, rallies, and ranks, but no shared PRESENCE on the world map and no collective
  build goal. Added the Banner Stronghold: a fortress the whole banner raises together, the roadmap's "shared map
  territory" in a bounded, fully-testable form.
- Server: the leader founds it with /api/fortfound, which places it on the nearest open map tile to the leader's hold
  (a deterministic spiral scan past camps, ruins, cities and other forts). Any member raises it with /api/fortdonate,
  pledging owned resources; accumulated pledges level it up (cost 6000 * 1.55^(level-1)), to a cap of level 10. Every
  level grants the WHOLE banner +2% march speed (up to +20%), folded into the march, city-attack, and rally travel
  formulas via allyFortSpeed. The stronghold appears as a map tile for everyone in view; allianceView carries its
  level, pledge progress, next cost, and speed bonus. Founding and each level-up post to the War Table.
- Client: a gold Banner Stronghold card atop the Banners panel (level, +march%, a pledge progress bar, a Pledge
  Resources dialog, or a Found button for the leader); a distinct gold (allied) or violet (rival) banner marker on the
  world map. Runtime stays deterministic and server-authoritative.
- Verified END TO END: a non-leader was blocked from founding; the leader founded it at (398,398); founding twice was
  rejected; a 6000 pledge raised it to level 2 (+4% march, next cost 9300) and the storage-capped donation math was
  correct; an empty pledge was rejected; the fort showed on a member's map as an allied L2 banner; a real allied march
  computed its travel with the buff folded in without error. Screenshots fort-panel.png / fort-map.png. No JS errors,
  guest smoke clean (no regression).

## DONE (iteration 44: THE CHAMPION GETS A FACE in the Forge)
- The Forge hero bar, the most-opened progression screen (relics, traits, panoply, fusion, rally leadership), showed
  the Champion as a faceless shield glyph. That was the single most glaring remaining art gap, on a high-traffic
  screen. Gave the Champion a portrait.
- The most coherent fix uses the locked, already-baked art: the Champion now wears the player's CHOSEN lord likeness
  (the lord0-3 portrait picked at founding), so "Your Champion" is unmistakably YOU. The hero bar renders the chosen
  lord portrait in the existing gilt frame with the level badge, with a graceful fallback to the old shield glyph if
  the image ever fails. No new bake was needed and a generic baked face would have been LESS coherent than the
  founding choice, so this reuse is the right design, not a shortcut. Runtime stays a static image; zero AI at runtime.
- Verified END TO END: a Hero with portrait 2 (the Lady Commander) shows lord2.png in the Forge hero bar (image
  loaded, framed, with the Level XXII badge), reflecting the founding choice. Screenshot hero-face.png. No JS errors,
  guest smoke clean (no regression).

## DONE (iteration 43: MOBILE POLISH SWEEP + coach-bubble fix)
- Audited every newer panel at a 390px phone viewport with an automated horizontal-overflow detector: the Forge
  (hero bar, Champion's Traits, fusion/salvage bars, relic stash), the Banners panel (rally card + rank chips +
  management buttons), the warlord raid dialog, and the map. All were overflow-free and the alliance grid already
  collapses to a single column, so the newer screens hold up on mobile.
- Fixed a real defect found in the sweep that affected ALL screen sizes: the early-game coached objective bubble
  (rendered in #tutorial) lingered ON TOP of an open modal, because it was only re-evaluated on the next state sync.
  showModal and showModalWide now hide the bubble the instant any modal opens, and closeModal re-renders the
  objective so it returns immediately when the modal closes (no wait for the next sync).
- Tightened dual-action dialogs on narrow screens: the warlord raid dialog's two buttons (Call a rally / Send the
  march) wrapped to two lines at 390px; a small max-width:430px rule trims the modal-action button font so each label
  now sits on one line (both buttons a clean 44px).
- Verified END TO END at 390px: the coach bubble is HIDDEN with the Forge open and RETURNS after closing it; no panel
  exceeds the viewport width; the warlord buttons are single-line. Screenshots m-forge.png / m-ally.png / m-warlord2.png.
  No JS errors, guest smoke clean (no regression).

## DONE (iteration 42: PER-BUILDING VISUAL TIERS, the baked painterly art pass)
- Building portraits were a single static painting regardless of level, so upgrading never changed how a building
  looked. Baked a grander tier-2 portrait for ALL NINE buildings so the most-repeated action (upgrading) is visibly
  rewarded. This is the roadmap's "per-building visual tiers", and the remaining locked-art item.
- Baked OFFLINE with Qwen-Image via ComfyUI (127.0.0.1:8188), the locked slow high-quality pass (26 steps, cfg 3.5,
  no Lightning LoRA) as img2img from each existing portrait at a moderate 0.5 denoise, so the composition is kept and
  each grand version unmistakably reads as the SAME building grown greater (more towers, fuller, richer, prouder), in
  the locked painterly golden-hour style. Source portraits were uploaded through ComfyUI's /upload/image so no input
  path was assumed. Saved as web/img/bld/<name>2.png. Pipeline: scratchpad/bld_tier_bake.py (reusable for re-bakes).
- Client: the building modal loads the grand portrait at level BLD_TIER2 (10) and up, with a graceful fallback chain
  (tier2 -> tier1 -> the woodcut icon) so a missing bake never breaks the screen; a gilt double-border frame and a
  gold "Grand" tag mark the upgraded art. Runtime stays a static image swap; zero AI at game time.
- Verified END TO END: a level-15 Keep and level-11 Barracks both showed their tier-2 portrait with the Grand frame
  and tag (keep2/barracks2.png), while a level-9 Sawmill correctly stayed on its tier-1 art; all nine PNGs baked and
  non-empty. Screenshots grand-keep.png / grand-barracks.png. No JS errors, guest smoke clean (no regression).

## DONE (iteration 41: BANNER RANKS, officers and a transferable leadership)
- The alliance had no internal structure: every member was equal and only the founder was nominally "leader" with no
  powers. Added member ranks (the roadmap's last alliance gap): leader, officer, and member, with a real permission
  ladder for moderation and succession.
- Server: ranks are an a.officers list beside the existing a.leader. /api/alliancepromote and /api/alliancedemote
  (leader only) raise and lower officers; /api/alliancetransfer (leader only) passes the mantle, the old leader
  stepping down to officer; /api/alliancekick lets a leader expel anyone and an officer expel only plain members, a
  kicked lord's banner cleared. pruneAlliance keeps officers valid and, when a leader departs, passes the mantle to
  an officer first, then the next member. allianceView now returns each member's rank plus a per-viewer can{} set so
  the client only ever shows permitted actions. Every change is announced on the War Table.
- Client: the roster shows a gold Leader chip and a purple Officer chip, and renders only the management buttons the
  viewer is allowed (Raise to officer / Lower to member / Pass the banner / Expel), with a confirm on the two
  destructive ones. Members are sorted leader, then officers, then by might.
- Verified END TO END with three banded lords: leader promoted an officer; the officer was correctly BLOCKED from
  promoting or from kicking the leader but COULD expel a plain member (whose banner was then cleared); leadership
  transfer swapped the ranks; a demote returned an officer to the ranks; and when the leader left, the mantle passed
  to the officer. The leader's roster screenshot (ranks.png) shows the chips and the correct per-member buttons. No
  JS errors, guest smoke clean (no regression).

## DONE (iteration 40: ALLIANCE RALLY, a joint march where banded lords fight as one)
- Verified first that the early curve and combat scaling are already well-tuned (first builds affordable from the
  start, 46s to 2m timers, no degenerate combat), so a forced balance tweak would be low value. Built the headline
  missing MMO feature instead: joint rally marches, the roadmap's "joint rally marches", aimed at the new warlords.
- Server: a rally is escrowed, lazy, and deterministic. /api/rally calls one on a warlord (the leader's host is
  pulled from their barracks into the rally); banded members add their own host with /api/rallyjoin during a 180s
  muster. When the muster ends the COMBINED host marches as one from the leader's city; on arrival a SINGLE combat
  resolves with the rally leader's champion leading (their hero attack bonus, the warlord's 1.35 defense edge). Each
  lord keeps survivors in proportion and splits the loot by surviving carry; the warlord's relic and shards go to the
  lord who called it. Outcomes are delivered through each lord's own lazy resolve (deliverRallies, no cross-player
  writes), so it survives restarts and offline gaps. One rally per banner; a collapsed rally refunds every host.
- Client: the Banners panel shows an oxblood rally card (warlord, a live muster countdown, who has mustered, a Join
  button); the warlord raid dialog gains a "Call a rally" action for banded lords; a rally battle report shows your
  share of the spoils and (for the caller) the relic. The War Table announces the muster call and each join.
- Verified END TO END with two banded lords (Rax + Bran): Rax called a rally on Ysolde and Bran joined (host 1000 ->
  2000, double-join rejected); after muster + march + battle + return both won, each got survivors home (~1993 of
  2000) and an equal 7,625 loot split and +80 hero XP, and ONLY the caller Rax banked the 60 shards + the relic; the
  rally cleared after delivery. Screenshot rally-banner.png. No JS errors, guest smoke clean (no regression).

## DONE (iteration 39: NAMED WARLORD CAMPS, elite PvE with a guaranteed relic)
- The world map's barbarian camps were anonymous and flat: every level-N camp identical, no aspirational target.
  Added named Warlord camps, the roadmap's "named barbarian warlords", as rare elite camps that reward a strong host.
- Server: about one in seven camp tiles becomes a Warlord (deterministic from coords; levels 5 to 8). A WARLORDS
  corpus of eight named barbarians (Gorruk the Bonebreaker, Ysolde the Red Oath, ...) each carries a baked taunt.
  warlordGarrison is far tougher (spearmen, archers, swordsmen AND knights) and the warlord fights with a wall-like
  1.35 defense edge. A felled warlord ALWAYS yields shards (30 + 6/level) and a guaranteed relic rolled to the field
  (deterministic per clear via a wlN counter, seed-collision-guarded), plus double the loot, double hero XP, and a
  bigger season gain. A warlord stays broken for 3h (a camp returns in 30 min). All deterministic, zero runtime AI.
- Client: warlord tiles render as a distinct dark-crimson, gold-bordered sword marker (vs the bright numbered camps);
  the raid dialog is reskinned with the warlord's name and title in the header, a gold taunt, and a reward hint that
  a felled warlord drops shards and a relic. Battle reports read "Felled <name>... took a Legendary relic, N shards,
  ..." with a SLAIN seal; the cinematic and the returning-player recap name the warlord too.
- Verified END TO END: a 12k host marched on Ysolde (L5) and won, banking 60 shards and a Legendary War Banner into
  the stash with +80 hero XP; re-raiding the felled tile is blocked by the 3h cooldown; the map showed 15 warlord
  cells (14 raidable) and the dialog rendered "Gorruk, the Bonebreaker" with taunt + reward hint. Screenshots
  warlord-map.png / warlord-dialog.png. No JS errors, guest smoke clean (no regression).

## DONE (iteration 38: RELIC FUSION, ascend duplicates a tier)
- The stash filled with duplicate low relics that could only be salvaged for shards. Added the last equipment lever
  on the roadmap: tier-up fusion, so duplicates become a progression path, not just scrap.
- Server: /api/fuse {tier} consumes FUSE_N (3) stash relics of a tier and forges one of the NEXT tier up
  (Common->Rare->Epic->Legendary; Legendary cannot be fused). It always consumes the THREE LOWEST-value relics of
  that tier, so a player never loses their best to a fusion. A new relicAtTier() forges at an EXACT tier (a fresh
  deterministic slot/affix/value roll) so the "ascend to Rare/Epic" promise is honest, with a seed-collision nudge
  to keep stash seeds unique. Shard-neutral by design (3 Commons salvage for the same 15 as the Rare they make), so
  it is a real choice against salvage rather than a strictly better one. view() exposes fuseN.
- Client: an "Ascend relics" bar in the Forge stash with gold "Fuse 3 Common -> Rare" / "Fuse 3 Rare -> Epic" /
  "Fuse 3 Epic -> Legendary" buttons that appear only when you hold enough of a tier, distinct from the green salvage
  bar; a toast announces the ascended relic.
- Verified END TO END: seeded 4 Common + 3 Rare + 1 Epic; fusing Common consumed the 3 lowest (kept the best Common)
  and yielded a Rare; a second fuse with one Common left returned "Fusing needs 3 Common relics. You hold 1."; fusing
  Rare yielded an Epic; tier 3 was rejected. In-UI the two buttons rendered with correct labels and clicking Rare
  raised the Epic count from 1 to 2. Screenshot fuse.png. No JS errors, guest smoke clean (no regression).

## DONE (iteration 37: CHAMPION'S TRAITS, build-defining hero choices)
- The hero levelled and equipped relics but every champion grew identically. Added rankable traits (the roadmap
  "hero skills") so the player makes lasting, build-defining choices as the hero levels.
- Server: a HERO_TRAITS table of seven (Warmonger +atk, Bulwark +def, Outrider +march speed, Plunderer +spoils,
  Quartermaster +1 march slot, Master Mason +build speed, Drillmaster +train speed), each rankable up to 3. The
  champion earns one trait point every 5 hero levels (traitPoints/traitsSpent track earned vs spent). heroTraitBonus
  folds into every relevant system: heroBonusOf (atk/def/speed/gold for combat, loot, march), buildSpeedMult,
  marchCap, and the train route's per-unit time. /api/trait spends a point with full guards (unknown id, no points
  with a helpful message, already-mastered at rank 3). view() exposes hero.points/nextAt/traits + traitDefs/traitMax.
- Client: a "Champion's Traits" section in the Forge with seven trait cards (icon, name, rank dots, "+" to spend),
  a "N points to spend" pill or "next at Level X" hint, and a Forge rail badge that lights when points are unspent.
- Verified END TO END: a Level 22 hero reads 1 spare point (4 earned, 3 spent on Warmonger x2 + Outrider x1) with
  nextAt 25; spending Bulwark decrements to 0 points and records {warmonger:2,outrider:1,bulwark:1}; a further spend
  returns "No trait points. Your champion earns one every 5 levels." The hero bar reflects the trait bonuses in its
  affix line (Atk +54% / Def +42% / March +12%). Screenshot traits.png. No JS errors, guest smoke clean (no regression).

## DONE (iteration 36: THE PANOPLY, a hero set bonus)
- The hero was shallow beyond levelling and equipping four relics. Added a set bonus that rewards completing
  and refining the loadout, giving the whole relic economy (Forge, ruins, reforge, salvage) a long-term goal.
- heroBonusOf now computes a Panoply = (relics equipped) + (sum of their tiers), 0 when bare up to 16 for four
  Legendaries, and adds it as a FLAT bonus to every affix (attack, defense, march, spoils). It flows through
  everywhere the hero bonus is used: combat, loot, and march speed. So filling all four slots with finer relics
  broadly strengthens the champion, not just the slotted stats.
- Client: a green "Panoply +N (M/4)" badge in the Forge hero bar (with a tooltip explaining it), so the player
  sees the set bonus and how full the loadout is; the combined affix line already reflects it.
- Verified END TO END: a bare hero reads panoply 0 / all bonuses 0; equipping a full Epic/Legendary loadout
  (tiers 2,2,3,3) gave panoply 14 and lifted every affix by 14 on top of the per-relic rolls. The hero bar
  shows "Panoply +12 (4/4)" for a test loadout with Attack +31% / Defense +26% / March +32% / Spoils +44%.
  Screenshot panoply.png. No JS errors, no regression (a bare hero is unchanged from before).

## DONE (iteration 35: RELIC SALVAGE + REFORGE, depth for the Forge)
- The Forge gacha and now ruin delving both grant relics, so the stash fills with duplicates and junk that had
  no use. Added the two grounded gacha levers (on the roadmap as "relic salvage/fusion"): salvage and reforge.
- /api/salvage: melt a stash relic down for shards by tier (Common 5 / Rare 15 / Epic 40 / Legendary 100), as
  a single {seed} or in bulk {maxTier} (all stash relics up to a tier, for decluttering). Equipped relics are
  protected from salvage. /api/reforge {seed}: spend 45 shards to re-roll a relic's value within its tier
  range (works on stash OR equipped relics; it can come up better or worse, which is the gamble). Both costs
  are in /api/state.
- Client: each stash relic now offers Equip / Reforge (with the shard cost) / Salvage (with the tier payout),
  a "Clear the clutter: Salvage all Common / up to Rare" bulk bar, and equipped relics gain a reforge link
  beside unequip. The toast reports whether a reforge came up stronger or weaker.
- Verified END TO END: forged 8 relics, salvaged one (Common -> +5 shards), bulk-salvaged Common+Rare (6
  relics -> 40 shards), reforged an EQUIPPED relic (+7 to +4, the gamble), and confirmed an equipped relic
  cannot be salvaged. The Forge stash renders 9 salvage + 9 reforge buttons and the bulk bar with correct
  per-tier payouts (Epic +40, Rare +15, Common +5). Screenshot forge_salvage.png. No JS errors, no regression.

## DONE (iteration 34: DELVING THE FALLEN GIANTS, ruins become explorable)
- The world is named for the fallen giants, but their RUINS on the map were purely decorative. Now they are
  explorable points of interest that finally pay off the central theme and reward exploring the big map.
- /api/delve {x,y}: validates a ruin actually lies there, charges a small provisioning fee (250 grain + 250
  timber), and yields a DETERMINISTIC one-time reward seeded by the ruin's coords: a buried resource cache
  (~45%), a hoard of shards (~30%), or a Rare-or-better RELIC (~25%, via the same Forge roller, giving relics
  a second source). Each ruin can be delved only once (tracked in p.delved), and the map marks delved ruins.
- Every delve also reveals a baked EPITAPH of the giant who lies there (a new 8-line FLAVOR pool, picked by
  coord seed), and drops a "delve" entry in the battle log.
- Client: undelved ruins on the world map are now clickable (delved ones dim out); a "A Fallen Giant" dialog
  ("Send a delving party", with the provisions cost) opens a reveal ("The Ruin Gives Up Its Dead") that shows
  the reward and the giant's epitaph between carved rules.
- Verified END TO END: delving a ruin returned a resource cache and the epitaph "Here lies one who held a
  mountain on his shoulders...", a second delve of the same ruin was rejected ("already picked clean"),
  delving a camp coord errored ("No ruin lies there"), and the map flips the ruin to delved. The client dialog
  and reveal render in the locked aesthetic (a shards find showed "43 SHARDS OF THE OLD AGE" + epitaph).
  Screenshots delve_dialog.png, delve_reveal.png. No JS errors, no regression.

## DONE (iteration 33: ADVERSARIAL SWEEP, prototype-key hardening)
- Ran a systematic adversarial sweep of every state-mutating route with hostile inputs (negative and huge
  numbers, out-of-range indices, bogus ids, no auth, prototype keys). Most rejected cleanly and state stayed
  intact, but the sweep surfaced one real bug.
- BUG FIXED: build and train looked up the client key directly (BUILD[bid], UNITS[unit]). A key like
  "__proto__" or "constructor" returns a truthy object from the prototype chain, so the `if (!BUILD[bid])`
  guard passed and the route then operated on garbage (a 500 error, and a risk of polluting p.b with a NaN
  "constructor" level). Added a has(obj, k) own-property helper and used it in both guards.
- Verified: build/train with __proto__ or constructor now return a clean "no such building" / "no such unit"
  (no 500, no pollution; p.b still holds only the nine real building keys), while legitimate build/train work
  and the server logs zero errors. The rest of the sweep (speedup/cancel out-of-range, buygems bad pack,
  season out-of-range, achv bogus, scout-self, alliancehelp with no banner, missing auth) all rejected and
  left gems/troops/resources untouched. Clean-db guest smoke renders with no JS errors.

## DONE (iteration 32: SECURITY FIX + README refresh, the production capstone)
- Audited the routes added since the iteration-15 hardening pass. Found and fixed a real EXPLOIT: the march /
  attack / reinforce routes deducted troops with `p.t[u] -= troops[u]` on unsanitized input, so a NEGATIVE
  count (e.g. {spearman:50, swordsman:-10}) added soldiers instead of removing them, fabricating troops from
  nothing (confirmed: a march minted 10 swordsmen). Added cleanTroops() which keeps only known units as
  non-negative integers, applied to all three routes. Verified: the same payload now sanitizes to
  {spearman:20} (junk keys and the negative dropped), no fabrication, and legitimate marches still work.
- Refreshed the README, which was 16 iterations stale (written at iteration 15). It now documents the full
  current game: the war layer (scout, incoming warnings, PvP attack, the infirmary, the battle cinematic),
  alliances with reinforcement, the retention + progression suite, the Realm Ladder, the Council recap,
  settings/accessibility, a how-to-play section, the ACE-Step composed music and img2img city growth in the
  offline-baking section, and the troop-sanitization line under Hardening.
- Verified END TO END: a clean-db guest smoke renders with no JS errors and every asset serves 200 (index,
  splash, city3, lord0, theme.mp3, battle.mp3). The exploit-fix curl test passed.

## DONE (iteration 31: INCOMING ATTACK WARNINGS, a host marches on you)
- Closed the last PvP defensive gap: an attacker's march was invisible to the target until it landed (the
  post-hoc defense report). Now the defender is WARNED while the enemy is still on the road and can prepare.
- Server: incomingFor(p) scans every other lord's marches for an unresolved city attack aimed at p that has not
  yet arrived, returning the attacker's name, home coords, depart/arrive times, and total host size; exposed as
  `incoming` in /api/state (sorted soonest-first).
- Client: a prominent pulsing oxblood HUD banner ("A HOST MARCHES ON YOU" + attacker, an estimate of the host
  size, and a live countdown; "+N more" when several converge), which opens the world map on click. A defeat
  sting and a toast fire once per fresh threat (tracked by from+arrive so it never nags). On the world map the
  incoming host is drawn as a bright-red dashed line from the attacker's hold with a fast-pulsing marker
  advancing on your home star, alongside your own marches.
- Verified END TO END with two accounts: Cwar's live attack on Adef surfaced in Adef's state as
  incoming [Cwar, ~245 strong], the HUD banner read "A HOST MARCHES ON YOU / Cwar · ~215 strong · 3:56" with a
  ticking countdown, and the map drew the advancing red marker. A guest with no threats keeps the banner
  hidden. Screenshots incoming_banner.png, incoming_hud.png, incoming_map.png. No JS errors, no regression.

## DONE (iteration 30: SETTINGS + ACCESSIBILITY)
- There was only a mute toggle and no real settings. Built a proper Settings panel in the Steward (gear).
- SOUND: independent Music and Effects volume sliders (0-100) and a Mute-all toggle. audio.js gained per-channel
  volume (musicVol/sfxVol, persisted) that scales every music node (the baked theme, the procedural bed, and
  the battle cue, including its ducking) and the sfx bus respectively; the Effects slider previews a click on
  release. The settings mute toggle stays in sync with the top-bar mute icon.
- DISPLAY: a Reduce-motion toggle for accessibility. It adds body.reduce-motion (persisted, re-applied at boot)
  which a CSS block uses to kill the splash Ken Burns drift, the tutorial pulse, and all keyframe animations
  (durations collapse to ~0), while leaving the timed cinematic phases functional.
- YOUR HOLD: the lord, coordinates and banner, plus the existing Chronicle and Leave actions.
- Verified END TO END: the panel renders the gold sliders and pill toggles in the locked carved-oak style;
  moving the Music slider set GA.musicVol() to 0.35 and persisted gr_musicvol; the Reduce-motion toggle set
  the body class and gr_reducemotion, and after a reload the class was re-applied and the splash animation
  computed to "none". Screenshot settings.png. No JS errors, no regression.

## DONE (iteration 29: THE COUNCIL, a returning-player recap)
- A time-based game leaves you returning after hours to scattered glowing badges. Added a "Council" digest
  shown once per session to RETURNING lords (tutorial >= 1; brand-new players still get the Welcome instead).
- Two sections. WHILE YOU WERE AWAY: a tally of the battles resolved since your last visit (camp raids, marches
  on rival holds, and how many times your hold was assailed and whether any broke through), with a "See the
  battle reports" link that opens the world map's report list. AWAITING YOUR WORD: clickable rows for every
  thing ready to claim right now (daily tribute, task chests, the free chest, the VIP audience, season pass
  rewards, wounded to tend, banner aid to give), each jumping straight to its modal.
- Implemented entirely client-side from the existing /api/state snapshot; the "since last visit" boundary is a
  per-lord localStorage timestamp, and a sessionStorage flag keeps it to once per browser session (no nag on
  every sync or reload). It only appears when there is actually something to report or claim.
- Verified END TO END: a seeded returning lord with recent reports, wounded, and an unclaimed daily got the
  Council showing "2 raids resolved / hold assailed 1 time (1 broke through)" plus four claim rows (daily, free
  chest, VIP, 35 wounded); a brand-new guest correctly gets the Welcome and NO council. Screenshot council.png.
  No JS errors.

## DONE (iteration 28: THE REALM LADDER, a real competitive hub)
- The leaderboard was a plain top-20-by-might list, despite "climb the realm ladder" being the stated endgame.
  Rebuilt it into a tabbed competitive hub.
- /api/leaderboard now returns three categories plus YOUR own rank: LORDS (top 20 by might, each with rank,
  lord portrait, alliance tag, might + keep), WARLORDS (top 15 by lifetime raids won), and BANNERS (top 15
  alliances by combined member might). The requester's might rank is always computed, even when outside the
  top 20.
- Client: a tabbed ladder modal (Lords / Warlords / Banners) with gold rank badges (top 3 highlighted), the
  lord's chosen portrait thumbnail, alliance tag chips, and a header count ("N lords contend"). Your own row
  is gold-highlighted; if you rank outside the top 20 a "Your standing" footer pins your row at the bottom.
- Verified END TO END: seeded a 25-lord realm with varied might, portraits, raids and two banners; the API
  returned the correct top lords / warlords (by raids) / banners (by might) and put the weak requester at rank
  25; all three tabs render with portraits and tags, and the "Your standing" footer shows the highlighted self
  row. Screenshots ladder_lords.png, ladder_warlords.png, ladder_banners.png, ladder_youfoot.png. No JS errors.

## DONE (iteration 27: ALLIANCE REINFORCEMENT, banding together matters in war)
- Alliances gave a production bonus, build-help, and chat, but did not matter in battle. Now you can send
  troops to GARRISON a banded member's hold; they join every defense of that hold until recalled or slain.
- /api/reinforce {member, troops} launches a reinforce march (counts against the army cap, not scouts) that on
  arrival adds the troops to the target's reinforcements garrison (keyed by sender). resolveCityAttack now
  forms the defending host from the lord's own troops PLUS every reinforcement contingent; casualties are
  distributed proportionally, owner losses become wounded as before, and each reinforcing ally loses their
  share and gets a report of how the garrison fared. /api/recall {member} marches your survivors home.
- Reports to all three parties: the attacker (their attack), the defender (the defense, now bolstered), and
  each reinforcer (kind "reinf": held/fell + losses; plus a "reinfsent" log when troops arrive).
- Client: the alliance roster shows each member's "N garrisoned" badge, a green Reinforce button (a troop
  picker, "Send to the walls") and a gold Recall button when you have troops there. Reinforce marches read
  "Aiding X", draw GREEN on the world-map overlay, and the battle log shows the new report kinds.
- Verified END TO END with three accounts: a weak defender who would have fallen alone HELD once a banded ally
  garrisoned 290 troops (attacker lost 85%); casualties split proportionally (the reinforcer lost 61%, same as
  the defender), all three got their reports, and recall returned the 113 survivors. The resolver re-entrancy
  guard handles the nested defender resolve. Screenshots reinf_roster.png (garrison + buttons) and
  reinf_dialog.png. No JS errors, no regression.

## DONE (iteration 26: MARCHES VISUALIZED ON THE WORLD MAP)
- Balance check first: measured the new-player early curve and it is well-tuned (starting stores cover ~15
  builds, every early build is cheap and under 69s so the first several finish FREE instantly, 2 build slots).
  So the real gap was the world map: active marches lived only in a side panel, never shown on the map.
- Now your hosts ride across the Reach. An SVG overlay inside #mapinner draws, per active march, a dashed
  route line from your hold to the target and a pulsing marker that moves along it in real time, color-coded
  by kind: gold for camp raids, red for war on a rival hold, blue for scouts. Outbound marches travel toward
  the target (progress from depart to arrive); resolved ones head home (arrive to ret). The marker positions
  update every frame in the main loop and ride the map's pan/zoom transform; the layer rebuilds if the set of
  marches changes. Server now sends each march's depart time so progress can be computed client-side.
- Verified END TO END: seeded three in-flight marches (a raid outbound ~45%, a raid returning, and a scout);
  the overlay drew 3 routes + 3 markers at the correct progress and colors; a guest with no marches opens the
  map cleanly (layer present, 0 markers). Screenshot map_marches.png. No JS errors, no regression.

## DONE (iteration 25: LORD PORTRAITS, choose the face the realm knows you by)
- Personalized identity: the HUD avatar was a generic shield. Baked FOUR painterly lord/lady portraits OFFLINE
  with Qwen on the locked recipe (768x768, 24 steps, cfg 3.5): The Veteran (a grizzled scarred warlord), The
  Young Lord (a circleted noble), The Lady Commander (an auburn-braided warrior-lady), and The Old King (a
  crowned greybeard). All bust portraits in warm golden rim light, distinct but consistent. web/img/lord/.
- A choose-your-likeness picker in the WELCOME flow at founding ("First, my lord, show the realm your face")
  with four selectable tiles, and a changeable-anytime picker opened by clicking the avatar ("Your Likeness").
  The chosen portrait fills the carved-oak avatar tile with the keep-level badge over it.
- Server: p.portrait (0-3, clamped), in /api/state; /api/portrait {i} persists the choice. Endpoint validated
  (set i=2 -> 2; i=9 clamps to 3).
- Verified END TO END: a fresh founder gets the picker in the welcome; selecting The Lady Commander set the
  HUD avatar to lord2.png (confirmed src + visible), and the avatar opens the likeness picker. Portraits serve
  200. Screenshots lord_welcome.png (founding picker), lord_hud.png (avatar), lord_picker.png. No JS errors.

## DONE (iteration 24: THE CITY GROWS WITH THE KEEP, painterly img2img tiers)
- The marquee progression payoff: your home city now visibly transforms as the Keep rises. The long-standing
  blocker was that re-baking the city would break the building markers (the chips are placed at fixed
  percentages over city.png). SOLVED with IMG2IMG: copied city.png into ComfyUI's input and baked two grander
  variants with Qwen at MODERATE denoise (0.46 for tier 2, 0.58 for tier 3), so the composition (and therefore
  the marker positions) is preserved while the city grows. Locked painterly recipe (26 steps, cfg 3.5).
- Three tiers: city.png (a modest walled hold, keep 1-4) -> city2.png (a grown, more fortified town with a
  taller keep, keep 5-10) -> city3.png (a grand multi-towered capital with soaring spires and banners, keep
  11+). The fallen giant's hand and head, the river and bridge, and the wall layout all carry through, so it
  reads as the SAME place risen, not a different picture.
- Client: setCityTier() swaps #worldinner's background-image by keep level, only when the tier changes (no
  flicker). The interactive building chips are unchanged and still land on their buildings because the layout
  is preserved.
- Verified END TO END: seeded keep-6 and keep-13 players; the town backdrop loaded city2.png and city3.png
  respectively (confirmed via computed style), the markers stayed aligned over the grander art, and the
  building/HUD all render. Screenshots tier2_town.png (grown) and tier3_town.png (grand capital). city2.png
  and city3.png serve 200. No JS errors.

## DONE (iteration 23: SCOUTING + a re-entrancy fix for the resolver)
- Completed the conquest loop with recon. The Watchtower (previously near-useless) now governs scouting both
  ways: your scout reveals a rival, and your watchtower turns back theirs.
- /api/scout {x,y}: requires a Watchtower (level >= 1), costs 300 grain + 150 iron in provisions, sends a FAST
  scout march (speed 22, does not count against the army march cap; 3 scouts max). resolveScout on arrival:
  if the target's watchtower outranks yours the scout is CAUGHT (no intel, and the target gets a "spotted"
  warning that war may come); otherwise it returns full INTEL: the target's garrison by unit, wall and
  watchtower levels, keep, might, and current stores. Intel is cached in p.intel[target] and attached to the
  map's city tiles; scout/spotted entries join the battle log; scout marches read "Scouting X".
- The attack dialog now shows the latest scout report (garrison, wall, watchtower, stores, age) above the
  troop picker, plus a blue Scout button, so you recon then commit informed. Watchtower level is in the map
  payload to gate the button.
- FIXED a real recursion bug this surfaced: resolve(p) resolves a scout/attack's target via resolve(target),
  and two players targeting each other with pending marches caused infinite mutual resolution (stack overflow,
  500s). Added a re-entrancy guard (a Set of in-flight player names; a recursive resolve returns the current
  state). This also hardens the pre-existing PvP attack path against the same mutual-resolve case.
- Verified END TO END with two seeded accounts: a strong-watchtower scout revealed the target's exact army /
  wall / might / stores; a weak scout against a strong watchtower was caught and the defender got the spotted
  warning; the recursion guard makes mutual scouts resolve cleanly. Screenshots scout_attack.png (intel +
  scout button) and scout_map.png (INTEL + SPOTTED reports). No JS errors, no regression to camp/guest play.

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

## NEXT STEPS (roadmap: pick the next meaningful one each loop, keep it tested)
1. POLISH + TEST the foundation hard: balance the early curve so the first 3 builds feel fast
   and rewarding (research: early dopamine), fix any UI overflow at common resolutions, make the
   tutorial bullet-proof, screenshot-verify each screen.
2. ART: [DONE] dedicated splash/title key-art (iteration 11); [DONE] painterly building portraits in the
   upgrade modal (iteration 17); [DONE] city image that grows with keep level via img2img tiers (iteration 24).
   [DONE] per-building grand visual tiers via img2img (iteration 42); [DONE] a majestic tier-3 Keep capstone
   (iteration 50). Remaining (optional): a Founder/lord portrait variety. All painterly, offline.
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
7. [DONE] EQUIPMENT + HEROES (iteration 7): a hero with a gear loadout, deterministic Forge gacha with a
   transparent pity counter, hero buffs to combat/loot/march; [DONE] relic salvage + reforge (iteration 35);
   [DONE] the Panoply set bonus (iteration 36); [DONE] rankable Champion's Traits, the "hero skills" (iteration 37);
   [DONE] relic tier-up fusion (iteration 38). Future: a second hero slot.
8. [DONE] ALLIANCES (banners) (iteration 13): create/join/leave/browse, timer-shaving help (max(1%,60s),
   20 per order), +1%/member production bonus, War Table chat; [DONE] reinforcement (iteration 27): garrison
   a member's hold, join their defense, recall; [DONE] joint rally marches on warlords (iteration 40); [DONE]
   member ranks: officers + transferable leadership + moderation (iteration 41); [DONE] the Banner Stronghold, a
   shared map fortress raised by pledges for an alliance-wide march buff (iteration 45); [DONE] rival strongholds are
   contestable, battered down a level per assault and razable (iteration 46); [DONE] members garrison their own troops
   into the stronghold to defend it, with recall (iteration 47); [DONE] a baked painterly stronghold portrait on the
   fort card and assault dialog (iteration 48). Future: alliance ranks gating fort actions.
9. [DONE] A LITTLE AI / FLAVOR (iteration 14): camp taunts, battle narration, steward counsel, a lore
   codex, all baked offline to a static corpus, deterministic, never an API call at runtime; [DONE] named
   barbarian warlords as elite map camps (iteration 39); [DONE] per-building flavor lines in the upgrade modal
   (iteration 53); [DONE] defender + siege war-report narration (iteration 54); [DONE] a season-turn recap with the
   lord's realm standing (iteration 57). Future: a realm-wide event chronicle.
10. [DONE] MOBILE LAYOUT pass (iteration 12); [DONE] SETTINGS + accessibility (iteration 30); [DONE] in-game
    incoming-attack alerts (iteration 31). Remaining (optional): a SERVER SELECT (one realm) screen.
11. PRODUCTION HARDENING: rate-limiting, input validation, save integrity, error states,
    a real README, and only THEN a git checkpoint.

## Working rules
- Test before commit. Keep the server runnable at all times. No em-dashes anywhere. Never call
  an AI API at runtime. Bake assets offline. Keep the locked art direction. Do not stop while
  anything can still be improved.
