//! Inkstudio project files (`.inks`).
//!
//! One SQLite file per work:
//!
//! ```text
//! meta(key, value)                    format, format_version, app_version
//! work(id = 1, json)                  app-defined settings (sheet, book)
//! page(idx, width, height, dpi, paper, json)   json = app-defined page data
//! layer(page, id, z, props)           props = LayerProps as JSON
//! tile(page, layer, tx, ty, codec, data)       sparse 256×256 tiles
//! ```
//!
//! * Tiles are stored LZ4-compressed (`lz4`), or as one color when uniform (`solid`).
//! * Saving writes a complete new file next to the target and renames it over the old one,
//!   so a crash or a full disk can never leave a half-written project.
//! * Files from a newer format version are refused instead of being misread.

use ink_core::tile::{TILE, TILE_PX};
use ink_core::{Document, Layer, LayerId, LayerProps, Rgba, Tile};
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const FORMAT: &str = "inkstudio";
pub const FORMAT_VERSION: i64 = 1;
pub const EXTENSION: &str = "inks";

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Db(rusqlite::Error),
    Json(serde_json::Error),
    NotInkstudio,
    TooNew(i64),
    Corrupt(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "{e}"),
            Error::Db(e) => write!(f, "database: {e}"),
            Error::Json(e) => write!(f, "data: {e}"),
            Error::NotInkstudio => write!(f, "not an Inkstudio file"),
            Error::TooNew(v) => write!(f, "made by a newer Inkstudio (format {v}); please update"),
            Error::Corrupt(s) => write!(f, "damaged file: {s}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
impl From<rusqlite::Error> for Error {
    fn from(e: rusqlite::Error) -> Self {
        Error::Db(e)
    }
}
impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Json(e)
    }
}

/// A page to save: the app's own JSON plus the pixel document.
pub struct PageOut<'a> {
    pub json: String,
    pub doc: &'a Document,
}

/// A loaded page.
pub struct PageIn {
    pub json: String,
    pub doc: Document,
}

pub struct ProjectIn {
    pub work_json: String,
    pub pages: Vec<PageIn>,
    pub app_version: String,
}

const SCHEMA: &str = "
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL) STRICT;
CREATE TABLE work (id INTEGER PRIMARY KEY CHECK (id = 1), json TEXT NOT NULL CHECK (json_valid(json))) STRICT;
CREATE TABLE page (
  idx INTEGER PRIMARY KEY,
  width INTEGER NOT NULL CHECK (width > 0),
  height INTEGER NOT NULL CHECK (height > 0),
  dpi INTEGER NOT NULL CHECK (dpi > 0),
  paper INTEGER NOT NULL,
  json TEXT NOT NULL CHECK (json_valid(json))
) STRICT;
CREATE TABLE layer (
  page INTEGER NOT NULL REFERENCES page(idx) ON DELETE CASCADE,
  id INTEGER NOT NULL,
  z INTEGER NOT NULL,
  props TEXT NOT NULL CHECK (json_valid(props)),
  PRIMARY KEY (page, id),
  UNIQUE (page, z)
) STRICT;
CREATE TABLE tile (
  page INTEGER NOT NULL,
  layer INTEGER NOT NULL,
  tx INTEGER NOT NULL,
  ty INTEGER NOT NULL,
  codec TEXT NOT NULL CHECK (codec IN ('lz4', 'solid')),
  data BLOB NOT NULL,
  PRIMARY KEY (page, layer, tx, ty),
  FOREIGN KEY (page, layer) REFERENCES layer(page, id) ON DELETE CASCADE
) STRICT, WITHOUT ROWID;
";

fn rgba_u32(c: Rgba) -> i64 {
    u32::from_le_bytes(c.0) as i64
}
fn u32_rgba(v: i64) -> Rgba {
    Rgba((v as u32).to_le_bytes())
}

fn encode_tile(t: &Tile) -> (&'static str, Vec<u8>) {
    let first = t.px[0];
    if t.px.iter().all(|p| *p == first) {
        return ("solid", first.0.to_vec());
    }
    // Rgba is #[repr(C)] [u8; 4]: view the tile as bytes without copying.
    let bytes: Vec<u8> = t.px.iter().flat_map(|p| p.0).collect();
    ("lz4", lz4_flex::compress_prepend_size(&bytes))
}

