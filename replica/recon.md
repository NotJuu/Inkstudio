# Recon map: Clip Studio Paint → Inkstudio

Clean-room study of **what Clip Studio Paint (CSP) does and how users move through it**, as the basis for Inkstudio. Nothing here comes from CSP's code, binaries, file format internals, assets or artwork. Inkstudio will be built from scratch with its own UI art, brushes, tones and materials.

## 1. Scope

| question | answer |
| --- | --- |
| App | Clip Studio Paint (Celsys), the EX edition's feature set as reference |
| Platform | **Native desktop app in Rust** (Windows / macOS / Linux). No web app, no Tauri, no webview |
| Slice | Illustration + manga/comics (multi-page) + frame-by-frame animation, **manga-first** |
| Audience | Comic and manga creators: inking, screentones, panels, balloons, multi-page books, plus simple animation |
| Core loop | Open a manga project → lay out panels → pencil/ink with pen pressure and stabilization → tone and fill → letter with balloons → manage pages → export for print or web |

CSP's own split confirms where this sits: PRO is aimed at illustration and one-page manga, while EX adds multi-page works and unlimited-frame animation (PRO caps animation at 24 frames). Inkstudio targets the EX set for comics and animation, minus the 3D features.

**Terms note:** this map uses public sources only. No account was used. If you later check flows in your own CSP install, that's your own licensed use. Read the CSP EULA before using a licensed copy as a reference for a competing product, and never copy assets, materials or the `.clip` format from it.

**Network note:** this session's network blocks `clipstudio.net`, `help.clip-studio.com` and `tips.clip-studio.com`, so I couldn't open pages directly. The map comes from search-result excerpts of those pages plus well-documented public knowledge of the app. Rows marked *medium* or *low* confidence should be checked against the manual before the build.

## 2. Sources

| # | source | what it gives | URL |
| --- | --- | --- | --- |
| 1 | Official manual (Ver. 4) | full feature reference | https://help.clip-studio.com/en-us/manual_en/ |
| 2 | Manual: Frames and panels | frame border tools, dividing | https://help.clip-studio.com/en-us/manual_en/540_comic/Frames_and_Panels.htm |
| 3 | Manual: Screentones | tone layers, LPI, density | https://help.clip-studio.com/en-us/manual_en/540_comic/Screentones.htm |
| 4 | Manual: Animation | timeline, cels, export | https://help.clip-studio.com/en-us/manual_en/600_animation/600_animation.htm |
| 5 | Manual: Light table layers | light table vs onion skin | https://help.clip-studio.com/en-us/manual_en/600_animation/Using_light_table_layers.htm |
| 6 | Manual: Rulers | ruler basics, symmetry, perspective | https://help.clip-studio.com/en-us/manual_en/510_ruler/510_ruler.htm |
| 7 | Manual: Layer properties | layer property effects | https://help.clip-studio.com/en-us/manual_en/180_layers/Layer_properties.htm |
| 8 | Manual: Eraser tools | vector eraser modes | https://help.clip-studio.com/en-us/manual_en/240_brushes/Eraser_tools.htm |
| 9 | Manual: Creating a new canvas | presets, comic settings | https://help.clip-studio.com/en-us/manual_en/210_file/Creating_a_New_Canvas.htm |
| 10 | Manual: Canvas size and settings | change work settings | https://help.clip-studio.com/en-us/manual_en/210_file/Change_canvas_size_and_settings.htm |
| 11 | Manual: Convert to lines and tones | photo → manga background (EX) | https://help.clip-studio.com/en-us/manual_en/390_filters/Convert_to_lines_and_tones_(EX_only).htm |
| 12 | Manual: Ver. 4.0 updates | recent priorities (Puppet Warp etc.) | https://help.clip-studio.com/en-us/manual_en/030_new/ |
| 13 | User guide: Show all comic settings | binding, bleed, inner border, page numbers | https://www.clip-studio.com/site/gd_en/csp/userguide/csp_userguide/500_menu/500_menu_page_tojiichi_all.htm |
| 14 | User guide: Frame border edit tools | frame editing | https://www.clip-studio.com/site/gd_en/csp/userguide/csp_userguide/700_komawari_com/700_koma_edit_tool.htm |
| 15 | List of functions (detailed) | edition-by-edition feature table | https://www.clipstudio.net/en/functional_list/ |
| 16 | FAQ: PRO vs EX | what pros pay EX for | https://support.clip-studio.com/en-us/faq/articles/20200186 |
| 17 | Ver. 4.0 release notes | changelog | https://www.clipstudio.net/en/dl/release_note_old/v4/ |
| 18 | Ver. 4.0 FAQ | release and licensing | https://support.clip-studio.com/en-us/faq/articles/20240063 |
| 19 | Purchase page | pricing / editions | https://clipstudio.net/en/purchase |
| 20 | Tips: Specific features for manga | manga workflow walkthrough | https://tips.clip-studio.com/en-us/articles/9950 |
| 21 | Tips: Using animation features | animation walkthrough | https://tips.clip-studio.com/en-us/articles/9951 |
| 22 | Tips: Creating a new manga and storyboard | project creation flow | https://tips.clip-studio.com/en-us/articles/501 |
| 23 | Tips: How to use screentones | tone workflow | https://tips.clip-studio.com/en-us/articles/4511 |
| 24 | Tips: Vector layers in your workflow | vector editing | https://tips.clip-studio.com/en-us/articles/7537 |
| 25 | Tips: Special rulers | parallel / concentric / focus lines | https://tips.clip-studio.com/en-us/articles/5455 |
| 26 | Tips: Digital manuscript settings | print setup | https://tips.clip-studio.com/en-us/articles/7673 |
| 27 | Bleed/trim/safe area explainer (3rd party) | print terms | https://chanchauart.com/blog/2022/6/18/clip-studio-paint-how-to-make-a-comic-page-bleed-trim-margins-safe-areas |
| 28 | CG Channel: CSP 4.0 | press summary of 4.0 | https://www.cgchannel.com/?p=163891 |
| 29 | AlternativeTo: CSP 4.1 | 4.1 additions | https://alternativeto.net/news/2025/7/clip-studio-paint-4-1-adds-creative-hours-chart-transform-alignment-and-velocity-brus-graph |

