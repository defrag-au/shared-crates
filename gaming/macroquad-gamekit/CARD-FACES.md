# Card faces with the gamekit

How to build a card face that reads like a printed trading card and stays readable in a hand.
The worked example is `card.rs` in augminted-bots' `fight-macroquad`; this note is the part that
carries over to another card game. Read `CATALOG.md` for the modules named here.

## Readability comes first

Decide what must be readable when the card is mostly covered. In a fan each card shows only a
strip down its left side, about 0.4 of its width, so these go there, top-left first:

1. **Cost**, as a gem over the title plate's left end.
2. **What it does**, as emblems: an icon and a large number per effect, stacked down the art's
   left edge.
3. **Its kind**, as the frame colour, readable from any visible sliver.

The name, the type line and the rules text are for a card that is lifted or zoomed. Put numbers
and icons in the view as data (an effect kind and an amount), not only in a sentence, so the face
never parses prose.

Size everything as a fraction of the card's width, `W`. The fight's card is `W = 112` design
units by `1.4 W`. At that size the rules text is about `0.072 W`, which is 8 units, the harness
report's minimum. Zoomed copies are the same face baked at a larger scale, not a second layout.

## Layers, back to front

| layer | module | note |
|---|---|---|
| Rim | `fill_gradient` | near-black, a little light at the top; about `0.022 W` wide |
| Frame | `fill_gradient`, `grain`, `bevel` | the kind colour as metal, light to dark; shows as a band between rim and plates |
| Title plate | `fill_gradient`, `bevel` | dark; name in a condensed display face, shrunk to fit, then cut with an ellipsis |
| Cost gem | `fill_circle_gradient` | a dark setting, a radial body, a specular glint; red when unaffordable |
| Art window | `RoundedArt`, `inner_shadow`, `stroke_rounded` | the inner shadow sets the art behind the frame |
| Emblems | `fill_gradient`, `stroke_rounded`, `RichText` | over the art, edged in the effect's colour |
| Type line | `fill_gradient`, `bevel` | kind and faction, with the kind icon at the right |
| Rules box | `fill_gradient`, `grain`, `inner_shadow`, `RichText` | parchment; mechanic labels in the kind colour |

Inset the plates and window by about `0.075 W` so the frame reads as a band, not a hairline.

## Baking

Bake each face once per card and state (`Baker::render`), keyed on what changes how it looks:
the card, whether its art has arrived, whether it is affordable. Draw it every frame through a
`Pose`. Dim an unaffordable face with the draw tint (about 0.55) rather than a second bake of
every layer.

Use `Baker`, not `Baked::render`, for any face with translucent layers: it seals the alpha
those layers wear down, and draws twice so text survives the glyph atlas growing mid-bake.

## Light, at draw time

- **Shadow:** `Lighting::soft` under each card, offset and blurred more as the card lifts, so a
  lifted card reads as nearer.
- **Glint:** `Lighting::sheen` on the held or zoomed card only, its phase driven by the game's
  clock, a sweep every few seconds. On every card it is noise.

## Review

Give the game a sheet mode that draws every face at hand size (`--sheet`), including one
unaffordable and one of each special kind. Read the sheet's report for text that is cut, small
or overlapping, then look at the image for the rest.
