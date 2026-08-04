use std::path::PathBuf;

/// Represents a detected shell with its configuration.
#[derive(Debug, Clone)]
pub struct ShellInfo {
    pub name: String,
    pub export_file: PathBuf,
    pub export_format: String,
}

/// Detects the user's shell and returns shell information.
pub fn detect_shell() -> (String, PathBuf, String) {
    let shell_env = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let shell_name = PathBuf::from(&shell_env)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    match shell_name.as_str() {
        "bash" => (
            "bash".to_string(),
            detect_bash_profile(),
            "export {VAR}=\"{VALUE}\"".to_string(),
        ),
        "zsh" => (
            "zsh".to_string(),
            detect_zsh_profile(),
            "export {VAR}=\"{VALUE}\"".to_string(),
        ),
        "fish" => (
            "fish".to_string(),
            detect_fish_config(),
            "set -gx {VAR} \"{VALUE}\"".to_string(),
        ),
        _ => (
            shell_name.clone(),
            PathBuf::from("/etc/shell_exports".to_string()),
            "export {VAR}=\"{VALUE}\"".to_string(),
        ),
    }
}

fn detect_bash_profile() -> PathBuf {
    if let Some(home) = home_dir() {
        let profile = home.join(".bash_profile");
        if profile.exists() {
            return profile;
        }
        let bashrc = home.join(".bashrc");
        if bashrc.exists() {
            return bashrc;
        }
        let profile_d = home.join(".profile");
        if profile_d.exists() {
            return profile_d;
        }
        home.join(".bash_profile")
    } else {
        PathBuf::from("/etc/bash_profile")
    }
}

fn detect_zsh_profile() -> PathBuf {
    if let Some(home) = home_dir() {
        let zprofile = home.join(".zprofile");
        if zprofile.exists() {
            return zprofile;
        }
        let zshrc = home.join(".zshrc");
        if zshrc.exists() {
            return zshrc;
        }
        let profile = home.join(".profile");
        if profile.exists() {
            return profile;
        }
        home.join(".zshrc")
    } else {
        PathBuf::from("/etc/zshrc")
    }
}

fn detect_fish_config() -> PathBuf {
    if let Some(config_home) = config_home_dir() {
        let fish_config = config_home.join("fish");
        if fish_config.exists() {
            let config_fish = fish_config.join("config.fish");
            if config_fish.exists() {
                return config_fish;
            }
        }
    }
    if let Some(home) = home_dir() {
        let fish_home = home.join(".config/fish");
        if fish_home.exists() {
            let config_fish = fish_home.join("config.fish");
            if config_fish.exists() {
                return config_fish;
            }
        }
        home.join(".config/fish/config.fish")
    } else {
        PathBuf::from("/etc/fish/config.fish")
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var("HOME").ok().map(PathBuf::from)
}

fn config_home_dir() -> Option<PathBuf> {
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| home_dir().map(|h| h.join(".config")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_shell_returns_name() {
        let result = detect_shell();
        assert!(!result.0.is_empty());
        assert!(matches!(
            result.0.as_str(),
            "bash" | "zsh" | "fish" | "unknown"
        ));
    }

    #[test]
    fn test_shell_export_format() {
        let (name, _, format) = detect_shell();
        assert!(format.contains("{VAR}"));
        assert!(format.contains("{VALUE}"));
        match name.as_str() {
            "fish" => assert!(format.contains("set -gx")),
            "bash" | "zsh" => assert!(format.contains("export")),
            _ => {}
        }
    }

    #[test]
    fn test_home_dir() {
        let home = home_dir();
        assert!(home.is_some());
    }
}