fn decode_tile(codec: &str, data: &[u8]) -> Result<Tile, Error> {
    match codec {
        "solid" if data.len() == 4 => Ok(Tile::filled(Rgba([data[0], data[1], data[2], data[3]]))),
        "lz4" => {
            let raw = lz4_flex::decompress_size_prepended(data).map_err(|e| Error::Corrupt(e.to_string()))?;
            if raw.len() != TILE_PX * 4 {
                return Err(Error::Corrupt(format!("tile has {} bytes", raw.len())));
            }
            let mut t = Tile::empty();
            for (i, c) in raw.chunks_exact(4).enumerate() {
                t.px[i] = Rgba([c[0], c[1], c[2], c[3]]);
            }
            Ok(t)
        }
        other => Err(Error::Corrupt(format!("unknown tile codec {other}"))),
    }
}

/// Save a project. Writes `<path>.tmp` then renames it over `path`.
pub fn save(path: &Path, work_json: &str, pages: &[PageOut<'_>], app_version: &str) -> Result<(), Error> {
    let tmp: PathBuf = {
        let mut s = path.as_os_str().to_owned();
        s.push(".tmp");
        s.into()
    };
    let _ = std::fs::remove_file(&tmp);
    {
        let mut db = Connection::open(&tmp)?;
        db.pragma_update(None, "journal_mode", "OFF")?;
        db.pragma_update(None, "synchronous", "OFF")?; // the rename below is the commit point
        db.pragma_update(None, "page_size", 65536)?;
        db.execute_batch(SCHEMA)?;
        let tx = db.transaction()?;
        {
            let mut meta = tx.prepare("INSERT INTO meta (key, value) VALUES (?1, ?2)")?;
            meta.execute(params!["format", FORMAT])?;
            meta.execute(params!["format_version", FORMAT_VERSION.to_string()])?;
            meta.execute(params!["app_version", app_version])?;
            tx.execute("INSERT INTO work (id, json) VALUES (1, ?1)", params![work_json])?;
            let mut pst = tx.prepare("INSERT INTO page (idx, width, height, dpi, paper, json) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
            let mut lst = tx.prepare("INSERT INTO layer (page, id, z, props) VALUES (?1, ?2, ?3, ?4)")?;
            let mut tst = tx.prepare("INSERT INTO tile (page, layer, tx, ty, codec, data) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")?;
            for (pi, p) in pages.iter().enumerate() {
                let d = p.doc;
                pst.execute(params![pi as i64, d.width as i64, d.height as i64, d.dpi as i64, rgba_u32(d.paper), p.json])?;
                for (z, (id, layer)) in d.layers().iter().enumerate() {
                    lst.execute(params![pi as i64, id.0 as i64, z as i64, serde_json::to_string(&layer.props)?])?;
                    let mut keys: Vec<_> = layer.raster.keys().copied().collect();
                    keys.sort();
                    for k in keys {
                        let t = layer.raster.tile(k).expect("listed key");
                        if t.is_transparent() {
                            continue;
                        }
                        let (codec, data) = encode_tile(t);
                        tst.execute(params![pi as i64, id.0 as i64, k.0 as i64, k.1 as i64, codec, data])?;
                    }
                }
            }
        }
        tx.commit()?;
        db.close().map_err(|(_, e)| e)?;
    }
    // Make sure the bytes are on disk before replacing the old file.
    std::fs::File::open(&tmp)?.sync_all()?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

/// Open a project.
pub fn load(path: &Path) -> Result<ProjectIn, Error> {
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let get_meta = |k: &str| -> Option<String> { db.query_row("SELECT value FROM meta WHERE key = ?1", params![k], |r| r.get(0)).ok() };
    if get_meta("format").as_deref() != Some(FORMAT) {
        return Err(Error::NotInkstudio);
    }
    let version: i64 = get_meta("format_version").and_then(|v| v.parse().ok()).ok_or(Error::NotInkstudio)?;
    if version > FORMAT_VERSION {
        return Err(Error::TooNew(version));
    }
    let app_version = get_meta("app_version").unwrap_or_default();
    let work_json: String = db.query_row("SELECT json FROM work WHERE id = 1", [], |r| r.get(0))?;
    let mut pages = Vec::new();
    let mut pst = db.prepare("SELECT idx, width, height, dpi, paper, json FROM page ORDER BY idx")?;
    let rows: Vec<(i64, i64, i64, i64, i64, String)> = pst.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?.collect::<Result<_, _>>()?;
    let mut lst = db.prepare("SELECT id, props FROM layer WHERE page = ?1 ORDER BY z")?;
    let mut tst = db.prepare("SELECT tx, ty, codec, data FROM tile WHERE page = ?1 AND layer = ?2")?;
    for (idx, w, h, dpi, paper, json) in rows {
        let mut doc = Document::new(w as u32, h as u32, dpi as u32);
        doc.paper = u32_rgba(paper);
        let layers: Vec<(i64, String)> = lst.query_map(params![idx], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?;
        for (id, props) in layers {
            let props: LayerProps = serde_json::from_str(&props)?;
            let mut layer = Layer { props, raster: Default::default() };
            let tiles: Vec<(i64, i64, String, Vec<u8>)> = tst.query_map(params![idx, id], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<Result<_, _>>()?;
            for (tx, ty, codec, data) in tiles {
                let t = decode_tile(&codec, &data)?;
                layer.raster.put_tile((tx as i32, ty as i32), Some(Arc::new(t)));
            }
            doc.push_layer_with_id(LayerId(id as u64), layer);
        }
        pages.push(PageIn { json, doc });
    }
    Ok(ProjectIn { work_json, pages, app_version })
}

const _: () = assert!(TILE == 256);

#[cfg(test)]
mod tests {
    use super::*;
    use ink_core::blend::BlendMode;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ink-io-test-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    fn sample_doc() -> Document {
        let mut d = Document::new(1000, 700, 350);
        d.paper = Rgba::TRANSPARENT;
        let mut a = Layer::new("Ink");
        a.props.reference = true;
        a.props.blend = BlendMode::Multiply;
        for i in 0..900 {
            a.raster.set(i, (i / 2) as i32, Rgba::from_straight(10, 20, (i % 255) as u8, 200));
        }
        let mut b = Layer::new("Fill");
        for y in 256..512 {
            for x in 0..256 {
                b.raster.set(x, y, Rgba::from_straight(250, 0, 0, 255)); // a solid tile
            }
        }
        d.push_layer(b);
        d.push_layer(a);
        d
    }

    #[test]
    fn roundtrip_preserves_pixels_props_ids_and_json() {
        let doc = sample_doc();
        let path = tmp("roundtrip.inks");
        save(&path, r#"{"name":"test"}"#, &[PageOut { json: r#"{"p":1}"#.into(), doc: &doc }], "0.1.0").unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.work_json, r#"{"name":"test"}"#);
        assert_eq!(back.pages.len(), 1);
        let d2 = &back.pages[0].doc;
        assert_eq!((d2.width, d2.height, d2.dpi, d2.paper), (1000, 700, 350, Rgba::TRANSPARENT));
        assert_eq!(d2.layers().len(), 2);
        for ((ia, la), (ib, lb)) in doc.layers().iter().zip(d2.layers()) {
            assert_eq!(ia, ib, "layer ids survive");
            assert_eq!(la.props.name, lb.props.name);
            assert_eq!(la.props.blend, lb.props.blend);
            assert_eq!(la.props.reference, lb.props.reference);
            assert!(la.raster.read_rect(0, 0, 1000, 700) == lb.raster.read_rect(0, 0, 1000, 700), "pixels survive");
        }
    }

    #[test]
    fn refuses_foreign_and_newer_files() {
        let path = tmp("foreign.inks");
        let _ = std::fs::remove_file(&path);
        Connection::open(&path).unwrap().execute_batch("CREATE TABLE x (a); INSERT INTO x VALUES (1);").unwrap();
        assert!(matches!(load(&path), Err(Error::NotInkstudio)));

        let path = tmp("newer.inks");
        let doc = sample_doc();
        save(&path, "{}", &[PageOut { json: "{}".into(), doc: &doc }], "x").unwrap();
        // Bump the version in place.
        Connection::open(&path).unwrap().execute("UPDATE meta SET value = '99' WHERE key = 'format_version'", []).unwrap();
        assert!(matches!(load(&path), Err(Error::TooNew(99))));
    }

    #[test]
    fn overwrite_replaces_atomically_and_compresses() {
        let path = tmp("overwrite.inks");
        let doc = sample_doc();
        save(&path, "{}", &[PageOut { json: "{}".into(), doc: &doc }], "x").unwrap();
        let mut d2 = sample_doc();
        d2.push_layer(Layer::new("extra"));
        save(&path, "{}", &[PageOut { json: "{}".into(), doc: &d2 }, PageOut { json: "{}".into(), doc: &doc }], "x").unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.pages.len(), 2);
        assert_eq!(back.pages[0].doc.layers().len(), 3);
        assert!(!path.with_extension("inks.tmp").exists());
        // 2 pages × ~6 tiles of raw RGBA would be ~3 MB; compressed it's far smaller.
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(size < 1_000_000, "file is {size} bytes");
    }
}
