-- Inkstudio project file schema (SQLite).
-- One .inks file = one Work (illustration, comic book or animation).
-- Opened with: PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;
--
-- Conventions (adapted from server-side rules for a local, single-user file):
--   * ids are UUIDv7 stored as 16-byte BLOBs (time-ordered, fast to index).
--   * created_at / updated_at are INTEGER Unix milliseconds, always UTC.
--   * no owner column / no row-level security: the file belongs to whoever can
--     open it; access is the OS's file permissions. Authorisation is n/a.
--   * every ON DELETE rule is explicit.
--   * ordering uses a fractional text key (sort_key, LexoRank-style) so a
--     reorder is a single-row update and uniqueness can be enforced without
--     deferrable constraints (SQLite lacks deferrable UNIQUE).
--   * enum-like columns are CHECK-constrained TEXT.
--   * physical sizes are stored in micrometres (INTEGER) to avoid float drift
--     between mm / inch / pt UI units.
--   * opaque per-kind parameters live in a versioned `params` JSON column.

PRAGMA user_version = 1;

-- ---------------------------------------------------------------------------
-- File metadata
-- ---------------------------------------------------------------------------
create table meta (
  key   text primary key,
  value text not null
) strict;
-- rows: format = 'inkstudio', format_version = '1', app_version, created_by_os

-- ---------------------------------------------------------------------------
-- Work and its settings (S02, S20)
-- ---------------------------------------------------------------------------
create table work (
  id            blob primary key check (length(id) = 16),
  singleton     integer not null default 1 unique check (singleton = 1),  -- exactly one row per file
  title         text not null default 'Untitled',
  kind          text not null check (kind in ('illustration','comic','animation')),
  binding_side  text not null default 'right' check (binding_side in ('left','right')),
  start_side    text not null default 'left'  check (start_side in ('left','right')),
  has_cover     integer not null default 0 check (has_cover in (0,1)),
  created_at    integer not null,
  updated_at    integer not null
) strict;

create table work_settings (
  work_id            blob primary key references work(id) on delete cascade,
  dpi                integer not null check (dpi between 72 and 2400),
  canvas_w_px        integer not null check (canvas_w_px between 1 and 30000),
  canvas_h_px        integer not null check (canvas_h_px between 1 and 30000),
  trim_w_um          integer check (trim_w_um > 0),          -- null = no comic settings
  trim_h_um          integer check (trim_h_um > 0),
  bleed_um           integer not null default 0 check (bleed_um >= 0),
  inner_border_json  text check (inner_border_json is null or json_valid(inner_border_json)),
  expression_color   text not null default 'color' check (expression_color in ('color','gray','mono')),
  paper_color        integer not null default 4294967295,   -- 0xAARRGGBB
  page_numbering_json text check (page_numbering_json is null or json_valid(page_numbering_json)),
  updated_at         integer not null,
  constraint trim_pair check ((trim_w_um is null) = (trim_h_um is null))
) strict;

-- ---------------------------------------------------------------------------
-- Pages (S18). An illustration or animation work has exactly one page.
-- ---------------------------------------------------------------------------
create table page (
  id          blob primary key check (length(id) = 16),
  work_id     blob not null references work(id) on delete cascade,
  sort_key    text not null,
  role        text not null default 'body' check (role in ('body','cover_front','cover_back','inside_front','inside_back')),
  is_spread   integer not null default 0 check (is_spread in (0,1)),  -- double-width page
  thumbnail   blob,                                                     -- small PNG
  created_at  integer not null,
  updated_at  integer not null,
  unique (work_id, sort_key)
) strict;
create index page_work on page (work_id);

