# Inkstudio

A native desktop drawing app for manga and comic artists, written in Rust. It is built for fast inking, real print output (screentones, 1-bit, bleed/trim) and large pages that stay smooth.

> Status: **engine first.** The drawing engine works and is tested. The app window (winit + wgpu + egui) is next. See `replica/` for the plan.

![Showcase page](replica/showcase.png)

*A B4 page at 600 dpi (6071 × 8598 px) drawn entirely by the engine:*
- *Panel 1: focus lines with the focus ruler.*
- *Panel 2: a building on a two-point perspective ruler, a gap-closing fill that refers to the line art, and a 60 lpi screentone clipped to it.*
- *Panel 3: a vector cross-hatch cleaned with "erase up to intersection".*

## What makes it a manga tool

| feature | where | status |
| --- | --- | --- |
| Pen feel: pressure curves, stabilization + post-correction, start/end taper, stroke buffer (overlapping dabs never exceed the stroke's opacity) | `ink-raster::brush` | engine ✔ |
| G-pen, mapping pen, pencil, airbrush and eraser presets | `ink-raster::brush` | engine ✔ |
| Vector layers: editable strokes drawn with the same brush engine; line width correction | `ink-vector::stroke` | engine ✔ |
| Vector eraser: touched part / **up to intersection** / whole line | `ink-vector::stroke` | engine ✔ |
| Fill that refers to other layers, closes gaps in line art and grows under the lines | `ink-raster::fill` | engine ✔ |
| Screentones: LPI, density (calibrated per dot shape), angle, 6 dot shapes, 1-bit print output | `ink-raster::tone` | engine ✔ |
| Rulers: perspective 1/2/3-point, parallel, focus lines, concentric, straight, symmetry (rotational and mirror) | `ink-vector::ruler` | engine ✔ |
| Layers: blend modes, opacity, clipping, alpha lock, reference and draft layers, copy-on-write undo | `ink-core` | engine ✔ |
| **54 brushes**, manga first: ink pens, pencils, paint, airbrush, erasers, manga effects, decoration, tone pens | `ink-raster::presets` | engine ✔ |
| Brush engine options: tip shapes (round, flat/chisel, square, star, heart, flower, ring, multi-line), paper grain, scatter, speed → size, follow stroke direction, dashes, paint-with-screentone | `ink-raster::{brush, tip}` | engine ✔ |
| Panels, balloons, vertical text, multi-page books, animation timeline, 3D mannequin, material packs | `replica/architecture.md` | planned |

## Brushes

![Brush catalog](replica/brushes.png)

| category | brushes |
| --- | --- |
| Ink pens | G-pen, mapping pen, round pen, turnip pen, brush pen (thins when fast), sumi brush, calligraphy nib, fineliner, marker, dry ink pen, effect-line pen |
| Pencils | HB, soft 2B, mechanical, layout pencil, charcoal, colored pencil, crayon |
| Paint | round, flat, dry brush (bristle lines), watercolor wash, gouache, flatting brush |
| Airbrush & spray | soft, hard, spray, grit |
| Erasers | hard, soft, kneaded, rough, tone scratch (erases in a line pattern), scratch knife |
| Manga effects | speed-line brush (many parallel lines in one stroke), hair strands, dashed line, stitches, rain, snow/bokeh, cloud/smoke, bubbles, grass, leaves |
| Decoration | sparkles, stars, hearts, flowers, stipple |
| Tone pens | dot tone 20% / 40%, line tone, cross-hatch tone, sand tone. They paint the screentone pattern directly |

Every preset costs **≤ 0.2 ms per pen sample** (most 0.003–0.07 ms), measured with
`cargo run --release -p ink-raster --example brush_sheet`.

## Performance

Measured on 4 CPU cores, B4 at 600 dpi (52 MP), release build (`cargo run --release -p ink-vector --example showcase`):

| operation | time |
| --- | --- |
| One pen sample, G-pen (stamp + recomposite of the changed rectangle) | 0.06–0.12 ms |
| Recomposite one dirty tile for display | 0.07 ms |
| Fill with "refer to line art" + 6 px gap closing, whole page | ~0.43 s |
| 60 lpi screentone over a quarter page | ~0.1–0.2 s |
| Full-page composite (816 tiles, parallel) | ~0.08 s |
| Flatten whole page for export | ~0.08 s |
| Undo / redo of any step | < 0.1 ms |

How:
- **Sparse 256 px tiles.** Empty areas cost nothing.
- **Copy-on-write tiles shared through `Arc`.** An undo step stores pointers to the old tiles, not pixels.
- **Per-stroke coverage buffer.** Recompositing covers only the changed rectangle, written in place.
- **Fill passes are linear time.** Tile-based barrier mask, sliding-window dilation in parallel row bands, and growth seeded only at the region outline.
- **rayon** runs compositing, tones, fill and export in parallel.
- Pixel crates are built with `opt-level = 3` even in dev builds.

## Workspace

```
crates/
  ink-core     tiles, layers, blend modes, compositing, undo
  ink-raster   brushes + stabilizer, fill, screentones
  ink-vector   vector strokes + eraser, rulers
  ink-ui       design tokens, theme and egui widgets (see replica/design)
replica/       recon, architecture, schema, design system, feature matrix
```

## Build and test

```sh
cargo test --workspace
cargo run --release -p ink-vector --example showcase -- replica/showcase.png
cargo run --release -p ink-ui --example design_gallery -- replica/design 2
```

Requires Rust 1.95+ (2024 edition).

## License

MIT. Inkstudio is an original implementation. It contains no code, assets, file formats or artwork from Clip Studio Paint or Krita.
