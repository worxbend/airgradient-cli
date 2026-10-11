//! Headless, synthetic UI gallery. See docs/tui-design.md for the capture contract.
use std::{
    convert::Infallible,
    fmt::Write as _,
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use airgradient_cli::{
    device_url::DeviceUrl,
    history::Reading,
    refresh_interval::RefreshInterval,
    sensors::SensorSnapshot,
    tui::{app::TuiApp, theme, ui},
};
use ratatui::{
    Terminal,
    backend::TestBackend,
    buffer::{Buffer, Cell},
    style::{Color, Modifier},
    text::Span,
};

pub const SCENES: &[&str] = &[
    "wide", "compact", "themes", "config", "history", "palette", "help", "alerts", "recovery",
    "error", "feedback", "splash",
];

pub fn acquisition_time() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_700_000_000)
}

fn snapshot(index: u64) -> SensorSnapshot {
    let wave = (index % 12) as f64;
    SensorSnapshot {
        aqi: Some(32.0 + wave * 2.0),
        co2: Some(580.0 + wave * 18.0),
        pm25: Some(4.0 + wave * 0.6),
        pm1: Some(2.6 + wave * 0.3),
        pm10: Some(8.2 + wave * 0.7),
        pm03_count: Some(820.0 + wave * 40.0),
        tvoc: Some(72.0 + wave * 3.0),
        nox: Some(2.0),
        temperature_c: Some(22.4 + wave * 0.03),
        humidity: Some(46.0 + wave * 0.3),
    }
}

/// Fixture-only telemetry; never connects to a monitor or reads/writes config.
pub fn fixture(candidate: theme::Theme, scene: &str) -> TuiApp {
    let mut app = TuiApp::new(
        Some(DeviceUrl::parse("http://synthetic.invalid/").expect("fixture URL")),
        RefreshInterval::clamping(Duration::from_secs(30)),
    );
    app.theme = candidate;
    app.restore_history((0..48).map(|index| {
        Reading::new(
            acquisition_time() - Duration::from_secs((48 - index) * 30),
            snapshot(index),
        )
    }));
    app.finish_fetch_success(snapshot(48), Duration::from_millis(84), acquisition_time());
    app.previous_successful_snapshot = Some(snapshot(47));
    app.animation_frame = 8;
    match scene {
        "wide" | "compact" => {}
        "themes" => app.open_theme_settings(),
        "config" => {
            app.open_config_editor();
            app.select_config_field_index(1);
        }
        "history" => app.open_history(),
        "palette" => app.open_command_palette(),
        "help" => app.toggle_leader(),
        "alerts" | "recovery" => {
            let mut unhealthy = snapshot(48);
            unhealthy.co2 = Some(2200.0);
            app.finish_fetch_success(unhealthy, Duration::from_millis(84), acquisition_time());
            if scene == "recovery" {
                app.dismiss_alerts();
                app.finish_fetch_success(
                    snapshot(48),
                    Duration::from_millis(84),
                    acquisition_time(),
                );
            }
            assert!(app.current_alert().is_some(), "fixture must raise an alert");
        }
        "error" => app.finish_fetch_failure(
            "Synthetic fixture: monitor unavailable",
            Duration::from_secs(2),
        ),
        "feedback" => {
            app.open_command_palette();
            app.palette_input = "refresh 60".into();
            app.palette_submit();
        }
        "splash" => app.splash_frame = Some(12),
        _ => panic!("unknown preview scene: {scene}"),
    }
    assert!(app.config_path.is_none() && app.history_path.is_none());
    app
}

pub fn render(app: &TuiApp, width: u16, height: u16) -> Result<(Buffer, ui::HitMap), Infallible> {
    let mut terminal = Terminal::new(TestBackend::new(width, height))?;
    let mut hits = ui::HitMap::default();
    terminal.draw(|frame| ui::draw_with_hits_at(frame, app, &mut hits, acquisition_time()))?;
    Ok((terminal.backend().buffer().clone(), hits))
}

/// Use Ratatui's cell width, including wide/combining graphemes, not byte counts.
pub fn row_text(buffer: &Buffer, y: u16) -> String {
    let mut line = String::new();
    let mut x = 0;
    while x < buffer.area.width {
        let symbol = buffer[(x, y)].symbol();
        line.push_str(symbol);
        x += Span::raw(symbol).width().max(1) as u16;
    }
    line
}

