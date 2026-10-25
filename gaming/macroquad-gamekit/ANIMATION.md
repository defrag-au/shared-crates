# Animating sprites so they look and feel good

A sprite that looks right in a still can still move badly, and a scene hides both: things overlap,
move, and show whichever frame the clock lands on. So a sprite is tested **alone, first still and
then in motion**, against the rubric below. Each rule has a check that names it in a report, so a
review that reads images and text (an agent's) can apply it the same way an animator's eye would.

## The loop

The game lists its art in `Game::sprites`: each sprite's frames, the scale it is drawn at, its
`Gait`, and `.body(bob, surge)`, the art pixels its body moves with the gait. Take those numbers
from the same function the scene uses, so what is tested is what is drawn.

```sh
game --sprites .tmp/s.png --only whale                        # 1. still: shape and the cycle
game --sprites .tmp/f.png --only whale --strip 24 --every 6   # 2. alone, in motion
game --shot .tmp/m.png --size 814x844 --at 9 --strip 16 --every 4 --crop x,y,w,h   # 3. in the scene
game --view whale                                             # 4. a person's review: the viewer
```

**The viewer is where a person judges it.** `--view` opens a window, in place of the game, on
the sprites `Game::sprites` lists. It shows one sprite looping, large against where it rests and at
its real size, and a crowd of its clones each on its own phase. Keys tune the period, each step's
hold, the jitter, the bob and the surge live, and `P` prints the `Gait::new(...)` and `.body(...)`
lines to paste back; the motion problems for the tuned values show beside it. It runs through the
kit's loop, so `--view whale --shot v.png --size 1280x800` captures it, the tuned lines and the
problems in the report: the agent's way to see what the person is looking at.

The numbers below are for the agent's iterations and the person's attention. They are not a gate:
nothing fails a build on them, and a rule broken on purpose (a smear frame, a snap for a hit) is
just said so.

1. **The sheet** (`s.png`, `s.txt`). Every frame zoomed, flipped, as a white silhouette and at
   real size. Then a row of the cycle as played at 60 Hz, each pose with the pose before it ghosted
   in red, and a trail with the moving part's path. A pose that pops or flickers is labelled on the
   image. **Read `s.txt`'s Problems first.** The sheet wraps at about 1900 px, so it is read whole.
2. **The film** (`f.png`, `f.txt`). The sprite alone on a plain canvas, played through its gait
   and moved by its body. The frames run in time order, `--every` display frames apart. Pick
   `--every` so the frames cover one cycle: `period × 60 / count`. Faint guides mark where it rests.
   The last row holds the trail (every frame averaged) and the change map (what changed each step,
   blue early, yellow late). The report gives the gait cycle's problems, then the film's.
3. **The scene**, only once the sprite passes alone. `--strip` takes frames from `--at` on.
   `--crop` cuts them to one mover, in design units. Always give `--size`: the layout follows the
   window, so a crop without it lands somewhere else.
4. **Hand it over.** Tell the person to open `--view NAME`. What they tune there comes back as
   lines to paste.

Fix the art, not the threshold. A threshold that names something good as a problem is a bug in
the check: fix the check, saying why (the `line` rule once flagged a pivoting tail whose arc is
shallow, so it now asks whether the part turns).

## The rubric

| Rule | Why it matters | Check |
|---|---|---|
| **Reads as its silhouette at real size** | A shape that only reads in colour, zoomed, is lost in a scene. A one-pixel stalk reads as a line, not a tail. | Sheet: white silhouette and real-size panel. Name the creature from the white shape alone. |
| **Spacing: no part travels over 2 art px a pose** | Further, and the eye sees two pictures, not one thing moving. | `pop` (cycle), `uneven` (film). Cure it with an in-between pose, never by slowing the timing. |
| **8–12 poses a second for continuous motion** | Fewer is a slideshow: a 900 ms hold on a swimming tail reads as frozen. | `slideshow`: under 6 a second with real travel. Hold the extremes at most ~250 ms in a loop. |
| **Even spacing, eased at the ends** | Speed should change smoothly: slow into an extreme, quick through the middle. | Film `uneven` (a step over 2× its neighbours). The change map should ramp evenly. |
| **Arcs, not lines** | Tails, fins and limbs pivot, so their tips trace curves. Sliding straight up and down reads as a piston. | `line`: the moving part's path is flat *and* it does not turn. Pivot it about its root. |
| **Overlap and follow-through** | The root of a tail moves first and the tip a pose later, so the tail bends rather than turning stiffly. | Onion skins: the root changes in one step and the tip in the next, not all at once. |
| **The body moves with the stroke, eased** | Two frames on a still body look pasted on. | Use `Stride::swing` for bob and `Stride::surge` for push, never linear. Visible against the film's guides. |
| **Mass is kept** | A silhouette that grows and shrinks reads as swelling. | `mass`: area changes over 12%. |
| **Clones out of step** | A school flicking in unison looks mechanical. | `lockstep`: the gait has no `jitter`. |
| **Every pose is seen** | A hold under 2 display frames strobes, and under 1 is never drawn. | `flicker`. |
| **Clean pixels** | Lone pixels, and curves stepped unevenly (1-3-1 rather than 1-1-1 or 2-2-2), read as noise. The outline should be darker than the fill. | The zoomed panels and the onion skins. |

## Reading a film

- **Rows are time.** Look along them for one thing moving, not poses swapping. If a part jumps
  between neighbouring frames, the report says where.
- **The trail.** An even smear is smooth motion. Clumps are holds, and distinct ghosts far apart are
  pops.
- **The guides.** The body should move against them by its bob and surge. If it doesn't, only the
  frames are moving.
- **Frame changes are only part of it.** A film of a two-frame sprite with a good body motion can
  still pass. Judge the tail itself on the onion skins and the as-played row.
