# Visual review corpus

A fixed set of rendered UI captures with known defects and clean controls, for
comparing vision models and agent harnesses on the same task. It is data only:
the harness owns the model call, the prompt and the scoring.

## Layout

```
cases/<slug>/
  case.toml      case metadata: story, arch, os, intent, and the images
  expected.toml  scorer-facing: defect or clean, boxes, severity
  full.png       the 1:1 capture
  crop.png       optional, a 3–6x view of the region in question
```

`RUBRIC.md` is the prompt. Use it unchanged for every model and every run, or
the comparison is not comparable.

## Provenance

Every defect case is a real regression from `macroquad-storybook`, and every
clean control is the same story after the fix.

| case | story | what it showed |
| --- | --- | --- |
| `buttons-before` | `atoms/buttons` | the first row label had no headroom above it |
| `stick-before` | `input/floating-stick` | the third sample and label were cut at the right edge |
| `art-before` | `gamekit/rounded-art` | the seventh finish was outside the canvas |
| `wall-before` | `gamekit/wall` | the capture happened before any tile loaded |
| `holder-before` | `page/page-holder-profile` | sidebar labels bled into the capture's left edge |

The `*-clean` cases are those stories after the fix. `app-grid-clean`,
`shell-phone-clean` and `shell-desk-clean` are clean compositions with no
before/after pair.

The images are aarch64 macOS captures from `--shot --no-chrome`. Their clean
controls are byte-identical to `tests/goldens-aarch64/<slug>/macos.png` at the
time of writing; both the before and clean images are frozen, so a later golden
re-bless does not rewrite a case.

The before images are the captures that showed the bug and are not regenerated
from the current tree. Do not “correct” a before image.

The corpus is one architecture and one OS. The storybook keeps
`tests/goldens-<arch>/<slug>/<os>.png` because the renderer differs by
architecture and OS; a cross-architecture or cross-OS review would need its own
before-images and is a separate corpus.

## Running

The harness reads every `cases/*/case.toml`, sends `image` and, when present,
`crop` with `RUBRIC.md` and `intent`, and parses the JSON answer. It reads
`expected.toml` only to score the answer; that file must never be in the prompt.

The runner is being built in
`~/code/defrag/agent-playbook/integrations/review-bench` on rig; the design and
its constraints are in
`~/code/defrag/agent-playbook/docs/design/visual-review.md`. This directory is
data only.

Suggested runs: three per model, recording every answer. Report detection,
localisation and false positives separately.

## Scoring

- **Detection** — a defect case is detected when at least one reported defect
  has the expected kind (or `other` with a note that names it) and its box
  touches the expected box. The model's `[x0, y0, x1, y1]` in 0–1000 is
  converted to full-image pixels before the comparison; `expected.toml` boxes
  are already `[x, y, w, h]` pixels.
- **Localisation** — IoU of the best matching box. Report the mean over
  detected cases; a detection with no overlap is a false positive, not a miss.
- **False positives** — any defect reported on a `clean` case, and every
  unmatched report on a defect case. A model that always says “defect” is caught
  here; one that always says “clean” fails detection.
- `severity = "should"` counts half; `must` counts full. Report both raw and
  weighted.
- Three runs; report the median, and note any case whose answer changed between
  runs. A model that is right once in three is not reliable.

## Adding a case

1. Capture the story at its `--no-chrome` canvas on the architecture you are
   recording, and record `arch` and `os` in `case.toml`. Keep the failing image;
   that is the case.
2. Write `case.toml` with `story`, `arch`, `os`, `intent`, `image` and, if
   useful, `crop` and `crop_box` (full-image pixels, top-left origin).
3. Generate the crop; do not hand-make it. A crop must not cut through content
   except at the screen edge under test: a crop edge is not a screen edge, and
   the first run mistook a crop edge for a clipped sample.

   ```sh
   image-shot crop cases/<slug>/full.png --box X,Y,W,H --zoom 4 \
       --artifacts cases/<slug> --out crop.png
   ```

4. Write `expected.toml` with `expected = "defect"` and one `[[defect]]` per
   visible problem, or `expected = "clean"` and a `note` naming anything a
   reviewer might mistake for a defect.
5. State the screen's intent, not the answer. “Seven finishes, the last named
   warm” is an intent; “the seventh is missing” is the answer.

## Not in the corpus

The shell's 34-point band under the bottom bar is an open design question, not
a rendering defect. It stays out until there is an answer, then it becomes a
case with that answer.
