use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::NativeCliKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageScope {
    Context,
    ModelStep,
    ToolCall,
    Turn,
    Run,
}

/// 上游直接报告的值及语义确定的换算。缺项保持 `None`。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Telemetry {
    pub turns: Option<u32>,
    pub steps: Option<u32>,
    pub used_context_tokens: Option<u64>,
    pub context_window_limit: Option<u64>,
    pub total_tokens: Option<u64>,
    pub api_cost_usd: Option<f64>,
    pub avg_speed_tps: Option<f64>,
    pub peak_speed_tps: Option<f64>,
    pub recent_speed_tps: Option<f64>,
    pub cache_hit_pct: Option<f64>,
    pub cache_note: Option<String>,
    pub model_duration_sec: Option<f64>,
    pub tool_duration_sec: Option<f64>,
    pub ttft_sec: Option<f64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Succeeded,
    Cancelled,
    NativeFailed,
    ProcessFailed { code: Option<i32> },
    Incomplete,
    IoFailed,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DriverEvent {
    Session {
        id: String,
    },
    TurnStarted,
    Text(String),
    Tool {
        id: String,
        name: Option<String>,
        finished: bool,
        succeeded: Option<bool>,
    },
    Usage {
        scope: UsageScope,
        telemetry: Box<Telemetry>,
    },
    ControlAck {
        id: u64,
        accepted: bool,
    },
    NativeError,
    MalformedOutput,
    Finished(Outcome),
}

pub(crate) struct Decoder {
    kind: NativeCliKind,
    session: Option<String>,
    terminal: Option<Outcome>,
    saw_text: bool,
    partial_blocks: HashSet<u64>,
    partial_messages: HashSet<String>,
    copilot_turn_ended: bool,
    pi_stop: Option<Outcome>,
    pi_active: bool,
}

impl Decoder {
    pub(crate) fn new(kind: NativeCliKind) -> Self {
        Self {
            kind,
            session: None,
            terminal: None,
            saw_text: false,
            partial_blocks: HashSet::new(),
            partial_messages: HashSet::new(),
            copilot_turn_ended: false,
            pi_stop: None,
            pi_active: false,
        }
    }

    pub(crate) fn is_terminal(&self) -> bool {
        self.terminal.is_some()
    }

    pub(crate) fn outcome(&self) -> Outcome {
        if self.kind == NativeCliKind::Copilot && self.terminal.is_none() && self.copilot_turn_ended
        {
            return Outcome::Succeeded;
        }
        self.terminal.unwrap_or(Outcome::Incomplete)
    }

    pub(crate) fn accepts_controls(&self) -> bool {
        self.pi_active && !self.is_terminal()
    }

    pub(crate) fn decode(&mut self, record: &Value) -> Vec<DriverEvent> {
        let field = if self.kind == NativeCliKind::Antigravity {
            "event"
        } else {
            "type"
        };
        let Some(tag) = string(&record[field]) else {
            return vec![DriverEvent::MalformedOutput];
        };
        let mut events = Vec::new();
        match self.kind {
            NativeCliKind::Codex => self.codex(tag, record, &mut events),
            NativeCliKind::Pi => self.pi(tag, record, &mut events),
            NativeCliKind::ClaudeCode | NativeCliKind::QwenCode => {
                self.stream_json(tag, record, &mut events)
            }
            NativeCliKind::OpenCode => self.opencode(tag, record, &mut events),
            NativeCliKind::Antigravity => self.agy(tag, record, &mut events),
            NativeCliKind::Copilot => self.copilot(tag, record, &mut events),
        }
        events
    }

    fn session(&mut self, value: &Value, events: &mut Vec<DriverEvent>) {
        if let Some(id) = string(value).filter(|id| self.session.as_deref() != Some(*id)) {
            self.session = Some(id.to_owned());
            events.push(DriverEvent::Session { id: id.to_owned() });
        }
    }

    fn text(&mut self, value: &Value, events: &mut Vec<DriverEvent>) {
        if let Some(text) = string(value) {
            self.saw_text = true;
            events.push(DriverEvent::Text(text.to_owned()));
        }
    }

    fn finish(&mut self, outcome: Outcome, events: &mut Vec<DriverEvent>) {
        if outcome == Outcome::NativeFailed {
            events.push(DriverEvent::NativeError);
        }
        if !matches!(
            self.terminal,
            Some(Outcome::NativeFailed | Outcome::Cancelled)
        ) {
            self.terminal = Some(outcome);
        }
    }

