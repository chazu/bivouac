# BIVOUAC — Design Document

Status: living draft. Last updated 2026-10-24.

Each item is marked **[decided]** (the designer has agreed to it), **[proposed]** (suggested, still needs sign-off), or **[open]** (not discussed yet). This document replaces the README where they disagree. In particular the README says the game uses ratatui; it now targets its own graphical window.

---

## 1. Pitch

A real-time strategy game drawn in ASCII art. Armies march along a long side-scrolling front. Each player's main fort sits at one end, and captured or neutral forts lie between them. You win by pushing the enemy back until they give up or you take their main fort.

The fun is in **where you put your armies and how you look after them**: when to march and when to rest, road or open country, intercepting enemy armies, keeping troops fed, and slipping small hidden groups through forests to raid supply lines. Weather and the sky (moon phase, clouds, rain, fog, omens) change what's possible from hour to hour.

### Setting **[decided: fantasy, dying earth]**

Fantasy in a dying-earth vein: an old, exhausted world under a swollen sun and strange skies. The leaning is a **faux-Chinese dying earth** **[proposed, first pass built]**: walled garrison towns, mills and granaries along imperial roads, banners, omens read in the heavens.

First naming and look pass (v0.3):
- **Factions:** *the Azure Remnant* (the player, west, azure) against *the Vermilion Host* (east, vermilion).
- **Forts:**
  - Lantern Gate (the Remnant's capital)
  - Willow Ford Mill
  - Nine Wells
  - Crow Pagoda
  - Salt Terrace
  - Cinnabar Citadel (the Host's capital)
- **Roads:** the Imperial Road, the Pilgrim Road, the Ridge Road, the Ferry Road.
- **Armies:**
  - The Remnant: First Banner, Second Banner, the Grey Sparrows (scouts). New armies are numbered Banners.
  - The Host: the Cinnabar Host, the Ash Wolves (raiders). New armies are numbered Cohorts.
- **Look:**
  - A swollen, spotted, amber dying sun.
  - Dusty slate and amber daytime skies, never blue.
  - Warm daylight on the land.
  - Pagoda roofs on forts (two tiers on capitals), with tall hanging banners in the owner's color.

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
 BIVOUAC · Day 1, dusk · ● blood moon (full) · clear · wind W, light            ▶▶ ×2   ␣ pause
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
 BIVOUAC · Day 5, 02:00 · ○ new moon · heavy rain · lowland fog                 ▶ ×1    ␣ pause
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
- **One unit type** for now **[decided]**. Mixed types (cavalry, skirmishers) may come later.
- **Army stats:** men, stamina, food, morale **[decided]**. Morale breaking is how battles end (§13).
- **Formation:** an army on the move is a **column** strung along its route, 3 companies to a cell (1800 men make 12 cells of road). On a road it is drawn riding the road line. A halted army stays in column unless there's a reason to deploy. After 20 minutes still it deploys into a **line** across the valley's rows, center first, with extra ranks behind if the ground runs out, if a known enemy is within 10 cells, it is fighting, it is in a fort, or it was ordered to Hold. Lines block enemy movement, so one army can hold a line across the valley **[decided]**. Marching from a line, the column forms afresh at the head. Ordered back the way it came, a column turns about, so its tail leads **[built]**.
- **Steering:** clicks near a road line pick the road cell. Shift+right-click adds waypoints, so you can route a march along a chosen road **[built]**.
- **Stances** **[decided]**: *aggressive* (attacks enemies that come within reach and chases them), *defensive* (fights back when attacked and holds its ground), *evasive* (avoids contact and falls back when threatened).
- **Orders:** march, attack (right-click an enemy), rest, hold, divide, join, garrison, set stance **[decided]**. Route preferences (road, direct, concealed) and resupply orders are **[proposed]**.

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
- How capture works: a fort's garrison fights any hostile army within reach of its walls. Once the garrison breaks, an army that holds the fort alone for an hour takes it. Neutral forts start with a small militia garrison **[decided]**.
- Sieges are **minimal** for now: storm the garrison **[decided]**. Starving a fort out, siege works and the like can come later.
- Garrisons count as fully dug in. Whether **fort walls** give an extra bonus is **[open]**.

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
| Moon phase | Night visibility: a new moon is darkest, so stealth is best; a full moon is brightest **[decided]**. The lunar cycle is 8 days, one phase a night, so it visibly changes **[built]** |
| Cloud cover | Hides moon and starlight, so it's darker and helps stealth **[decided]** |
| Rain | Slows **all** troops **[decided]**. Makes hidden units **easier to track** because of mud tracks **[decided]**. Keeps down the dust from big armies, so they're harder to see from afar by day **[proposed]** |
| Fog | Cuts sight range sharply, mostly in lowlands and near rivers **[decided in concept]** |
| Blood moon | The full moon of every other cycle (every 16 days). An omen: hurts morale (maybe more for one side?). Mostly for looks at first **[proposed]** |
| Comet, eclipse, etc. | Later: morale and event mechanics **[decided: later]** |

## 12. Visibility, detection and stealth

Detection uses **partly dice rolls** **[decided]**. Main armies should be visible from relatively far away so the cat-and-mouse positioning works **[decided]**.

### Detection levels **[decided, built]**

| Level | What the observer sees | How |
|---|---|---|
| Unseen | Nothing | — |
| Rumored | Dust by day, campfire glow by night. Position off by up to ±3 cells, size rounded to 250 with ±30% error | Within the army's rumor range; no roll |
| Sighted | Exact cell, size within about ±15% | Inside an observer's sight, plus a roll |
| Identified | Exact numbers, stamina, morale, food | Within 1 cell, within 3 on a successful roll, or in battle |
| Remembered | A faded marker where the army was last seen, for up to 24 hours | Lost contact |

### How it works **[built, v0.3; numbers are first guesses]**

- **When:** detection rolls every 5 game minutes, deterministically, so the AI and netplay obey it too.
- **Sight radius** (in cells): 12 by day. At night it's 4, plus up to 6 more from moonlight (cut to a third under cloud or rain). Rain multiplies sight by 0.7; fog caps it at 3. Forts see 2 farther, and a watchtower adds 6. Routing armies see half as far.
- **Chance to sight** a group inside an observer's range:
  - Base: 30% + 4% per company, up to 90%.
  - Changes: +20% if it's marching, and +15% for campfires (camped at night). Forest multiplies the chance by 0.35, and very small groups (3 companies or fewer) by 0.6. +25% for a fresh trail through mud (marching in rain), and +40% if you were already tracking it.
  - Distance: the result is scaled from 100% right next to the observer down to 50% at the edge of its range.
- **Rumor range** (dust or fires): 8 + 1.5 × companies, ×1.3 while marching. At night it's halved unless the army is camped with fires. Fog multiplies it by 0.4, daytime rain by 0.6, forest by 0.3. A marching host of 2000 is rumored from about 88 cells away.
- **Tracks:** armies leave tracks off-road. They last 6 hours, or 18 in rain-mud. You see your own tracks faintly everywhere, and enemy tracks only within your sight.
- **Ambush:** attacking an army whose side hadn't sighted you costs it 15% morale on the spot.
- **The AI is fogged too.** It only attacks what it can see. Pathfinding only avoids enemies you can see, so hidden armies are discovered by bumping into them.
- **The log is fogged too.** Each side only sees its own movements and scout reports. Battles, fort captures and weather are public news.
- **Deception** (decoy campfires and so on) is still **[proposed]**.

## 13. Combat **[decided]**

It should feel like an RTS: fights start by themselves, play out over time, and are won mostly by position.

- **Fights start on contact** when someone wants them: an aggressive army, or one ordered to attack. Two defensive armies can stand next to each other without fighting. Once started, a fight goes on while the armies touch, until one routs.
- **Only the front fights.** Each map cell of an army that touches an enemy cell fights, and its strength is split across the enemy cells it touches. A column caught on the road fights with its head; a line fights with everything. Attacked from more than one direction (flanked), an army loses morale much faster.
- **Battles end in a rout** when morale breaks **[decided]**, usually at around 15–25% losses. Losses cost morale in proportion to the army's size. A routed army flees toward the nearest friendly fort for two hours, ignores orders, takes double losses if it's chased, then rallies.
- **Withdrawing** while engaged is allowed, at half speed and under fire.
- **Short battles** **[decided]**: an even fight breaks in about 2–4 game hours (20–40 real seconds at ×1). A clearly bigger army wins faster.
- **Modifiers**:
  - Defending in **forest** cuts losses **[decided]**.
  - Attacking from a **bridge** narrows the front **[decided]**.
  - **Entrenchment** builds while an army stands still (full after about 5½ hours) and cuts its losses by up to 40% **[decided]**.
  - Stamina scales damage.
  - Night and rain dull everyone.
  - Garrisons count as fully dug in.
  - Each blow has a dice roll (50–150%).
  - Ambush bonuses wait on detection (§12).
- Battles don't add any special food rules **[decided]**.

## 14. Victory

- Take the enemy's main fort **[decided, built]**, or
- Push the enemy until they give up **[decided]**. Proposal: surrender happens when an overall morale or "war will" score drops below a threshold, driven by lost forts, losses and starvation **[proposed]**.

## 15. Fort / depot screen

Opening a fort turns the bottom panel into a larger fort panel. The sky collapses and a compact map strip stays visible so you keep an eye on the front. The game keeps running.

### Mockup 4 — Ashford Mill depot

```
 BIVOUAC · Day 1, 19:10 · ● blood moon · clear                                   ▶▶ ×2   ␣ pause
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

1. Fort walls: an extra defensive bonus beyond the garrison being dug in?
2. Setting details: names, look and lore for the dying-earth world.
3. Time scale: real seconds per game hour, length of a typical game.
4. Forest capacity: how many men per cell can move through.
5. Pausing in netplay.
6. Foraging.
7. Map generation: hand-made maps, generated maps, or both?
8. Scouts as a separate unit type, or just small detachments?

## 20. Implementation status (v0.1, 2026-10-24)

Built:
- A workspace with `sim`, `ai` and `client` crates. The macroquad renderer draws its own glyph grid: braille, block and shade characters are drawn as shapes so they tile seamlessly, and everything else comes from the bundled DejaVu Sans Mono.
- The sky: day/night gradient, sun, moon with real phases and rise times, blood moon with glow and a red wash over the land, stars, drifting clouds. It collapses to a 2-row strip, and the cells grow to fill the freed space.
- The map: generated valley (ridge, spurs, forests, winding river), 6 forts, 4 spline roads drawn as quantized curves, bridges.
- Armies: braille density, columns stretching back along their trail, selection, path preview, march/rest/hold/divide/join, stamina, food, resupply in friendly forts, starvation.
- Forts: stores, daily production from extras, garrison eating, capture by holding for 60 minutes.
- Weather: clear/overcast/rain/fog as a Markov chain rolled every 3 hours (rain about 7% of the time, a spell every ~3 days); rain slows movement and costs extra stamina; fog and rain overlays.
- The HUD: status bar, minimap, hover / selected / log panel.
- A deterministic sim with a checksum, plus tests for determinism, road connectivity, bridges, the forest cap, capture, and divide/join.
- **Combat (v0.2)**, as in §13. The sim tests cover routs, a bigger army winning, big armies reaching a decision, garrisons having to be beaten first, deploying into line, and routed armies ignoring orders. `cargo test -p bivouac-sim battle_report -- --ignored --nocapture` prints battle-pacing numbers, and `cargo test -p bivouac-ai campaign_log -- --ignored --nocapture` prints a 4-day AI campaign.
- On screen: sparks along the lines of contact, drifting dust, earthworks in front of dug-in armies, shaking and faded routing armies, a dashed line to an attack target, morale and stance in the panel, and a victory/defeat banner.

- **Detection and fog of war (v0.3)**, as in §12. On screen, everything outside your sight is darkened. Rumored enemies show as a pulsing `░?░` smudge, and remembered ones as a fading ghost. Tracks show as dot trios, and the hover panel says what you know. The selected-army panel shows your army's *exposure* (how well the enemy knows it), and the status bar shows the current sight radius.
- **Making it playable (v0.3):**
  - An opening briefing; the game starts paused.
  - Help (F1 or `?`).
  - A random map each game (`--seed N` to repeat one).
  - A victory/defeat screen with `[N]` for a new game.
  - Mustering 400 men from a fort's garrison (`M`).
  - Daily recruits at held forts: capitals +120, villages +40, others +15.
  - Capitals produce 2500 grain a day.
  - The AI musters new cohorts too.
  - `--reveal` lifts the fog for debugging.

Placeholders, until the open questions are settled:
- **AI** keeps its armies aggressive and marches idle ones at the nearest fort it doesn't hold. It attacks defenders it can see there, and rests when tired. It has no real strategy yet.
- **Time scale:** 1 game hour = 10 s at ×1 (`TICKS_PER_SECOND` in the client).
- **Forest cap** is 150 men (`path::FOREST_CAP`).
- The full depot screen (Mockup 4) is not built yet. Selecting a fort shows a compact version in the panel.
