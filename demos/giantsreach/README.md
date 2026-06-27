# Giantsreach

A complete, production-quality browser strategy MMO in the Travian / Clash of Kings / Rise of Kingdoms tradition. Raise a hold among the fallen stone giants, grow your economy on time-based timers, train a host, march on the barbarian camps, war with rival lords, band your banner with others, and climb the realm ladder.

The whole game is a single dependency-free Node.js server plus a static web client. The runtime is fully deterministic and server-authoritative. There are no third-party packages, no build step, and no AI calls at runtime.

![Giantsreach: the painterly home town and carved-oak HUD](screenshot.png)

Above: your hold rendered as the playfield, with the carved-oak-and-gold HUD, the resource bar, the action rails, tappable building markers, and the construction and army panels.

## Run it

```sh
./launch.sh
```

That starts the server on `http://localhost:8787` and opens the browser. To choose a port:

```sh
PORT=9000 ./launch.sh
```

Or run the server directly:

```sh
node server/server.js
```

Requirements: Node.js (any modern version). Nothing to install.

## Configuration

The game needs no configuration and makes ZERO AI calls at runtime: it only serves pre-rendered static assets, so
there is nothing to wire up to play or host it. The only runtime settings are environment variables:

- `PORT` sets the server port (default `8787`), for example `PORT=9000 node server/server.js`.
- `RALLY_MUSTER` sets the alliance rally muster window in seconds (default `180`).

## What is in it

Economy and building
- User registration and login (scrypt-hashed passwords) plus one-tap guest play, and a choose-your-likeness lord portrait at founding.
- Time-based construction, training, and march timers that resolve lazily and deterministically, so they keep ticking correctly across server restarts and while you are offline.
- A resource economy (grain, timber, stone, iron, gold) with storage caps, grounded in Travian/RoK formulas.
- Structure upgrades with painterly building portraits, an interactive pannable/zoomable home town with tappable building markers. The town itself visibly grows from a modest hold to a grand capital as the Keep rises.
- Acceleration/speedups: finish a timer instantly with shards on a Clash-style gem-to-time curve, or free under five minutes (from the queue or the building modal).

