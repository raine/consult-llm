use serde::Deserialize;

pub use crate::models::TaskMode;

/// Accepts either a single model identifier or an array of identifiers.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ModelSelector {
    Many(Vec<String>),
    One(String),
}

impl ModelSelector {
    pub fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(s) => vec![s],
            Self::Many(v) => v,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct GitDiffArgs {
    /// Path to git repository (defaults to current working directory)
    pub repo_path: Option<String>,
    /// Specific files to include in diff
    pub files: Vec<String>,
    /// Git reference to compare against (e.g., "HEAD", "main", commit hash)
    #[serde(default = "default_base_ref")]
    pub base_ref: String,
}

fn default_base_ref() -> String {
    "HEAD".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConsultLlmArgs {
    /// Your question or request for the consultant LLM. Ask neutral, open-ended questions without suggesting specific solutions to avoid biasing the analysis.
    pub prompt: String,
    /// Array of file paths to include as context. All files are added as context with file paths and code blocks.
    pub files: Option<Vec<String>>,
    /// Optional model selector. Pass a single string (e.g. "gemini") for one model, or an array (e.g. ["gemini", "openai"]) to consult multiple models in parallel. Usually omit to use the configured default. Max 5 models per call.
    pub model: Option<ModelSelector>,
    /// Controls the system prompt persona. Choose based on the task: "review": critical code reviewer for finding bugs, security issues, and quality problems. "debug": focused troubleshooter for root cause analysis from errors, logs, and stack traces — ignores style issues. "plan": constructive architect for exploring trade-offs and designing solutions — always includes a final recommendation. "create": generative writer for producing documentation, content, or designs. "general" (default): neutral prompt that defers to your instructions in the prompt field.
    #[serde(default)]
    pub task_mode: TaskMode,
    /// If true, copy the formatted prompt to the clipboard instead of querying an LLM. When true, the `model` parameter is ignored. Use this to paste the prompt into browser-based LLM services. IMPORTANT: Only use this when the user specifically requests it. When true, wait for the user to provide the external LLM's response before proceeding with any implementation.
    #[serde(default)]
    pub web_mode: bool,
    /// Thread/session ID for resuming a conversation. Works with all backends. CLI backends maintain native sessions; API backends replay conversation history from disk. Returned in the response prefix as [thread_id:xxx].
    pub thread_id: Option<String>,
    /// Provide git diff context. Repository-capable CLI backends inspect the requested diff directly; other backends receive its contents. Shows uncommitted changes by default.
    pub git_diff: Option<GitDiffArgs>,
}
