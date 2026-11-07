use std::io::Read;
use std::time::Duration;

use super::sse::{SseEvent, SseParser};

/// Error-message labels for one provider's transport. Kept short and
/// provider-specific so log lines remain greppable per-provider.
#[derive(Clone, Copy)]
pub struct StreamLabels {
    /// Used in `"{request} to {model} failed: ..."` and
    /// `"{request} failed with status ..."`.
    pub request: &'static str,
    /// Used in `"{stream} idle timeout: ..."`, `"{stream} error for ..."`,
    /// and `"{stream} parse error for ..."`.
    pub stream: &'static str,
}

/// Inputs to a streaming POST request. The transport owns the wire phase
/// (send + status check + SSE read loop + idle-timeout error mapping); the
/// caller owns body construction and per-event decoding.
pub struct StreamRequest<'a> {
    pub agent: &'a ureq::Agent,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub idle_timeout: Duration,
    pub model: String,
    pub labels: StreamLabels,
}

/// Per-event decoder. Returning `Ok(true)` ends the stream loop early.
/// Called for every event surfaced by `SseParser::feed` and once more for
/// any event emitted by `SseParser::flush()` after EOF.
pub trait StreamHandler {
    type Outcome;

    fn on_event(&mut self, ev: &SseEvent) -> anyhow::Result<bool>;
    fn finish(self, model: &str) -> anyhow::Result<Self::Outcome>;
}

pub fn run_stream<H: StreamHandler>(
    req: StreamRequest,
    mut handler: H,
) -> anyhow::Result<H::Outcome> {
    let StreamRequest {
        agent,
        url,
        headers,
        body,
        idle_timeout,
        model,
        labels,
    } = req;

    let idle_secs = idle_timeout.as_secs();
    let mut builder = agent
        .post(&url)
        .config()
        // Per-read socket idle: ureq applies this as a fresh budget on every
        // read, so heartbeat bytes (and any data, parsed or not) reset the
        // timer. The agent-level timeout_global bounds the whole request as
        // an absolute backstop.
        .timeout_recv_body(Some(idle_timeout))
        .http_status_as_error(false)
        .build();
    for (k, v) in headers {
        builder = builder.header(k, v);
    }

    let resp = builder.send(&body[..]);
    let mut resp = match resp {
        Ok(r) => r,
        Err(e) => anyhow::bail!("{} to {model} failed: {e}", labels.request),
    };

    if !resp.status().is_success() {
        let status = resp.status();
        let body_text = resp.body_mut().read_to_string().unwrap_or_default();
        anyhow::bail!(
            "{} failed with status {status}: {body_text}",
            labels.request
        );
    }

    let mut reader = resp.into_body().into_reader();
    let mut sse = SseParser::new();
    let mut buf = [0u8; 8192];

    let mut stopped = false;
    'outer: loop {
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) => {
                if is_timeout_err(&e) {
                    anyhow::bail!(
                        "{} idle timeout: no bytes from {model} for {idle_secs}s",
                        labels.stream
                    );
                }
                anyhow::bail!("{} error for {model}: {e}", labels.stream);
            }
        };
        let events = sse
            .feed(&buf[..n])
            .map_err(|e| anyhow::anyhow!("{} parse error for {model}: {e}", labels.stream))?;
        for ev in events {
            if handler.on_event(&ev)? {
                stopped = true;
                break 'outer;
            }
        }
    }
    if !stopped && let Some(ev) = sse.flush() {
        let _ = handler.on_event(&ev)?;
    }
    handler.finish(&model)
}

/// True if the IO error originated from a ureq timeout (or stdlib TimedOut).
/// `timeout_recv_body` raises a `ureq::Error::Timeout` which surfaces as
/// `io::Error` with kind `TimedOut` when reading from `BodyReader`.
fn is_timeout_err(e: &std::io::Error) -> bool {
    if e.kind() == std::io::ErrorKind::TimedOut {
        return true;
    }
    let s = e.to_string();
    s.contains("timeout") || s.contains("Timeout")
}