`replica/screens/` is empty: no screenshots could be fetched from this session. Add reference captures from your own copy if you have one, named by screen ID (`S03-workspace.png`). They never ship.

## 3. Screen inventory

Desktop app, so "route" means how you open it.

| ID | screen | how you get there | purpose | key components | states to build |
| --- | --- | --- | --- | --- | --- |
| S01 | Start screen | app launch | recent files, new, open | recent-file grid, New/Open buttons | first run (empty), filled, missing file |
| S02 | New project dialog | File > New, Ctrl+N | pick use (Illustration / Comic / Animation), size, DPI, color mode | use tabs, preset list, numeric fields with units, preview | per-use variants, invalid size, preset saved |
| S03 | Main workspace | open/create a file | draw | canvas view, docked palettes, menu bar, command bar, status bar | empty canvas, filled, huge file, no tablet, palettes hidden (Tab) |
| S04 | Tool + sub tool palettes | docked in S03 | choose tool group, then a preset | icon column, sub tool list, groups | selected, renamed, custom subtool, drag-reorder |
| S05 | Tool property / sub tool detail | docked; detail via wrench button | tune the active tool | sliders, pressure-curve popups, checkboxes, category list | per-tool variants, reset to default |
| S06 | Color palettes | docked | pick colors | color wheel, sliders (HSV/RGB), color set, history, main/sub/transparent swatches | eyedropper active, out of gamut (CMYK) |
| S07 | Layer palette | docked | stack, blend, mask, lock | layer rows (thumb, name, type icon), blend mode, opacity, folder, mask thumb, reference/draft flags, lock icons | empty folder, many layers, selected multi, drag-hover |
| S08 | Layer property palette | docked | layer effects | border effect, tone toggle, layer color, expression color (color / gray / mono) | raster vs vector vs tone variants |
| S09 | Navigator | docked | overview, zoom, rotate, flip | thumbnail with view rect, zoom/rotate sliders | rotated, flipped |
| S10 | Selection launcher | floats under a selection | quick actions on a selection | small toolbar: deselect, invert, fill, new tone, transform, crop | shown/hidden |
| S11 | Transform mode | Edit > Transform, Ctrl+T | scale/rotate/skew/mesh/puppet | handles, mode dropdown, OK/Cancel bar | free / mesh / puppet warp, interpolation choice |
| S12 | Frame border editing (on-canvas) | Frame Border tool / Object tool on a frame folder | create and split panels | frame outlines, gutters, handles, divide line preview | single frame, divided, overlapping, outside-canvas bleed panel |
| S13 | Balloon + text editing (on-canvas) | Text / Balloon tool | letter the page | text box, balloon shapes, tails, font controls | editing, vertical text, overflow, tail dragged |
| S14 | Screentone dialog | Layer > New layer > Tone, or selection launcher | make a tone from a selection | LPI, density, dot shape, angle, preview | dots / lines / noise, gradient tone |
| S15 | Rulers on canvas | Ruler tools, Layer > Ruler/Frame | perspective, symmetry, special rulers | ruler guides, vanishing points, handles, snap toggle | 1/2/3-point, symmetry N lines, concentric, parallel, focus lines |
| S16 | Filter dialogs | Filter menu | blur, sharpen, correction, lines & tones | parameter sliders, live preview | preview on/off, long-running progress |
| S17 | Tonal correction dialogs | Edit > Tonal correction / correction layers | levels, curves, HSL, binarize | histogram, curve editor | as layer vs destructive |
| S18 | Page manager | open a multi-page work | see, reorder, add, delete pages | page thumbnails in spread layout, binding side, page numbers | 1 page, many pages, page open (dirty), cover pages |
| S19 | Story editor | Story > Story editor | edit all page text in one list | text blocks per page, sync with canvas | empty, long |
| S20 | Work settings (comic) | Story > Change basic work settings; Edit > Canvas properties | binding/trim size, bleed, inner border (safe area), start page, page numbers, units | numeric fields, toggles, preview | single page vs multi-page work |
| S21 | Timeline palette | Window > Timeline; animation project | frames, tracks, playback | track rows, frame cells, playhead, FPS, play range, camera keyframes | new timeline, many tracks, playing |
| S22 | Animation cels palette / light table | Window > Animation cels | assign and stack cels, light table | cel list, light table slots, opacity | onion on/off, cel-specific vs canvas light table |
| S23 | Onion skin settings | Animation > Onion skin settings | before/after counts, colors | number fields, color swatches, opacity | defaults |
| S24 | Export dialogs | File > Export (single layer, multi-page, animation) | PNG/JPG/PSD/PDF/GIF/MP4/image sequence | format picker, size/scale, color mode, page range, preview | per-format, progress, failure |
| S25 | Material palette | Window > Material | browse tones, brushes, images, templates | folder tree, thumbnail grid, search, tags | empty, filtered |
| S26 | Preferences | File > Preferences | pen pressure, cursors, canvas, performance | categories list, settings forms, pen pressure calibration | restart-required notice |
| S27 | Shortcut / command bar settings | File > Shortcut settings | rebind keys | command tree, key capture, conflicts | conflict warning |
| S28 | Workspace manager | Window > Workspace | save/restore palette layouts | named layouts | default reset |
| S29 | Recovery prompt | launch after crash | restore unsaved work | file list, restore/discard | none / several |

