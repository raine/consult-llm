use std::io::{IsTerminal, Read};

pub fn read_prompt(prompt_file: Option<&str>) -> Result<String, CliError> {
    if let Some(path) = prompt_file {
        return std::fs::read_to_string(path)
            .map_err(|e| CliError::Usage(format!("--prompt-file: {e}")));
    }
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        return Err(CliError::Usage(
            "no prompt provided\n\n  Usage:  cat <<'EOF' | consult-llm [-m <model>] [-f <file>]\n          Your question here.\n          EOF\n\n  From your agent: /consult, /debate, /collab\n  Run 'consult-llm --help' for full options."
                .into(),
        ));
    }
    let mut buf = String::new();
    stdin
        .lock()
        .read_to_string(&mut buf)
        .map_err(|e| CliError::Generic(format!("stdin read failed: {e}")))?;
    Ok(buf)
}

#[derive(Debug)]
pub enum CliError {
    Usage(String),
    Config(String),
    Generic(String),
    Domain { code: &'static str, message: String },
    System { code: &'static str, message: String },
}

impl From<anyhow::Error> for CliError {
    fn from(e: anyhow::Error) -> Self {
        Self::Generic(e.to_string())
    }
}

impl CliError {
    pub fn domain(code: &'static str, message: impl Into<String>) -> Self {
        Self::Domain {
            code,
            message: message.into(),
        }
    }

    pub fn system(code: &'static str, message: impl Into<String>) -> Self {
        Self::System {
            code,
            message: message.into(),
        }
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Domain { .. } => 1,
            Self::System { .. } => 2,
            Self::Generic(_) => 1,
            Self::Usage(_) => 2,
            Self::Config(_) => 3,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            Self::Usage(_) => "invalid_usage",
            Self::Config(_) => "invalid_config",
            Self::Generic(_) => "operation_failed",
            Self::Domain { code, .. } | Self::System { code, .. } => code,
        }
    }

    pub fn message(&self) -> &str {
        match self {
            Self::Usage(s) | Self::Config(s) | Self::Generic(s) => s,
            Self::Domain { message, .. } | Self::System { message, .. } => message,
        }
    }
}
