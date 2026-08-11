use crate::config::AppsConfig;

/// Represents a launchable (or preview) game door within the BBS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GameDoorKind {
    TinyHack,
    TinyMush,
}

#[derive(Debug, Clone, Copy)]
pub struct GameDoor {
    pub kind: GameDoorKind,
    pub title: &'static str,
    pub slug: &'static str,
    pub status_note: Option<&'static str>,
    pub legacy_aliases: &'static [&'static str],
}

impl GameDoor {
    pub fn display_name(self) -> &'static str {
        self.title
    }
}

pub fn enabled_doors(config: &AppsConfig) -> Vec<GameDoor> {
    let mut doors = Vec::new();
    if config.tinyhack.enabled {
        doors.push(GameDoor {
            kind: GameDoorKind::TinyHack,
            title: "TinyHack",
            slug: "tinyhack",
            status_note: None,
            legacy_aliases: &["T", "TINYHACK"],
        });
    }
    if config.tinymush.enabled {
        doors.push(GameDoor {
            kind: GameDoorKind::TinyMush,
            title: "TinyMUSH",
            slug: "tinymush",
            status_note: None,
            legacy_aliases: &["MUSH", "TINYMUSH"],
        });
    }
    doors
}

pub fn has_enabled_doors(config: &AppsConfig) -> bool {
    config.tinyhack.enabled || config.tinymush.enabled
}

pub fn format_games_menu(doors: &[GameDoor]) -> String {
    if doors.is_empty() {
        return "No games are currently enabled.\n".to_string();
    }
    let mut out = String::from("Games Menu:\n");
    for (idx, door) in doors.iter().enumerate() {
        if let Some(note) = door.status_note {
            out.push_str(&format!(
                "{:>2}) {} ({})\n",
                idx + 1,
                door.display_name(),
                note
            ));
        } else {
            out.push_str(&format!("{:>2}) {}\n", idx + 1, door.display_name()));
        }
    }
    out.push_str("Use G# or a game name to launch.\n");
    out
}

pub fn resolve_games_command<'a>(cmd_upper: &str, doors: &'a [GameDoor]) -> Option<&'a GameDoor> {
    for door in doors {
        if door
            .legacy_aliases
            .iter()
            .any(|alias| alias.eq_ignore_ascii_case(cmd_upper))
        {
            return Some(door);
        }
        if door.title.eq_ignore_ascii_case(cmd_upper) {
            return Some(door);
        }
    }

    if let Some(rest) = cmd_upper.strip_prefix('G') {
        let trimmed = rest.trim();
        if trimmed.is_empty() {
            return None;
        }
        if let Ok(idx) = trimmed.parse::<usize>() {
            if idx >= 1 && idx <= doors.len() {
                return doors.get(idx - 1);
            }
        }

        let normalized = trimmed
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        if normalized.is_empty() {
            return None;
        }
        for door in doors {
            if door.slug == normalized {
                return Some(door);
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AppsConfig;

    #[test]
    fn no_games_returns_empty_menu() {
        let mut cfg = AppsConfig::default();
        // Disable all games
        cfg.tinyhack.enabled = false;
        cfg.tinymush.enabled = false;
        let doors = enabled_doors(&cfg);
        assert!(doors.is_empty());
        assert_eq!(
            format_games_menu(&doors),
            "No games are currently enabled.\n"
        );
        assert!(resolve_games_command("G1", &doors).is_none());
    }

    #[test]
    fn tinyhack_menu_and_selection() {
        let mut cfg = AppsConfig::default();
        // Disable all first, then enable only tinyhack
        cfg.tinyhack.enabled = true;
        cfg.tinymush.enabled = false;
        let doors = enabled_doors(&cfg);
        assert_eq!(doors.len(), 1);
        assert!(format_games_menu(&doors).contains("1) TinyHack"));
        let door = resolve_games_command("G1", &doors).expect("G1 should select tinyhack");
        assert_eq!(door.kind, GameDoorKind::TinyHack);
        assert!(resolve_games_command("T", &doors).is_some());
        assert!(resolve_games_command("TINYHACK", &doors).is_some());
        assert!(resolve_games_command("G TINYHACK", &doors).is_some());
        assert!(resolve_games_command("G 1", &doors).is_some());
    }

    #[test]
    fn tinymush_preview_in_menu() {
        let mut cfg = AppsConfig::default();
        cfg.tinyhack.enabled = true;
        cfg.tinymush.enabled = true;
        let doors = enabled_doors(&cfg);
        assert_eq!(doors.len(), 2);
        let menu = format_games_menu(&doors);
        assert!(menu.contains("1) TinyHack"));
        assert!(menu.contains("2) TinyMUSH"));
        let mush = resolve_games_command("G2", &doors).expect("G2 should resolve");
        assert_eq!(mush.kind, GameDoorKind::TinyMush);
        assert!(resolve_games_command("TINYMUSH", &doors).is_some());
    }
}
