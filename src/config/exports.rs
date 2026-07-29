use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

use crate::utils::ui::Ui;

const EXPORT_MARKER_START: &str = "# === granite-cli exports start ===";
const EXPORT_MARKER_END: &str = "# === granite-cli exports end ===";

pub struct Exporter {
    pub shell_name: String,
    pub export_file: String,
    pub export_format: String,
}

impl Exporter {
    pub fn new(shell_name: String, export_file: String, export_format: String) -> Self {
        Self {
            shell_name,
            export_file,
            export_format,
        }
    }

    pub fn generate_export(&self, var: &str, value: &str) -> String {
        self.export_format
            .replace("{VAR}", var)
            .replace("{VALUE}", value)
    }

    pub fn add_exports(&self, ui: &dyn Ui, vars: &[(&str, &str)]) -> Result<()> {
        let path = Path::new(&self.export_file);
        let existing = if path.exists() {
            fs::read_to_string(path)
                .with_context(|| format!("Failed to read export file: {}", self.export_file))?
        } else {
            String::new()
        };

        let mut new_content = String::new();
        let mut in_granite_section = false;

        for line in existing.lines() {
            if line.trim() == EXPORT_MARKER_START {
                in_granite_section = true;
            }
            if !in_granite_section {
                new_content.push_str(line);
                new_content.push('\n');
            }
            if line.trim() == EXPORT_MARKER_END {
                in_granite_section = false;
            }
        }

        if !new_content.ends_with('\n') && !new_content.is_empty() {
            new_content.push('\n');
        }

        new_content.push_str(EXPORT_MARKER_START);
        new_content.push('\n');

        for (var, value) in vars {
            new_content.push_str(&self.generate_export(var, value));
            new_content.push('\n');
        }

        new_content.push_str(EXPORT_MARKER_END);
        new_content.push('\n');

        if path.exists() {
            let backup = format!("{}.granite-cli-backup", self.export_file);
            if let Err(e) = fs::write(&backup, &existing) {
                ui.warn(&format!("Could not create backup at {}: {}", backup, e));
            }
        }

        fs::write(path, new_content)
            .with_context(|| format!("Failed to write export file: {}", self.export_file))?;

        Ok(())
    }

    pub fn remove_exports(&self) -> Result<()> {
        let path = Path::new(&self.export_file);
        if !path.exists() {
            return Ok(());
        }

        let existing = fs::read_to_string(path)
            .with_context(|| format!("Failed to read export file: {}", self.export_file))?;

        let mut new_content = String::new();
        let mut in_granite_section = false;

        for line in existing.lines() {
            if line.trim() == EXPORT_MARKER_START {
                in_granite_section = true;
                continue;
            }
            if line.trim() == EXPORT_MARKER_END {
                in_granite_section = false;
                continue;
            }
            if !in_granite_section {
                new_content.push_str(line);
                new_content.push('\n');
            }
        }

        fs::write(path, new_content)
            .with_context(|| format!("Failed to write export file: {}", self.export_file))?;

        Ok(())
    }

    pub fn check_shell_profile_updated(&self) -> bool {
        let path = Path::new(&self.export_file);
        if !path.exists() {
            return false;
        }

        let content = fs::read_to_string(path)
            .unwrap_or_default();

        content.contains(EXPORT_MARKER_START) && content.contains(EXPORT_MARKER_END)
    }
}

pub fn detect_shell_profile() -> (String, String) {
    use crate::config::shell::detect_shell;
    let (name, path, _) = detect_shell();
    (name, path.to_string_lossy().to_string())
}

