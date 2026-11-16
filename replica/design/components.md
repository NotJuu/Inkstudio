# Inkstudio components

Specs for every repeated UI part in `replica/recon.md` §5. Tokens are in `tokens.json`, generated into `crates/ink-ui/src/theme/tokens.rs`, and the code is in `crates/ink-ui/src/widgets/`. `gallery-*.png` shows every primitive, and `workspace-*.png` shows them assembled into the main workspace.

## Look and feel

Inkstudio should feel like a **real professional drawing program**, in the same family as Krita and Clip Studio Paint. `workspace-dark.png` / `workspace-light.png` show the target: the main workspace (S03), built only from these primitives.

What we take from the genre (conventions every pro paint app shares, not anyone's trade dress):
- **Neutral greys with no hue.** Panels sit between `#222` and `#464646`, so the UI never tints how you judge color. The canvas surround is a mid grey.
- **Dense docking.** A menu bar, a tool-options bar for the active tool, a single tool column, docked palettes on both sides (sub tools and tool properties on the left; color, navigator and layers on the right) and a status bar. Every palette has a slim title strip with a ⋯ menu.
- **Dark 1px separators** between panels instead of light outlines or cards.
- **Near-square corners** (2–3px), small 12px type and 24px rows.
- **The canvas is the hero.** The page sits on the grey surround with a shadow, trim/bleed/safe-area guides and crop marks. Everything else is quiet.

What stays Inkstudio's own: every color value, the Phosphor icons, the labels, the exact proportions, and the accent color (still a placeholder until replica-brand). Nothing is copied from Krita (GPL) or Clip Studio Paint, not even screenshots. The resemblance comes only from the shared conventions above.

## How this was made

- **System, not skin.** These are the roles, scales and behaviours common to pro paint and manga tools (dense dockable palettes, 24px controls, layer rows with thumbnails). Nothing is traced from Clip Studio Paint: no screenshots were used, and every color, label and icon is Inkstudio's own.
- **Icons:** [Phosphor](https://phosphoricons.com) Regular (MIT) via `egui-phosphor`. Icons are always drawn with `theme::icon_font()` and never mixed into a text string (see "Font notes").
- **Fonts:** Inter (OFL) for UI, JetBrains Mono (OFL) for numbers in code views, Noto Sans/Serif CJK JP (OFL) for Japanese UI text and default lettering. All three are open fonts, and the app bundles them under `assets/fonts/` (to add, with license files).
- **Accent:** `#3565E6` / `#2F5FD6` is a **placeholder**. replica-brand replaces it, along with `accent-hover`, `accent-subtle`, `accent-text`, `accent-indicator` and `focus-ring`.

## Token summary

| group | values |
| --- | --- |
| Greys (dark) | bg `#222222`, surface `#2C2C2C`, raised `#363636`, sunken `#1C1C1C`, hover `#3A3A3A`, active `#464646`, track `#4A4A4A`; separators `#161616`. Neutral, no hue |
| Greys (light) | bg `#C6C6C6`, surface `#DADADA`, raised `#EAEAEA`, sunken `#F6F6F6`; separators `#A6A6A6` |
| Canvas surround | dark `#484848`, light `#A8A8A8`. Mid grey, so it doesn't bias color judgement |
| Type | 11 / 12 / 13 / 15 / 20 / 28 px. Body is **12px**, typical for dense pro tools. Weights 400 / 500 / 600 |
| Space | 0, 2, 4, 6, 8, 12, 16, 24, 32 (4px base, 2px half-step) |
| Controls | 20 / 24 / 28 / 32 px high; icons 14 / 16 / 20 |
| Radius | 2 / 2 / 3 (near-square, pro-tool style) |
| Shadow | popup (0 4 12 35%), floating (0 8 24 45%) |
| Motion | 80 ms / 140 ms; 0 when the OS asks for reduced motion |
| Layout | menu bar 24, command bar 32, status bar 22, tool column 40, palette 260 (180–480), layer row 40 with 32px thumbnail, list row 24, timeline cell 16×22, handle 8 (+4 hit slop) |

## Contrast

`tools/contrast.py` checks 68 text/background pairs (34 roles in 2 themes): **0 failures**. The full table is in `contrast-report.md`. Thresholds are 4.5:1 for body text, and 3:1 for large text, input borders, focus rings and indicators.

Fixes made in the tokens while checking (the second round came after switching to neutral greys: dark `text-muted`, `border-input` and `overlay-ruler` were lightened):
- dark `accent` darkened (white text was 4.49:1);
- dark `accent-hover` changed to go darker on hover instead of lighter;
- dark `border-input` lightened (was 2.82:1 on raised surfaces);
- light `canvas-surround` and `overlay-ruler` adjusted so focus rings and rulers reach 3:1;
- new role **`accent-indicator`**: thin accent marks (active tab underline, selected tool bar, clipping bar) need a lighter blue than solid accent fills on dark panels;
- new role **`track`**: slider rails disappeared in light dialogs, where the sunken and raised surfaces are both white.

## Shared rules (all widgets)

- Colors come from `theme::palette(ui)` and sizes from `tokens::*`. **No raw hex or pixel values in widgets.**
- **Focus.** Every interactive widget is a Tab stop (Shift+Tab goes back) and draws a 2px `focus-ring` outline when focused. Space/Enter activates.
- **Screen readers.** Every widget reports its role, name, value and enabled/selected state to AccessKit through `WidgetInfo`. Icon-only widgets *require* a label, which is also the tooltip.
- **Not color alone.** State is never shown by color only: selected tools also get a bar, toast kinds have icons, swatches announce their hex value, and hidden layers change their icon.
- **Disabled.** Shown at reduced emphasis (`text-disabled`, fill at 50%), not focusable, and announced as disabled.
- **Hit areas.** At least 20px for palette controls. Canvas handles are drawn at 8px and hit at 16px.
- **Motion.** Hover and press changes are instant. Only popups and docking animate (`motion.base`), and nothing animates under reduced motion.

## Components

Status: **built** = in `crates/ink-ui/src/widgets` and the gallery; **egui** = an egui built-in themed by `theme::style`; **shell** = built with the app shell (M1/M2), spec only for now.

```
Button                                                              built
  variants  primary, secondary, ghost, danger
  sizes     sm 24px, md 28px, lg 32px; padding 6 / 8 / 12; min width = height
  content   optional leading icon (16px, icon font) + label (body-strong 12/500)
  states    default, hover, pressed, focus-visible (2px focus-ring), disabled, loading (spinner, no clicks)
  tokens    primary bg accent → accent-hover, text on-accent; secondary bg surface-raised, border border-strong;
            ghost transparent → surface-hover; danger bg danger-solid, text on-danger; radius md
  a11y      role button; name = label; loading appends ", working"; Space/Enter activates
  used on   S01, S02, S10, S14, S16, S17, S20, S23, S24, S29
```

```
IconButton (tool column, command bar, row toggles)                  built
  variants  action (button) | toggle (selected: bool)
  sizes     32 (tool column / command bar), 24 (rows, fields), 20 (palette headers)
  states    default, hover (surface-hover), pressed (surface-active), selected (accent-subtle fill,
            2px accent-indicator bar on the left, icon accent-text), focus-visible, disabled
  a11y      label required: tooltip + accessible name; toggle reports checked state
  used on   S03, S04, S07 (eye, lock), S05 (pressure), S21
```

```
SliderField                                                         built
  parts     label (72px, text-muted) · slider (rail = track) · numeric field with suffix · optional pressure toggle
  states    default, dragging, typing, focus (each part focusable), disabled; value always clamped
  keyboard  arrows step the slider; the numeric field accepts typing, Enter commits, Esc reverts
  a11y      slider and field share the label; pressure toggle "Pen pressure on/off"
  used on   S05, S08, S14, S16, S17, S23
```

```
UnitField                                                           built
  parts     caption label above · numeric field with unit suffix (px / mm / in / pt) · error line
  model     value stored in micrometres; display = value / unit size (px uses the work's DPI)
  states    default, focus, invalid (danger message under the field), disabled
  a11y      label + unit in the name; error text announced
  used on   S02, S20, S24
```

```
Dropdown (blend mode, presets, formats)                             egui ComboBox, themed
  states    closed, hover, open (surface-active), item hover, item selected, disabled
  keyboard  Space/Enter opens, arrows move, Enter picks, Esc closes
  used on   S02, S07, S11, S14, S24
```

```
Checkbox / text field                                               egui, themed
  tokens    box/rail = track, border border-input, tick = text; text field bg surface-sunken
  states    default, hover, focus (focus-ring), checked, disabled, invalid (danger stroke + message)
  used on   S02, S05, S16, S20, S24, S26
```

```
TabStrip                                                            built
  states    unselected (text-muted), hover (surface-hover), selected (text + 2px accent-indicator underline), focus
  keyboard  each tab is a Tab stop; ←/→ move the selection while focused
  a11y      selectable labels with selected state
  used on   S06, S20, S24, S26
```

```
PaletteHeader (dockable palette chrome)                             built (header) + shell (docking)
  parts     title (body-strong) · palette menu button (… icon)
  docking   egui_dock: docked, tabbed, floating, collapsed to icon; splitter 4px; drop targets highlight
            with accent-subtle; floating palettes use the floating shadow
  used on   S04–S09, S21, S22, S25
```

```
ListRow (sub tools, materials, recent files)                        built
  size      24px high, icon 16 + label body
  states    default, hover, selected (accent-subtle), focus, drag source / drop target (shell)
  a11y      selectable label
  used on   S01, S04, S25, S27
```

```
LayerRow                                                            built
  parts     eye toggle · lock toggle · indent (12px per depth) · clipping bar (accent-indicator) ·
            thumbnail 32 (white paper + border) · mask thumbnail · name (body-strong) ·
            meta line "blend · opacity% · ref · draft" (caption, text-muted) · kind icon
  states    default, hover, selected, hidden (eye-slash + muted name), locked, clipped, masked,
            reference, draft, focus, renaming (F2 / double-click → inline text field, shell),
            drag-hover above/inside/below (shell)
  a11y      name "Ink, vector layer, hidden, locked, clipped to layer below, Multiply 40%";
            eye and lock are separate Tab stops with their own labels
  used on   S07
```

```
ColorSwatch / ColorPair                                             built
  swatch    20px (sets) / 44px (main-sub pair); transparent shows a checkerboard
  states    default, selected (2px focus-ring border), focus
  a11y      name = "#RRGGBB, NN% opacity" so color is never conveyed by color alone
  pair      main over sub, swap button "Swap main and sub color"
  used on   S06, S14, S23
```

```
ColorWheel                                                          built
  parts     HSV hue ring + saturation/value square, dual-ring markers (visible on any color)
  keyboard  ←/→ hue, ↑/↓ value, Shift+↑/↓ saturation
  a11y      "Color wheel, hue 223°, saturation 35%, value 12%, #141720"
  used on   S06
```

```
Color sliders / hex field                                           shell (M2)
  parts     RGB/HSV sliders (SliderField), hex field
  a11y      each channel is a slider with a numeric value; the wheel is pointer-only, with the sliders as the keyboard path
  used on   S06
```

```
DialogFrame (modal)                                                 built (panel) + shell (scrim, centring)
  parts     title (15/600) · body · footer: Cancel + primary OK, right-aligned, 80px min width
  widths    sm 360 · md 480 · lg 640
  tokens    bg surface-raised, border border-strong, radius lg, shadow floating, scrim behind
  keyboard  focus starts on the first field and is trapped while open; Enter = OK (if enabled); Esc = Cancel
  states    default, OK disabled (invalid input), working (OK → loading, inputs disabled)
  used on   S02, S14, S16, S17, S20, S23, S24, S26, S27, S29
```

```
ProgressBar                                                         built
  parts     label + percent · 6px bar (accent on track) · optional Cancel
  states    determinate, indeterminate (animated), cancelled, failed (switches to an error toast)
  a11y      "Exporting page 7 of 24, 29%"
  used on   S16, S24
```

```
Toast                                                               built
  kinds     info, success, warning, error: icon + color, never color alone
  parts     icon · message · optional ghost action (Retry, Undo, Replace)
  behaviour bottom-centre, stacks upward, 4s auto-dismiss except errors; hover pauses the timer;
            announced as a polite live region (errors: assertive)
  used on   S03
```

```
TimelineTrack                                                       built
  parts     name column 120px · frame cells 16×22, alternating sunken/bg · cel start (number, accent-subtle) ·
            hold (bar + line) · blank · current-frame outline (2px focus-ring)
  keyboard  ←/→ change frame, Space play/pause, Enter new cel at frame (shell)
  a11y      "Track Character, frame 5"; cel number read on frame change
  used on   S21
```

```
Canvas overlays                                                     built (line, handle)
  line      dual stroke: 3px black 55% under a 1px color line → visible on black and white art
  handle    8px white square, 1px dark border; hot = focus-ring fill; hit area 16px
  colors    overlay-ruler (purple), overlay-frame-preview (cyan), overlay-trim (pink), overlay-safe (teal),
            selection marching ants (black/white dashes), onion-before (red), onion-after (green)
  a11y      overlays are pointer-first; each overlay tool also has numeric fields in its Tool property palette
  used on   S11, S12, S13, S15, S22
```

```
Menu bar / context menus                                            egui menus, themed
  tokens    bg surface-raised, hover surface-hover, radius md, popup shadow, shortcuts right-aligned in text-muted
  states    hover, disabled, checked (check icon), submenu open
  keyboard  Alt opens the menu bar on Windows/Linux; arrows navigate; Esc closes
  used on   everywhere
```

```
Thumbnail grid (pages, materials, recent files)                     shell (M2/M4)
  page tile 96px wide, real page aspect; spread pairs side by side, ordered right-to-left for manga
  states    default, hover, selected (2px focus-ring), open+unsaved (dot badge), loading (track fill)
  a11y      "Page 7, left page, unsaved"
  used on   S01, S18, S25
```

```
KeyCapture field                                                    shell (M6)
  states    idle, listening ("Press keys…"), conflict (warning text naming the other command)
  used on   S27
```

```
Brush cursor                                                        shell (M2)
  variants  brush outline (dual stroke), crosshair, dot; outline hidden below 4px screen size
  used on   S03
```

## Font notes

- Phosphor is **not** in the text fallback chain. Its font has empty glyphs for Latin letters, and Inter has its own glyphs in the Private Use Area where Phosphor keeps its icons. Putting either first breaks the other: the first render showed icons as letters, the second showed text as icons. Always draw icons with `theme::icon_font(size)`.
- Inter is installed in 3 weights (Regular, Medium, SemiBold) as separate egui families, because egui can't synthesize weights.
- egui can't turn on tabular figures, so numeric fields align right instead.

## Tooling

| command | what |
| --- | --- |
| `python3 -I replica/design/tools/gen_theme.py` | regenerate `tokens.rs` after editing `tokens.json` (`--check` for CI) |
| `python3 -I replica/design/tools/contrast.py --md replica/design/contrast-report.md` | WCAG check of every pair; exits 1 on failure |
| `cargo run --release -p ink-ui --example design_gallery -- replica/design 2` | render `gallery-*.png` and `workspace-*.png` on the CPU (no GPU needed) |

## Not done yet

- **No live window.** The gallery is rendered headless, so hover, drag and keyboard states are shown as static examples only. An interactive gallery window comes with the app shell (winit + wgpu) in M0/M1.
- **Static spinner.** In the screenshot the loading spinner is a still frame (a dot).
- **AccessKit untested.** Labels are set, but nothing has been tested with a real screen reader. Test with Narrator, VoiceOver and Orca once the shell exists.
- **Fonts not bundled.** `assets/fonts/` with OFL license files still needs adding. The gallery currently uses system fonts.
