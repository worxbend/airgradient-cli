//! Tests for theme lookup, palette coverage, and derived styles.

use super::{
    palette::{DEFAULT, MONO},
    *,
};

#[test]
fn by_id_falls_back_to_default_for_unknown_name() {
    assert_eq!(Theme::by_id("does-not-exist"), Theme::default_theme());
}

#[test]
fn by_id_is_case_insensitive() {
    assert_eq!(Theme::by_id("BTOP").id, "btop");
    assert_eq!(Theme::by_id("Nord").id, "nord");
}

#[test]
fn by_id_resolves_default_id() {
    assert_eq!(Theme::by_id("default"), DEFAULT);
    assert_eq!(Theme::by_id("DEFAULT"), DEFAULT);
}

#[test]
fn all_themes_have_unique_ids() {
    let mut ids: Vec<&str> = ALL.iter().map(|theme| theme.id).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), ALL.len());
}

#[test]
fn there_are_twenty_built_in_themes() {
    assert_eq!(ALL.len(), 20);
}

#[test]
fn index_matches_position_in_all() {
    for (i, theme) in ALL.iter().enumerate() {
        assert_eq!(theme.index(), i);
    }
}

#[test]
fn mono_theme_does_not_override_terminal_background() {
    assert_eq!(MONO.bg, Color::Reset);
}

#[test]
fn blend_at_zero_returns_base_and_at_one_returns_tint() {
    let base = Color::Rgb(10, 20, 30);
    let tint = Color::Rgb(200, 150, 100);
    assert_eq!(blend(base, tint, 0.0), base);
    assert_eq!(blend(base, tint, 1.0), tint);
}

#[test]
fn blend_passes_through_non_rgb_colors() {
    assert_eq!(blend(Color::Reset, Color::Rgb(1, 2, 3), 0.5), Color::Reset);
}

#[test]
fn status_color_covers_every_status_distinctly_for_default_theme() {
    let theme = Theme::default_theme();
    let colors = [
        theme.status_color(Status::Unknown),
        theme.status_color(Status::Good),
        theme.status_color(Status::Moderate),
        theme.status_color(Status::Elevated),
        theme.status_color(Status::Unhealthy),
        theme.status_color(Status::VeryUnhealthy),
    ];
    for (i, a) in colors.iter().enumerate() {
        for (j, b) in colors.iter().enumerate() {
            if i != j {
                assert_ne!(a, b, "status colors {i} and {j} should differ");
            }
        }
    }
}
