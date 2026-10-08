//! Text-shaping probe: Spanish, Hebrew, Arabic and mixed-direction strings.
//! It renders them (see `results/text-shaping.png`) and counts glyphs per
//! character, which is what egui's text editing relies on.

use std::sync::Arc;

pub const SAMPLES: &[(&str, &str)] = &[
    ("Spanish", "Señalización: ¿Pilar 30×30 cm? Sí, hormigón H-30, ø12 c/15"),
    ("Hebrew", "שלום עולם"),
    ("Arabic", "مرحبا بالعالم"),
    // Left-to-right paragraph with an Arabic word inside.
    ("LTR + Arabic", "Viga V-101 مرحبا 42"),
    // Right-to-left paragraph with Latin text and a number inside: needs the
    // Unicode bidi algorithm. Correct display, left to right: "עולם Viga 42 שלום".
    ("RTL + Latin", "שלום Viga 42 עולם"),
];

/// Find a system font file with `fc-match`. Linux only; fine for a spike.
fn system_font(family: &str) -> Option<Vec<u8>> {
    let run = |fmt: &str| {
        let out = std::process::Command::new("fc-match").args(["-f", fmt, family]).output().ok()?;
        String::from_utf8(out.stdout).ok()
    };
    // fc-match always returns *something*; make sure it's the family we asked for.
    if !run("%{family}")?.contains(family) {
        return None;
    }
    std::fs::read(run("%{file}")?.trim()).ok()
}

/// Add Arabic and Hebrew fallback fonts, if the system has them.
/// egui 0.36 doesn't find system fonts by itself (0.37 will, through fontique).
pub fn install_fallback_fonts(ctx: &egui::Context) -> Vec<&'static str> {
    let mut found = vec![];
    for family in ["IBM Plex Sans Arabic", "IBM Plex Sans Hebrew", "Noto Sans Arabic", "Noto Sans Hebrew"] {
        if let Some(data) = system_font(family) {
            ctx.add_font(egui::epaint::text::FontInsert::new(
                family,
                egui::FontData::from_owned(data),
                vec![egui::epaint::text::InsertFontFamily {
                    family: egui::FontFamily::Proportional,
                    priority: egui::epaint::text::FontPriority::Lowest,
                }],
            ));
            found.push(family);
        }
    }
    found
}

#[derive(Debug, Clone)]
pub struct Measure {
    pub name: &'static str,
    pub text: &'static str,
    pub chars: usize,
    /// egui's cursor and selection code assume one glyph per character
    /// (egui issue #8576). More glyphs than characters breaks text editing.
    pub glyphs: usize,
}

pub fn measure(ctx: &egui::Context) -> Vec<Measure> {
    SAMPLES
        .iter()
        .map(|&(name, text)| {
            let galley: Arc<egui::Galley> = ctx.fonts_mut(|f| {
                f.layout_no_wrap(text.to_owned(), egui::FontId::proportional(20.0), egui::Color32::BLACK)
            });
            let glyphs = galley.rows.iter().map(|r| r.row.glyphs.len()).sum();
            Measure { name, text, chars: text.chars().count(), glyphs }
        })
        .collect()
}

pub fn ui(ui: &mut egui::Ui, fonts: &[&str]) {
    ui.label(format!("Fallback fonts loaded: {}", if fonts.is_empty() { "none".into() } else { fonts.join(", ") }));
    ui.separator();
    egui::Grid::new("shaping").striped(true).show(ui, |ui| {
        ui.strong("Sample");
        ui.strong("Rendered");
        ui.strong("Chars");
        ui.strong("Glyphs");
        ui.end_row();
        for m in measure(ui.ctx()) {
            ui.label(m.name);
            ui.label(egui::RichText::new(m.text).size(20.0));
            ui.label(m.chars.to_string());
            ui.label(m.glyphs.to_string());
            ui.end_row();
        }
    });
}