    fn codex(&mut self, tag: &str, record: &Value, events: &mut Vec<DriverEvent>) {
        let item = &record["item"];
        match tag {
            "thread.started" => self.session(&record["thread_id"], events),
            "turn.started" => events.push(DriverEvent::TurnStarted),
            "item.completed" if item["type"] == "agent_message" => self.text(&item["text"], events),
            "item.started" | "item.completed"
                if matches!(
                    item["type"].as_str(),
                    Some("command_execution" | "mcp_tool_call" | "web_search" | "file_change")
                ) =>
            {
                tool(
                    events,
                    &item["id"],
                    item["tool"].as_str().or(item["type"].as_str()),
                    tag == "item.completed",
                    item["exit_code"]
                        .as_i64()
                        .map(|code| code == 0)
                        .or_else(|| status(&item["status"])),
                );
            }
            "turn.completed" if !record["usage"].is_object() => {
                events.push(DriverEvent::MalformedOutput)
            }
            "turn.completed" => {
                let native = &record["usage"];
                let input = number(&native["input_tokens"]);
                let output = number(&native["output_tokens"]);
                let cached = number(&native["cached_input_tokens"]);
                usage(
                    events,
                    UsageScope::Turn,
                    Telemetry {
                        turns: Some(1),
                        input_tokens: input,
                        output_tokens: output,
                        cache_read_tokens: cached,
                        cache_write_tokens: number(&native["cache_write_input_tokens"]),
                        total_tokens: input
                            .zip(output)
                            .and_then(|(input, output)| input.checked_add(output)),
                        cache_hit_pct: ratio(cached, input),
                        ..Telemetry::default()
                    },
                );
                self.finish(Outcome::Succeeded, events);
            }
            "error" => events.push(DriverEvent::NativeError),
            "turn.failed" => self.finish(Outcome::NativeFailed, events),
            _ => {}
        }
    }

    fn pi(&mut self, tag: &str, record: &Value, events: &mut Vec<DriverEvent>) {
        let message = &record["message"];
        match tag {
            "response" => {
                let accepted = record["success"] == true;
                let data = &record["data"];
                match record["id"].as_str() {
                    Some("state") if accepted => {
                        self.session(&data["sessionId"], events);
                        if let Some(limit) = number(&data["model"]["contextWindow"]) {
                            usage(
                                events,
                                UsageScope::Context,
                                Telemetry {
                                    context_window_limit: Some(limit),
                                    ..Telemetry::default()
                                },
                            );
                        }
                    }
                    Some("prompt") if !accepted => self.finish(Outcome::NativeFailed, events),
                    Some("prompt") if data["disposition"] == "handled" => {
                        self.finish(Outcome::Succeeded, events)
                    }
                    Some("stats") if accepted => {
                        let context = &data["contextUsage"];
                        usage(
                            events,
                            UsageScope::Context,
                            Telemetry {
                                used_context_tokens: number(&context["tokens"]),
                                context_window_limit: number(&context["contextWindow"]),
                                ..Telemetry::default()
                            },
                        );
                    }
                    Some(id) => {
                        if let Ok(id) = id.parse() {
                            events.push(DriverEvent::ControlAck { id, accepted });
                        }
                    }
                    None => {}
                }
            }
            "agent_start" => {
                self.pi_stop = None;
                self.pi_active = true;
                events.push(DriverEvent::TurnStarted);
            }
            "message_start" if message["role"] == "assistant" => self.partial_blocks.clear(),
            "message_update" if record["assistantMessageEvent"]["type"] == "text_delta" => {
                let delta = &record["assistantMessageEvent"];
                self.partial_blocks
                    .insert(delta["contentIndex"].as_u64().unwrap_or(0));
                self.text(&delta["delta"], events);
            }
            "message_end" if message["role"] == "assistant" => {
                self.message_content(&message["content"], events);
                self.pi_stop = match message["stopReason"].as_str() {
                    Some("error") => Some(Outcome::NativeFailed),
                    Some("aborted") => Some(Outcome::Cancelled),
                    Some("stop" | "length" | "toolUse") => Some(Outcome::Succeeded),
                    _ => None,
                };
                let native = &message["usage"];
                if native.is_object() {
                    let input = number(&native["input"]);
                    let cached = number(&native["cacheRead"]);
                    let cache_write = number(&native["cacheWrite"]);
                    usage(
                        events,
                        UsageScope::ModelStep,
                        Telemetry {
                            steps: Some(1),
                            input_tokens: input,
                            output_tokens: number(&native["output"]),
                            cache_read_tokens: cached,
                            cache_write_tokens: cache_write,
                            total_tokens: number(&native["totalTokens"]),
                            api_cost_usd: nonnegative(&native["cost"]["total"]),
                            cache_hit_pct: ratio(cached, sum(input, sum(cached, cache_write))),
                            ..Telemetry::default()
                        },
                    );
                }
            }
            "tool_execution_start" | "tool_execution_end" => tool(
                events,
                &record["toolCallId"],
                record["toolName"].as_str(),
                tag == "tool_execution_end",
                record["isError"].as_bool().map(|failed| !failed),
            ),
            "agent_settled" => {
                self.pi_active = false;
                let outcome = if record["aborted"] == true {
                    Outcome::Cancelled
                } else {
                    self.pi_stop.unwrap_or(Outcome::Incomplete)
                };
                self.finish(outcome, events);
            }
            _ => {}
        }
    }

