# egui-widgets

Reusable egui widget library for defrag frontends.

## Icons & Special Characters

**NEVER use raw Unicode symbols** (e.g. `●` `○` `✓` `✕` `→` `★`) — they will render as broken boxes in the browser. The default egui font and the Phosphor font do not include geometric/symbol Unicode blocks.

**ALWAYS use `PhosphorIcon`** from `icons.rs` for any icon or symbol:
- `PhosphorIcon::CheckCircle` not `✓` or `●`
- `PhosphorIcon::X` not `✕` or `×`
- `PhosphorIcon::Clock` not `○` or `◌`
- `PhosphorIcon::Warning` not `⚠`
- `PhosphorIcon::Plus` / `PhosphorIcon::Minus` not `+` / `−`
- `PhosphorIcon::ArrowRight` not `→`

Basic ASCII characters (`!`, `?`, `#`, `+`, `-`) are fine.

To add new icons: look up the codepoint from the Phosphor CSS at `https://unpkg.com/@phosphor-icons/web@2.1.1/src/regular/style.css`, add the variant to `PhosphorIcon` in `icons.rs` (enum, `codepoint()`, `ALL`, `name()`).

## egui API Notes (v0.33)

- `Rounding` is renamed to `CornerRadius` and takes `u8` not `f32`
- `painter.rect_stroke()` requires a 4th `StrokeKind` argument (`egui::StrokeKind::Inside` or `Outside`)
- `Image::rounding()` is renamed to `Image::corner_radius()`
- `Frame::corner_radius()` still takes `f32` (different from `CornerRadius`)
- `Margin::symmetric()` takes `i8` not `f32`
- `truncate_hex()` takes 3 args: `(hex, prefix_len, suffix_len)`
