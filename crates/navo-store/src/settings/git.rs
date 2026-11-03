use serde::{Deserialize, Deserializer, Serialize};

/// Which code reads repository status.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GitEngine {
    #[default]
    Auto,
    Gix,
    Git,
}

/// The `[git]` table. `fetch-minutes` loads clamped to 5 to 120.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct GitSettings {
    pub badges: bool,
    pub engine: GitEngine,
    pub tool: String,
    pub tool_command: Option<String>,
    pub fetch: bool,
    #[serde(deserialize_with = "fetch_minutes")]
    pub fetch_minutes: u16,
    pub network: bool,
}

impl Default for GitSettings {
    fn default() -> Self {
        Self {
            badges: true,
            engine: GitEngine::Auto,
            tool: "auto".to_owned(),
            tool_command: None,
            fetch: false,
            fetch_minutes: 15,
            network: false,
        }
    }
}

fn fetch_minutes<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u16, D::Error> {
    Ok(u16::deserialize(deserializer)?.clamp(5, 120))
}

#[cfg(test)]
mod tests {
    use super::super::{Settings, parse, to_toml};
    use super::{GitEngine, GitSettings};

    #[test]
    fn defaults_are_badges_on_auto_everything_and_fetch_off() {
        let git = Settings::default().git;
        assert!(git.badges && !git.fetch && !git.network);
        assert_eq!(git.engine, GitEngine::Auto);
        assert_eq!((git.tool.as_str(), git.fetch_minutes), ("auto", 15));
        assert_eq!(git.tool_command, None);
        assert_eq!(parse("").unwrap().0.git, git);
    }

    #[test]
    fn fetch_minutes_load_clamped() {
        for (text, want) in [("1", 5), ("500", 120), ("30", 30)] {
            let (settings, _) = parse(&format!("[git]\nfetch-minutes = {text}\n")).unwrap();
            assert_eq!(settings.git.fetch_minutes, want, "{text}");
        }
    }

    #[test]
    fn keys_round_trip_and_unknown_keys_are_reported() {
        let text = "[git]\nbadges = false\nengine = \"gix\"\ntool = \"fork\"\n\
                    tool-command = \"x {path}\"\nfetch = true\nfetch-minutes = 60\nnetwork = true\n";
        let (settings, unknown) = parse(text).unwrap();
        assert!(unknown.is_empty());
        let want = GitSettings {
            badges: false,
            engine: GitEngine::Gix,
            tool: "fork".to_owned(),
            tool_command: Some("x {path}".to_owned()),
            fetch: true,
            fetch_minutes: 60,
            network: true,
        };
        assert_eq!(settings.git, want);
        assert_eq!(parse(&to_toml(&settings)).unwrap().0, settings);
        assert_eq!(parse("[git]\nengin = \"git\"\n").unwrap().1, ["git.engin"]);
        assert!(parse("[git]\nengine = \"other\"\n").is_err());
    }
}