#[cfg(test)]
mod tests {
    //! End-to-end mock-server tests for the shared API transport. Both the
    //! OpenAI-compatible (`ApiExecutor`) and Anthropic (`AnthropicApiExecutor`)
    //! executors are pointed at an in-process TCP listener and exercised
    //! through a real `ureq::Agent`. We verify (a) the request body the
    //! shared transport sends matches a recorded golden for each provider,
    //! and (b) decoding a recorded response stream produces the expected
    //! `ExecuteResult`.

    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};
    use std::thread;

    use consult_llm_core::monitoring::{RunEvent, RunEventKind, RunMeta, RunSpool};
    use consult_llm_core::stream_events::ParsedStreamEvent;

    use super::super::anthropic_api::AnthropicApiExecutor;
    use super::super::api::ApiExecutor;
    use super::super::thread_store::{self, StoredThread, StoredTurn};
    use super::super::types::{ExecuteResult, ExecutionRequest, LlmExecutor};
    use super::*;
    use crate::models::{ApiProtocol, Provider};

    type RecordedHeaders = Arc<Mutex<Vec<(String, String)>>>;

    /// Spin up a one-shot HTTP/1.1 server on 127.0.0.1 that accepts a single
    /// POST, records the request body and headers, and replies with
    /// `response_bytes`. Returns `(base_url, recorded_body, recorded_headers)`.
    fn start_mock_server(
        response_bytes: Vec<u8>,
    ) -> (String, Arc<Mutex<Vec<u8>>>, RecordedHeaders) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let recorded_body = Arc::new(Mutex::new(Vec::<u8>::new()));
        let recorded_headers = Arc::new(Mutex::new(Vec::<(String, String)>::new()));
        let recorded_body_clone = Arc::clone(&recorded_body);
        let recorded_headers_clone = Arc::clone(&recorded_headers);

        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .ok();

            let mut all = Vec::<u8>::new();
            let mut buf = [0u8; 4096];
            // Read until headers complete + we have the full body per
            // Content-Length. The executors always send Content-Length
            // (no chunked uploads).
            let body = loop {
                let n = match stream.read(&mut buf) {
                    Ok(0) | Err(_) => break Vec::new(),
                    Ok(n) => n,
                };
                all.extend_from_slice(&buf[..n]);
                if let Some(hdr_end) = find_subslice(&all, b"\r\n\r\n") {
                    let headers = std::str::from_utf8(&all[..hdr_end]).unwrap_or("");
                    *recorded_headers_clone.lock().unwrap() = parse_headers(headers);
                    let cl: usize = headers
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            (k.eq_ignore_ascii_case("content-length"))
                                .then(|| v.trim().parse::<usize>().ok())
                                .flatten()
                        })
                        .unwrap_or(0);
                    let body_start = hdr_end + 4;
                    while all.len() - body_start < cl {
                        let n = match stream.read(&mut buf) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => n,
                        };
                        all.extend_from_slice(&buf[..n]);
                    }
                    break all[body_start..body_start + cl].to_vec();
                }
            };
            *recorded_body_clone.lock().unwrap() = body;

            let _ = stream.write_all(&response_bytes);
            let _ = stream.flush();
            // Half-close so ureq sees EOF and stops reading.
            let _ = stream.shutdown(std::net::Shutdown::Write);
        });

        (
            format!("http://127.0.0.1:{port}"),
            recorded_body,
            recorded_headers,
        )
    }

    fn parse_headers(headers: &str) -> Vec<(String, String)> {
        headers
            .lines()
            .skip(1)
            .filter_map(|line| {
                let (name, value) = line.split_once(':')?;
                Some((name.to_ascii_lowercase(), value.trim().to_string()))
            })
            .collect()
    }

    fn assert_header(headers: &RecordedHeaders, name: &str, expected: &str) {
        let name = name.to_ascii_lowercase();
        let values: Vec<String> = headers
            .lock()
            .unwrap()
            .iter()
            .filter(|(k, _)| k == &name)
            .map(|(_, v)| v.clone())
            .collect();
        assert_eq!(values, vec![expected.to_string()]);
    }

    fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        if needle.is_empty() || haystack.len() < needle.len() {
            return None;
        }
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    fn http_response(body: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
        out.extend_from_slice(b"Content-Type: text/event-stream\r\n");
        out.extend_from_slice(b"Connection: close\r\n");
        out.extend_from_slice(b"\r\n");
        out.extend_from_slice(body);
        out
    }

    fn isolate_state_dir() -> tempfile::TempDir {
        // Test runs may persist stored threads via ApiChatSession::commit_turn.
        // Pin XDG_STATE_HOME to a tempdir so we never write to the user's real
        // state directory. The lock keeps us serialized with any other test
        // in this crate that mutates XDG_STATE_HOME — see `test_util`.
        let tmp = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("XDG_STATE_HOME", tmp.path());
        }
        tmp
    }

    struct RecordingHandler {
        seen: Vec<String>,
        stop_on: &'static str,
    }

    impl super::StreamHandler for RecordingHandler {
        type Outcome = Vec<String>;

        fn on_event(&mut self, ev: &SseEvent) -> anyhow::Result<bool> {
            self.seen.push(ev.data.clone());
            Ok(ev.data == self.stop_on)
        }

        fn finish(self, _model: &str) -> anyhow::Result<Self::Outcome> {
            Ok(self.seen)
        }
    }

    fn recording_stream_request(url: String) -> StreamRequest<'static> {
        let agent = Box::leak(Box::new(ureq::Agent::new_with_defaults()));
        StreamRequest {
            agent,
            url,
            headers: vec![("Content-Type".to_string(), "application/json".to_string())],
            body: b"{}".to_vec(),
            idle_timeout: std::time::Duration::from_secs(5),
            model: "test-model".to_string(),
            labels: StreamLabels {
                request: "test request",
                stream: "test stream",
            },
        }
    }

    fn run_recording_stream(response_body: &[u8], stop_on: &'static str) -> Vec<String> {
        let response = http_response(response_body);
        let (base, _recorded, _headers) = start_mock_server(response);
        run_stream(
            recording_stream_request(base),
            RecordingHandler {
                seen: Vec::new(),
                stop_on,
            },
        )
        .unwrap()
    }

    fn execute_openai_compat_mock(
        response_body: &[u8],
        provider: Provider,
        api_key: &str,
        req: ExecutionRequest,
    ) -> (ExecuteResult, serde_json::Value) {
        execute_openai_compat_mock_with_effort(response_body, provider, api_key, req, None)
    }

    fn execute_openai_compat_mock_with_effort(
        response_body: &[u8],
        provider: Provider,
        api_key: &str,
        req: ExecutionRequest,
        reasoning_effort: Option<String>,
    ) -> (ExecuteResult, serde_json::Value) {
        let (base, recorded, _headers) = start_mock_server(http_response(response_body));
        let executor = ApiExecutor::new(
            ureq::Agent::new_with_defaults(),
            api_key.to_string(),
            Some(format!("{base}/custom/")),
            std::time::Duration::from_secs(5),
            runtime_for(provider),
            reasoning_effort,
        );
        let result = executor.execute(req).expect("execute");
        let body: serde_json::Value =
            serde_json::from_slice(&recorded.lock().unwrap()).expect("req json");
        (result, body)
    }

    #[test]
    fn run_stream_honors_terminal_event_from_eof_flush() {
        let seen = run_recording_stream(b"data: stop", "stop");
        assert_eq!(seen, vec!["stop"]);
    }

    #[test]
    fn run_stream_skips_flush_after_in_loop_stop() {
        let seen = run_recording_stream(b"data: stop\n\ndata: later", "stop");
        assert_eq!(seen, vec!["stop"]);
    }

    fn build_request(prompt: &str, model: &str) -> (ExecutionRequest, Arc<Mutex<RunSpool>>) {
        build_request_with_spool(prompt, model, RunSpool::disabled())
    }

    fn build_request_with_spool(
        prompt: &str,
        model: &str,
        spool: RunSpool,
    ) -> (ExecutionRequest, Arc<Mutex<RunSpool>>) {
        let spool = Arc::new(Mutex::new(spool));
        let req = ExecutionRequest {
            prompt: prompt.to_string(),
            model: model.to_string(),
            system_prompt: "you are helpful".to_string(),
            file_paths: None,
            thread_id: None,
            spool: Arc::clone(&spool),
        };
        (req, spool)
    }

    fn build_resume_request(prompt: &str, model: &str, thread_id: &str) -> ExecutionRequest {
        let (mut req, _spool) = build_request(prompt, model);
        req.thread_id = Some(thread_id.to_string());
        req
    }

    fn save_thread(id: &str) {
        thread_store::save(&StoredThread {
            id: id.to_string(),
            turns: vec![StoredTurn {
                user_prompt: "old prompt".to_string(),
                assistant_response: "old answer".to_string(),
                model: "old-model".to_string(),
                usage: None,
            }],
        })
        .expect("save thread");
    }

    fn runtime_for(provider: Provider) -> crate::models::OpenAiCompatRuntime {
        let ApiProtocol::OpenAiCompat(runtime) = provider.api_protocol() else {
            panic!("{provider:?} is not OpenAI-compatible");
        };
        runtime
    }

    fn active_spool(model: &str) -> (tempfile::TempDir, RunSpool) {
        let state = isolate_state_dir();
        let spool = RunSpool::new(RunMeta {
            v: 1,
            run_id: format!("run-{model}"),
            pid: std::process::id(),
            started_at: "t".into(),
            project: "p".into(),
            cwd: "/tmp".into(),
            model: model.into(),
            backend: "api".into(),
            thread_id: None,
            task_mode: None,
            reasoning_effort: None,
        });
        (state, spool)
    }

    fn recorded_thinking_events(run_id: &str) -> Vec<String> {
        let path = consult_llm_core::monitoring::runs_dir().join(format!("{run_id}.events.jsonl"));
        let events = std::fs::read_to_string(path).unwrap();
        events
            .lines()
            .filter_map(|line| serde_json::from_str::<RunEvent>(line).ok())
            .filter_map(|event| match event.kind {
                RunEventKind::Stream {
                    event: ParsedStreamEvent::Thinking { text },
                } => Some(text),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn mock_server_gemini_25_keeps_thought_tags_without_google_thinking_config() {
        let _xdg_guard = crate::test_util::XDG_STATE_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (state, spool) = active_spool("gemini-2.5-pro");

        let gemini_sse = b"\
data: {\"choices\":[{\"delta\":{\"content\":\"<thought>check\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\"ing</thought>Hello\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n\
";
        let (req, _spool) = build_request_with_spool("hi", "gemini-2.5-pro", spool);
        let (result, body) =
            execute_openai_compat_mock(gemini_sse, Provider::Gemini, "test-gemini-key", req);
        assert_eq!(result.response, "Hello world");
        assert!(body.get("extra_body").is_none());
        assert_eq!(
            recorded_thinking_events("run-gemini-2.5-pro"),
            vec!["check", "ing"]
        );
        drop(state);
    }

    #[test]
    fn mock_server_gemini_3_includes_google_thinking_level() {
        let gemini_sse = b"\
data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n\
";
        let (req, _spool) = build_request("hi", "gemini-3.1-pro-preview");
        let (result, body) =
            execute_openai_compat_mock(gemini_sse, Provider::Gemini, "test-gemini-key", req);
        assert_eq!(result.response, "Hello");
        assert_eq!(
            body["extra_body"],
            serde_json::json!({
                "google": {
                    "thinking_config": {
                        "thinking_level": "high",
                        "include_thoughts": true
                    }
                }
            })
        );
    }

    #[test]
    fn mock_server_openrouter_includes_reasoning_effort() {
        let openrouter_sse = b"\
data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n\
";
        let (req, _spool) = build_request("hi", "openrouter/z-ai/glm-5.2");
        let (result, body) = execute_openai_compat_mock_with_effort(
            openrouter_sse,
            Provider::OpenRouter,
            "test-openrouter-key",
            req,
            Some("high".into()),
        );
        assert_eq!(result.response, "Hello");
        assert_eq!(body["reasoning"], serde_json::json!({"effort": "high"}));
        assert_eq!(body["model"], "z-ai/glm-5.2");
    }

    #[test]
    fn mock_server_api_model_ids() {
        let sse = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
        for (provider, model, expected) in [
            (
                Provider::OpenRouter,
                "openrouter/deepseek/deepseek-v4-pro",
                "deepseek/deepseek-v4-pro",
            ),
            (
                Provider::OpenRouter,
                "openrouter/moonshotai/kimi-k3.1-pro",
                "moonshotai/kimi-k3.1-pro",
            ),
            (
                Provider::OpenRouter,
                "openrouter/z-ai/glm-5.3",
                "z-ai/glm-5.3",
            ),
            (Provider::OpenRouter, "openrouter/auto", "openrouter/auto"),
            (
                Provider::OpenRouter,
                "openrouter/openrouter/auto",
                "openrouter/auto",
            ),
            (Provider::DeepSeek, "deepseek-chat", "deepseek-chat"),
            (Provider::OpenAI, "gpt-5.2", "gpt-5.2"),
        ] {
            let (req, _spool) = build_request("hi", model);
            let (result, body) = execute_openai_compat_mock(sse, provider, "test-key", req);
            assert_eq!(result.response, "Hello");
            assert_eq!(body["model"], expected, "request model for {model}");
        }
    }

    #[test]
    fn mock_server_minimax_runtime_metadata_ignores_base_url() {
        let _xdg_guard = crate::test_util::XDG_STATE_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let (state, spool) = active_spool("MiniMax-M2.7");

        let minimax_sse = b"\
data: {\"choices\":[{\"delta\":{\"content\":\"<think>plan</think>Answer\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\" done\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n\
";
        let (req, _spool) = build_request_with_spool("hi", "MiniMax-M2.7", spool);
        let (result, body) =
            execute_openai_compat_mock(minimax_sse, Provider::MiniMax, "test-minimax-key", req);
        assert_eq!(result.response, "Answer done");
        assert!(body.get("extra_body").is_none());
        assert_eq!(recorded_thinking_events("run-MiniMax-M2.7"), vec!["plan"]);
        drop(state);
    }

    #[test]
    fn openai_compat_resume_request_replays_stored_turns_after_system_prompt() {
        let _xdg_guard = crate::test_util::XDG_STATE_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let _state = isolate_state_dir();
        let thread_id = "api_resume_openai";
        save_thread(thread_id);

        let openai_sse = b"\
data: {\"choices\":[{\"delta\":{\"content\":\"new answer\"},\"finish_reason\":\"stop\"}]}\n\n\
data: [DONE]\n\n\
";
        let req = build_resume_request("new prompt", "gpt-test", thread_id);
        let (result, body) =
            execute_openai_compat_mock(openai_sse, Provider::OpenAI, "test-openai-key", req);

        assert_eq!(result.response, "new answer");
        assert_eq!(result.thread_id, Some(thread_id.to_string()));
        assert_eq!(
            body["messages"],
            serde_json::json!([
                {"role": "system", "content": "you are helpful"},
                {"role": "user", "content": "old prompt"},
                {"role": "assistant", "content": "old answer"},
                {"role": "user", "content": "new prompt"},
            ])
        );
    }

    #[test]
    fn anthropic_resume_request_replays_stored_turns_without_system_message_role() {
        let _xdg_guard = crate::test_util::XDG_STATE_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let _state = isolate_state_dir();
        let thread_id = "api_resume_anthropic";
        save_thread(thread_id);

        let anthropic_sse = b"\
data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":9,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":0}}}\n\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"new answer\"}}\n\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\n\
data: {\"type\":\"message_stop\"}\n\n\
";
        let (base, recorded, _headers) = start_mock_server(http_response(anthropic_sse));
        let anthropic = AnthropicApiExecutor::new(
            ureq::Agent::new_with_defaults(),
            "test-anthropic-key".to_string(),
            Some(base),
            std::time::Duration::from_secs(5),
        );
        let req = build_resume_request("new prompt", "claude-test", thread_id);
        let result = anthropic.execute(req).expect("anthropic execute");

        assert_eq!(result.response, "new answer");
        assert_eq!(result.thread_id, Some(thread_id.to_string()));
        let body: serde_json::Value =
            serde_json::from_slice(&recorded.lock().unwrap()).expect("anth req json");
        assert_eq!(body["system"], "you are helpful");
        assert_eq!(
            body["messages"],
            serde_json::json!([
                {"role": "user", "content": "old prompt"},
                {"role": "assistant", "content": "old answer"},
                {"role": "user", "content": "new prompt"},
            ])
        );
    }

    #[test]
    fn mock_server_openai_and_anthropic_executors() {
        let _xdg_guard = crate::test_util::XDG_STATE_LOCK
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        let _state = isolate_state_dir();

        // --- OpenAI-compat provider -----------------------------------
        let openai_sse = b"\
data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n\
data: {\"choices\":[{\"delta\":{\"content\":\" world\"},\"finish_reason\":\"stop\"}]}\n\n\
data: {\"choices\":[],\"usage\":{\"prompt_tokens\":7,\"completion_tokens\":2}}\n\n\
data: [DONE]\n\n\
";
        let (openai_base, openai_recorded, openai_headers) =
            start_mock_server(http_response(openai_sse));
        let agent = ureq::Agent::new_with_defaults();
        let openai = ApiExecutor::new(
            agent.clone(),
            "test-openai-key".to_string(),
            Some(format!("{openai_base}/v1/")),
            std::time::Duration::from_secs(5),
            runtime_for(Provider::OpenAI),
            None,
        );
        let (req, _spool) = build_request("hi there", "gpt-test");
        let result = openai.execute(req).expect("openai execute");
        assert_eq!(result.response, "Hello world");
        let usage = result.usage.expect("usage");
        assert_eq!(usage.prompt_tokens, 7);
        assert_eq!(usage.completion_tokens, 2);

        let recorded: serde_json::Value =
            serde_json::from_slice(&openai_recorded.lock().unwrap()).expect("openai req json");
        let expected_openai = serde_json::json!({
            "model": "gpt-test",
            "messages": [
                {"role": "system", "content": "you are helpful"},
                {"role": "user", "content": "hi there"},
            ],
            "stream": true,
            "stream_options": {"include_usage": true},
        });
        assert_eq!(recorded, expected_openai);
        assert_header(&openai_headers, "authorization", "Bearer test-openai-key");
        assert_header(&openai_headers, "content-type", "application/json");

        // --- Anthropic provider --------------------------------------
        let anthropic_sse = b"\
data: {\"type\":\"message_start\",\"message\":{\"usage\":{\"input_tokens\":11,\"cache_creation_input_tokens\":0,\"cache_read_input_tokens\":0}}}\n\n\
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\"}}\n\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"hi \"}}\n\n\
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"there\"}}\n\n\
data: {\"type\":\"content_block_stop\",\"index\":0}\n\n\
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\n\
data: {\"type\":\"message_stop\"}\n\n\
";
        let (anthropic_base, anthropic_recorded, anthropic_headers) =
            start_mock_server(http_response(anthropic_sse));
        let anthropic = AnthropicApiExecutor::new(
            agent,
            "test-anthropic-key".to_string(),
            Some(anthropic_base),
            std::time::Duration::from_secs(5),
        );
        let (req, _spool) = build_request("hello?", "claude-test");
        let result = anthropic.execute(req).expect("anthropic execute");
        assert_eq!(result.response, "hi there");
        let usage = result.usage.expect("anthropic usage");
        assert_eq!(usage.prompt_tokens, 11);
        assert_eq!(usage.completion_tokens, 5);

        let recorded: serde_json::Value =
            serde_json::from_slice(&anthropic_recorded.lock().unwrap()).expect("anth req json");
        let expected_anthropic = serde_json::json!({
            "model": "claude-test",
            "system": "you are helpful",
            "messages": [{"role": "user", "content": "hello?"}],
            "max_tokens": 32_000,
            "stream": true,
        });
        assert_eq!(recorded, expected_anthropic);
        assert_header(&anthropic_headers, "x-api-key", "test-anthropic-key");
        assert_header(&anthropic_headers, "anthropic-version", "2023-06-01");
        assert_header(&anthropic_headers, "content-type", "application/json");
    }
}