-- ---------------------------------------------------------------------------
-- Layers (S07, S08). A tree per page; folders, frame folders and animation
-- folders are layers with children.
-- ---------------------------------------------------------------------------
create table layer (
  id              blob primary key check (length(id) = 16),
  page_id         blob not null references page(id) on delete cascade,
  parent_id       blob references layer(id) on delete cascade,  -- null = page root
  sort_key        text not null,
  name            text not null,
  kind            text not null check (kind in (
                    'raster','vector','tone','fill','gradient','text','balloon',
                    'folder','frame_folder','animation_folder','correction','paper','image_ref')),
  visible         integer not null default 1 check (visible in (0,1)),
  opacity         integer not null default 255 check (opacity between 0 and 255),
  blend_mode      text not null default 'normal' check (blend_mode in (
                    'normal','darken','multiply','color_burn','linear_burn','subtract',
                    'darker_color','lighten','screen','color_dodge','glow_dodge','add',
                    'add_glow','lighter_color','overlay','soft_light','hard_light',
                    'vivid_light','linear_light','pin_light','hard_mix','difference',
                    'exclusion','hue','saturation','color','luminosity','divide','pass_through')),
  lock            text not null default 'none' check (lock in ('none','alpha','all')),
  clip_below      integer not null default 0 check (clip_below in (0,1)),
  is_reference    integer not null default 0 check (is_reference in (0,1)),
  is_draft        integer not null default 0 check (is_draft in (0,1)),
  expression_color text check (expression_color in ('color','gray','mono')), -- null = inherit
  mono_black      integer,                                   -- 0xAARRGGBB, mono layers
  mono_white      integer,
  layer_tint      integer,                                   -- "layer color", null = off
  has_mask        integer not null default 0 check (has_mask in (0,1)),
  mask_enabled    integer not null default 1 check (mask_enabled in (0,1)),
  offset_x        integer not null default 0,
  offset_y        integer not null default 0,
  params          text not null default '{}' check (json_valid(params)),  -- per-kind: border effect, correction curve, gradient stops, fill color
  created_at      integer not null,
  updated_at      integer not null,
  unique (page_id, parent_id, sort_key)
) strict;
-- UNIQUE treats NULL parents as distinct, so enforce root ordering separately
create unique index layer_root_order on layer (page_id, sort_key) where parent_id is null;
create index layer_page   on layer (page_id);
create index layer_parent on layer (parent_id);

-- Pixel data: sparse 256x256 tiles, zstd-compressed. Absent tile = transparent.
-- plane 'color' holds RGBA8 premultiplied (or Gray8 / 1-bit by layer depth);
-- plane 'mask' holds the layer mask (Gray8); 'tone_area' holds a tone layer's coverage.
create table tile (
  layer_id   blob not null references layer(id) on delete cascade,
  plane      text not null check (plane in ('color','mask','tone_area')),
  tx         integer not null,
  ty         integer not null,
  format     text not null check (format in ('rgba8','gray8','bit1')),
  codec      text not null default 'zstd' check (codec in ('zstd','raw','solid')),
  data       blob not null,
  primary key (layer_id, plane, tx, ty)
) strict, without rowid;

-- ---------------------------------------------------------------------------
-- Vector strokes (vector layers, and balloon/frame outlines drawn as strokes)
-- ---------------------------------------------------------------------------
create table vector_stroke (
  id          blob primary key check (length(id) = 16),
  layer_id    blob not null references layer(id) on delete cascade,
  sort_key    text not null,
  brush_json  text not null check (json_valid(brush_json)),   -- frozen brush params at draw time
  color       integer not null,
  closed      integer not null default 0 check (closed in (0,1)),
  points      blob not null,    -- packed f32: x, y, pressure, width_scale, tilt_x, tilt_y
  bbox_x0     real not null, bbox_y0 real not null,
  bbox_x1     real not null, bbox_y1 real not null,
  unique (layer_id, sort_key)
) strict;
create index vector_stroke_layer on vector_stroke (layer_id);
create index vector_stroke_bbox  on vector_stroke (layer_id, bbox_x0, bbox_y0);

