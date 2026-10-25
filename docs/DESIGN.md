# BIVOUAC — Design Document

Status: living draft. Last updated 2026-10-24.

Each item is marked **[decided]** (the designer has agreed to it), **[proposed]** (suggested, still needs sign-off), or **[open]** (not discussed yet). This document replaces the README where they disagree. In particular the README says the game uses ratatui; it now targets its own graphical window.

---

## 1. Pitch

A real-time strategy game drawn in ASCII art. Armies march along a long side-scrolling front. Each player's main fort sits at one end, and captured or neutral forts lie between them. You win by pushing the enemy back until they give up or you take their main fort.

The fun is in **where you put your armies and how you look after them**: when to march and when to rest, road or open country, intercepting enemy armies, keeping troops fed, and slipping small hidden groups through forests to raid supply lines. Weather and the sky (moon phase, clouds, rain, fog, omens) change what's possible from hour to hour.

## 2. Design pillars

1. **Position over clicking.** You give orders to armies (march, rest, hold, divide, join), not to individual soldiers. Reflexes shouldn't decide games.
2. **Logistics is tactics.** Stamina and food matter as much as numbers. A starving army is a beaten army.
3. **Stealth is possible but expensive.** Hiding takes forethought, small numbers and the right conditions, and it leaves traces.
4. **The sky is part of the game.** Weather and the heavens are both atmosphere and rules.
5. **ASCII, but beautiful.** Text art with real-window effects: glow, animation, tinting.

## 3. Platform and technology

| Topic | Decision |
|---|---|
| Language | Rust **[decided]** |
| Display | Its own graphical window with visual effects (shaders, glow, animation) **[decided]**. A terminal version is optional and later. |
| Renderer | **macroquad** with our own glyph grid and font atlas **[proposed]**. It's lightweight, supports shaders and compiles fast. Avoid Bevy because its build folders get huge. A ratatui terminal version could share the simulation. |
| Font | Ship our own font so we control every glyph: braille, box-drawing, custom road pieces, terrain **[proposed]** |
| Architecture | Workspace with a `sim` crate (pure, no rendering, deterministic), a `client` crate (rendering and input), and later `ai` and `net` crates **[proposed]** |
| Determinism | Fixed-tick simulation, seeded random numbers, no floats in game logic (or carefully controlled ones). This gives lockstep netplay, replays and AI testing **[proposed]** |
| Build hygiene | Build output is cleaned up automatically so the disk doesn't fill **[decided]**. Plan: in `.cargo/config.toml` lower debug info and use a single target folder. A `justfile` runs `cargo sweep`, or removes the target folder once it passes a size cap, after builds. Keep dependencies few **[proposed]** |

## 4. Screen layout

The rough proportions in the README were just an indication. The layout aims for the best gameplay and looks **[decided]**.

From top to bottom:

1. **Status bar** (1 row): day and time, moon, weather, wind, game speed, pause.
2. **Sky** (about 10–12 rows, collapsible to 1–2 rows) **[decided]**: big text art and animation. Example: a huge blood moon hanging over the map like a demon. When collapsed, a one-row weather strip stays (falling rain, fog bands).
3. **Map** (about 12–16 rows): side view in three-quarter perspective, far terrain in the top rows and near terrain at the bottom. Scrolls left and right **[decided]**.
4. **Minimap / strip of the whole front** (1–2 rows): the whole map width, forts, known armies, and a box showing the current view **[proposed]**.
5. **Information panel** (about 8–10 rows): hover details, the selected army, the event log **[decided: bottom area is information]**.

**Hovering** over a map cell lists the troops in it in the bottom panel **[decided]**.

### Mockup 1 — dusk, sky expanded, blood moon (curved roads)

