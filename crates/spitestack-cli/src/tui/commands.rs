//! Command registry for autocomplete.
//!
//! SpiteStack - Code Angry.
//!
//! Centralized command definitions with metadata for autocomplete suggestions.

/// A registered command with metadata.
#[derive(Debug, Clone, Copy)]
pub struct CommandDef {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub category: &'static str,
}

/// All available commands.
pub const COMMANDS: &[CommandDef] = &[
    CommandDef {
        name: "mix",
        aliases: &["compile", "c"],
        description: "mix your domain (compile)",
        category: "recording",
    },
    CommandDef {
        name: "play",
        aliases: &["dev", "d", "p"],
        description: "playback mode (dev server)",
        category: "recording",
    },
    CommandDef {
        name: "stop",
        aliases: &["s"],
        description: "stop the track",
        category: "recording",
    },
    CommandDef {
        name: "remix",
        aliases: &["fix", "f", "r"],
        description: "fix errors",
        category: "recording",
    },
    CommandDef {
        name: "record",
        aliases: &["rec", "init", "i"],
        description: "start new session",
        category: "recording",
    },
    CommandDef {
        name: "master",
        aliases: &["prod"],
        description: "production build",
        category: "recording",
    },
    CommandDef {
        name: "clear",
        aliases: &[],
        description: "clear output",
        category: "session",
    },
    CommandDef {
        name: "quit",
        aliases: &["q"],
        description: "exit studio",
        category: "session",
    },
    CommandDef {
        name: "help",
        aliases: &["h", "?"],
        description: "show commands",
        category: "session",
    },
];

/// Get commands matching a prefix.
///
/// If input is empty (just "/"), returns all commands.
/// Otherwise, filters to commands whose name or aliases start with the input.
pub fn get_suggestions(input: &str) -> Vec<&'static CommandDef> {
    let input = input.trim_start_matches('/').to_lowercase();

    if input.is_empty() {
        // Show all commands when just "/" is typed
        return COMMANDS.iter().collect();
    }

    COMMANDS
        .iter()
        .filter(|cmd| {
            cmd.name.starts_with(&input)
                || cmd.aliases.iter().any(|a| a.starts_with(&input))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_all_commands_on_empty() {
        let suggestions = get_suggestions("/");
        assert_eq!(suggestions.len(), COMMANDS.len());
    }

    #[test]
    fn test_filter_by_prefix() {
        let suggestions = get_suggestions("/m");
        assert!(suggestions.iter().any(|c| c.name == "mix"));
        assert!(suggestions.iter().any(|c| c.name == "master"));
        assert!(!suggestions.iter().any(|c| c.name == "quit"));
    }

    #[test]
    fn test_filter_by_alias() {
        let suggestions = get_suggestions("/c");
        assert!(suggestions.iter().any(|c| c.name == "mix")); // alias "c"
        assert!(suggestions.iter().any(|c| c.name == "clear"));
    }
}
