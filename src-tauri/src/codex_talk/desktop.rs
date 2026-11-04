//! Adapter for the Windows Codex desktop's versioned, local follower protocol.
//! This is desktop IPC, not a second app-server or a different Codex account.
use serde_json::{json, Value};

#[cfg(target_os = "windows")]
pub struct Desktop {
    pipe: tokio::net::windows::named_pipe::NamedPipeClient,
    client_id: String,
    owner: Option<String>,
    snapshot: Option<Value>,
}

#[cfg(target_os = "windows")]
impl Desktop {
    pub async fn connect() -> Result<Self, String> {
        Self::connect_at(r"\\.\pipe\codex-ipc").await
    }

    async fn connect_at(path: &str) -> Result<Self, String> {
        use tokio::net::windows::named_pipe::ClientOptions;
        let pipe = ClientOptions::new().open(path).map_err(|_| {
            "Open Codex desktop on Windows, then check the thread again.".to_string()
        })?;
        let mut desktop = Self {
            pipe,
            client_id: String::new(),
            owner: None,
            snapshot: None,
        };
        let init = desktop
            .rpc("initialize", json!({"clientType":"handy-codex-talk"}), 0)
            .await?;
        desktop.client_id = init["clientId"]
            .as_str()
            .ok_or("Codex IPC initialization failed")?
            .into();
        Ok(desktop)
    }

    async fn write(&mut self, value: &Value) -> Result<(), String> {
        use tokio::io::AsyncWriteExt;
        let body = serde_json::to_vec(value).map_err(|e| e.to_string())?;
        self.pipe
            .write_all(&(body.len() as u32).to_le_bytes())
            .await
            .map_err(|e| e.to_string())?;
        self.pipe.write_all(&body).await.map_err(|e| e.to_string())
    }

    async fn rpc(&mut self, method: &str, params: Value, version: u32) -> Result<Value, String> {
        use tokio::io::AsyncReadExt;
        let id = uuid::Uuid::new_v4().to_string();
        let request = json!({"type":"request","requestId":id,"sourceClientId":self.client_id,
            "version":version,"method":method,"params":params,"timeoutMs":10000,
            "targetClientId":self.owner});
        tokio::time::timeout(std::time::Duration::from_secs(12), async {
            self.write(&request).await?;
            loop {
                let length = self.pipe.read_u32_le().await.map_err(|e| e.to_string())? as usize;
                if length == 0 || length > 16 * 1024 * 1024 { return Err("Invalid Codex IPC frame".into()); }
                let mut body = vec![0; length];
                self.pipe.read_exact(&mut body).await.map_err(|e| e.to_string())?;
                let response: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
                if response["type"] == "client-discovery-request" {
                    self.write(&json!({"type":"client-discovery-response","requestId":response["requestId"],
                        "response":{"canHandle":false}})).await?;
                }
                if response["type"] == "broadcast" && response["method"] == "thread-stream-state-changed"
                    && response["params"]["change"]["type"] == "snapshot" {
                    self.snapshot = Some(response["params"]["change"]["conversationState"].clone());
                }
                if response["requestId"] != id { continue; }
                if response["resultType"] != "success" {
                    return Err(response["error"].as_str().unwrap_or("Codex rejected the request").into());
                }
                if method != "initialize" { self.owner = response["handledByClientId"].as_str().map(str::to_owned); }
                return Ok(response["result"].clone());
            }
        }).await.map_err(|_| "Codex delivery timed out; check the thread before retrying to avoid duplicates.".to_string())?
    }

    pub async fn check(&mut self, thread: &str) -> Result<String, String> {
        let result = self.rpc("thread-follower-read-model-settings", json!({"conversationId":thread}), 1).await
            .map_err(|e| format!("Cannot access this thread in Windows Codex. Open the thread in Codex desktop and check again. ({e})"))?;
        result["settings"]["model"]
            .as_str()
            .map(str::to_owned)
            .ok_or("Codex returned no thread model settings".into())
    }

