# BIVOUAC - an ASCII-based RTS/Strategy game based on the idea of troop movement in a linear fashion. ASCII cells contain troops (not necessarily one troop to one cell) and troop movement progresses through terrain based on...well, the terrain. The top two thirds of the screen is the map. The top third of that is the sky, which shows weather conditions and astrological conditions (e.g. a comet) which may affect gameplay later on. the lower two thirds of the map shows a three-quarters ASCII view of the terrain, including branching roads. the map is wider than the screen and can scroll left and right. at either end is the player's fortification and between are neutral fortifications of different types or sizes. the objective is to beat back your opponent until they give up or you take over their main fort. the _fun_ of the game - or part of it, anyhow - should be in positioning your troops, resting them, marching them down roads or through terrain (some terrain like fields should be passable but slower than roads and takes stamina away from troops, forests are passable only by small numbers at a time, mountains are impassable) and intercepting other armies, and in managing your troops stamina and supply lines for things like food. i guess food only for now. anyhow, moving small numbers of sneaky units through forests should be a viable tactic - stealth should be possible but not easy and it should require a lot of forethought.

i want to make this in rust with ratatui. be sure when you set up the build scripts and so on that you keep build artifacts cleaned up consistently and automatically - i dont want to run out of disk space from compiling too much.

---

## Running it

See [docs/DESIGN.md](docs/DESIGN.md) for the full design. It replaces the notes above where they disagree; for example, the game now runs in its own graphical window instead of ratatui.

```sh
make run        # build and play (dev build)
make release    # optimized build
make test       # sim tests
make clippy     # lint
```

Every `make` target runs `scripts/trim-target.sh` afterwards. It deletes stale incremental caches, and it wipes `target/` once it grows past `TARGET_CAP_MB` (default 3000).

**Controls:** left-click selects an army or fort (click again to cycle). Right-click marches the selected army, or attacks if you click an enemy; shift+right-click adds a waypoint. With an army selected, hovering shows the route it would take and its ETA. Clicking right on a road line picks the road, and marching armies string out in column along it. `R`est, `H`old (halt and deploy into line of battle), `D` detaches a 150-man raiding party (shift+`D` divides in half), `J`oin, `G`arrison (inside your own fort), `Q` cycles stance (aggressive / defensive / evasive), `P` cycles pace (easy / steady / forced march), and `M` (with your fort selected) musters 400 men from its garrison. **Supply:** with your fort selected, right-click another fort you hold to run wagons between them, and press `X` to recall them. Right-click enemy wagons with an army selected to raid them. `F1` or `?` shows all the controls. After a win or loss, `N` starts a new game. `Tab` next army, `C` center on it. `←/→` or the mouse wheel scroll, and clicking the minimap jumps there. `S` collapses or expands the sky. `Space` pauses, `1–4` sets speed ×1/×2/×4/×8.

**Development flags:** `bivouac --screenshot out.png [--advance MINUTES] [--weather clear|cloudy|rain|fog] [--scroll X] [--select ID] [--collapse-sky] [--overlay intro|help]` renders a frame, saves it and exits. `--seed N` replays a map, and `--reveal` lifts the fog of war.

## Layout

- `crates/sim`: the game rules. Deterministic, with no rendering, so it supports lockstep netplay, replays and headless AI games.
- `crates/ai`: the computer opponent. It sends the same `Order`s a player does.
- `crates/client`: the macroquad window. A glyph-grid renderer, sky, map and HUD.
- `assets/fonts`: DejaVu Sans Mono (license alongside).