**29 screens.** S03 to S15 are the illustration core. S12, S13, S14, S18, S19 and S20 are the manga layer, and S21 to S23 are animation.

## 4. User flows

Click counts are best guesses from the docs, for the happy path in CSP. They are the numbers to beat.

```
F01 Start a manga page (single)
    S01 -> S02 (Comic use, preset B4 / A4 with bleed) -> S03 with trim, bleed, inner border guides
    happy path: ~4 clicks
    edge: custom size in mm vs px, DPI mismatch for print, changing bleed later (S20)

F02 Start a multi-page manga work
    S01 -> S02 (Comic, Multiple pages on, N pages, binding side, cover) -> S18 page manager -> double-click page -> S03
    happy path: ~6 clicks
    edge: right-to-left binding (manga), odd page count, adding pages later, start on left page

F03 Lay out panels
    S03 -> Frame Border tool -> rectangle frame over the page -> divide frame (drag a split) -> adjust gutters (S12)
    happy path: 1 frame + ~1 drag per split
    edge: bleed panel past trim, diagonal split, panel inside panel, drawing that overflows a panel (mask)

F04 Pencil -> ink
    S07 new raster layer (draft) -> sketch -> lower opacity / blue lines -> new vector layer -> ink with pen pressure + stabilization -> vector eraser "up to intersection" -> hide sketch
    happy path: ~5 clicks plus drawing
    edge: no tablet (mouse), tablet with tilt, huge brush sizes, undo hundreds of strokes

F05 Tone a region
    S03 select (lasso / auto-select by reference layer) -> S10 launcher "new tone" -> S14 pick LPI/density -> OK -> tone layer with mask
    happy path: ~4 clicks
    edge: preview at screen zoom (moire), gradient tone, tone on vector region, export at print DPI

F06 Fill flats
    Fill tool with "refer to other layers" + close gaps -> fill on a layer under the ink
    edge: gaps in line art, antialiasing halos, fill into the reference layer by mistake

F07 Letter a page
    Text tool -> click -> type (vertical text for Japanese) -> Balloon tool around it -> Balloon tail -> S19 story editor to proofread
    happy path: ~4 clicks per balloon
    edge: vertical text with Latin, text overflow, balloon over panel border, fonts missing on another machine

F08 Speed / focus lines
    Layer > Ruler/Frame > special ruler (parallel / focus) or a speed-lines tool -> drag
    edge: focus center off the canvas, varied line widths

F09 Manage pages
    S18 -> drag reorder, add / delete, open page, spread view
    edge: unsaved page open while reordering, page numbers shift, cover pages

F10 Export a book
    File > Export multiple pages -> S24 (format, page range, size, bleed on/off, color: mono 1-bit / gray / color) -> files
    edge: 1-bit export of tones without moire, PDF for print, web-size JPEGs per page

F11 Make a short animation
    S02 (Animation use, size, FPS, length) -> S21 timeline -> new animation folder -> draw cel per frame with onion skin (S23) -> playback -> S24 GIF/MP4/PNG sequence
    happy path: ~5 clicks plus drawing
    edge: frame holds (one cel over several frames), retiming, light table registration (S22), audio track (could)

F12 Illustrate (single image)
    S02 (Illustration) -> sketch -> lineart -> color with clipping masks and blend modes -> correction layers -> export PNG/PSD
    edge: large canvas (8000px+), CMYK preview, 16-bit (CSP does 8-bit work color, note in data model)

F13 Customize tools
    S04 duplicate sub tool -> S05 / detail change brush tip, pressure curves, stabilization -> save
    edge: reset, import/export brushes in Inkstudio's own format

F14 Crash recovery
    launch -> S29 -> restore
    edge: corrupted file, disk full on save
```

