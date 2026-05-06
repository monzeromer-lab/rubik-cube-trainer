# rubiks-trainer

A virtual 3D Rubik's cube game and trainer built in Rust with [Bevy](https://bevy.org). Supports 2×2 / 3×3 / 4×4 / 5×5 cubes with custom-built solvers (no external solver crates), a progressive learning track from beginner LBL through CFOP, and a daily-use timer + drill mode for speedcubers.

## Status

Active development. Milestones M1–M14 shipped; M15 (docs + polish) is in progress.

| Milestone | Output |
|-----------|--------|
| M1 | Cube model, moves, scrambles |
| M2 | 2×2 optimal solver (full distance table, ~3.5 MB) |
| M3 | 3×3 two-phase Kociemba-style solver (perf-limited, see below) |
| M4 | Bevy app skeleton, lighting, orbit camera |
| M5 | Move animation system |
| M6 | Drag-to-turn input |
| M7 | All four cube sizes render |
| M8 | Sticker-input validation pipeline (cube_core::Facelets::validate) |
| M9 | Trainer core: timer, AO5/AO12, session stats |
| M10 | Drill model: per-case stats, weakness ranking, OLL drill set |
| M11 | Lesson schema + GoalPredicate evaluator |
| M12 | Algorithm library starter set (CFOP OLL/PLL/F2L) with round-trip verification |
| M13 | 4×4 reduction predicates + Solver4x4 (already-reduced inputs) + S-key wiring |
| M14 | 5×5 reduction predicates + Solver5x5 (already-reduced inputs) + N×N reduction lesson |

### Known limitations (M15 polish)

- **3×3 solver is slow on multi-move scrambles.** Phase-1 currently uses only the `(co, udslice)` heuristic — `(eo, udslice)` and `(co, eo)` tables are sparse single-representative BFS and unsafe to combine. Phase-2 heuristic is similarly loose. Result: scrambles ≥4 moves can take minutes. Tighter joint pruning is M15 work.
- **4×4 / 5×5 solvers only handle already-reduced inputs.** The full IDA* center-solving and edge-pairing/-assembly searches are not yet implemented. Scrambled cubes return `NotReducedYet`.
- **4×4 PLL parity fix** is a placeholder — the current 6-move alg is order-2 but doesn't preserve reduction. Real PLL-parity inputs return `UnresolvedParity`.
- **Sticker-input UI** is not yet wired up; the validation backend exists but there's no painting interface.
- **OLL parity** has no fix algorithm yet — the speedcubing-standard 9-move forms have net Rw rotation and break reduction.

## Building & running

Requires a recent stable Rust (the workspace uses edition 2024). Linux is the developed platform; Bevy supports macOS and Windows but binaries aren't tested there.

```bash
# build
cargo build --workspace

# run the trainer app
cargo run -p rubiks-trainer --release

# run the test suite
cargo test --workspace
```

## In-app controls

| Input | Action |
|-------|--------|
| `2` / `3` / `4` / `5` | Switch cube size |
| `U` `D` `L` `R` `F` `B` | Quarter-turn that face (clockwise) |
| `Shift` + face | Counter-clockwise quarter |
| `Alt` + face | Half turn |
| `Space` | Random scramble (size-appropriate) |
| `Backspace` | Reset to solved |
| Left-click + drag on a sticker | Turn the corresponding face |
| Right-click + drag | Orbit camera |
| Middle-click + drag | Pan camera |
| `S` | Solve the current cube (lazy table build on first press) |
| `T` | Start a timed solve (scramble + inspection) |
| `Enter` | End inspection / begin solving |
| `Escape` | Abandon current solve |

The first press of `S` for a given size builds that size's tables — a few seconds for 3×3 (and the 4×4 / 5×5 wrappers, which include a 3×3 internally). Subsequent solves are instant.

## Architecture

```
crates/
├── cube_core/      pure-logic cube state, moves, facelets, scrambles, validation
├── cube_solver/    custom solvers: ida (generic IDA*), two, three, four, five
├── cube_render/    Bevy plugin: meshes, materials, animation, ECS systems
├── cube_input/     drag-to-turn observer + decision logic
├── cube_trainer/   timer, session stats, drill mode
├── cube_guides/    lesson schema, GoalPredicate, algorithm library
└── app/            the binary (rubiks-trainer)
```

`cube_core` has no engine or rendering deps and is fully testable as a plain library. The Bevy plugins (`cube_render`, `cube_input`) sit on top of it. The trainer/guides crates depend only on `cube_core` for their domain logic.

### Key invariants

- `Cube::is_solved` is **sticker-based** — visual comparison via `Facelets`, not strict cubie equality. Use `Cube::is_strictly_solved` for internal invariant checks. (Center cubie rotations on a 3×3 are unobservable.)
- `MoveSeq::parse` requires `size` because cube rotations (`x/y/z`) and slice-depth tokens (`2R`) only make sense at a known cube size.
- `Solver2x2` requires the DBL corner to be at home (it uses an R/U/F alphabet); a cube-rotation prefix is needed for off-canonical input.
- Big-cube solvers (`Solver4x4`, `Solver5x5`) require the input to be already-reduced. Centers + edges must be uniform/paired/assembled.

## Development

The implementation plan lives outside the repo at `RUBIKS_CUBE_GAME_PLAN.md`. The plan defines milestones M1–M15, design decisions, target performance budgets, and verification strategies.

Pull requests should:
1. Run `cargo test --workspace` and ensure no new failures.
2. Maintain the no-engine-deps boundary on `cube_core`.
3. Mark perf-blocked or M15-polish-deferred tests with `#[ignore = "..."]` and a short reason rather than letting CI hang.

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

Contributions are welcome — see the implementation plan for the open M15 polish items if you're looking for a starting point.