```
 BIVOUAC · Day 14, dusk · ● blood moon (full) · clear · wind W, light            ▶▶ ×2   ␣ pause
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━ sky [▾] ━━
        .               *                    ▄▄▄██████▄▄▄                   .               *
   *            .                        ▄▄██▓▓▓▓████▓▓▓▓██▄▄                         .
              .                 *      ▄██▓▓░░░▓▓▓██▓▓▓░░▓▓▓██▄          *
      .                               ██▓▓▓░░▓▓▓▓██▓▓▓▓░░░▓▓▓███                      .        *
                   *         .       ▐██▓▓▓▓▓▓██▓▓▓░░▓▓▓▓▓██▓▓██▌        .
          .                          ▐██▓▓░░▓▓▓▓▓▓▓░░░▓▓▓██▓▓▓██▌                  *
   *                    .             ██▓▓▓▓░░▓▓██▓▓▓▓▓▓▓▓▓▓▓▓██           .                  .
                .                      ▀██▓▓▓▓▓▓▓░░▓▓▓▓██▓▓▓▓██▀
   ░░░░░░░             .                 ▀▀██▓▓▓▓████▓▓▓▓██▀▀              ░░░░░░░░░░
 ░░░░░░░░░░░░░░                              ▀▀▀██████▀▀▀             ░░░░░░░░░░░░░░░░░░      .
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  ▲▲▲   ▲▲   ▲▲▲▲▲      ▲▲▲▲        ▲▲▲▲▲▲▲▲▲       ▲▲▲     ▲▲▲▲▲▲      ▲▲▲▲       ▲▲▲▲▲▲   ▲▲
 ▲▲▲▲▲▲▲▲▲▲▲▲▲▲▲▲▲▲  ▲▲▲▲▲▲▲▲  ▲▲▲▲▲▲▲▲▲▲▲▲▲▲   ▲▲▲▲▲▲▲  ▲▲▲▲▲▲▲▲▲▲  ▲▲▲▲▲▲▲▲   ▲▲▲▲▲▲▲▲▲▲▲▲▲▲
 ♣♣♣♣♣ · · · · · · · · ♣♣♣♣♣♣ · · · · · · · · · · · ♣♣♣♣♣♣♣♣ · · · · · · · · · · · · ♣♣♣♣♣♣♣♣
  ♣♣♣ · · · ___────‾‾‾‾‾‾‾‾‾‾───___ · · · · · · ♣♣♣♣♣♣♣♣♣♣♣ · · · · · ___───‾‾‾‾‾‾───___ · · ♣
   · · · ·╱ · · · · · ♣♣♣♣ · · · · · ‾‾╲___ · · ♣♣♣♣♣⡇♣♣♣♣♣ · · · · ╱ · · · · · · · · ‾╲ · ·
  · · · ·│ · · · · ♣♣♣♣♣♣♣ · · · · · · · · ‾‾──___ · ♣♣♣♣♣♣♣ · · · ╱ · · · ░░░ · · · · · │ · ·
 ▛▀▀▀▜ ·╱ · · · · · · ♣♣♣ · · · · · · · · · · · · ‾‾──___ · · ▛▀▀▜╯ · · · ░░░░ · · · · · ╲ ▛▀▀▀
 ▌▓▓▓▐─╯───────────────────╮ · · · · · · · · · · · · · · ‾‾───▌▒▒▐────────────⣾⣿─────────╰─▌▓▓▓
 ▙▄▄▄▟ · · · ⣿⣷ · · · · · · ╲ · · · · · · · · · · · · · · · · ▙▄▄▟ · · · · · · · · · · · · ▙▄▄▄
  · · · · · · · · · · · · · · │ ≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈
  ♣♣♣♣ · · · · · · · · · · · · ╰──╪──⣦──────‾‾‾‾‾‾‾────────────___________────────── · · ♣♣♣♣
 ♣♣♣♣♣♣♣ · · · · · · · · · · · · ≈ · · · · · · · · ♣♣♣♣♣ · · · · · · · · · · · · · · ♣♣♣♣♣♣♣♣
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 ◄ [■]··▲▲··[▒]··♣♣··[·]····░····[▒]··▲··[■] ►      yours 2 · theirs 2 · neutral 2
   └─ view ─┘
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 HOVER  road · Ashford bridge        │ SELECTED  2nd Column "The Greys"     │ LOG
                                     │                                      │ 18:40 Scouts: dust east
 ⣦ 2nd Column (yours)       640 men  │ men      640                         │       of Ashford Mill
   stamina ▰▰▰▰▰▱▱▱  62%  marching   │ stamina  ▰▰▰▰▰▱▱▱  62%               │ 18:12 Blood moon rises.
   food    ▰▰▰▱▱▱▱▱  3 days          │ food     ▰▰▰▱▱▱▱▱  3d (−1/day)       │       Morale wavers.
   morale  ▰▰▰▰▱▱▱▱  uneasy (moon)   │ order    march → Ashford Mill        │ 17:55 1st Army rests at
                                     │ ETA      2h 10m via road             │       Greywall Keep
 enemy can see you: likely           │                                      │
   (open ground, bright moon)        │ [M]arch [R]est [H]old [D]ivide [J]oin│
```