    fn stream_json(&mut self, tag: &str, record: &Value, events: &mut Vec<DriverEvent>) {
        if record["parent_tool_use_id"].is_string() {
            return;
        }
        self.session(&record["session_id"], events);
        match tag {
            "system" if matches!(record["subtype"].as_str(), Some("init" | "session_start")) => {
                events.push(DriverEvent::TurnStarted)
            }
            "stream_event" => {
                let event = &record["event"];
                match event["type"].as_str() {
                    Some("message_start") => self.partial_blocks.clear(),
                    Some("content_block_delta") if event["delta"]["type"] == "text_delta" => {
                        self.partial_blocks
                            .insert(event["index"].as_u64().unwrap_or(0));
                        self.text(&event["delta"]["text"], events);
                    }
                    _ => {}
                }
            }
            "assistant" => self.message_content(&record["message"]["content"], events),
            "user" => {
                if let Some(blocks) = record["message"]["content"].as_array() {
                    for block in blocks.iter().filter(|block| block["type"] == "tool_result") {
                        tool(
                            events,
                            &block["tool_use_id"],
                            None,
                            true,
                            block["is_error"].as_bool().map(|failed| !failed),
                        );
                    }
                }
            }
            "result" => {
                let native = &record["usage"];
                let input = number(&native["input_tokens"]);
                let output = number(&native["output_tokens"]);
                let read = number(&native["cache_read_input_tokens"]);
                let write = number(&native["cache_creation_input_tokens"]);
                let total_input = sum(input, sum(read, write));
                let claude = self.kind == NativeCliKind::ClaudeCode;
                usage(
                    events,
                    UsageScope::Run,
                    Telemetry {
                        turns: count(&record["num_turns"]),
                        input_tokens: input,
                        output_tokens: output,
                        cache_read_tokens: read,
                        cache_write_tokens: write,
                        cache_hit_pct: if claude {
                            ratio(read, total_input)
                        } else {
                            None
                        },
                        total_tokens: number(&native["total_tokens"]).or_else(|| {
                            if claude {
                                sum(total_input, output)
                            } else {
                                None
                            }
                        }),
                        api_cost_usd: nonnegative(&record["total_cost_usd"]),
                        ..Telemetry::default()
                    },
                );
                let outcome = match (record["subtype"].as_str(), record["is_error"].as_bool()) {
                    (_, Some(true)) => Outcome::NativeFailed,
                    (Some("success"), _) => Outcome::Succeeded,
                    (Some(subtype), _) if subtype == "error" || subtype.starts_with("error_") => {
                        Outcome::NativeFailed
                    }
                    _ => Outcome::Incomplete,
                };
                if !self.saw_text && outcome == Outcome::Succeeded {
                    self.text(&record["result"], events);
                }
                self.finish(outcome, events);
            }
            _ => {}
        }
    }

    fn message_content(&mut self, value: &Value, events: &mut Vec<DriverEvent>) {
        if let Some(blocks) = value.as_array() {
            for (index, block) in blocks.iter().enumerate() {
                match block["type"].as_str() {
                    Some("text") if !self.partial_blocks.contains(&(index as u64)) => {
                        self.text(&block["text"], events)
                    }
                    Some("tool_use") => {
                        tool(events, &block["id"], block["name"].as_str(), false, None)
                    }
                    Some("toolCall") => {
                        tool(events, &block["id"], block["name"].as_str(), false, None)
                    }
                    _ => {}
                }
            }
        } else if self.partial_blocks.is_empty() {
            self.text(value, events);
        }
        self.partial_blocks.clear();
    }

