# Inkstudio architecture

Built from `replica/recon.md` and `replica/features.csv`. The schema is in `replica/schema.sql`.

Inkstudio is a **native desktop app written in Rust**: no server, no web app, no webview, no Tauri. The usual replica stack (Next.js, Postgres, auth, payments, hosting) doesn't apply. Each part of that stack maps to a desktop equivalent:

| web-app layer | Inkstudio equivalent |
| --- | --- |
| web app / mobile | one Rust binary per OS |
| database | one SQLite file per Work (`.inks`) + one app library file |
| API routes / server actions | the in-process **command layer** (undoable editor commands) |
| auth / payments / email / hosting | none. Local, single-user app. Distribution and licensing come later |
| background jobs | worker threads: autosave, thumbnails, export, tile eviction |
| files | tiles and resources stored inside the `.inks` file |

## 1. Stack

The rule is one process, one binary and one Cargo workspace, with no microservices and no plugin runtime in v1.

| layer | choice | why |
| --- | --- | --- |
| language | Rust (stable, 2024 edition) | your requirement; memory-safe, and its threads make parallel pixel work safe |
| windowing + events | `winit` 0.30 | the standard cross-platform window/event loop; gives IME events and raw window handles for platform pen code |
| GPU | `wgpu` | one API over D3D12 / Metal / Vulkan; draws canvas compositing and the UI |
| UI toolkit | `egui` (immediate mode) + `egui_dock` | most mature Rust GUI for dense tool UIs. Docking, trees and custom widgets exist, and the canvas is a custom `wgpu` paint callback. Alternative: `iced` is prettier but weak on docking. Decide in M0 with a spike |
| pen / tablet input | own crate `ink-input`: Windows `WM_POINTER` (Windows Ink) via `windows`, WinTab as fallback; macOS `NSEvent` tablet events via `objc2`; Linux Wayland `tablet-v2` and X11 XInput2. Evaluate `octotablet` first | `winit` doesn't yet report pressure and tilt reliably on all platforms. This is the top risk, so it's spiked in M0 |
| pixel engine | CPU, `rayon` + portable SIMD (`wide`) on 256×256 tiles | deterministic (preview = export), simpler than GPU brushes, and fast enough when only touched tiles are processed. Dirty tiles are uploaded to the GPU |
| compositing | `wgpu` compute/fragment shaders on a tile texture cache for the view; the same blend math on CPU for export | interactive speed on huge canvases; the CPU path is the reference |
| geometry | `kurbo` (curves, intersections), `lyon` (tessellation for UI overlays) | vector eraser "up to intersection", frame polygons, balloon paths |
| text | `cosmic-text` for horizontal text, `rustybuzz` + `swash` with our own **vertical** layout; `fontdb` for system fonts; bundled Noto Sans/Serif CJK (OFL) | no Rust crate does Japanese vertical layout with tate-chu-yoko, so we write that part |
| project file | `rusqlite` (bundled SQLite, WAL) + `zstd` tile compression | crash-safe transactional saves, incremental save (only dirty tiles), pages load lazily, one portable file |
| serialization | `serde` + `serde_json` (params columns), TOML for prefs/shortcuts/workspaces | readable, versionable |
| image IO | `image` (PNG/JPEG/WebP), own PSD writer + `psd` crate for reading | PSD round-trip is a should-have |
| PDF | `pdf-writer` | small, fast, used by Typst; needed for 1-bit and gray print PDFs |
| GIF / video | `gif` crate; MP4 (H.264) via `openh264` + `mp4` muxer, behind a feature flag | GIF and PNG sequences need no codec. H.264 has patent/licensing questions, so it's optional at first (see §4) |
| color | sRGB 8-bit working space; `lcms2` for CMYK soft-proof (could) | the same working space as CSP-style tools; keep linear-light blending as an option |
| IDs | `uuid` v7 | time-ordered and stored as 16-byte blobs |
| errors / logs | `thiserror`, `anyhow` (app edge), `tracing` | |
| tests | `cargo test`, `insta` snapshots, golden-image pixel tests, `proptest` for undo/redo | brush and blend output must not drift |
| packaging | `cargo-dist` or `cargo-packager`: MSI/MSIX, signed `.dmg`, AppImage/Flatpak | |
| CI | GitHub Actions on windows-latest / macos-latest / ubuntu-latest | three-OS pen and GPU differences show up early |

### Workspace layout

