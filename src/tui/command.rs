//! Grammar for the `:` command palette: a small `verb [args...]` language
//! modeled on obsctl-rs's `domain::parser` (tokenize on whitespace, match a
//! lowercased verb, validate the argument count/shape), adapted for this
//! CLI's needs — setting the device URL, refresh interval, and theme, opening
//! the full config editor or theme picker, saving, and quitting.

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PaletteCommand {
    SetUrl(String),
    SetRefresh(u64),
    SetTheme(String),
    OpenConfig,
    OpenThemes,
    Save,
    Quit,
}

/// Parse one submitted palette line. Returns a human-readable error (shown
/// to the user via `palette_message`) rather than failing silently.
pub fn parse(input: &str) -> Result<PaletteCommand, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("empty command".to_string());
    }

    let (verb, rest) = match trimmed.split_once(char::is_whitespace) {
        Some((verb, rest)) => (verb, rest.trim()),
        None => (trimmed, ""),
    };

    match verb.to_ascii_lowercase().as_str() {
        "url" | "seturl" => {
            if rest.is_empty() {
                Err("usage: url <URL>".to_string())
            } else {
                Ok(PaletteCommand::SetUrl(rest.to_string()))
            }
        }
        "refresh" | "interval" => {
            if rest.is_empty() {
                return Err("usage: refresh <SECONDS>".to_string());
            }
            rest.parse::<u64>()
                .map(PaletteCommand::SetRefresh)
                .map_err(|_| {
                    format!("invalid refresh interval: {rest:?} is not a whole number of seconds")
                })
        }
        "theme" => {
            if rest.is_empty() {
                Err("usage: theme <ID>".to_string())
            } else {
                Ok(PaletteCommand::SetTheme(rest.to_string()))
            }
        }
        "config" | "settings" => Ok(PaletteCommand::OpenConfig),
        "themes" => Ok(PaletteCommand::OpenThemes),
        "save" => Ok(PaletteCommand::Save),
        "quit" | "q" => Ok(PaletteCommand::Quit),
        other => Err(format!("unknown command: {other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_url_command() {
        assert_eq!(
            parse("url 192.168.1.201"),
            Ok(PaletteCommand::SetUrl("192.168.1.201".to_string()))
        );
        assert_eq!(
            parse("seturl http://192.168.1.201/"),
            Ok(PaletteCommand::SetUrl("http://192.168.1.201/".to_string()))
        );
    }

    #[test]
    fn url_requires_an_argument() {
        assert!(parse("url").is_err());
        assert!(parse("url   ").is_err());
    }

    #[test]
    fn parses_refresh_command() {
        assert_eq!(parse("refresh 60"), Ok(PaletteCommand::SetRefresh(60)));
        assert_eq!(parse("interval 30"), Ok(PaletteCommand::SetRefresh(30)));
    }

    #[test]
    fn refresh_rejects_non_numeric_argument() {
        assert!(parse("refresh soon").is_err());
        assert!(parse("refresh").is_err());
    }

    #[test]
    fn parses_theme_command() {
        assert_eq!(
            parse("theme nord"),
            Ok(PaletteCommand::SetTheme("nord".to_string()))
        );
    }

    #[test]
    fn parses_zero_arg_commands() {
        assert_eq!(parse("config"), Ok(PaletteCommand::OpenConfig));
        assert_eq!(parse("settings"), Ok(PaletteCommand::OpenConfig));
        assert_eq!(parse("themes"), Ok(PaletteCommand::OpenThemes));
        assert_eq!(parse("save"), Ok(PaletteCommand::Save));
        assert_eq!(parse("quit"), Ok(PaletteCommand::Quit));
        assert_eq!(parse("q"), Ok(PaletteCommand::Quit));
    }

    #[test]
    fn rejects_unknown_verb() {
        assert!(parse("frobnicate").is_err());
    }

    #[test]
    fn rejects_empty_input() {
        assert!(parse("").is_err());
        assert!(parse("   ").is_err());
    }

    #[test]
    fn verbs_are_case_insensitive() {
        assert_eq!(parse("QUIT"), Ok(PaletteCommand::Quit));
        assert_eq!(
            parse("THEME dracula"),
            Ok(PaletteCommand::SetTheme("dracula".to_string()))
        );
    }
}