**14 flows.** The core loop is F02 → F03 → F04 → F05 → F07 → F10.

## 5. Components

The repeated UI parts, for replica-design. All will be drawn natively in Rust, so the widget toolkit has to support all of these.

| component | variants | states | used in |
| --- | --- | --- | --- |
| Dockable palette (panel) | docked, floating, tabbed, collapsed to icon | focused, resizing, drag-to-dock target | S03–S09, S21, S22, S25 |
| Menu bar + context menus | main, right-click on layer / canvas / page | hover, disabled, checked, submenu | everywhere |
| Command bar (toolbar) | icon buttons | hover, pressed, toggled | S03 |
| Icon button | normal, toggle, with dropdown | hover, active, disabled | everywhere |
| Slider with numeric field | linear, logarithmic (brush size), with pressure toggle | drag, typing, out of range | S05, S06, S08, S14, S16 |
| Pressure curve editor | input→output curve, velocity curve | drag point | S05, S26 |
| Numeric field with units | px / mm / in / pt, DPI aware | invalid | S02, S20, S24 |
| Dropdown / combo | blend mode, preset | open, hover | S07, S02 |
| Tree / list row | layer row, sub tool row, page row, material row | selected, multi-select, drag source, drop target, renaming | S04, S07, S18, S25 |
| Layer row (special) | raster, vector, tone, text, balloon, frame folder, folder, correction, animation folder, paper | visible, locked, clipped, masked, reference, draft | S07 |
| Thumbnail grid | pages (spread layout), materials, recent files | selected, loading | S01, S18, S25 |
| Color wheel / sliders / swatches | HSV wheel+triangle/square, RGB/HSV/CMYK sliders, color set grid, history strip | eyedropper live | S06 |
| On-canvas handles | transform box, mesh grid, puppet pins, ruler handles, frame handles, balloon tail, vanishing points | hover, drag, snapped | S11, S12, S13, S15 |
| Floating mini-bar | selection launcher, transform OK/Cancel, text edit bar | shown/hidden | S10, S11, S13 |
| Modal dialog | settings form + live preview + OK/Cancel | preview on/off, progress | S02, S14, S16, S17, S20, S23, S24 |
| Progress / busy | bar, cancelable | running, cancelled, failed | S16, S24 |
| Toast / status message | info, warning | auto-dismiss | S03 |
| Timeline grid | track header, frame cells, playhead, keyframe diamonds | selected range, playing, scrubbing | S21 |
| Brush cursor | outline, crosshair, dot | over canvas, size preview | S03 |
| Key capture field | single key, chord | conflict | S27 |

