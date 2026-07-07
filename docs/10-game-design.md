# 10 — Game Design (v1)

> **Status:** Living design · **Governed by:** [`30-ai-agent-contract.md`](./30-ai-agent-contract.md) · **Load this doc for:** any gameplay/mechanics or balance task (with [`40-parameterisation.md`](./40-parameterisation.md)).

This is a **concrete, coherent v1 design** for our *inspired-by* god-game — our own mechanics in the Populous II spirit, not a clone. It is a **living document**: mechanics are refined via ADRs, and **every quantity below names a configuration key** (per I1) rather than a fixed number. The keys use the namespaces defined in [`40-parameterisation.md`](./40-parameterisation.md); values live in config, not here.

> Convention: `key.path` in `code font` denotes a config key. This doc never states the value — only that the value is configurable and what it governs.

---

## 1. Fantasy & goal

You are a young deity contesting a world with a rival deity (the LLM opponent). You cannot command mortals directly; you influence them by **shaping the land** and **spending faith** as divine power. You win by making your following dominant and the rival's untenable.

---

## 2. The world

- The world is an **integer height field sampled at grid vertices** (corners) — height lives on the *vertex*, and the land you see is the surface spanning them. A **face** (the square between four vertices) is what followers build on. Terrain *type* (water / shore / land / mountain) is derived from height and sea level; the thresholds that name shore and mountain are the type catalogue `content.terrain.*` (`shore.band`, `mountain.min_height`). See [ADR 0017](./decisions/0017-vertex-heightfield-terrain.md).
- **Step invariant:** orthogonally-adjacent vertices differ in height by at most `sim.terrain.max_step` (default 1) — so terrain is *stepped*, never a sheer cliff, and heights are integers (a determinism aid, I3). Diagonal neighbours may differ by up to twice that.
- **Worldgen** ([ADR 0021](./decisions/0021-seeded-parameterised-worldgen.md)) is a **pure, seeded function** that builds the height field: `sim.worldgen.*` carries the map `width`/`height`, `seed`, `sea_level`, `land_percent`, a **`shape`** (`island` / `continent` / `archipelago` / `inland`), and the **relief** controls (`relief`/`feature_size`/`detail`). It is *never one baked-in world* — the knobs span a shape × relief space, the seed varies the instance within it. The out-of-box world is an **island ringed by sea with mixed relief**. The field worldgen hands back already satisfies the step invariant. *Starting settlement placement* is design intent under `sim.worldgen.*` but stays parked (it sits above terrain).
- The world is generated from a **seed** (I3); the same seed + knobs reproduces the same world, forever.

## 3. Core verb — land shaping

- The player **raises and lowers** terrain **vertices**. A raise/lower **cascades** to neighbouring vertices as far as needed to keep the step invariant (§2), forming stepped plateaus; faith cost scales with the number of vertices actually moved. Flat, dry, contiguous faces are buildable; steep or flooded ground is not.
- The cascade is **naturally bounded** by `sim.terrain.max_height` (a cone cannot rise past the world maximum). **Immovable** features — rock, trees, and cross-subsystem entities such as opponent buildings — can halt a cascade or forbid an op: terrain shaping must not silently destroy what another subsystem owns ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md); model in [ADR 0017](./decisions/0017-vertex-heightfield-terrain.md)).
- Each shaping op consumes faith and is bounded by cost/step/height rules in `sim.terrain.*` (e.g. `sim.terrain.raise.mana_cost`, `sim.terrain.max_step`, `sim.terrain.max_height`).
- Shaping is the root of the loop: **flatten land → followers build & multiply → more faith → more power to shape and intervene.**

## 4. Followers, settlements & population

- **Followers** live in **settlements** built on buildable land. Housing capacity, growth, and migration are governed by `sim.population.*` (e.g. `sim.population.follower.growth_per_tick`, housing capacity keys) and follower-type definitions in `content.followers.*`.
- Population is the engine of the economy: more (and happier, safer) followers → more faith.
- Followers can shift allegiance under pressure/incentive; conversion rules in `sim.population.*`.

## 5. Economy — faith / mana

