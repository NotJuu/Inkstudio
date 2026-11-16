//! Render every brush preset as a sample stroke and time it.
//!
//!   cargo run --release -p ink-raster --example brush_sheet -- <out.png>
//!
//! Writes the sheet PNG plus `<out>.labels.tsv` (x, y, size, text) for tools/label_sheet.py,
//! and prints the cost per pen sample of every preset (sorted, slowest first).

use ink_core::{Document, Layer, Rgba};
use ink_raster::presets::{catalog, Category};
use ink_raster::{Sample, Stroke};
use std::fmt::Write as _;
use std::time::Instant;

const COLS: usize = 4;
const CELL_W: i32 = 600;
const CELL_H: i32 = 230;
const HEADER: i32 = 70;
const MARGIN: i32 = 40;

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "replica/brushes.png".into());
    let presets = catalog();

    // Layout: a header row per category, then its presets in a grid.
    let mut cells = Vec::new();
    let mut labels = String::new();
    let mut y = MARGIN;
    for cat in Category::ALL {
        let list: Vec<_> = presets.iter().filter(|p| p.category == cat).collect();
        writeln!(labels, "{}\t{}\t{}\t{}", MARGIN, y + 18, 34, cat.name()).unwrap();
        y += HEADER;
        for (i, p) in list.iter().enumerate() {
            let (cx, cy) = (MARGIN + (i % COLS) as i32 * CELL_W, y + (i / COLS) as i32 * CELL_H);
            cells.push((cx, cy, *p));
            writeln!(labels, "{}\t{}\t{}\t{}", cx + 10, cy + CELL_H - 34, 22, p.params.name).unwrap();
        }
        y += list.len().div_ceil(COLS) as i32 * CELL_H + 20;
    }
    let (w, h) = (MARGIN * 2 + COLS as i32 * CELL_W, y + MARGIN);
    let mut doc = Document::new(w as u32, h as u32, 600);
    let layer = doc.push_layer(Layer::new("sheet"));
    let mut timings = Vec::new();

    for (cx, cy, p) in &cells {
        let erase = p.category == Category::Eraser;
        if erase {
            // Something to erase: a black block (scratches show white lines through it).
            let l = doc.layer_mut(layer).unwrap();
            for yy in cy + 20..cy + CELL_H - 60 {
                for xx in cx + 20..cx + CELL_W - 40 {
                    l.raster.set(xx, yy, Rgba::BLACK);
                }
            }
        }
        let n = 160;
        // Very large brushes are shown smaller so they fit their cell.
        let mut params = p.params.clone();
        if params.size > 90.0 {
            params.size = 90.0;
        }
        let mut stroke = Stroke::begin(&doc, layer, params, Rgba::from_straight(18, 18, 22, 255));
        let t0 = Instant::now();
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let x = *cx as f32 + 60.0 + t * (CELL_W as f32 - 140.0);
            let yy = *cy as f32 + 85.0 + (t * std::f32::consts::TAU).sin() * 38.0;
            let pressure = 0.15 + 0.85 * (t * std::f32::consts::PI).sin();
            let mut s = Sample::new(x, yy, pressure);
            s.time = t as f64 * 1.2;
            stroke.add(&mut doc, s);
        }
        let _ = stroke.end(&mut doc);
        timings.push((t0.elapsed().as_secs_f64() * 1000.0 / (n + 1) as f64, p.params.name.clone(), p.params.size));
    }

    timings.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("{} presets. Cost per pen sample (incl. stabilizer, dabs, recomposite):", cells.len());
    for (ms, name, size) in &timings {
        println!("  {ms:>7.3} ms  {name} ({size} px)");
    }

    let rgba = doc.flatten_rgba8();
    let f = std::fs::File::create(&out).expect("create png");
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header().unwrap().write_image_data(&rgba).unwrap();
    std::fs::write(format!("{out}.labels.tsv"), labels).unwrap();
    println!("wrote {out} ({w} x {h})");
}