```
crates/
  ink-core      document model, commands, undo, selection, no GPU/UI deps
  ink-raster    tiles, brush engine, fill, filters, tone rasterizer, blend math
  ink-vector    strokes, hit-testing, vector eraser, frame/balloon geometry
  ink-text      shaping, vertical layout, balloon fitting
  ink-anim      timeline, cel resolution, onion/light table, playback clock
  ink-io        .inks (SQLite) load/save/migrate, PNG/PSD/PDF/GIF/MP4 export
  ink-render    wgpu tile cache, compositor, canvas view, overlays
  ink-input     pen/tablet per OS, stroke sampling, stabilizer
  ink-ui        egui palettes and dialogs (S01–S29)
  inkstudio     binary: wires everything, event loop, threads
```

`ink-core` and `ink-raster` must not depend on `wgpu` or `egui`, so they can be tested headless and reused in a CLI exporter.

### Threads

- **UI thread**: winit event loop, egui, sends commands.
- **Input thread** (per OS where possible): reads pen samples at full rate (200+ Hz) with timestamps, so frame rate never limits pen sampling.
- **Stroke worker**: stabilizes samples, stamps dabs into tiles, publishes dirty tile IDs.
- **Render**: uploads dirty tiles, composites, presents. Target 120 Hz on capable displays.
- **Pool (`rayon`)**: filters, fills, export, thumbnail generation.
- **IO thread**: SQLite writes (autosave, save, tile eviction).

Tiles are `Arc<Tile>`, copy-on-write: a stroke clones only the tiles it touches. The undo entry keeps the old `Arc`s, which makes undo cheap and memory honest.

## 2. Schema

The full DDL is `replica/schema.sql`. It was checked by loading it into SQLite 3.53 and testing the singleton, ordering and cascade constraints.

**19 tables in the project file** plus **5 in the app library** (sub tools, materials, material tags, color sets, recent files).

How the usual rules were adapted:

| rule | how |
| --- | --- |
| uuid primary keys | UUIDv7 as `blob(16)` |
| created_at / updated_at | INTEGER Unix ms, UTC (SQLite has no `timestamptz`) |
| owner column | **n/a**: a file is owned by the OS user who opens it |
| access rules | **none in the data layer**: no multi-user access, OS file permissions only |
| on delete | explicit on every FK: cascade down the tree (page → layer → tile/stroke), `set null` where a link is optional (text → balloon, key → cel) |
| indexes | every FK, plus layer order, story order and stroke bbox for hit-testing |
| enums | `check (x in (...))` on every kind/status column |
| money | none |
| units | physical sizes in **micrometres (integer)** so mm ↔ inch ↔ pt never drifts |

Hard constraints the recon called for, enforced in the database rather than the UI:

- **One Work per file**: `work.singleton unique check (= 1)`.
- **Page order without collisions**: `unique (work_id, sort_key)` with fractional keys, so a drag-reorder is a one-row update (S18 / F09).
- **Layer order**: `unique (page_id, parent_id, sort_key)` plus a partial unique index for root layers (NULL parents slip past a normal unique).
- **One exposure per frame per track**: `primary key (track_id, frame)` on `track_key`.
- **An animation folder is on at most one track**: `track.layer_id unique`.
- **Trim size is all-or-nothing**: `check ((trim_w is null) = (trim_h is null))`.
- **Sparse tiles**: `primary key (layer_id, plane, tx, ty)` and `without rowid`. A missing row means a transparent tile.

Migrations: `PRAGMA user_version` + ordered migration functions in `ink-io`. Opening a newer file than the app understands is refused, read-only.

## 3. Command API

Every user action is a **command**: a struct that implements `apply(&mut Document) -> UndoEntry`. The UI, shortcuts and (later) scripting all go through it, so everything is undoable and testable without a window. These are the routes of this app.

Format: `command | what it does | called from | input | output | flow`

### Files and project

| command | what | from | input | output | flow |
| --- | --- | --- | --- | --- | --- |
| `NewWork` | create a work from a preset | S01, S02 | kind, size/dpi or trim+bleed, pages, binding | open document | F01, F02, F11, F12 |
| `OpenWork` | open `.inks`, migrate if needed, offer recovery | S01, OS | path | document / error | F14 |
| `SaveWork` / `SaveAs` | write dirty tiles + rows in one transaction | menu, Ctrl+S | path? | ok / error | all |
| `ChangeWorkSettings` | trim, bleed, inner border, DPI, numbering | S20 | settings | updated settings | F01 |
| `ResizeCanvas` | change canvas px, keep or rescale content | S20 | size, anchor, resample | — | F01 |
| `Export` | image, PSD, PDF, GIF, PNG sequence, MP4 | S24 | format, page range, scale, color mode, draft excluded | files / progress | F10, F11, F12 |
| `ImportImage` | place a PNG/JPEG/PSD as layer(s) | menu, drag-drop | path | layer ids | F12 |

