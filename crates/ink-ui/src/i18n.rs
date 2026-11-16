//! UI translations (English source strings, German table).
//!
//! `tr("Save")` returns the string in the current language. English strings are the keys,
//! so untranslated text still shows in English. A test in the app crate checks that every
//! `tr("…")` used in the UI has a German entry.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Lang {
    #[default]
    En,
    De,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::En, Lang::De];
    pub fn code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::De => "de",
        }
    }
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::En => "English",
            Lang::De => "Deutsch",
        }
    }
    pub fn from_code(c: &str) -> Option<Lang> {
        match c.get(..2)?.to_ascii_lowercase().as_str() {
            "en" => Some(Lang::En),
            "de" => Some(Lang::De),
            _ => None,
        }
    }
}

static LANG: AtomicU8 = AtomicU8::new(0);

pub fn set_lang(l: Lang) {
    LANG.store(l as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    if LANG.load(Ordering::Relaxed) == 1 { Lang::De } else { Lang::En }
}

/// Language from the environment (LC_ALL, LC_MESSAGES, LANG, LANGUAGE), English otherwise.
pub fn system_lang() -> Lang {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
        if let Ok(v) = std::env::var(var) {
            if let Some(l) = Lang::from_code(&v) {
                return l;
            }
        }
    }
    Lang::En
}

/// Translate an English UI string.
pub fn tr(s: &str) -> &str {
    if lang() == Lang::En {
        return s;
    }
    de().get(s).copied().unwrap_or(s)
}

/// True if `s` has a German translation (used by tests).
pub fn has_de(s: &str) -> bool {
    de().contains_key(s)
}

fn de() -> &'static HashMap<&'static str, &'static str> {
    static T: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    T.get_or_init(|| DE.iter().copied().collect())
}