What's on the map: `⣿⣷` is your 1st Army at Greywall Keep. `⣦` is your 2nd Column crossing the bridge. `⡇` is your hidden raiding party in the forest, drawn dim. `⣾⣿` is a sighted enemy army. `░░░` is a rumored enemy, seen only as dust. In the real window the moon glows and pulses and tints the map red.

### Mockup 2 — 2am, sky collapsed, new moon, heavy rain

```
 BIVOUAC · Day 15, 02:00 · ○ new moon · heavy rain · lowland fog                 ▶ ×1    ␣ pause
 ╱ ╱  ╱ ╱ ╱  ╱  ╱ ╱ ╱  ╱ ╱  ╱ ╱ ╱  ╱ ╱  ╱ ╱  ╱ ╱ ╱  ╱ ╱ ╱  ╱  ╱ ╱  ╱ ╱ ╱  ╱ ╱  ╱ ╱  ╱ ╱ sky [▸]
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  ♣♣♣♣♣♣ · · · · · · ♣♣♣♣♣♣♣♣ · · · · ╭──────────────────╮ · · · ♣♣♣♣♣♣♣♣♣♣ · · · · · · · · · ·
 ♣♣♣♣♣♣♣♣ · · · · · ♣♣♣♣⡆♣♣♣♣ ⁖ ⁖ ⁖ ⁖ │ · · · · · · · · ·│ · · · ♣♣♣♣♣♣♣♣♣♣♣ · · · · · · · · · ·
  ♣♣♣♣ · · · · · · · ♣♣♣♣♣♣ · · · · · ·│ ░░░░░░░░░░░░░░░░ │░░░░░░░░♣♣♣♣♣♣♣♣ · · · · ▛▀▀▀▜ · · ·
 · · · · · · · · · · · · · · · · · · ·│░░░░░░ ⣿? ░░░░░░░░│░░░░░░░░░░♣♣♣ · · · · · · ▌▓▓▓▐ · · ·
 ─────────────────────────────────────┴──────────────────┴──────────────────────────▙▄▄▄▟──────
```

`⡆` is your raiding party. `⁖ ⁖ ⁖` are its tracks in the mud, which you can see so you know what you're giving away. `░` is fog. `⣿?` is enemy campfire glow through the fog; the size and exact cell are uncertain.

## 5. The map

- **Shape:** wider than the screen, scrolls left and right. Each player's main fort is at one end, with neutral forts of different types and sizes between **[decided]**.
- **View:** three-quarter perspective. Top rows are far away, bottom rows are near. Branching roads run across the rows **[decided]**.
- **Cells:** a cell can hold more than one troop, and there's no fixed one-troop-per-cell rule **[decided]**.

### Terrain

| Terrain | Glyph | Movement | Stamina | Notes |
|---|---|---|---|---|
| Road | `─ ‾ _ ╱ ╲ │ ╭╮╰╯` | Fast | Low cost | **[decided]** |
| Field | `·` | Passable, slower | Costs stamina | **[decided]** |
| Forest | `♣` | Passable only by small numbers at a time | Medium | Concealment **[decided]**. The exact size cap per cell is **[open]**. |
| Mountain | `▲` | Impassable | — | Blocks line of sight **[proposed]** |
| River | `≈` | Impassable except at bridges and fords **[proposed]** | — | |
| Bridge / ford | `╪` | Chokepoint | — | **[proposed]** |
| Fort | `▛▀▜ ▌▓▐ ▙▄▟` | — | Rest and resupply | See §9 |

Rain slows movement on every terrain **[decided]**. Roads turn to mud more slowly than fields **[proposed]**.

### Roads (curves) **[proposed]**

- Each road is stored as a **spline** (Catmull-Rom through control points). It's converted into a chain of cells for the simulation (movement graph, costs), so the simulation never sees curves.
- The renderer picks each cell's glyph from the spline's angle at that cell. Shallow slopes use the half-step trick: `‾` sits at the top of a cell, `─` in the middle and `_` at the bottom. Steep stretches use `╱ ╲ │`, and bends use `╭╮╰╯`.
- Because we ship our own font, we add custom road glyphs at about 8 heights per cell and many angles, so roads look smooth while the game stays on the grid. A terminal version falls back to the plain set.
- Road types are told apart by line thickness: main roads thick, cart tracks thin or dotted. This replaces the double-line `═`, because box-drawing has no rounded double-line corners.