    async fn conversation_state(&mut self, thread: &str) -> Result<Value, String> {
        self.check(thread).await?;
        self.snapshot = None;
        self.write(&json!({"type":"broadcast","sourceClientId":self.client_id,
            "targetClientIds":[self.owner],"version":1,"method":"thread-stream-following-changed",
            "params":{"hostId":"local","conversationId":thread,"following":true}}))
            .await?;
        self.rpc(
            "thread-follower-load-complete-history",
            json!({"conversationId":thread}),
            1,
        )
        .await?;
        let snapshot = self
            .snapshot
            .take()
            .ok_or("Codex did not return a conversation snapshot. Try loading again.")?;
        if snapshot["id"] != thread {
            return Err("Codex returned history for a different thread".into());
        }
        Ok(snapshot)
    }

    pub async fn conversation(&mut self, thread: &str) -> Result<CodexConversation, String> {
        let state = self.conversation_state(thread).await?;
        conversation_from_state(thread, &state)
    }

    pub async fn reset_conversation(&mut self, thread: &str) -> Result<CodexConversation, String> {
        let state = self.conversation_state(thread).await?;
        let first = reset_target(&state)?;
        let old_ids: Vec<String> = ordered_turns(&state)?
            .iter()
            .filter_map(|t| t["turnId"].as_str().map(str::to_owned))
            .collect();
        self.rpc(
            "thread-follower-edit-last-user-turn",
            json!({"conversationId":thread,
            "turnId":first,"allowEarlierMessages":true,"message":RESET_MESSAGE}),
            2,
        )
        .await?;
        let refreshed = self.conversation_state(thread).await?;
        let remaining = ordered_turns(&refreshed)?;
        if remaining.iter().any(|turn| {
            turn["turnId"]
                .as_str()
                .is_some_and(|id| old_ids.iter().any(|old| old == id))
        }) {
            return Err("Codex reset was not verified: previous turns are still present. Reload the conversation before retrying.".into());
        }
        conversation_from_state(thread, &refreshed)
    }

    pub async fn send(
        &mut self,
        thread: &str,
        text: &str,
        cancelled: impl Fn() -> bool,
    ) -> Result<&'static str, String> {
        // Refresh the owner and model selection each time. No model/effort overrides.
        self.check(thread).await?;
        if cancelled() {
            return Err("Recording cancelled".into());
        }
        let input = json!([{"type":"text","text":text,"text_elements":[]}]);
        let message_id = uuid::Uuid::new_v4().to_string();
        let restore = json!({"text":text,"input":input,"context":{"workspaceRoots":[]}});
        let result = self
            .rpc(
                "thread-follower-steer-turn",
                json!({"conversationId":thread,
            "input":input,"clientUserMessageId":message_id,"restoreMessage":restore}),
                1,
            )
            .await;
        match result {
            Ok(_) => Ok("steered"),
            Err(error) if inactive_turn(&error) => {
                // Only this explicit pre-admission rejection permits a new turn.
                // Timeouts and ambiguous failures must never cause a duplicate send.
                if cancelled() {
                    return Err("Recording cancelled".into());
                }
                self.rpc("thread-follower-start-turn", json!({"conversationId":thread,
                    "turnStart":{"request":{"threadId":thread,"input":input,"clientUserMessageId":message_id},
                    "context":{"inheritThreadSettings":true}}}), 2).await?;
                Ok("sent")
            }
            Err(error) => Err(error),
        }
    }
}

pub const RESET_MESSAGE: &str = "Conversation reset at the user's request. Follow the current workspace instructions. Wait for the next user message; reply only Ready.";

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct ConversationMessage {
    pub turn_id: Option<String>,
    pub role: String,
    pub text: String,
    pub timestamp: i64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, specta::Type)]
pub struct CodexConversation {
    pub thread_id: String,
    pub model: String,
    pub messages: Vec<ConversationMessage>,
    pub active: bool,
}

fn ordered_turns(state: &Value) -> Result<Vec<&Value>, String> {
    let history = &state["turnHistory"]["history"];
    if let (Some(islands), Some(entities)) = (
        history["islands"].as_array(),
        history["entitiesByKey"].as_object(),
    ) {
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for island in islands {
            let entries = island["entries"]
                .as_array()
                .ok_or("Unrecognized Codex history island")?;
            for entry in entries {
                let key = entry["value"]
                    .as_str()
                    .ok_or("Unrecognized Codex history entry")?;
                if !seen.insert(key) {
                    continue;
                }
                result.push(
                    entities
                        .get(key)
                        .ok_or("Codex history references a missing turn")?,
                );
            }
        }
        return Ok(result);
    }
    state["turns"]
        .as_array()
        .map(|turns| turns.iter().collect())
        .ok_or("Unrecognized Codex conversation history format".into())
}