pub fn text(buffer: &Buffer) -> String {
    (0..buffer.area.height)
        .map(|y| format!("{}\n", row_text(buffer, y)))
        .collect()
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// The sample terminal's ANSI palette, not a replacement for theme backgrounds.
const ANSI: [&str; 16] = [
    "#000000", "#800000", "#008000", "#808000", "#000080", "#800080", "#008080", "#c0c0c0",
    "#808080", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff",
];

fn color_css(color: Color, foreground: bool) -> String {
    use Color::*;
    let index = match color {
        Reset => {
            return if foreground {
                "var(--terminal-fg)"
            } else {
                "var(--terminal-bg)"
            }
            .into();
        }
        Rgb(r, g, b) => return format!("#{r:02x}{g:02x}{b:02x}"),
        Indexed(index) => return indexed_css(index),
        Black => 0,
        Red => 1,
        Green => 2,
        Yellow => 3,
        Blue => 4,
        Magenta => 5,
        Cyan => 6,
        Gray => 7,
        DarkGray => 8,
        LightRed => 9,
        LightGreen => 10,
        LightYellow => 11,
        LightBlue => 12,
        LightMagenta => 13,
        LightCyan => 14,
        White => 15,
    };
    ANSI[index].into()
}

fn indexed_css(index: u8) -> String {
    if index < 16 {
        return ANSI[usize::from(index)].into();
    }
    if index >= 232 {
        let value = 8 + (index - 232) * 10;
        return format!("#{value:02x}{value:02x}{value:02x}");
    }
    let cube = index - 16;
    let channel = |value: u8| if value == 0 { 0 } else { 55 + 40 * value };
    format!(
        "#{:02x}{:02x}{:02x}",
        channel(cube / 36),
        channel(cube / 6 % 6),
        channel(cube % 6)
    )
}

fn cell_colors(cell: &Cell) -> (String, String) {
    let fg = color_css(cell.fg, true);
    let bg = color_css(cell.bg, false);
    if cell.modifier.contains(Modifier::REVERSED) {
        (bg, fg)
    } else {
        (fg, bg)
    }
}

fn font_css(cell: &Cell) -> String {
    let mut css = String::new();
    for (modifier, rule) in [
        (Modifier::BOLD, "font-weight:bold;"),
        (Modifier::DIM, "opacity:.5;"),
        (Modifier::ITALIC, "font-style:italic;"),
        (Modifier::HIDDEN, "visibility:hidden;"),
    ] {
        if cell.modifier.contains(modifier) {
            css.push_str(rule);
        }
    }
    let underline = cell.modifier.contains(Modifier::UNDERLINED);
    let strike = cell.modifier.contains(Modifier::CROSSED_OUT);
    if underline || strike {
        let _ = write!(
            css,
            "text-decoration:{} {};",
            if underline { "underline" } else { "" },
            if strike { "line-through" } else { "" }
        );
    }
    css
}

/// Two layers retain every cell's background without covering wide glyphs.
/// Original terminal styles remain inspectable in data attributes.
pub fn buffer_html(buffer: &Buffer) -> String {
    let mut html = format!(
        "<div class=terminal style=\"width:{}ch;height:{}em\">",
        buffer.area.width,
        f64::from(buffer.area.height) * 1.3
    );
    for glyph_layer in [false, true] {
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                let cell = &buffer[(x, y)];
                let (fg, bg) = cell_colors(cell);
                let position = format!("left:{x}ch;top:{}em;", f64::from(y) * 1.3);
                if glyph_layer {
                    let width = Span::raw(cell.symbol()).width().max(1);
                    let _ = write!(
                        html,
                        "<span class=glyph style=\"{position}width:{width}ch;color:{fg};{}\">{}</span>",
                        font_css(cell),
                        escape(cell.symbol())
                    );
                } else {
                    let _ = write!(
                        html,
                        "<span class=cell data-fg=\"{:?}\" data-bg=\"{:?}\" data-modifiers=\"{:?}\" style=\"{position}background:{bg}\"></span>",
                        cell.fg, cell.bg, cell.modifier
                    );
                }
            }
        }
    }
    html.push_str("</div>");
    html
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(args.next().unwrap_or_else(|| "target/tui-preview".into()));
    let selection = args
        .next()
        .unwrap_or_else(|| theme::Theme::default_theme().id.into());
    if args.next().is_some() {
        return Err(
            "usage: cargo run --example tui_preview -- [output-directory] [theme-id|all]".into(),
        );
    }
    let candidates = if selection == "all" {
        theme::ALL.to_vec()
    } else {
        vec![theme::Theme::find(&selection).ok_or("unknown theme id")?]
    };
    fs::create_dir_all(&output)?;
    let mut html = String::from(
        "<!doctype html><html lang=en><meta charset=utf-8><title>Synthetic TUI previews</title><style>body{background:#17191e;color:#e4e8ef;font-family:system-ui;padding:24px}section{margin:32px 0;overflow:auto}.terminal{--terminal-fg:#c0c0c0;--terminal-bg:#000000;position:relative;font:14px/1.3 monospace;font-variant-ligatures:none;white-space:pre}.cell,.glyph{position:absolute;display:block;width:1ch;height:1.3em}.glyph{background:transparent}</style><h1>Synthetic fixtures — not live sensor data</h1><p>Real Ratatui TestBackend cell buffers. Fixed acquisition timestamps and animation frames. Reset colors use a declared sample terminal palette; blinking modifiers are captured in their visible phase. Nerd Font glyphs require a compatible local monospace font.</p>",
    );
    let mut frames = 0;
    for candidate in candidates {
        for scene in SCENES {
            let (width, height) = if *scene == "compact" {
                (60, 24)
            } else {
                (144, 46)
            };
            let (buffer, _) = render(&fixture(candidate, scene), width, height)?;
            let name = format!("{}-{scene}", candidate.id);
            fs::write(
                output.join(format!("{name}.txt")),
                format!(
                    "SYNTHETIC FIXTURE / {name} / {width}x{height}\n{}",
                    text(&buffer)
                ),
            )?;
            let _ = write!(
                html,
                "<section><h2>{} / {} / {width}×{height}</h2>{}</section>",
                escape(candidate.label),
                scene,
                buffer_html(&buffer)
            );
            frames += 1;
        }
    }
    html.push_str("</html>");
    fs::write(output.join("index.html"), html)?;
    println!(
        "Rendered {frames} synthetic cell buffers to {}",
        output.display()
    );
    Ok(())
}