    fn opencode(&mut self, tag: &str, record: &Value, events: &mut Vec<DriverEvent>) {
        self.session(&record["sessionID"], events);
        let part = &record["part"];
        match tag {
            "step_start" if !self.saw_text && self.terminal.is_none() => {
                if part["type"] == "step-start" {
                    events.push(DriverEvent::TurnStarted);
                }
            }
            "text" => self.text(&part["text"], events),
            "tool_use" => {
                let state = &part["state"];
                tool(
                    events,
                    part.get("callID").unwrap_or(&part["id"]),
                    part["tool"].as_str(),
                    status(&state["status"]).is_some(),
                    status(&state["status"]),
                );
                let time = &state["time"];
                if let Some(duration) = number(&time["end"])
                    .zip(number(&time["start"]))
                    .and_then(|(end, start)| end.checked_sub(start))
                {
                    usage(
                        events,
                        UsageScope::ToolCall,
                        Telemetry {
                            tool_duration_sec: Some(duration as f64 / 1000.0),
                            ..Telemetry::default()
                        },
                    );
                }
            }
            "step_finish" => {
                let tokens = &part["tokens"];
                usage(
                    events,
                    UsageScope::ModelStep,
                    Telemetry {
                        steps: Some(1),
                        input_tokens: number(&tokens["input"]),
                        output_tokens: number(&tokens["output"]),
                        total_tokens: number(&tokens["total"]),
                        cache_read_tokens: number(&tokens["cache"]["read"]),
                        cache_write_tokens: number(&tokens["cache"]["write"]),
                        api_cost_usd: nonnegative(&part["cost"]),
                        ..Telemetry::default()
                    },
                );
                if matches!(
                    part["reason"].as_str(),
                    Some("stop" | "length" | "content-filter")
                ) {
                    self.finish(Outcome::Succeeded, events);
                }
            }
            "error" => self.finish(Outcome::NativeFailed, events),
            _ => {}
        }
    }

    fn agy(&mut self, tag: &str, record: &Value, events: &mut Vec<DriverEvent>) {
        let data = &record[tag];
        self.session(&record["conversation_id"], events);
        self.session(&data["conversation_id"], events);
        match tag {
            "init" => events.push(DriverEvent::TurnStarted),
            "step_update" if data["step_type"] == "agent_response" => {
                self.text(&data["text_delta"], events);
                if data["state"] == "DONE" {
                    let native = &data["usage"];
                    usage(
                        events,
                        UsageScope::ModelStep,
                        Telemetry {
                            steps: Some(1),
                            model_duration_sec: nonnegative(&data["duration_seconds"]),
                            input_tokens: number(&native["input_tokens"]),
                            output_tokens: number(&native["output_tokens"]),
                            cache_read_tokens: number(&native["cache_read_tokens"]),
                            total_tokens: number(&native["total_tokens"]),
                            ..Telemetry::default()
                        },
                    );
                }
            }
            "step_update" if data["step_type"] == "tool" => {
                let finished = data["state"] == "DONE";
                if let Some(index) = number(&data["step_index"]) {
                    let id = Value::String(index.to_string());
                    let result = &data["tool_info"];
                    tool(
                        events,
                        &id,
                        data["tool_name"].as_str(),
                        finished,
                        (finished && result.is_object()).then(|| result["error"].is_null()),
                    );
                }
                if finished && let Some(duration) = nonnegative(&data["duration_seconds"]) {
                    usage(
                        events,
                        UsageScope::ToolCall,
                        Telemetry {
                            tool_duration_sec: Some(duration),
                            ..Telemetry::default()
                        },
                    );
                }
            }
            "result" => {
                let native = &data["usage"];
                usage(
                    events,
                    UsageScope::Run,
                    Telemetry {
                        turns: count(&data["num_turns"]),
                        input_tokens: number(&native["input_tokens"]),
                        output_tokens: number(&native["output_tokens"]),
                        cache_read_tokens: number(&native["cache_read_tokens"]),
                        total_tokens: number(&native["total_tokens"]),
                        ..Telemetry::default()
                    },
                );
                let outcome = match data["status"].as_str() {
                    Some("SUCCESS") => Outcome::Succeeded,
                    Some("CANCELED" | "INTERRUPTED") => Outcome::Cancelled,
                    Some(_) => Outcome::NativeFailed,
                    _ => Outcome::Incomplete,
                };
                if !self.saw_text && outcome == Outcome::Succeeded {
                    self.text(&data["response"], events);
                }
                self.finish(outcome, events);
            }
            _ => {}
        }
    }