fn conversation_from_state(thread: &str, state: &Value) -> Result<CodexConversation, String> {
    let turns = ordered_turns(state)?;
    let mut messages = Vec::new();
    for turn in &turns {
        let items = turn["items"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        // The desktop can publish a newly accepted turn before its user-message
        // item arrives. Its request already contains the user input.
        if !items.iter().any(|item| item["type"] == "userMessage") {
            let text = turn["params"]["input"]
                .as_array()
                .map(|input| {
                    input
                        .iter()
                        .filter(|item| item["type"] == "text")
                        .filter_map(|item| item["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .unwrap_or_default();
            if !text.trim().is_empty() {
                messages.push(ConversationMessage {
                    role: "user".into(),
                    text,
                    turn_id: turn["turnId"].as_str().map(str::to_owned),
                    timestamp: turn["turnStartedAtMs"].as_i64().unwrap_or(0),
                });
            }
        }
        for item in items {
            let (role, text) = match item["type"].as_str() {
                Some("userMessage") => (
                    "user",
                    item["content"]
                        .as_array()
                        .map(|content| {
                            content
                                .iter()
                                .filter(|c| c["type"] == "text")
                                .filter_map(|c| c["text"].as_str())
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default(),
                ),
                Some("steeringUserMessage") => (
                    "user",
                    item["input"]
                        .as_array()
                        .map(|input| {
                            input
                                .iter()
                                .filter(|c| c["type"] == "text")
                                .filter_map(|c| c["text"].as_str())
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default(),
                ),
                Some("agentMessage") => (
                    "assistant",
                    item["text"].as_str().unwrap_or_default().to_string(),
                ),
                _ => continue,
            };
            if text.trim().is_empty() {
                continue;
            }
            messages.push(ConversationMessage {
                role: role.into(),
                text,
                turn_id: turn["turnId"].as_str().map(str::to_owned),
                timestamp: item["startedAtMs"]
                    .as_i64()
                    .or_else(|| turn["turnStartedAtMs"].as_i64())
                    .unwrap_or(0),
            });
        }
    }
    Ok(CodexConversation {
        thread_id: thread.into(),
        model: state["latestModel"].as_str().unwrap_or_default().into(),
        active: turns.iter().any(|t| t["status"] == "inProgress")
            || state["threadRuntimeStatus"]["type"] == "active",
        messages,
    })
}

fn reset_target(state: &Value) -> Result<String, String> {
    if state["turnHistory"]["history"]["isComplete"] != true
        && state["turnsPagination"]["hasLoadedOldest"] != true
    {
        return Err("Load the complete Codex conversation before clearing it.".into());
    }
    let turns = ordered_turns(state)?;
    if turns.iter().any(|turn| turn["status"] == "inProgress")
        || state["threadRuntimeStatus"]["type"] == "active"
    {
        return Err(
            "Wait for Codex to finish its active turn before clearing the conversation.".into(),
        );
    }
    let first = turns
        .iter()
        .find(|turn| {
            turn["params"]["input"]
                .as_array()
                .is_some_and(|input| !input.is_empty())
        })
        .ok_or("The Codex conversation is already empty.")?;
    let input = first["params"]["input"]
        .as_array()
        .ok_or("Unrecognized initial Codex message")?;
    if input.iter().any(|item| item["type"] != "text")
        || ["attachments", "commentAttachments"].iter().any(|key| {
            first["params"][*key]
                .as_array()
                .is_some_and(|items| !items.is_empty())
        })
        || first["mcpAppModelContextAttachments"]
            .as_array()
            .is_some_and(|items| !items.is_empty())
    {
        return Err("This conversation starts with attachments. Reset it in Codex desktop to avoid retaining those attachments.".into());
    }
    first["turnId"]
        .as_str()
        .map(str::to_owned)
        .ok_or("Codex did not provide the first turn ID".into())
}

pub fn inactive_turn(error: &str) -> bool {
    error.starts_with("Cannot steer conversation ")
        && error.ends_with("because its active turn already ended")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn history_state() -> Value {
        json!({"id":"target","latestModel":"selected-model","threadRuntimeStatus":{"type":"idle"},
            "turnsPagination":{"hasLoadedOldest":true},
            "turnHistory":{"history":{"isComplete":true,"entitiesByKey":{
                "a":{"turnId":"first","status":"completed","params":{"input":[{"type":"text","text":"original"}]},
                     "items":[{"type":"userMessage","content":[{"type":"text","text":"original"}]},{"type":"reasoning","text":"private reasoning"},{"type":"agentMessage","text":"answer"},{"type":"steeringUserMessage","input":[{"type":"text","text":"steered follow-up"}]}]},
                "b":{"turnId":"second","status":"completed","params":{"input":[{"type":"text","text":"follow-up"}]},"items":[]}},
                "islands":[{"entries":[{"value":"a"},{"value":"b"}]}]}}})
    }

    #[test]
    fn conversation_loads_saved_user_and_assistant_messages_in_order() {
        let state = history_state();
        let history = conversation_from_state("target", &state).unwrap();
        assert_eq!(history.thread_id, "target");
        assert_eq!(history.model, "selected-model");
        assert_eq!(history.messages.len(), 4);
        assert_eq!(history.messages[0].role, "user");
        assert_eq!(history.messages[0].text, "original");
        assert_eq!(history.messages[1].role, "assistant");
        assert_eq!(history.messages[2].role, "user");
        assert_eq!(history.messages[2].text, "steered follow-up");
        assert!(!history.active);
        assert_eq!(reset_target(&state).unwrap(), "first");
    }

    #[test]
    fn reset_refuses_active_incomplete_and_attachment_history() {
        let mut state = history_state();
        state["turnHistory"]["history"]["entitiesByKey"]["b"]["status"] = json!("inProgress");
        assert!(reset_target(&state).is_err());
        assert!(conversation_from_state("target", &state).unwrap().active);
        state = history_state();
        state["turnHistory"]["history"]["isComplete"] = json!(false);
        state["turnsPagination"]["hasLoadedOldest"] = json!(false);
        assert!(reset_target(&state).is_err());
        state = history_state();
        state["turnHistory"]["history"]["entitiesByKey"]["a"]["params"]["input"] =
            json!([{"type":"image"}]);
        assert!(reset_target(&state).is_err());
    }

    #[test]
    fn only_explicit_inactive_rejection_allows_starting_a_turn() {
        assert!(inactive_turn(
            "Cannot steer conversation abc because its active turn already ended"
        ));
        for message in [
            "request-timeout",
            "no-client-found",
            "NoActiveTurn",
            "transport disconnected",
        ] {
            assert!(!inactive_turn(message));
        }
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::windows::named_pipe::{NamedPipeServer, ServerOptions};

    async fn exchange(
        server: &mut NamedPipeServer,
        method: &str,
        result: Result<Value, &str>,
    ) -> Value {
        let length = server.read_u32_le().await.unwrap();
        let mut bytes = vec![0; length as usize];
        server.read_exact(&mut bytes).await.unwrap();
        let request: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(request["method"], method);
        let mut response = json!({"type":"response","requestId":request["requestId"],
            "handledByClientId":"desktop-owner","method":method});
        match result {
            Ok(result) => {
                response["resultType"] = json!("success");
                response["result"] = result;
            }
            Err(error) => {
                response["resultType"] = json!("error");
                response["error"] = json!(error);
            }
        }
        let bytes = serde_json::to_vec(&response).unwrap();
        server.write_u32_le(bytes.len() as u32).await.unwrap();
        server.write_all(&bytes).await.unwrap();
        request
    }

    #[tokio::test]
    async fn routes_to_desktop_owner_and_preserves_model_without_duplicate_fallbacks() {
        for (error, expected) in [
            (None, "steered"),
            (
                Some("Cannot steer conversation target because its active turn already ended"),
                "sent",
            ),
            (Some("request-timeout"), "error"),
        ] {
            let path = format!(r"\\.\pipe\handy-codex-test-{}", uuid::Uuid::new_v4());
            let mut server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&path)
                .unwrap();
            let task = tokio::spawn(async move {
                server.connect().await.unwrap();
                exchange(&mut server, "initialize", Ok(json!({"clientId":"handy"}))).await;
                exchange(
                    &mut server,
                    "thread-follower-read-model-settings",
                    Ok(json!({"settings":{"model":"thread-model"}})),
                )
                .await;
                let steer = exchange(
                    &mut server,
                    "thread-follower-steer-turn",
                    error.map_or(Ok(json!({"result":{"turnId":"active"}})), Err),
                )
                .await;
                assert_eq!(steer["targetClientId"], "desktop-owner");
                assert_eq!(steer["params"]["conversationId"], "target");
                assert_eq!(steer["params"]["input"][0]["text"], "message");
                assert!(steer["params"].get("model").is_none());
                if expected == "sent" {
                    let start = exchange(
                        &mut server,
                        "thread-follower-start-turn",
                        Ok(json!({"result":{"turn":{"id":"next"}}})),
                    )
                    .await;
                    let request = &start["params"]["turnStart"]["request"];
                    assert_eq!(request["threadId"], "target");
                    assert!(request.get("model").is_none());
                    assert!(request.get("effort").is_none());
                    assert_eq!(
                        start["params"]["turnStart"]["context"]["inheritThreadSettings"],
                        true
                    );
                } else if expected == "error" {
                    // An uncertain delivery must not retry or start another turn.
                    let mut byte = [0];
                    assert_eq!(server.read(&mut byte).await.unwrap(), 0);
                }
            });
            let mut desktop = Desktop::connect_at(&path).await.unwrap();
            let result = desktop.send("target", "message", || false).await;
            if expected == "error" {
                assert!(result.is_err());
            } else {
                assert_eq!(result.unwrap(), expected);
            }
            drop(desktop);
            task.await.unwrap();
        }
    }

    #[tokio::test]
    #[ignore = "requires the user's Windows Codex desktop and selected thread"]
    async fn live_desktop_thread_is_accessible() {
        let thread = std::env::var("HANDY_CODEX_TEST_THREAD")
            .unwrap_or_else(|_| "01a1a777-2733-73c3-999d-c7d0e0f7cfaf".into());
        let mut desktop = Desktop::connect().await.unwrap();
        let model = desktop.check(&thread).await.unwrap();
        assert!(!model.is_empty());
        println!("Windows desktop thread accessible; selected model: {model}");
        if std::env::var("HANDY_CODEX_TEST_RESET").as_deref() == Ok("scratch-only") {
            assert_ne!(
                thread, "01a1a777-2733-73c3-999d-c7d0e0f7cfaf",
                "Never reset the user's destination in a test"
            );
            let before = desktop.conversation(&thread).await.unwrap();
            let mut after = desktop.reset_conversation(&thread).await.unwrap();
            for _ in 0..10 {
                if after.messages.iter().any(|message| {
                    message
                        .text
                        .contains("Conversation reset at the user's request")
                }) {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                after = desktop.conversation(&thread).await.unwrap();
            }
            assert_eq!(before.thread_id, after.thread_id);
            assert_eq!(before.model, after.model);
            assert!(after.messages.iter().any(|message| message
                .text
                .contains("Conversation reset at the user's request")));
            println!(
                "Live desktop reset verified on scratch thread; messages {} -> {}",
                before.messages.len(),
                after.messages.len()
            );
        }
        if std::env::var("HANDY_CODEX_TEST_SEND").as_deref() == Ok("scratch-only") {
            assert_ne!(
                thread, "01a1a777-2733-73c3-999d-c7d0e0f7cfaf",
                "Never send test messages to the user's destination"
            );
            let result = desktop.send(&thread, "Integration test: write numbers from 1 through 200, one per line. Do not use tools or modify files.", || false).await.unwrap();
            println!("Idle delivery: {result}");
            assert_eq!(result, "sent");
            let steer = desktop.send(&thread, "Stop the numbers and reply exactly HANDY_CODEX_TALK_STEER_OK. Do not use tools or modify files.", || false).await.unwrap();
            assert_eq!(steer, "steered");
            println!("Active delivery: {steer}");
        }
    }
}
