//! The `<Space>` leader sequence, modeled on AstroNvim's which-key.
//!
//! Pressing the leader does not act on its own — it arms a pending state and
//! shows the popup listing what the next key does. That is the whole point of
//! which-key: the menu is discoverable rather than memorized, so an unbound
//! second key cancels quietly instead of guessing.

use super::TuiApp;

/// One row of the which-key popup: the key to press, and what it does.
///
/// Mnemonics follow AstroNvim's conventions where they overlap (`q` quit,
/// `t` toggles/themes, `c` config), so muscle memory carries over.
pub struct LeaderBinding {
    pub key: char,
    pub label: &'static str,
}

pub const LEADER_BINDINGS: &[LeaderBinding] = &[
    LeaderBinding {
        key: 'r',
        label: "Refresh now",
    },
    LeaderBinding {
        key: 't',
        label: "Themes",
    },
    LeaderBinding {
        key: 'c',
        label: "Config editor",
    },
    LeaderBinding {
        key: ':',
        label: "Command palette",
    },
    LeaderBinding {
        key: '+',
        label: "Longer interval",
    },
    LeaderBinding {
        key: '-',
        label: "Shorter interval",
    },
    LeaderBinding {
        key: 'q',
        label: "Quit",
    },
];

/// What the runtime should do once the leader sequence resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderAction {
    Refresh,
    OpenThemes,
    OpenConfig,
    OpenPalette,
    LongerInterval,
    ShorterInterval,
    Quit,
    /// The key was not bound — close the popup and do nothing else.
    Dismiss,
}

impl TuiApp {
    /// Arms the leader. A second `<Space>` cancels, matching how which-key
    /// lets you back out with the same key you came in on.
    pub fn toggle_leader(&mut self) {
        self.leader_pending = !self.leader_pending;
        self.pending_g = false;
    }

    pub fn cancel_leader(&mut self) {
        self.leader_pending = false;
    }

    /// Resolves the second key of the sequence and disarms.
    pub fn resolve_leader(&mut self, key: char) -> LeaderAction {
        self.leader_pending = false;

        match key {
            'r' => LeaderAction::Refresh,
            't' => LeaderAction::OpenThemes,
            'c' => LeaderAction::OpenConfig,
            ':' => LeaderAction::OpenPalette,
            '+' | '=' => LeaderAction::LongerInterval,
            '-' | '_' => LeaderAction::ShorterInterval,
            'q' => LeaderAction::Quit,
            _ => LeaderAction::Dismiss,
        }
    }
}