## 6. Inferred data model

Guesses at what the clone needs, from documented behaviour, not from CSP's file format.

```
Work (project)            id, title, kind (illustration | comic | animation), pages[], settings,
                          binding_side (left | right), start_page (left | right), cover (bool)
                          evidence: S02 use tabs, S20 settings, S18 page manager (EX multi-page)
                          confidence: high

WorkSettings              trim_size (w,h mm), bleed_mm, inner_border (safe area rect), dpi,
                          default_expression_color (color | gray | mono), page_numbering {font, pos, start}
                          evidence: "Show all comic settings" guide, digital manuscript tips
                          confidence: high (fields), medium (page numbering details)

Page                      id, work_id, index, canvas (Canvas), is_spread (bool), dirty
                          evidence: S18, F09; pages saved separately in page management mode
                          confidence: high

Canvas                    width_px, height_px, dpi, paper_color, layers (tree root), rulers[], timeline?
                          confidence: high

Layer (tree node)         id, parent_id, name, kind, visible, locked, opacity, blend_mode,
                          clip_to_below, mask (Raster 8-bit)?, reference, draft,
                          expression_color (color | gray | mono), layer_color (tint), border_effect?
                          kinds: raster | vector | tone | fill | gradient | text | balloon |
                                 frame_folder | folder | correction | animation_folder | paper | image(ref)
                          evidence: S07, S08, layer property manual
                          confidence: high

RasterData                tiled pixel storage (e.g. 256x256 tiles), RGBA8 or Gray8 or 1-bit
                          guess: tiling is our choice, not observed
                          confidence: medium

VectorStroke              points[] {x, y, pressure, width}, brush_ref, color, closed?
                          evidence: control points editable, vector eraser "up to intersection"
                          confidence: high (behaviour), our own representation

ToneSettings              lpi, density, dot_shape (circle | square | line | noise ...), angle,
                          position (follow layer | fixed), gradient?
                          evidence: screentone manual (LPI: higher = smaller dots)
                          confidence: high

FrameFolder (panel)       id, polygon (frame shape), border_width, mask of contents, gutter settings
                          evidence: frame border tool, divide frame folder
                          confidence: high

TextObject                id, frames[] {text, font, size, direction (horizontal | vertical),
                          leading, alignment}, position
                          evidence: text tool, story editor syncing text across pages
                          confidence: medium

Balloon                   id, shape (ellipse | rect | curve | flash), tails[] {points, style},
                          line_width, fill, text_ref?
                          evidence: balloon + balloon tail sub tools
                          confidence: medium

Ruler                     id, kind (straight | curve | figure | symmetry | perspective | concentric |
                          parallel | focus | guide), params (json), snap_on, show_in_all_layers
                          evidence: ruler manual, special ruler tips
                          confidence: high

Timeline                  id, fps, start_frame, end_frame, tracks[]
Track                     id, layer_id (animation folder), keys[] {frame, cel_id | null}
Cel                       id, track_id, layer_id (child of animation folder)
LightTable                scope (canvas | cel), entries[] {layer_ref, opacity, offset}
OnionSkin                 before_count, after_count, before_color, after_color, opacity
                          evidence: animation manual, light table manual; PRO 24-frame cap shows frames are first-class
                          confidence: high (concepts), medium (field names)

Tool / SubTool            id, tool_group (pen | pencil | brush | airbrush | eraser | fill | select |
                          text | balloon | frame | ruler | figure | ...), name, params (json),
                          brush_tip, pressure_curves {size, opacity, density}, stabilization, icon
                          evidence: S04/S05; sub tools are user-duplicated presets
                          confidence: high

Material (asset)          id, kind (tone | brush | image | template | balloon | ...), name, tags[], data
                          evidence: S25. Inkstudio ships its own originals
                          confidence: medium

UndoEntry                 id, label, affected (layer ids + tile ids), before/after deltas
                          guess: our design choice
                          confidence: low (not observable)

Preferences / Workspace / Shortcuts   key-value + palette layout tree + keymap
                          confidence: high
```