-- ---------------------------------------------------------------------------
-- Tone settings (S14). One per tone layer.
-- ---------------------------------------------------------------------------
create table tone_settings (
  layer_id    blob primary key references layer(id) on delete cascade,
  lpi         real not null check (lpi between 5 and 300),
  density     real not null check (density between 0 and 1),   -- 1 = solid
  dot_shape   text not null default 'circle' check (dot_shape in ('circle','square','diamond','ellipse','line','cross','noise')),
  angle_deg   real not null default 45,
  anchor      text not null default 'layer' check (anchor in ('layer','canvas')),
  use_image_density integer not null default 0 check (use_image_density in (0,1)),  -- gradient / gray -> tone
  params      text not null default '{}' check (json_valid(params))
) strict;

-- ---------------------------------------------------------------------------
-- Frame panels (S12). The geometry of a frame_folder layer.
-- ---------------------------------------------------------------------------
create table frame_panel (
  layer_id      blob primary key references layer(id) on delete cascade,
  polygon       blob not null,           -- packed f32 x,y in canvas px
  border_width  real not null default 6 check (border_width >= 0),
  border_color  integer not null default 4278190080,
  mask_contents integer not null default 1 check (mask_contents in (0,1))
) strict;

-- ---------------------------------------------------------------------------
-- Text and balloons (S13, S19)
-- ---------------------------------------------------------------------------
create table text_object (
  id          blob primary key check (length(id) = 16),
  layer_id    blob not null references layer(id) on delete cascade,
  story_order integer not null,          -- reading order on the page for the story editor
  x           real not null, y real not null,
  rotation    real not null default 0,
  direction   text not null default 'vertical' check (direction in ('horizontal','vertical')),
  font_family text not null,
  font_size_pt real not null check (font_size_pt > 0),
  leading     real not null default 1.0,
  align       text not null default 'start' check (align in ('start','center','end','justify')),
  color       integer not null default 4278190080,
  runs_json   text not null check (json_valid(runs_json)),   -- [{text, bold, ruby, size}]
  balloon_id  blob references balloon(id) on delete set null,
  updated_at  integer not null
) strict;
create index text_object_layer   on text_object (layer_id, story_order);
create index text_object_balloon on text_object (balloon_id);

create table balloon (
  id          blob primary key check (length(id) = 16),
  layer_id    blob not null references layer(id) on delete cascade,
  shape       text not null check (shape in ('ellipse','rect','rounded','curve','flash','thought','polyline')),
  outline     blob not null,             -- packed f32 path
  line_width  real not null default 3,
  line_color  integer not null default 4278190080,
  fill_color  integer not null default 4294967295,
  fit_text    integer not null default 1 check (fit_text in (0,1)),
  updated_at  integer not null
) strict;
create index balloon_layer on balloon (layer_id);

create table balloon_tail (
  id          blob primary key check (length(id) = 16),
  balloon_id  blob not null references balloon(id) on delete cascade,
  style       text not null default 'straight' check (style in ('straight','curve','thought','none')),
  points      blob not null,
  base_width  real not null default 12
) strict;
create index balloon_tail_balloon on balloon_tail (balloon_id);

-- ---------------------------------------------------------------------------
-- Rulers (S15). Attached to a layer, or to the page when layer_id is null.
-- ---------------------------------------------------------------------------
create table ruler (
  id           blob primary key check (length(id) = 16),
  page_id      blob not null references page(id) on delete cascade,
  layer_id     blob references layer(id) on delete cascade,
  kind         text not null check (kind in ('straight','curve','figure','symmetry','perspective','concentric','parallel','focus','guide','grid')),
  params       text not null check (json_valid(params)),  -- vanishing points, line count, centre, etc.
  snap         integer not null default 1 check (snap in (0,1)),
  all_layers   integer not null default 0 check (all_layers in (0,1)),
  visible      integer not null default 1 check (visible in (0,1))
) strict;
create index ruler_page  on ruler (page_id);
create index ruler_layer on ruler (layer_id);