```
 ‾‾‾‾───────_____
                 ‾‾‾‾───────_____
                                 ‾‾‾───___
                                          ‾╲
                                            ╲
                                             ╰───────────
```

## 6. Troops and armies

- **Color shows side** **[decided]**.
- **Density is shown in braille** **[decided]**: one dot is roughly one company of about 50 men. A cell holding more than 8 dots spills into the next cell, so big armies cover several cells like a real column **[proposed details]**.
- **Soldier pools:** players can **create armies from a pool of soldiers, join armies, and divide them** **[decided]**. Pools exist at forts (garrison and recruits) **[proposed]**.
- **Army stats:** men, stamina, food, morale **[proposed; morale is new]**. Morale is affected by hunger, exhaustion, losses and omens like the blood moon.
- **Orders:** march (to a target, with a route preference: road, direct, or concealed), rest, hold, divide, join, garrison, resupply **[proposed]**.

### Mockup 3 — troop glyphs

```
 DENSITY (one braille dot ≈ one company, ~50 men; more than 8 spills into the next cell)
   ⠁  ⠃  ⠇  ⡇  ⣇  ⣧  ⣷  ⣿        ⣿⣿ ⣿⣿⣿ = big army spread over several cells

 STATE (color = side; brightness and animation = state)
   ⣷   marching     glyph shimmers or ripples in the direction it's marching
   ⣷˄  resting      campfire flicker at night, smoke wisps by day
   ⣷   exhausted    dim, slow pulse
   ⡇   hidden       drawn faint with a dotted halo (only its owner sees it)
   ⣾   enemy sighted    solid enemy color
   ░?  enemy rumored    dust or glow, drifts a little; position is approximate
   ⁖   tracks           fade over a few hours; last longer in mud
```

## 7. Time

- Real-time **[decided]**, pausable **[decided]**, with faster speeds **[decided]**. Proposed speeds: ×1, ×2, ×4 and maybe ×8.
- A full day and night cycle. The time scale (real seconds per game hour) is **[open]**.
- Opening fort or army screens does **not** pause the game; pausing is always one key away **[proposed]**. In netplay, pausing needs the other player to agree, or each player gets a limited number of pauses **[open]**.

## 8. Stamina and movement

- Marching uses stamina. Roads cost little, fields cost more, rain adds to the cost **[decided in principle]**.
- Resting restores stamina. Resting in a fort is faster **[proposed]**.
- Low stamina means slower marching, worse fighting, and a chance of men straggling (deserting or lagging behind) **[proposed]**.
- Forced march: an option to move faster at a heavy stamina cost **[proposed]**.

## 9. Supply (food only for now) **[decided: food only]**

- Every army eats food every tick. Marching eats more than resting **[proposed]**.
- Armies carry a limited number of days of food. They refill at depots or from supply wagons **[proposed]**.
- **Supply wagons** travel the roads from depots to forts or armies on routes the player sets. They can be **intercepted**, which is the main target for small hidden raiding groups **[proposed]**.
- Foraging: armies in fields can scrape up a little food, which drains the land **[open]**.

## 10. Forts

- Each player has a **main fort**. Taking the enemy's main fort wins the game **[decided]**.
- **Neutral forts can be captured** **[decided]**. They act as **supply depots and/or garrisons** **[decided]**, and they come with different **extras** ("accoutrements", e.g. dairy cows) **[decided in concept]**.
- How capture works: a fort is captured when its garrison is gone and an army holds it for N hours. Neutral forts start with a small militia garrison **[proposed]**.

### Fort extras **[proposed]**

| Extra | Effect |
|---|---|
| Water mill | +grain per day |
| Dairy herd | +cheese per day. Fresh milk can't travel; cheese can. |
| Granary | More storage |
| Smokehouse | Turns meat into salt pork, which keeps longer and travels well |
| Stables | Faster wagons and scouts |
| Watchtower | Longer sight range from the fort |
| Palisade / walls | Defense level |
| Village | Recruit pool grows each day while you hold it |

Players can also **burn the stores** when giving up a depot (scorched earth) **[proposed]**.

### Mockup 4 — depot screen

See §15.

## 11. Sky, weather and the heavens

The sky is drawn as big animated text art **[decided]**, and it affects gameplay:

| Condition | Effect |
|---|---|
| Moon phase | Night visibility: a new moon is darkest, so stealth is best; a full moon is brightest **[decided]** |
| Cloud cover | Hides moon and starlight, so it's darker and helps stealth **[decided]** |
| Rain | Slows **all** troops **[decided]**. Makes hidden units **easier to track** because of mud tracks **[decided]**. Keeps down the dust from big armies, so they're harder to see from afar by day **[proposed]** |
| Fog | Cuts sight range sharply, mostly in lowlands and near rivers **[decided in concept]** |
| Blood moon | An omen: hurts morale (maybe more for one side?). Mostly for looks at first **[proposed]** |
| Comet, eclipse, etc. | Later: morale and event mechanics **[decided: later]** |

## 12. Visibility, detection and stealth

Detection uses **partly dice rolls** **[decided]**. Main armies should be visible from relatively far away so the cat-and-mouse positioning works **[decided]**.

### Detection levels **[proposed]**

| Level | What the observer sees | How |
|---|---|---|
| Unseen | Nothing | — |
| Rumored | Dust by day, campfire glow by night. Position off by ±N cells, size as a bucket ("large?") | Large armies are nearly always rumored at long range |
| Sighted | Exact cell, size within about ±25% | Line of sight plus a roll |
| Identified | Exact numbers, stamina, food | Up close or by scouts |

### Signature vs. sight **[proposed]**

- **Signature** (how noticeable a group is) rises with its size and activity (marching > resting, campfires at night) and is changed by terrain (forest hides) and weather (rain cuts dust, fog cuts everything).
- **Sight** (how far an observer sees) depends on unit type (scouts see best), height (forts, watchtowers, hills) and light (daylight > full moon > clouded > new moon).
- Every detection tick, each observer rolls against each group in range, using signature × sight × distance falloff.
- **Tracks:** groups leave tracks that fade over hours and last longer in mud and rain. Trackers can follow them. Owners can see their own tracks.
- **Deception:** leave campfires burning after you march off, split off quietly, or use small groups as decoys.

## 13. Combat **[open]**

Not discussed yet. Questions to settle:
- Is combat automatic when armies meet, or something you order?
- Does a fight last over time (several ticks) so either side can reinforce or retreat?
- How do stamina, morale, terrain, fort walls and ambushes (attacking from hiding) change the result?
- What happens to the loser: rout, retreat, prisoners?

## 14. Victory

- Take the enemy's main fort **[decided]**, or
- Push the enemy until they give up **[decided]**. Proposal: surrender happens when an overall morale or "war will" score drops below a threshold, driven by lost forts, losses and starvation **[proposed]**.

## 15. Fort / depot screen

Opening a fort turns the bottom panel into a larger fort panel. The sky collapses and a compact map strip stays visible so you keep an eye on the front. The game keeps running.

### Mockup 4 — Ashford Mill depot

```
 BIVOUAC · Day 14, 19:10 · ● blood moon · clear                                   ▶▶ ×2   ␣ pause
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  · · · ‾‾──___ · · ▛▀▀▜╯ · · · ░░░░ · · · · · ╲ ▛▀▀▀
  · · · · · · ‾‾───▌▒▒▐────────────⣾⣿─────────╰─▌▓▓▓
  · · · · · · · · ·▙▄▄▟ · · · · · · · · · · · · ▙▄▄▄       ◄ [■]··[▒]··[·]··░··[▒]··[■] ►
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
 ASHFORD MILL · supply depot · yours since day 9 · walls ▰▰▱▱ palisade                    [Esc] close
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
                                    │ STORES                       │ PER DAY
          ╱╲       ,                │ grain     ▰▰▰▰▰▰▱▱   1,240    │ +180  water mill
    ▐█▀▀▀█▀▀╲   ,     ~             │ cheese    ▰▰▱▱▱▱▱▱     210    │  +30  dairy herd (12 cows)
    ▐█ ▓▓ █  ▌▄▄▄     ~             │ salt pork ▰▱▱▱▱▱▱▱      60    │    —  (no smokehouse)
    ▐█ ▓▓ █  ▌ ░ ▐ ◎  mill wheel    │ ─────────────────────────────│ ─────────────────────────
  ▄▄█████████▄▄▄▄▄█▄▄▄▄▄▄           │ capacity  1,510 / 2,000       │  −95  garrison (190 men)
  ≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈≈          │ lasts     ~16 days @ garrison │ +115  net
   ᴥ ᴥ  ᴥ    ᴥ ᴥ ᴥ   ᴥ  ᴥ ᴥ         │                               │
────────────────────────────────────┴───────────────────────────────┴─────────────────────────────
 TROOPS
   garrison      ⣧  190 men   stamina ▰▰▰▰▰▰▰▱ resting   morale ▰▰▰▰▰▱▱▱ uneasy (moon)
   present       ⣦  2nd Column "The Greys"  640 men   food 3d                  [R] resupply
   recruit pool     85 militia (raw)   +4/day while held

 SUPPLY ROUTES
   ▸ → Greywall Keep        north road   2 wagons out   next arrives 23:00              risk ▰▱▱ low
   ▸ → 2nd Column           follows army 1 wagon, 20 escort                            risk ▰▰▱ dust seen E
   + new route

 EXTRAS   water mill ✓   dairy herd ✓   granary ✓   palisade ✓   smokehouse ✗ [build]   watchtower ✗ [build]
────────────────────────────────────────────────────────────────────────────────────────────────────
 [S] send wagon  [R] resupply army  [G] set garrison  [F] form army  [U] build/upgrade  [B] burn stores
```