### Canvas and view (not undoable, view state only)

| command | what | from | input | output | flow |
| --- | --- | --- | --- | --- | --- |
| `SetView` | pan, zoom, rotate, flip | S03, S09 | transform | — | all |
| `SetActivePage` | open page in a tab | S18 | page id | — | F09 |

### Drawing

| command | what | from | input | output | flow |
| --- | --- | --- | --- | --- | --- |
| `BeginStroke` / `StrokeSamples` / `EndStroke` | stream pen samples, stamp dabs live, commit as one undo step | S03 | sub tool, samples (x, y, pressure, tilt, time) | dirty tiles | F04, F12 |
| `VectorStroke` | same, stored as a stroke on a vector layer | S03 | sub tool, samples | stroke id | F04 |
| `VectorErase` | erase touched / up to intersection / whole line | S03 | mode, path | modified strokes | F04 |
| `Fill` | flood fill, close gaps, refer to reference layers | S03 | point, tolerance, gap, refs | dirty tiles | F06 |
| `Gradient` | draw gradient or create gradient layer | S03 | stops, shape, points | layer / tiles | F12 |
| `Transform` | free / mesh / puppet warp on selection or layer | S11 | mode, handles, interpolation | dirty tiles | F12 |
| `ApplyFilter` | blur, sharpen, noise, lines-and-tones | S16 | filter, params | dirty tiles | F12 |

### Layers

| command | what | from | input | output | flow |
| --- | --- | --- | --- | --- | --- |
| `AddLayer` | add any kind of layer | S07, launcher | kind, parent, position | layer id | all |
| `DeleteLayers` / `DuplicateLayers` / `MergeDown` / `Flatten` | stack edits | S07 | ids | ids | all |
| `MoveLayers` | reorder / reparent (fractional key) | S07 drag | ids, target | — | all |
| `SetLayerProps` | visibility, opacity, blend, lock, clip, reference, draft, expression color, tint | S07, S08 | id, props | — | F04, F12 |
| `AddMask` / `ApplyMask` / `DeleteMask` | layer masks | S07 | id | — | F05, F12 |

### Selection

| command | what | from | input | output | flow |
| --- | --- | --- | --- | --- | --- |
| `Select` | rect / ellipse / lasso / polyline, add/sub/intersect | S03 | shape, op | selection mask | F05 |
| `AutoSelect` | magic wand with reference layers | S03 | point, tolerance, refs | selection | F05, F06 |
| `InvertSelection` / `Deselect` / `GrowShrink` | | S10 | | | F05 |

### Manga

| command | what | from | input | output | flow |
| --- | --- | --- | --- | --- | --- |
| `CreateFrame` | rectangle / polyline frame folder | S12 | polygon, border width | layer id | F03 |
| `DivideFrame` | split a frame (with gutter) into two | S12 | frame id, line, gutter | 2 frame ids | F03 |
| `EditFrame` | move vertices / borders | S12 | frame id, geometry | — | F03 |
| `CreateTone` | tone layer from selection | S10, S14 | settings, selection | layer id | F05 |
| `SetTone` | change LPI / density / shape / angle | S14, S08 | layer id, settings | — | F05 |
| `AddRuler` / `EditRuler` / `RemoveRuler` | all ruler kinds, snap toggle | S15 | kind, params | ruler id | F08 |
| `AddText` / `EditText` | horizontal / vertical text | S13 | position, runs, style | text id | F07 |
| `AddBalloon` / `AddTail` / `EditBalloon` | balloons and tails, fit to text | S13 | shape, path, text id | ids | F07 |
| `EditStory` | batch edit text across pages | S19 | text id → runs | — | F07 |

### Book

| command | what | from | input | output | flow |
| --- | --- | --- | --- | --- | --- |
| `AddPages` / `DeletePages` / `MovePages` | page management | S18 | count/ids, position | ids | F02, F09 |
| `SetPageRole` / `SetSpread` | covers, double pages | S18 | id, role | — | F09 |