const DE: &[(&str, &str)] = &[
    // ---- menus --------------------------------------------------------------------------------
    ("File", "Datei"),
    ("Edit", "Bearbeiten"),
    ("View", "Ansicht"),
    ("Page", "Seite"),
    ("Animation", "Animation"),
    ("Window", "Fenster"),
    ("Language", "Sprache"),
    ("New sheet…", "Neues Blatt…"),
    ("Open…", "Öffnen…"),
    ("Open recent", "Zuletzt geöffnet"),
    ("No recent files", "Keine zuletzt geöffneten Dateien"),
    ("Save", "Speichern"),
    ("Save as…", "Speichern unter…"),
    ("Export page as PNG", "Seite als PNG exportieren"),
    ("Export all pages as PNG", "Alle Seiten als PNG exportieren"),
    ("Export animation as GIF", "Animation als GIF exportieren"),
    ("Quit", "Beenden"),
    ("Undo", "Rückgängig"),
    ("Redo", "Wiederholen"),
    ("Fit page", "Seite einpassen"),
    ("Actual pixels (100%)", "Originalgröße (100 %)"),
    ("Zoom in", "Vergrößern"),
    ("Zoom out", "Verkleinern"),
    ("Rotate left 15°", "15° nach links drehen"),
    ("Rotate right 15°", "15° nach rechts drehen"),
    ("Reset rotation", "Drehung zurücksetzen"),
    ("Flip horizontally", "Horizontal spiegeln"),
    ("Simulate pressure for mouse", "Druck für Maus simulieren"),
    ("Add page after this", "Seite danach einfügen"),
    ("Delete this page", "Diese Seite löschen"),
    ("Replace panels with template", "Panels durch Vorlage ersetzen"),
    ("Play", "Abspielen"),
    ("Pause", "Pause"),
    ("Create an Animation sheet to use the timeline.", "Erstelle ein Animations-Blatt, um die Timeline zu nutzen."),
    ("Reset layout", "Layout zurücksetzen"),
    ("Show palettes", "Paletten anzeigen"),
    // ---- panes ---------------------------------------------------------------------------------
    ("Canvas", "Leinwand"),
    ("Sub tool", "Unterwerkzeug"),
    ("Tool property", "Werkzeugeigenschaft"),
    ("Color", "Farbe"),
    ("Layer", "Ebene"),
    ("Navigator", "Navigator"),
    ("Pages", "Seiten"),
    ("Timeline", "Timeline"),
    // ---- tools ---------------------------------------------------------------------------------
    ("Brush (B)", "Pinsel (B)"),
    ("Eraser (E)", "Radierer (E)"),
    ("Fill (G)", "Füllen (G)"),
    ("Panel frames (F)", "Panelrahmen (F)"),
    ("Balloons & text (T)", "Sprechblasen & Text (T)"),
    ("Hand (H / hold Space)", "Hand (H / Leertaste halten)"),
    ("Rotate view (R + drag)", "Ansicht drehen (R + ziehen)"),
    ("Brushes", "Pinsel"),
    ("Eraser", "Radierer"),
    ("Fill", "Füllen"),
    ("Panels", "Panels"),
    ("Balloon", "Sprechblase"),
    ("Balloon styles", "Sprechblasen-Stile"),
    ("Panel templates", "Panel-Vorlagen"),
    ("Tool", "Werkzeug"),
    ("Size", "Größe"),
    ("Opacity", "Deckkraft"),
    ("Stabilization", "Stabilisierung"),
    ("This layer", "Diese Ebene"),
    ("Reference layers", "Referenzebenen"),
    ("All layers", "Alle Ebenen"),
    ("Refer to", "Bezug"),
    ("Close gap", "Lücken schließen"),
    ("Tolerance", "Toleranz"),
    ("Expand", "Erweitern"),
    ("Draw rectangle", "Rechteck zeichnen"),
    ("Divide (drag a cut)", "Teilen (Schnitt ziehen)"),
    ("Select", "Auswählen"),
    ("Border", "Rahmen"),
    ("Gutter ↕", "Abstand ↕"),
    ("Vertical", "Vertikal"),
    ("Horizontal", "Horizontal"),
    ("Click empty canvas to place · drag body / tail tip / corners", "Auf leere Leinwand klicken zum Platzieren · Körper / Schwanzspitze / Ecken ziehen"),
    ("Drag to pan · scroll to zoom", "Ziehen zum Verschieben · scrollen zum Zoomen"),
    ("Drag to rotate around the view center", "Ziehen, um um die Mitte zu drehen"),
    ("Replaces the panels on this page (undoable).", "Ersetzt die Panels auf dieser Seite (rückgängig machbar)."),
    ("Rectangle: drag on the page.\nDivide: drag a line across a panel.\nSelect + Delete removes a panel.", "Rechteck: auf der Seite ziehen.\nTeilen: eine Linie über ein Panel ziehen.\nAuswählen + Entf löscht ein Panel."),
    ("Click inside line art. The color goes onto the selected layer; the shape is read from reference layers (Ink, Panels).", "In die Linienzeichnung klicken. Die Farbe landet auf der gewählten Ebene, die Form wird aus den Referenzebenen (Tusche, Panels) gelesen."),
    ("Drag to move the view. Scroll to pan, Ctrl+scroll or pinch to zoom, R+drag to rotate.", "Ziehen verschiebt die Ansicht. Scrollen verschiebt, Strg+Scrollen oder Zwei-Finger-Zoom zoomt, R+Ziehen dreht."),
    ("Size of new balloons", "Größe neuer Sprechblasen"),
    ("Pressure → size", "Druck → Größe"),
    ("Pressure → opacity", "Druck → Deckkraft"),
    ("Taper start", "Einspitzen"),
    ("Taper end", "Ausspitzen"),
    ("Anti-aliasing", "Kantenglättung"),
    // ---- balloons ------------------------------------------------------------------------------
    ("Text", "Text"),
    ("Type the dialogue…", "Dialog eingeben…"),
    ("Click on the page to place a balloon, or click a balloon to edit it.", "Klicke auf die Seite, um eine Sprechblase zu setzen, oder klicke eine Blase an, um sie zu bearbeiten."),
    ("Font size", "Schriftgröße"),
    ("Tail", "Schwanz"),
    ("Pointed", "Spitz"),
    ("Curved", "Gebogen"),
    ("Bubbles", "Bläschen"),
    ("None", "Keiner"),
    ("Fit balloon to text", "Blase an Text anpassen"),
    ("Bring to front", "Nach vorne"),
    ("Delete", "Löschen"),
    ("No font found: text will not render.", "Keine Schrift gefunden: Text wird nicht dargestellt."),
    ("Balloon placed. Type the text in the Text palette.", "Sprechblase gesetzt. Tippe den Text in der Text-Palette."),
    ("Speech", "Sprechen"),
    ("Rounded", "Abgerundet"),
    ("Box", "Kasten"),
    ("Thought", "Gedanke"),
    ("Shout", "Schrei"),
    ("Whisper", "Flüstern"),
    ("Wavy", "Wellig"),
    ("Narration", "Erzähler"),
    ("Electronic", "Elektronisch"),
    ("Text only", "Nur Text"),
    // ---- layers --------------------------------------------------------------------------------
    ("Clip", "Clip"),
    ("Lock α", "α sperren"),
    ("Ref", "Ref"),
    ("Draft", "Entwurf"),
    ("New layer above", "Neue Ebene darüber"),
    ("Delete layer", "Ebene löschen"),
    ("Move layer up", "Ebene nach oben"),
    ("Move layer down", "Ebene nach unten"),
    ("New layer", "Neue Ebene"),
    ("Layer properties", "Ebeneneigenschaften"),
    ("Move layer", "Ebene verschieben"),
    ("Layer {n}", "Ebene {n}"),
    ("Sketch", "Skizze"),
    ("Ink", "Tusche"),
    ("Balloons & text", "Sprechblasen & Text"),
    ("Cel", "Cel"),
    ("Normal", "Normal"),
    ("Multiply", "Multiplizieren"),
    ("Screen", "Negativ multiplizieren"),
    ("Overlay", "Ineinanderkopieren"),
    ("Darken", "Abdunkeln"),
    ("Lighten", "Aufhellen"),
    ("Add", "Addieren"),
    ("Subtract", "Subtrahieren"),
    ("Difference", "Differenz"),
    ("Color dodge", "Farbig abwedeln"),
    ("Color burn", "Farbig nachbelichten"),
    ("Hard light", "Hartes Licht"),
    ("Soft light", "Weiches Licht"),
    // ---- color / navigator ------------------------------------------------------------------------
    ("Wheel", "Rad"),
    ("Set", "Set"),
    ("Swap main/sub color (X)", "Haupt-/Nebenfarbe tauschen (X)"),
    ("Zoom", "Zoom"),
    ("Rotation", "Drehung"),
    ("Flip", "Spiegeln"),
    ("Fit", "Einpassen"),
    ("Reset", "Zurücksetzen"),
    ("Click or drag in the preview to move the view.", "In die Vorschau klicken oder ziehen, um die Ansicht zu verschieben."),
    // ---- pages / timeline -----------------------------------------------------------------------
    ("Add page", "Seite hinzufügen"),
    ("right binding: page 1 is on the right", "rechte Bindung: Seite 1 ist rechts"),
    ("left binding", "linke Bindung"),
    ("Page {n} · drag ← → to reorder", "Seite {n} · ← → ziehen zum Umsortieren"),
    ("This sheet has one page. Create a multi-page book to manage pages.", "Dieses Blatt hat eine Seite. Erstelle ein mehrseitiges Buch, um Seiten zu verwalten."),
    ("This sheet is not an animation.", "Dieses Blatt ist keine Animation."),
    ("First frame", "Erstes Bild"),
    ("Previous frame (,)", "Vorheriges Bild (,)"),
    ("Next frame (.)", "Nächstes Bild (.)"),
    ("Play / pause (Enter)", "Abspielen / Pause (Enter)"),
    ("Frame {a} / {b}", "Bild {a} / {b}"),
    (" fps", " fps"),
    (" frames", " Bilder"),
    ("New cel here", "Neues Cel hier"),
    ("Blank", "Leer"),
    ("Show nothing from this frame on", "Ab diesem Bild nichts zeigen"),
    ("Onion skin", "Zwiebelhaut"),
    ("Loop", "Schleife"),
    // ---- status / toasts ------------------------------------------------------------------------
    ("Zoom {z}%", "Zoom {z} %"),
    ("Rotation {r}°", "Drehung {r}°"),
    ("Mouse", "Maus"),
    ("Mouse (simulated pressure)", "Maus (simulierter Druck)"),
    ("Pen pressure {p}", "Stiftdruck {p}"),
    ("Mirrored", "Gespiegelt"),
    ("Unsaved", "Nicht gespeichert"),
    ("Create a sheet to start (Ctrl+N)", "Erstelle ein Blatt, um zu beginnen (Strg+N)"),
    ("Nothing to fill there", "Dort gibt es nichts zu füllen"),
    ("Drag the cut across a panel", "Ziehe den Schnitt über ein Panel"),
    ("Panel deleted", "Panel gelöscht"),
    ("This layer holds panels or balloons. Select a drawing layer.", "Diese Ebene enthält Panels oder Sprechblasen. Wähle eine Zeichenebene."),
    ("The layer is locked.", "Die Ebene ist gesperrt."),
    ("Saved {f}", "{f} gespeichert"),
    ("Opened {f}", "{f} geöffnet"),
    ("Couldn't save: {e}", "Speichern fehlgeschlagen: {e}"),
    ("Couldn't open: {e}", "Öffnen fehlgeschlagen: {e}"),
    ("Exported to {f}", "Exportiert nach {f}"),
    ("Export failed: {e}", "Export fehlgeschlagen: {e}"),
    // ---- dialogs ----------------------------------------------------------------------------------
    ("Cancel", "Abbrechen"),
    ("Create", "Erstellen"),
    ("Open", "Öffnen"),
    ("Folder", "Ordner"),
    ("File name", "Dateiname"),
    ("Home", "Persönlicher Ordner"),
    ("Up", "Nach oben"),
    ("Recent", "Zuletzt"),
    ("No Inkstudio files here.", "Hier gibt es keine Inkstudio-Dateien."),
    ("Replace the existing file?", "Vorhandene Datei ersetzen?"),
    ("Replace", "Ersetzen"),
    ("Save changes to “{f}”?", "Änderungen an „{f}“ speichern?"),
    ("Your changes will be lost if you don't save them.", "Deine Änderungen gehen verloren, wenn du sie nicht speicherst."),
    ("Don't save", "Nicht speichern"),
    ("New sheet", "Neues Blatt"),
    ("Name", "Name"),
    ("Format", "Format"),
    ("Orientation", "Ausrichtung"),
    ("Portrait", "Hochformat"),
    ("Landscape", "Querformat"),
    ("Resolution", "Auflösung"),
    ("Bleed", "Beschnitt"),
    ("Safe area margin", "Satzspiegel-Rand"),
    ("Color mode", "Farbmodus"),
    ("Paper", "Papier"),
    ("White", "Weiß"),
    ("Transparent", "Transparent"),
    ("Binding", "Bindung"),
    ("Right (manga, read ←)", "Rechts (Manga, lesen ←)"),
    ("Left (read →)", "Links (lesen →)"),
    ("Frame rate", "Bildrate"),
    ("Length", "Länge"),
    ("Custom", "Eigenes Format"),
    ("{n} pages", "{n} Seiten"),
    ("{w} × {h} mm finished · {d} dpi", "{w} × {h} mm Endformat · {d} dpi"),
    ("Very large canvas: drawing stays fast, but exports take longer.", "Sehr große Leinwand: Zeichnen bleibt schnell, aber Exporte dauern länger."),
    ("scroll ↓", "scrollen ↓"),
    ("Illustration", "Illustration"),
    ("Manga page", "Manga-Seite"),
    ("Multi-page book", "Mehrseitiges Buch"),
    ("Webtoon", "Webtoon"),
    ("Grayscale", "Graustufen"),
    ("Monochrome (1-bit)", "Schwarzweiß (1 Bit)"),
    ("Untitled", "Unbenannt"),
    ("1 panel", "1 Panel"),
    ("2 rows", "2 Reihen"),
    ("3 rows", "3 Reihen"),
    ("4-koma", "4-Koma"),
    ("2 × 2", "2 × 2"),
    ("2 × 3", "2 × 3"),
    ("5-panel manga", "5-Panel-Manga"),
    ("Square 210 mm", "Quadrat 210 mm"),
    ("Social post 1080 × 1350 px", "Social-Media-Beitrag 1080 × 1350 px"),
    ("Webtoon strip 800 × 12800 px", "Webtoon-Streifen 800 × 12800 px"),
    ("B5 (JIS) – manga magazine", "B5 (JIS) – Manga-Magazin"),
    ("B4 (JIS) – manga manuscript", "B4 (JIS) – Manga-Manuskript"),
    ("A5 – tankobon / doujinshi", "A5 – Tankōbon / Dōjinshi"),
    ("B6 – small paperback", "B6 – kleines Taschenbuch"),
    ("US comic (6.625 × 10.25 in)", "US-Comic (6,625 × 10,25 in)"),
    ("US Letter", "US-Letter"),
    ("Full HD 1920 × 1080 px", "Full HD 1920 × 1080 px"),
    ("HD 1280 × 720 px", "HD 1280 × 720 px"),
    // ---- widgets (ink-ui) --------------------------------------------------------------------------
    ("Hide layer", "Ebene ausblenden"),
    ("Show layer", "Ebene einblenden"),
    ("Lock layer", "Ebene sperren"),
    ("Unlock layer", "Ebene entsperren"),
    ("Swap main and sub color", "Haupt- und Nebenfarbe tauschen"),
    ("Pen pressure on", "Stiftdruck an"),
    ("Pen pressure off", "Stiftdruck aus"),
    // ---- brush categories and names --------------------------------------------------------------
    ("Ink pens", "Tuschestifte"),
    ("Pencils", "Bleistifte"),
    ("Paint brushes", "Malpinsel"),
    ("Airbrush & spray", "Airbrush & Spray"),
    ("Erasers", "Radierer"),
    ("Manga effects", "Manga-Effekte"),
    ("Decoration", "Dekoration"),
    ("Tone pens", "Raster-Stifte"),
    ("G-pen", "G-Pen"),
    ("Mapping pen", "Mapping-Pen"),
    ("Round pen", "Maru-Pen"),
    ("Turnip pen", "Kabura-Pen"),
    ("Brush pen", "Pinselstift"),
    ("Sumi brush", "Sumi-Pinsel"),
    ("Calligraphy nib", "Kalligrafie-Feder"),
    ("Fineliner", "Fineliner"),
    ("Marker", "Marker"),
    ("Dry ink pen", "Trockene Tusche"),
    ("Effect-line pen", "Effektlinien-Stift"),
    ("HB pencil", "HB-Bleistift"),
    ("Soft pencil 2B", "Weicher Bleistift 2B"),
    ("Mechanical pencil", "Druckbleistift"),
    ("Layout pencil", "Layout-Bleistift"),
    ("Charcoal", "Kohle"),
    ("Colored pencil", "Buntstift"),
    ("Crayon", "Wachsmalstift"),
    ("Round brush", "Rundpinsel"),
    ("Flat brush", "Flachpinsel"),
    ("Dry brush", "Trockenpinsel"),
    ("Watercolor wash", "Aquarell-Lasur"),
    ("Gouache", "Gouache"),
    ("Flatting brush", "Flächenpinsel"),
    ("Soft airbrush", "Weiche Airbrush"),
    ("Hard airbrush", "Harte Airbrush"),
    ("Spray", "Spray"),
    ("Grit", "Körnung"),
    ("Hard eraser", "Harter Radierer"),
    ("Soft eraser", "Weicher Radierer"),
    ("Kneaded eraser", "Knetradierer"),
    ("Rough eraser", "Rauer Radierer"),
    ("Tone scratch", "Raster-Kratzer"),
    ("Scratch knife", "Kratzmesser"),
    ("Speed-line brush", "Speedline-Pinsel"),
    ("Hair brush", "Haarpinsel"),
    ("Dashed line", "Gestrichelte Linie"),
    ("Stitch line", "Naht"),
    ("Rain", "Regen"),
    ("Snow / bokeh", "Schnee / Bokeh"),
    ("Cloud / smoke", "Wolken / Rauch"),
    ("Grass", "Gras"),
    ("Leaves", "Blätter"),
    ("Sparkles", "Glitzer"),
    ("Stars", "Sterne"),
    ("Hearts", "Herzen"),
    ("Flowers", "Blumen"),
    ("Stipple", "Punktierung"),
    ("Tone pen 60 lpi 20%", "Raster-Stift 60 lpi 20 %"),
    ("Tone pen 60 lpi 40%", "Raster-Stift 60 lpi 40 %"),
    ("Line tone pen", "Linienraster-Stift"),
    ("Cross-hatch tone pen", "Kreuzschraffur-Stift"),
    ("Sand tone pen", "Sandraster-Stift"),
    // ---- brush descriptions ----------------------------------------------------------------------
    ("Main inking: strong thick–thin with pressure", "Hauptwerkzeug zum Tuschen: starker Dick-dünn-Verlauf mit Druck"),
    ("Fine detail, faces, hatching", "Feine Details, Gesichter, Schraffuren"),
    ("Even fine lines for backgrounds and eyes", "Gleichmäßig feine Linien für Hintergründe und Augen"),
    ("Between G-pen and round pen: crisp, moderate swell", "Zwischen G-Pen und Maru-Pen: knackig, mäßiger Schwung"),
    ("Expressive outlines: thins when you draw fast", "Ausdrucksstarke Konturen: wird dünner, wenn du schnell zeichnest"),
    ("Ink brush with dry edges for bold strokes and effects", "Tuschpinsel mit trockenen Kanten für kräftige Striche und Effekte"),
    ("Flat nib at a fixed angle: lettering, sound effects", "Breite Feder im festen Winkel: Schrift, Soundeffekte"),
    ("Uniform technical line: panel borders, mechanical parts", "Gleichmäßige technische Linie: Panelrahmen, Mechanik"),
    ("Chisel marker for flat, bold shapes", "Keilspitz-Marker für flächige, kräftige Formen"),
    ("G-pen with a rough, dry-ink edge", "G-Pen mit rauer, trockener Tuschekante"),
    ("Long, very smooth lines that fade out: speed and focus lines", "Lange, sehr glatte, auslaufende Linien: Speed- und Fokuslinien"),
    ("Everyday sketching with paper texture", "Alltägliches Skizzieren mit Papierstruktur"),
    ("Soft, dark sketch lines; shade with low pressure", "Weiche, dunkle Skizzenlinien; mit wenig Druck schattieren"),
    ("Thin, even construction lines", "Dünne, gleichmäßige Konstruktionslinien"),
    ("Light, loose layout sketch (use with a blue color)", "Leichte, lockere Layout-Skizze (mit blauer Farbe verwenden)"),
    ("Charcoal: grainy, angled, for value studies", "Kohle: körnig, schräg, für Tonwertstudien"),
    ("Colored pencil with visible paper tooth", "Buntstift mit sichtbarer Papierstruktur"),
    ("Waxy crayon: very rough edges", "Wachsiger Malstift: sehr raue Kanten"),
    ("General painting: size and opacity follow pressure", "Allgemeines Malen: Größe und Deckkraft folgen dem Druck"),
    ("Flat brush: broad side strokes, crisp edges", "Flachpinsel: breite Seitenstriche, klare Kanten"),
    ("Bristle marks along the stroke: fur, wood, rough fills", "Borstenspuren entlang des Strichs: Fell, Holz, raue Flächen"),
    ("Transparent wash that builds up", "Transparente Lasur, die sich aufbaut"),
    ("Opaque, slightly flat paint", "Deckende, leicht flache Farbe"),
    ("Hard, uniform brush for flats and touch-ups", "Harter, gleichmäßiger Pinsel für Flächen und Korrekturen"),
    ("Soft gradients and glows", "Weiche Verläufe und Leuchten"),
    ("Airbrush with a firmer edge", "Airbrush mit festerer Kante"),
    ("Spray-can speckle", "Sprühdosen-Sprenkel"),
    ("Sand, dust and gritty texture", "Sand, Staub und körnige Struktur"),
    ("Clean, hard-edged erasing", "Sauberes Radieren mit harter Kante"),
    ("Soft edge for lightening", "Weiche Kante zum Aufhellen"),
    ("Gently lifts pencil sketches", "Hebt Bleistiftskizzen sanft ab"),
    ("Erases with a textured, broken edge", "Radiert mit strukturierter, gebrochener Kante"),
    ("Scratch lines into tone or black: highlights, shine", "Linien in Raster oder Schwarz kratzen: Glanzlichter, Schimmer"),
    ("Thin tapered scratch lines (like a knife on screentone)", "Dünne, spitz zulaufende Kratzlinien (wie ein Messer auf Rasterfolie)"),
    ("Bundle of parallel streaks in one stroke", "Bündel paralleler Striche in einem Zug"),
    ("Hair strands: several tapered lines at once", "Haarsträhnen: mehrere spitz zulaufende Linien gleichzeitig"),
    ("Dashed line: stitches, motion paths, cut lines", "Gestrichelte Linie: Nähte, Bewegungspfade, Schnittlinien"),
    ("Sewing stitches on clothes", "Nähte auf Kleidung"),
    ("Slanted rain streaks", "Schräge Regenstriche"),
    ("Snow, dust motes, bokeh dots (use white or a light color)", "Schnee, Staubpartikel, Bokeh-Punkte (mit Weiß oder heller Farbe)"),
    ("Clouds, smoke, steam", "Wolken, Rauch, Dampf"),
    ("Bubbles and water drops", "Blasen und Wassertropfen"),
    ("Grass tufts and fur clumps", "Grasbüschel und Fellbüschel"),
    ("Foliage: scattered leaf shapes", "Laub: verstreute Blattformen"),
    ("Four-point sparkles for shine and magic", "Vierzackiges Glitzern für Glanz und Magie"),
    ("Scattered five-point stars", "Verstreute fünfzackige Sterne"),
    ("Hearts for shoujo backgrounds and reactions", "Herzen für Shōjo-Hintergründe und Reaktionen"),
    ("Flower background for romance scenes", "Blumenhintergrund für Romantikszenen"),
    ("Stippled dots for shading by hand", "Punktierung zum Schattieren von Hand"),
    ("Paint screentone directly: 60 lpi, 20%", "Rasterfolie direkt malen: 60 lpi, 20 %"),
    ("Darker tone for shadows: 60 lpi, 40%", "Dunkleres Raster für Schatten: 60 lpi, 40 %"),
    ("Line tone: hatching texture, motion", "Linienraster: Schraffurstruktur, Bewegung"),
    ("Cross-hatch tone for dark shading", "Kreuzschraffur-Raster für dunkle Schattierung"),
    ("Sand/noise tone: ground, stone, grit", "Sand-/Rauschraster: Boden, Stein, Körnung"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_and_falls_back() {
        set_lang(Lang::De);
        assert_eq!(tr("Save"), "Speichern");
        assert_eq!(tr("no such string"), "no such string");
        set_lang(Lang::En);
        assert_eq!(tr("Save"), "Save");
    }

    #[test]
    fn no_duplicate_keys() {
        let mut seen = std::collections::HashSet::new();
        for (k, _) in DE {
            assert!(seen.insert(*k), "duplicate key {k:?}");
        }
    }

    #[test]
    fn codes() {
        assert_eq!(Lang::from_code("de_DE.UTF-8"), Some(Lang::De));
        assert_eq!(Lang::from_code("en_US"), Some(Lang::En));
        assert_eq!(Lang::from_code("fr"), None);
    }
}