-- ---------------------------------------------------------------------------
-- Animation (S21, S22, S23). A timeline belongs to a page (animation works
-- have one page). Tracks are animation folders; keys point at cels (child
-- layers of the folder). A null cel_id is a blank exposure.
-- ---------------------------------------------------------------------------
create table timeline (
  id           blob primary key check (length(id) = 16),
  page_id      blob not null unique references page(id) on delete cascade,
  fps          real not null default 24 check (fps > 0 and fps <= 120),
  start_frame  integer not null default 1,
  end_frame    integer not null check (end_frame >= start_frame),
  play_start   integer, play_end integer,
  onion_json   text not null default '{"before":1,"after":1,"before_color":4294901760,"after_color":4278255360,"opacity":0.35}' check (json_valid(onion_json)),
  updated_at   integer not null
) strict;

create table track (
  id           blob primary key check (length(id) = 16),
  timeline_id  blob not null references timeline(id) on delete cascade,
  layer_id     blob not null unique references layer(id) on delete cascade,  -- the animation_folder
  kind         text not null default 'cels' check (kind in ('cels','camera','audio')),
  sort_key     text not null,
  unique (timeline_id, sort_key)
) strict;
create index track_timeline on track (timeline_id);

create table track_key (
  track_id  blob not null references track(id) on delete cascade,
  frame     integer not null,
  cel_id    blob references layer(id) on delete set null,   -- null = blank
  params    text check (params is null or json_valid(params)), -- camera transform, easing
  primary key (track_id, frame)
) strict, without rowid;
create index track_key_cel on track_key (cel_id);

create table light_table_entry (
  id          blob primary key check (length(id) = 16),
  scope_layer blob not null references layer(id) on delete cascade, -- a cel, or the animation folder for canvas scope
  ref_layer   blob not null references layer(id) on delete cascade,
  sort_key    text not null,
  opacity     integer not null default 128 check (opacity between 0 and 255),
  offset_x    real not null default 0,
  offset_y    real not null default 0,
  unique (scope_layer, sort_key)
) strict;
create index light_table_ref on light_table_entry (ref_layer);

-- ---------------------------------------------------------------------------
-- Embedded resources: images placed by reference, fonts recorded for warnings
-- ---------------------------------------------------------------------------
create table resource (
  id          blob primary key check (length(id) = 16),
  kind        text not null check (kind in ('image','font_ref','material_snapshot')),
  name        text not null,
  sha256      blob not null check (length(sha256) = 32),
  data        blob,                          -- null for font_ref
  created_at  integer not null,
  unique (sha256)
) strict;

-- ---------------------------------------------------------------------------
-- Crash recovery journal (S29). Autosave writes here; a clean save empties it.
-- ---------------------------------------------------------------------------
create table autosave_state (
  id          integer primary key check (id = 1),
  dirty       integer not null check (dirty in (0,1)),
  saved_at    integer not null,
  undo_head   integer not null default 0
) strict;


-- ===========================================================================
-- App library (separate file: <config dir>/inkstudio/library.sqlite)
-- Holds user-wide things that are not part of any Work.
-- ===========================================================================
-- create table sub_tool (
--   id          blob primary key, tool_group text not null check (tool_group in
--               ('pen','pencil','brush','airbrush','decoration','eraser','fill','gradient',
--                'select','auto_select','move','transform','eyedropper','text','balloon',
--                'frame','ruler','figure','correct_line','blend','hand','zoom')),
--   sort_key text not null, name text not null,
--   params text not null check (json_valid(params)),   -- tip, pressure curves, stabilization
--   icon blob, is_builtin integer not null default 0, created_at integer, updated_at integer,
--   unique (tool_group, sort_key));
-- create table material (id blob primary key, kind text not null check (kind in
--   ('tone','brush_tip','texture','image','template','balloon','color_set')),
--   folder text not null, name text not null, data blob not null, created_at integer);
-- create table material_tag (material_id blob references material(id) on delete cascade,
--   tag text not null, primary key (material_id, tag)) without rowid;
-- create index material_tag_tag on material_tag (tag);
-- create table color_set (id blob primary key, name text not null, colors blob not null);
-- create table recent_file (path text primary key, opened_at integer not null, thumbnail blob);
-- create index recent_file_opened on recent_file (opened_at desc);
-- Preferences, shortcuts and workspace layouts are TOML files next to it
-- (human-editable, diff-friendly).