### Animation

| command | what | from | input | output | flow |
| --- | --- | --- | --- | --- | --- |
| `CreateTimeline` | timeline for the page | S21 | fps, length | timeline id | F11 |
| `AddAnimationFolder` | new track + folder | S21 | name | track id | F11 |
| `SetExposure` | assign cel / blank to frame, extend hold | S21 | track, frame, cel | — | F11 |
| `NewCelAtFrame` | create cel layer and expose it | S21 | track, frame | cel id | F11 |
| `SetPlayback` | fps, range, loop | S21 | params | — | F11 |
| `SetOnionSkin` | before/after count, colors, opacity | S23 | params | — | F11 |
| `EditLightTable` | register / remove / offset reference cels | S22 | scope, refs | — | F11 |

### Tools, library, app

| command | what | from | input | output | flow |
| --- | --- | --- | --- | --- | --- |
| `SelectTool` / `SelectSubTool` | change active tool | S04, keys | id | — | all |
| `EditSubTool` / `DuplicateSubTool` / `ResetSubTool` | presets in library | S05 | params | id | F13 |
| `PickColor` / `EditColorSet` | color state | S06 | color | — | all |
| `Undo` / `Redo` / `JumpHistory` | history | Ctrl+Z | — | — | all |
| `SetPreference` / `BindShortcut` / `SaveWorkspace` | app settings (TOML) | S26–S28 | key, value | — | F13 |

**About 60 commands.**

### Webhooks

None, because Inkstudio makes no network calls in v1. Update checks, if added later, should be opt-in.

### Background jobs

| job | schedule | what |
| --- | --- | --- |
| Autosave | every 3 min idle, and on focus loss | writes dirty tiles + rows to the open file's WAL; marks `autosave_state.dirty` |
| Tile eviction | when RAM over budget (default 50% of physical) | writes cold tiles of unopened pages to the file and drops them from memory |
| Thumbnail | 2 s after the last edit on a page | regenerates `page.thumbnail` for S18 / S01 |
| Export | on demand | `rayon` pool, cancelable, progress to S24 |
| Undo trim | after each command | caps history by memory (default 2 GB) and step count |
| Font scan | at startup, async | `fontdb` index for the text tool |

Only use official, public APIs: OS pen APIs, system font APIs, and open codecs. No Clip Studio code, file formats or endpoints.

## 4. The parts that bite

- **Pen input latency and accuracy.** Read samples on their own thread with OS timestamps. Never drop coalesced samples, and handle pen-in-proximity / eraser end / barrel buttons. Windows Ink and WinTab report different pressure curves, so calibrate in S26.
- **HiDPI and canvas math.** Keep three coordinate spaces (screen px, canvas px, physical µm) as separate types in `ink-core`. Mixing them is the classic bug.
- **Memory.** One B4 page at 600 dpi is about 6,000 × 8,600 px. At 4 B/px that's 200 MB per full layer. Sparse tiles, 1-bit/8-bit layer depths, lazy page loading and eviction are required.
- **Undo races.** The stroke worker and UI both touch the document. All mutations go through the command queue in one place, and readers get immutable snapshots (`Arc` tiles).
- **Crash-safe saving.** SQLite transaction + WAL means a crash never leaves a half-written file. "Save" checkpoints, autosave doesn't. Test by killing the process mid-save in CI.
- **File format versioning.** `user_version` migrations from day one. Never rename an enum string. Add new ones instead.
- **Preview ≠ export.** The GPU and CPU blend paths must match within ±1 per channel. Golden-image tests compare both.
- **Screentone moiré.** At screen zoom, draw tones as gray (density) below a threshold, or render dots at device resolution. The export rasterizes at the work DPI to exact 1-bit.
- **Gamma.** Blending in sRGB (like most paint apps) vs linear light gives different results. Pick sRGB as the default for artist expectations and test blend modes against known formulas.
- **Vertical Japanese text and IME.** Preedit display inside vertical text boxes, rotated Latin, punctuation positioning and ruby need custom layout. Test with Japanese IMEs on all three OSes.
- **Fonts on another machine.** Record used fonts in `resource` (`font_ref`). Warn when one is missing, and offer "outline text" before export.
- **GPU differences and device loss.** wgpu backends differ, so handle surface loss and recreate. Keep a CPU fallback view for very old GPUs.
- **Linux.** Wayland vs X11 pen stacks are different code paths. Ship X11 + Wayland support, and test on GNOME and KDE.
- **Video codecs.** H.264 patents: Cisco's OpenH264 binary covers the licence only when downloaded at runtime. Until that's sorted, ship GIF/PNG sequence + AV1/WebM (`rav1e`) or let users point at their own ffmpeg.
- **Large files.** Keep each page's tiles in rows so a 200-page book doesn't load at once. SQLite handles files of many GB fine.
- **Not applicable here:** time zones, webhooks, rate limits, multi-tenancy, email, GDPR (no data leaves the machine). If cloud sync is ever added, revisit all of these.

