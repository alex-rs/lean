use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::config::{ConfigError, LeanConfig};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorReport {
    pub ok: bool,
    pub config_path: String,
    pub checks: Vec<DoctorCheck>,
    pub diagnostics: Vec<DoctorDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheck {
    pub name: String,
    pub status: DoctorCheckStatus,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DoctorCheckStatus {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorDiagnostic {
    pub code: String,
    pub message: String,
}

pub fn run_doctor(config_path: impl AsRef<Path>) -> DoctorReport {
    let config_path = config_path.as_ref();
    let display_path = config_path.display().to_string();

    match LeanConfig::from_path(config_path) {
        Ok(config) => DoctorReport {
            ok: true,
            config_path: display_path,
            checks: vec![
                DoctorCheck {
                    name: "config".to_string(),
                    status: DoctorCheckStatus::Pass,
                    message: "configuration loaded".to_string(),
                },
                DoctorCheck {
                    name: "provider".to_string(),
                    status: DoctorCheckStatus::Pass,
                    message: format!(
                        "default provider '{}' is configured",
                        config.runtime.default_provider
                    ),
                },
            ],
            diagnostics: Vec::new(),
        },
        Err(error) => DoctorReport {
            ok: false,
            config_path: display_path,
            checks: vec![DoctorCheck {
                name: "config".to_string(),
                status: DoctorCheckStatus::Fail,
                message: error.to_string(),
            }],
            diagnostics: vec![DoctorDiagnostic {
                code: diagnostic_code(&error).to_string(),
                message: error.to_string(),
            }],
        },
    }
}

fn diagnostic_code(error: &ConfigError) -> &'static str {
    match error {
        ConfigError::Read { .. } => "config_read_failed",
        ConfigError::Parse { .. } => "config_parse_failed",
        ConfigError::Validation(_) => "config_validation_failed",
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{DoctorCheckStatus, run_doctor};

    #[test]
    fn valid_config_produces_success_report() {
        let report = run_doctor(fixture("valid.yaml"));

        assert!(report.ok);
        assert_eq!(report.diagnostics, []);
        assert_eq!(report.checks[0].status, DoctorCheckStatus::Pass);
    }

    #[test]
    fn invalid_config_produces_structured_diagnostic() {
        let report = run_doctor(fixture("invalid.yaml"));

        assert!(!report.ok);
        assert_eq!(report.checks[0].status, DoctorCheckStatus::Fail);
        assert_eq!(report.diagnostics[0].code, "config_validation_failed");
    }

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures/config")
            .join(name)
    }
}