    fn copilot(&mut self, tag: &str, record: &Value, events: &mut Vec<DriverEvent>) {
        if record["agentId"].is_string() {
            return;
        }
        let data = &record["data"];
        let message_id = data["messageId"].as_str().unwrap_or_default();
        match tag {
            "session.start" | "session.resume" => self.session(&data["sessionId"], events),
            "assistant.turn_start" => {
                self.partial_messages.clear();
                self.copilot_turn_ended = false;
                events.push(DriverEvent::TurnStarted);
            }
            "assistant.message_delta" => {
                self.partial_messages.insert(message_id.to_owned());
                self.text(&data["deltaContent"], events);
            }
            "assistant.message" => {
                if !self.partial_messages.remove(message_id) {
                    self.text(&data["content"], events);
                }
            }
            "assistant.turn_end" => {
                self.copilot_turn_ended = true;
                usage(
                    events,
                    UsageScope::Turn,
                    Telemetry {
                        turns: Some(1),
                        ..Telemetry::default()
                    },
                );
            }
            "session.usage_info" => usage(
                events,
                UsageScope::Context,
                Telemetry {
                    used_context_tokens: number(&data["currentTokens"]),
                    context_window_limit: number(&data["tokenLimit"]),
                    ..Telemetry::default()
                },
            ),
            "assistant.usage" => usage(
                events,
                UsageScope::ModelStep,
                Telemetry {
                    input_tokens: number(&data["inputTokens"]),
                    output_tokens: number(&data["outputTokens"]),
                    cache_read_tokens: number(&data["cacheReadTokens"]),
                    cache_write_tokens: number(&data["cacheWriteTokens"]),
                    model_duration_sec: nonnegative(&data["duration"]).map(|ms| ms / 1000.0),
                    ttft_sec: nonnegative(&data["timeToFirstTokenMs"]).map(|ms| ms / 1000.0),
                    recent_speed_tps: nonnegative(&data["interTokenLatencyMs"])
                        .filter(|ms| *ms > 0.0)
                        .map(|ms| 1000.0 / ms)
                        .filter(|speed| speed.is_finite()),
                    ..Telemetry::default()
                },
            ),
            "tool.execution_start" | "tool.execution_complete" => tool(
                events,
                &data["toolCallId"],
                data["toolName"].as_str(),
                tag == "tool.execution_complete",
                data["success"].as_bool(),
            ),
            "session.task_complete" => {
                if !self.saw_text {
                    self.text(&data["summary"], events);
                }
            }
            "session.error" => self.finish(Outcome::NativeFailed, events),
            "session.idle" if data["aborted"] == true => self.finish(Outcome::Cancelled, events),
            "session.idle" if self.copilot_turn_ended => self.finish(Outcome::Succeeded, events),
            "session.shutdown" if data["shutdownType"] == "error" => {
                self.finish(Outcome::NativeFailed, events)
            }
            _ => {}
        }
    }
}

fn string(value: &Value) -> Option<&str> {
    value.as_str().filter(|text| !text.is_empty())
}

fn number(value: &Value) -> Option<u64> {
    value.as_u64()
}

fn count(value: &Value) -> Option<u32> {
    number(value)?.try_into().ok()
}

fn nonnegative(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .filter(|number| number.is_finite() && *number >= 0.0)
}

fn sum(left: Option<u64>, right: Option<u64>) -> Option<u64> {
    left?.checked_add(right?)
}

fn ratio(numerator: Option<u64>, denominator: Option<u64>) -> Option<f64> {
    let (numerator, denominator) = numerator.zip(denominator)?;
    (denominator > 0 && numerator <= denominator)
        .then(|| numerator as f64 * 100.0 / denominator as f64)
}

fn status(value: &Value) -> Option<bool> {
    match value.as_str() {
        Some("completed") => Some(true),
        Some("failed" | "error" | "declined") => Some(false),
        _ => None,
    }
}

fn usage(events: &mut Vec<DriverEvent>, scope: UsageScope, telemetry: Telemetry) {
    if telemetry != Telemetry::default() {
        events.push(DriverEvent::Usage {
            scope,
            telemetry: Box::new(telemetry),
        });
    }
}

fn tool(
    events: &mut Vec<DriverEvent>,
    id: &Value,
    name: Option<&str>,
    finished: bool,
    succeeded: Option<bool>,
) {
    if let Some(id) = string(id) {
        events.push(DriverEvent::Tool {
            id: id.to_owned(),
            name: name.map(str::to_owned),
            finished,
            succeeded,
        });
    }
}