## 5. Build order

Each milestone lists its screens, tables and commands. The recon's phases map onto M1–M5.

### M0: spikes (1–2 weeks)
Prove the three riskiest choices before building on them.
- winit + wgpu window with a 10k×10k tiled canvas that pans and zooms at 120 fps.
- Pen pressure + tilt on Windows, macOS and Linux (Wayland), with octotablet vs own code compared.
- egui + egui_dock: one docked palette, one floating, plus a custom canvas callback.
- Exit criteria: pressure works on all three OSes and drawing latency is under 20 ms.

### M1: vertical slice (ugly, end to end)
New canvas → draw with pressure → layers → save → reopen → export PNG.
- Screens: S02 (minimal), S03, S04 (2 tools), S07 (basic).
- Tables: `meta`, `work`, `work_settings`, `page`, `layer`, `tile`.
- Commands: `NewWork`, `OpenWork`, `SaveWork`, `BeginStroke…EndStroke`, `AddLayer`, `SetLayerProps`, `Undo/Redo`, `Export (PNG)`.

### M2: ink core (must-haves: input, brush, layers, select, fill, transform, color, file)
- Screens: S01, S03–S11, S26, S29.
- Tables: + `autosave_state`, library `sub_tool`, `color_set`, `recent_file`.
- Commands: all drawing, layer, selection and tool commands, `Transform` (free), `Fill`, `AutoSelect`, autosave job, tile eviction.
- Exit criteria: a full illustration (F12) can be made and exported to PNG and PSD.

### M3: manga page (must-haves: vector, rulers, manga, text)
- Screens: S12–S15, S20, S24 (1-bit / PDF).
- Tables: `vector_stroke`, `tone_settings`, `frame_panel`, `ruler`, `text_object`, `balloon`, `balloon_tail`, `resource`; library `material`, `material_tag` (our own tones).
- Commands: `VectorStroke`, `VectorErase`, `CreateFrame`, `DivideFrame`, `CreateTone`, `SetTone`, rulers, text and balloons, `ChangeWorkSettings`.
- Exit criteria: F01, F03–F08 complete on one page, exported as 1-bit PNG and PDF.

### M4: manga book (must-haves: book)
- Screens: S18, S19, S24 (multi-page).
- Tables: `page` ordering, roles and spreads in use.
- Commands: `AddPages`, `DeletePages`, `MovePages`, `SetPageRole`, `EditStory`, batch `Export`.
- Exit criteria: F02, F09, F10 with right-to-left binding.

### M5: animation (must-haves: animation)
- Screens: S21–S23, S24 (GIF / PNG sequence).
- Tables: `timeline`, `track`, `track_key`, `light_table_entry`.
- Commands: all animation commands, playback clock.
- Exit criteria: F11 exported as GIF and PNG sequence.

### M6: should-haves, then could-haves
Mesh transform, correction layers, gradient tones, symmetry ruler, spreads, page numbering, MP4/WebM, workspaces, shortcuts editor (S27, S28), material palette (S25), themes. Then could-haves: puppet warp, liquify, CMYK proof, watercolor brushes, camera keys, audio, story editor polish.

### M7: fixes
Whatever replica-entrepreneur (or real users) find once M1–M5 ship.

## Summary

- **Stack:** Rust + winit + wgpu + egui/egui_dock, CPU tile engine (rayon + SIMD), SQLite `.inks` project files, cosmic-text/rustybuzz with custom vertical layout.
- **Tables:** 19 in the project file + 5 in the app library.
- **Commands (routes):** about 60, plus 6 background jobs, no webhooks.
- **Riskiest:** cross-platform pen pressure in native Rust, interactive compositing/undo at print resolution, and a pro dockable UI (with vertical CJK text) without a webview.
- **Next step:** `/replica-design`.
