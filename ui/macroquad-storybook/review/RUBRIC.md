# Visual review rubric

You are reviewing one rendered UI screenshot. You are given the image, an
optional magnified crop, and a one-line statement of what the screen is meant
to show. Report only defects. Do not describe the screen.

## Defect classes

| kind | a defect is |
| --- | --- |
| `clipped` | text or content cut off at an edge of the image or its own surface |
| `missing` | content the intent says should be there is absent |
| `overlap` | two elements cover each other unintentionally |
| `spacing` | a gap or margin is visibly wrong, or unlike its siblings |
| `alignment` | elements that should share an edge or centre do not |
| `contrast` | text or an icon cannot be read against what is behind it |
| `other` | a real visual problem that none of the above names |

## Rules

- `box` is four integers from 0 to 1000, relative to the **full image**:
  `[x0, y0, x1, y1]`. `(0, 0)` is the top-left and `(1000, 1000)` the
  bottom-right. Do not use pixels.
- The crop is a magnification of one region. Judge composition on the full
  image and detail on the crop; do not report a defect that exists only because
  the crop is a crop, and do not give coordinates in crop space.
- Intentional states named in the intent are not defects: a row clipped by a
  scroll viewport, a failed-image placeholder, a deliberately truncated title.
- Do not report a defect you cannot point at. If the screen looks right, say so.
- Do not invent a defect to be helpful. A clean screen must come back clean.
- One entry per distinct defect. A row of clipped labels is one entry, not ten.

## Answer

Return only this JSON, with no prose around it:

```json
{
  "defects": [
    {
      "kind": "clipped",
      "box": [0, 0, 0, 0],
      "confidence": 0.0,
      "note": "one line naming what is wrong and where"
    }
  ]
}
```

`confidence` is 0 to 1. A clean screen is `{"defects": []}`.