The small picture of the fort is animated in the real window: the mill wheel turns, cows wander, smoke rises from chimneys. Its look changes with ownership (banners in your color), damage and weather.

## 16. Opponent: AI **[decided: required]**

- AI that plays through the same order interface as a human, seeing only what its armies can detect (no cheating at normal difficulty) **[proposed]**.
- Probably layered: front strategy (which forts to attack or defend), army tasks (march, intercept, raid, resupply), and supply management **[proposed]**.
- Testing: AI-vs-AI games run headless on the deterministic simulation **[proposed]**.

## 17. Netplay **[decided: plan for it]**

- **Lockstep**: both clients run the same deterministic simulation and only exchange orders **[proposed]**.
- Fog of war in lockstep: each client runs the full simulation, so a cheater could read hidden state. That's acceptable for friendly play at first. A server-authoritative mode could come later **[proposed / open]**.
- Desync detection: compare hashes of the simulation state every N ticks **[proposed]**.

## 18. Input and controls **[proposed]**

- **Mouse:** hover shows cell details (the troops in it) **[decided]**. Click to select. Right-click to order a march. Drag to scroll the map, or push the cursor to the screen edge.
- **Keys:** `Space` pause; `1–4` speed; `M` march, `R` rest, `H` hold, `D` divide, `J` join; `Tab` cycle armies; `S` collapse or expand the sky; `Esc` close panel.

## 19. Open questions (summary)

1. Combat model (§13).
2. Time scale: real seconds per game hour, length of a typical game.
3. Forest capacity: how many men per cell can move through.
4. Pausing in netplay.
5. Foraging.
6. Map generation: hand-made maps, generated maps, or both?
7. Does morale exist as its own stat? (I proposed it; check.)
8. Scouts as a separate unit type, or just small detachments?

## 20. Implementation status (v0.1, 2026-10-24)

Built:
- A workspace with `sim`, `ai` and `client` crates. The macroquad renderer draws its own glyph grid: braille, block and shade characters are drawn as shapes so they tile seamlessly, and everything else comes from the bundled DejaVu Sans Mono.
- The sky: day/night gradient, sun, moon with real phases and rise times, blood moon with glow and a red wash over the land, stars, drifting clouds. It collapses to a 2-row strip, and the cells grow to fill the freed space.
- The map: generated valley (ridge, spurs, forests, winding river), 6 forts, 4 spline roads drawn as quantized curves, bridges.
- Armies: braille density, columns stretching back along their trail, selection, path preview, march/rest/hold/divide/join, stamina, food, resupply in friendly forts, starvation.
- Forts: stores, daily production from extras, garrison eating, capture by holding for 60 minutes.
- Weather: clear/overcast/rain/fog as a Markov chain; rain slows movement and costs extra stamina; fog and rain overlays.
- The HUD: status bar, minimap, hover / selected / log panel.
- A deterministic sim with a checksum, plus tests for determinism, road connectivity, bridges, the forest cap, capture, and divide/join.

Placeholders, until the open questions are settled:
- **No combat (§13).** Enemy armies can't enter each other's cell, so you can block a road but not fight for it. Garrisons don't resist capture.
- **No fog of war or detection (§12).** Everything is visible.
- **AI** marches idle armies at the nearest fort it doesn't hold, and rests when tired.
- **Time scale:** 1 game hour = 10 s at ×1 (`TICKS_PER_SECOND` in the client).
- **Forest cap** is 150 men (`path::FOREST_CAP`).
- The full depot screen (Mockup 4) is not built yet. Selecting a fort shows a compact version in the panel.