- Followers generate **faith** (the mana resource) each tick per `sim.economy.*` (e.g. `sim.economy.mana.regen_rate`, worship yield per follower, storage cap `sim.economy.mana.storage_cap`).
- Faith is the single currency for **all** divine action: land shaping (§3) and powers (§6).
- The economy is intentionally a tight loop so both sides face the same expand-vs-spend tension.
- Mana generation has a `sim.economy.mana.mode` (`normal` | `fast` | `unlimited`) — a first-class god-mode for isolated exploration, not a hack ([ADR 0016](./decisions/0016-exploration-lane-and-subsystem-isolation.md); see [`40-parameterisation.md`](./40-parameterisation.md) §7). Each deity reads its own budget, so raising it never leaks into the rival's economy.

## 6. Divine powers

- Powers are **data-defined content** in `content.powers.*` — a keyed catalogue. Each power record carries fields such as `mana_cost`, magnitude, radius, cooldown, and prerequisites.
- Expected v1 archetypes (final set is content, tunable/extensible without code): terrain-scale effects (e.g. flood, earthquake, raise-mountain), population effects (e.g. inspire growth, blight), and dramatic late-game effects. New powers are added by adding content records — **no code change** (I1).
- Terrain-scale powers mutate the height field through the **same rules as manual shaping**: they preserve the step invariant and respect immovable features ([ADR 0017](./decisions/0017-vertex-heightfield-terrain.md)). They differ in shape, scale, and cost (content), not in the terrain contract.
- Example key: `content.powers.flood.mana_cost` governs the flood power's price; balancing powers is a config task.

## 7. The rival deity

- A second god, driven by the **LLM strategic advisor** ([`50-llm-opponent.md`](./50-llm-opponent.md)), plays by the same rules and economy against you.
- Its intelligence, aggression, cadence, and handicaps are configured under `ai.*` (difficulty, strategy vocabulary, decision cadence). It proposes *intent*; the deterministic engine executes legal actions.
- The whole subsystem is toggleable via `sim.opponent.enabled`: `false` ⇒ no rival casts against the player, and the rest of the game is unaffected (the isolation seam, [`40-parameterisation.md`](./40-parameterisation.md) §7).

## 8. Turn / tick loop

- Time advances in **ticks**; tick length and per-tick action limits are in `sim.rules.*`.
- Each tick (see [`20-architecture.md`](./20-architecture.md) §4): apply human commands and resolved rival commands, run economy/population/terrain updates, evaluate win/loss.

## 9. Win & loss

- Victory/defeat conditions and their thresholds are in `sim.winconditions.*` — e.g. eliminating the rival's followers, or crossing a dominance threshold (share of world population/territory) sustained for a configured duration.
- Multiple win conditions can be toggled per scenario (`content.scenarios.*`), letting different maps play differently as pure content.
- The evaluation subsystem as a whole is toggleable via `sim.winloss.enabled`: `false` ⇒ no win/loss checks during free play (the isolation seam, [`40-parameterisation.md`](./40-parameterisation.md) §7).

---

## 10. What is fixed vs. tunable

- **Fixed (code):** the *existence* of terrain-shaping, an economy, powers-as-catalogue, a tick loop, and a rival deity; the algorithms that run them — including the **vertex height field and the step-invariant cascade** ([ADR 0017](./decisions/0017-vertex-heightfield-terrain.md)).
- **Tunable (config):** every rate, cost, cap, threshold, radius, cadence (incl. `sim.terrain.max_step` and `sim.terrain.max_height`), the entire power catalogue, terrain and follower types, scenarios, and opponent behaviour.

If a design change alters an *algorithm or structure* (not just numbers), it is architectural → ADR (contract §5). If it only changes numbers/content, it is a balance task → config + schema + test, no core edits.

---

## 11. Open questions (to resolve via ADR as v1 matures)

- Exact set of v1 powers and their interactions.
- Allegiance/conversion model detail.
- Precise dominance win metric and duration.
- Camera/interaction model for shaping (belongs partly to `render.*`/`input.*` and the environment discussion).

These are intentionally open; this doc will be refined, not frozen.