## 7. Feature matrix

See `replica/features.csv` (feature, area, priority, original, clone, notes). `clone` is `no` everywhere until the build starts.

## 8. What cannot be cloned

These are `skip` rows in the CSV.

- **Clip Studio Assets** (the material marketplace) and every downloadable or bundled material, brush, tone, 3D model and template. They belong to Celsys and their creators. Inkstudio ships its own.
- **3D drawing figures, 3D models and 3D head models**: licensed assets plus a full 3D engine. Out of scope for this slice.
- **`.clip` / `.cmc` file compatibility**: proprietary format. Reading it would mean reverse-engineering. Use PSD import/export and Inkstudio's own format instead.
- **Celsys cloud**: cloud sync, account, Clip Studio launcher, community (Ask / Tips), "my creative hours".
- **AI features tied to Celsys services** (pose scanner and similar). Possible later as Inkstudio's own, not as a copy.
- **Branding, icons, UI art, default brush shapes, the startup art.** All are drawn new for Inkstudio.
- **Licensed fonts and the Japanese fonts bundled with the app.** Use system fonts plus open fonts (e.g. Noto CJK).
- **Smartphone companion / tablet-as-remote features** (Companion mode).
- **Store, licences, plans, and the 24-frame PRO cap**: business rules, not features.

## 9. Size

| measure | count |
| --- | --- |
| Screens | 29 |
| Flows | 14 |
| Entities | ~20 |
| Feature rows | see CSV |

**Hard parts, ranked:**

1. **Brush engine with real pen input in native Rust.** Pressure, tilt and low latency on Windows (Windows Ink / WinTab), macOS (NSEvent tablet events) and Linux (libinput / XInput2). `winit`'s pen support is still partial, so this may need per-platform code. Plus stabilization, a GPU or SIMD dab renderer, and smooth strokes on 8K canvases.
2. **Layer compositing and undo at print resolution.** B4 at 600 DPI is about 6000×8500 px per page, times dozens of layers and pages. That needs tiled storage, GPU blend modes (`wgpu`), clipping and masks, tile-delta undo, and paging pages to disk.
3. **Native UI toolkit for a pro app.** Dockable palettes, trees, timeline grid, on-canvas handles, CJK text input (IME) and vertical text. No webview means picking and stretching `egui`, `iced` or `slint`, or writing custom widgets. That's a large share of the effort.
4. **Vector layers**: stroke editing, "erase up to intersection", line-width correction.
5. **Screentones**: resolution-correct halftones with no moiré on screen, exact 1-bit output for print.
6. **Text**: vertical Japanese layout, font shaping (`cosmic-text` / `swash`), balloons that fit the text.
7. **Animation**: timeline, cel holds, onion skin, real-time playback, video encoding (MP4 needs an encoder; GIF and PNG sequences are easy).
8. **PSD import/export and PDF export** with correct layers and color.

**Size: XL. Rescope it.** Illustration + manga + animation, native Rust with no webview, is several quarters of work for one person. Build it in phases, with each phase usable on its own:

| phase | contents | size |
| --- | --- | --- |
| 1. Ink core | canvas, pen input, 3–4 brush types, raster layers, blend/clip/mask, selection, fill, undo, save (own format) + PNG/PSD export | L |
| 2. Manga page | comic page setup (trim/bleed/safe area), frame borders, screentones, vector ink layers, rulers (straight, perspective, symmetry, focus/parallel), text + balloons | L |
| 3. Manga book | multi-page works, page manager, right-to-left binding, story editor, batch export to PDF / 1-bit PNG | M |
| 4. Animation | timeline, animation folders, cels, onion skin, light table, GIF/PNG sequence, then MP4 | M–L |

## Next step

Run `/replica-architect` to pick the Rust stack (window, GPU, UI toolkit, tablet input), the file format and the internal schema from this map.