pub fn print_export_instructions(ui: &dyn Ui, vars: &[(&str, &str)], _shell_name: &str, export_file: &str) {
    ui.info(&format!(
        "\nTo persist these settings, you can export them to your shell profile:\n  Granite CLI will add the following to {}:\n",
        export_file
    ));
    for (var, value) in vars {
        let masked = if value.len() > 8 {
            format!("{}****{}", &value[..4], &value[value.len() - 4..])
        } else {
            "****".to_string()
        };
        ui.info(&format!("  export {}=\"{}\"", var, masked));
    }
    ui.info("\nRun: granite-cli configure --export to write these to your shell profile.");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::ui::base::tests::CaptureUi;
    use tempfile::TempDir;

    #[test]
    fn test_generate_export_bash() {
        let exporter = Exporter::new(
            "bash".to_string(),
            "/tmp/testrc".to_string(),
            "export {VAR}=\"{VALUE}\"".to_string(),
        );
        let result = exporter.generate_export("API_KEY", "secret123");
        assert_eq!(result, "export API_KEY=\"secret123\"");
    }

    #[test]
    fn test_generate_export_zsh() {
        let exporter = Exporter::new(
            "zsh".to_string(),
            "/tmp/testrc".to_string(),
            "export {VAR}=\"{VALUE}\"".to_string(),
        );
        let result = exporter.generate_export("BASE_URL", "http://localhost:8080");
        assert_eq!(result, "export BASE_URL=\"http://localhost:8080\"");
    }

    #[test]
    fn test_generate_export_fish() {
        let exporter = Exporter::new(
            "fish".to_string(),
            "/tmp/testrc".to_string(),
            "set -gx {VAR} \"{VALUE}\"".to_string(),
        );
        let result = exporter.generate_export("API_KEY", "secret123");
        assert_eq!(result, "set -gx API_KEY \"secret123\"");
    }

    #[test]
    fn test_add_exports_creates_file() {
        let temp_dir = TempDir::new().unwrap();
        let export_file = temp_dir.path().join("test_shellrc");
        let export_path = export_file.to_string_lossy().to_string();

        let exporter = Exporter::new(
            "bash".to_string(),
            export_path.clone(),
            "export {VAR}=\"{VALUE}\"".to_string(),
        );

        let vars = vec![
            ("API_KEY", "secret123"),
            ("BASE_URL", "http://localhost:8080"),
        ];

        exporter.add_exports(&CaptureUi::default(), &vars).unwrap();

        assert!(export_file.exists());
        let content = fs::read_to_string(&export_file).unwrap();
        assert!(content.contains(EXPORT_MARKER_START));
        assert!(content.contains(EXPORT_MARKER_END));
        assert!(content.contains("export API_KEY=\"secret123\""));
        assert!(content.contains("export BASE_URL=\"http://localhost:8080\""));
    }

    #[test]
    fn test_add_exports_preserves_existing_content() {
        let temp_dir = TempDir::new().unwrap();
        let export_file = temp_dir.path().join("test_shellrc");
        let export_path = export_file.to_string_lossy().to_string();

        // Create file with existing content
        fs::write(&export_file, "# Existing content\nexport OTHER_VAR=\"value\"\n").unwrap();

        let exporter = Exporter::new(
            "bash".to_string(),
            export_path.clone(),
            "export {VAR}=\"{VALUE}\"".to_string(),
        );

        exporter.add_exports(&CaptureUi::default(), &vec![("API_KEY", "secret123")]).unwrap();

        let content = fs::read_to_string(&export_file).unwrap();
        assert!(content.contains("# Existing content"));
        assert!(content.contains("export OTHER_VAR=\"value\""));
        assert!(content.contains("export API_KEY=\"secret123\""));
    }

    #[test]
    fn test_remove_exports() {
        let temp_dir = TempDir::new().unwrap();
        let export_file = temp_dir.path().join("test_shellrc");
        let export_path = export_file.to_string_lossy().to_string();

        let exporter = Exporter::new(
            "bash".to_string(),
            export_path.clone(),
            "export {VAR}=\"{VALUE}\"".to_string(),
        );

        exporter.add_exports(&CaptureUi::default(), &vec![("API_KEY", "secret123")]).unwrap();

        let content_after_add = fs::read_to_string(&export_file).unwrap();
        assert!(content_after_add.contains("API_KEY"));

        exporter.remove_exports().unwrap();

        let content_after_remove = fs::read_to_string(&export_file).unwrap();
        assert!(!content_after_remove.contains(EXPORT_MARKER_START));
        assert!(!content_after_remove.contains(EXPORT_MARKER_END));
        assert!(!content_after_remove.contains("API_KEY"));
    }

    #[test]
    fn test_check_shell_profile_updated() {
        let temp_dir = TempDir::new().unwrap();
        let export_file = temp_dir.path().join("test_shellrc");
        let export_path = export_file.to_string_lossy().to_string();

        let exporter = Exporter::new(
            "bash".to_string(),
            export_path.clone(),
            "export {VAR}=\"{VALUE}\"".to_string(),
        );

        assert!(!exporter.check_shell_profile_updated());

        exporter.add_exports(&CaptureUi::default(), &vec![("API_KEY", "secret123")]).unwrap();

        assert!(exporter.check_shell_profile_updated());
    }

    #[test]
    fn test_add_exports_replaces_existing_granite_section() {
        let temp_dir = TempDir::new().unwrap();
        let export_file = temp_dir.path().join("test_shellrc");
        let export_path = export_file.to_string_lossy().to_string();

        let exporter = Exporter::new(
            "bash".to_string(),
            export_path.clone(),
            "export {VAR}=\"{VALUE}\"".to_string(),
        );

        exporter.add_exports(&CaptureUi::default(), &vec![("API_KEY", "old_secret")]).unwrap();
        exporter.add_exports(&CaptureUi::default(), &vec![("API_KEY", "new_secret")]).unwrap();

        let content = fs::read_to_string(&export_file).unwrap();
        assert!(content.contains("new_secret"));
        assert!(!content.contains("old_secret"));
    }
}