The war layer
- A big world map of barbarian camps, rival cities, fallen-giant ruins, and alliance strongholds, with marches drawn moving along their paths in real time.
- A deterministic mixed-arms combat resolver (no RNG) with a cinematic battle scene: a painterly clash backdrop, a stamped victory/defeat seal, casualties, spoils, and a dramatic war-music cue. Both the attacker's and the defender's champion bonuses fold into the math, so investing in defense genuinely matters.
- Named Warlord camps: rare elite barbarian lords (each with a baked taunt) holding a tougher garrison behind fortified walls, always dropping shards and a guaranteed relic to whoever fells them.
- Player-vs-player: scout a rival (countered by their watchtower; the scout also reads their champion's defense), see incoming attacks coming with a live warning, march on their hold, and carry off their stores. A beginner's peace shields new holds.
- The Infirmary: a share of every casualty is recoverable, tended back into the host for resources.
- Delving: search a fallen giant's ruin for a buried cache, a hoard of shards, or a lost relic.

Alliances ("banners")
- Create/join/leave/browse, a roster, the signature timer-shaving help, a +1%/member production bonus, and a War Table chat.
- Member ranks: a leader and officers with a real permission ladder (promote, demote, expel, and pass the mantle), the leadership passing to an officer first when a leader departs.
- Reinforcement: garrison a banded member's hold and your troops join every defense; recall the survivors.
- Joint rallies: muster a combined host with your banner against a Warlord; everyone's troops fight as one, survivors and spoils split by contribution, the relic to the lord who called it.
- The Banner Stronghold, a shared territory fortress on the map: the leader founds it, every member pledges resources to raise it (for an alliance-wide march-speed buff) and garrisons real troops to defend it. Rival banners can assault it, battering it down a level per victory and razing it at the last, with a rebuild shield between sieges.

Retention and progression
- A 7-day login calendar, a daily task ladder with reward chests, a free timed chest, permanent tiered achievements, a VIP track, and a 30-day season pass with free and gold tracks.
- Equipment and a hero champion: a deterministic Forge gacha with a transparent pity counter; relics buff combat, loot, and march speed. The Forge has real depth: salvage junk for shards, reforge a relic's roll within its tier, fuse three same-tier relics to ascend one tier, a Panoply set bonus that grows as the four slots fill with finer relics, and rankable Champion's Traits earned every five hero levels (a build-defining choice between attack, defense, march, spoils, march slots, and build or train speed).
- A multi-category Realm Ladder (Lords, Warlords, Banners) with portraits and your own standing.
- A returning-player Council recap (what happened while away, what awaits) and a coached first-session tutorial.

Presentation
- A monetization-style shop. Purchases are SIMULATED: "buying" simply grants shards, with no payment of any kind.
- A splash/title screen over baked key-art, a composed orchestral main theme and a battle cue, synthesized SFX (muted until the first click), and a Settings panel with separate music/effects volume and a reduce-motion accessibility toggle.
- A voice for the realm: barbarian taunts, battle narration, a steward's counsel, and a lore codex, all baked offline.

## How to play

- Tap a building (in the town or the bottom bar) to raise it. The Keep unlocks higher levels and speeds all construction.
- Open the rails on the left for the Daily gift, the Shop, the Army (train soldiers and tend the wounded), the Forge (heroes and relics), Tasks, Honors, Banners (alliances), and the World map.
- On the World map, tap a barbarian camp to raid it, a crimson Warlord camp to fell (alone or by calling a banner rally), a fallen-giant ruin to delve, or a red rival hold to scout or march on it.
- In the Banners panel, found and raise your alliance stronghold, pledge resources and garrison troops to it, and assault a rival banner's stronghold on the map.
- Watch the top of the screen for an incoming-attack warning, and your banner's roster to send reinforcements.

## Architecture

```
giantsreach/
  server/server.js   the authoritative game server (http, crypto, fs only)
  web/               the static client (index.html, game.js, audio.js, style.css)
  web/img/           baked painterly art (splash, city tiers, building portraits, lord portraits, battle)
  web/audio/         the baked composed theme + battle cue (mp3)
  db/db.json         JSON persistence (created on first run)
  launch.sh          start script
  GAME.md            full design doc, locked art direction, and build log
```

- Server-authoritative: the client never computes outcomes; it sends intents and renders the snapshot the server returns from `/api/state`. The client only interpolates resource counters smoothly between syncs.
- Deterministic and lazy: all time-based state (builds, training, marches, resources, seasons) is resolved from timestamps on read, so it survives restarts and offline gaps with no background tick loop. A re-entrancy guard keeps mutual scouts/attacks from recursing.
- No runtime AI: every "AI" element (art, music, flavor text) is baked offline into static assets and served by seed. The server never calls an external service.

## Hardening

- Atomic, durable saves: the database is written to a temp file, the previous file is backed up to `db.json.bak`, then renamed into place, so a crash mid-write cannot corrupt it. On boot the server falls back to the backup if the main file is unreadable.
- Graceful shutdown: SIGINT/SIGTERM and uncaught exceptions flush the pending save before exit.
- A per-IP sliding-window rate limit on the API, request-size and URL-length caps, method allow-listing, and static-path-traversal protection.
- Input validation on every route: bounded numerics, name/tag patterns, a JSON body cap, and sanitized troop selections (only known units, non-negative integers) so no march can fabricate soldiers.

## Assets

- All art was created offline in the locked painterly style and ships as static images under `web/img/`: the splash
  key-art, the lord portraits (which also serve as your champion's likeness), the battle backdrop, the banner-
  stronghold portrait, and the city growth tiers. Every building has a base portrait plus a grander tier-2 version
  that keeps its composition as it visibly grows on upgrade, and the Keep rises further to a majestic tier-3 citadel.
- The music (a warm orchestral main theme and a driving battle cue) was rendered offline and ships as small mp3s
  under `web/audio/`. SFX are procedural Web Audio, created on the first user gesture and muted until then.
- None of this runs at game time; the runtime only serves the finished static assets and never calls any model.

## Notes

- One server / one realm.
- Purchases are simulated. No real money is ever involved.
- No em-dashes anywhere in the code or docs, by project convention.

See `GAME.md` for the full design, the locked art direction, the grounded systems spec, and the iteration-by-iteration build log.
