//! The loop.

use crate::{AgentError, ChatModel, Message, ToolCall, ToolDef, ToolExecutor, Usage};

/// Bounds on a single run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How many times the model may ask for tools before it has to answer.
    ///
    /// Each round costs a model call plus its tool calls, so this is the main
    /// lever on what one question can spend. Four is enough for the chains we
    /// actually see — resolve a vocabulary, query with it, maybe refine once —
    /// with room for a wrong turn.
    pub max_tool_rounds: u8,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_tool_rounds: 4 }
    }
}

/// Why the run ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// The model stopped asking for tools.
    Answered,

    /// The round cap was hit and the model was made to answer from what it
    /// had. The answer is real, but it may be partial — worth surfacing
    /// differently in a UI, and worth alerting on if it happens often.
    ToolRoundsExhausted,
}

/// The result of one run.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRun {
    /// The model's final text. `None` means it produced no text at all, which
    /// is rare but not impossible — the caller decides what to show.
    pub answer: Option<String>,

    /// Every call made, in order. This is the telemetry surface, and it is
    /// also what a consumer records to make a result refinable later: the last
    /// query in here is what "show me the highest rank" would re-run.
    pub calls: Vec<ToolCall>,

    pub usage: Usage,
    pub stop: StopReason,
}

/// Run the loop until the model answers or the round cap forces it to.
///
/// `history` is seeded by the caller — typically a [`Message::System`] and a
/// [`Message::User`], but a consumer replaying prior context can pass more.
///
/// Errors are model-side only; see [`AgentError`].
pub async fn run<M, E>(
    model: &M,
    executor: &E,
    tools: &[ToolDef],
    mut history: Vec<Message>,
    limits: Limits,
) -> Result<AgentRun, AgentError>
where
    M: ChatModel,
    E: ToolExecutor,
{
    let mut calls = Vec::new();
    let mut usage = Usage::default();

    for _ in 0..limits.max_tool_rounds {
        let turn = model.turn(&history, tools).await?;
        usage.add(turn.usage);

        if turn.calls.is_empty() {
            return Ok(AgentRun {
                answer: turn.content,
                calls,
                usage,
                stop: StopReason::Answered,
            });
        }

        history.push(Message::Assistant {
            content: turn.content,
            calls: turn.calls.clone(),
        });

        // Sequential on purpose. Most turns ask for one tool, the transport is
        // the slow part either way, and a deterministic order makes both the
        // tests and a production trace readable. Revisit if fan-out turns show
        // up in real traffic.
        for call in turn.calls {
            let outcome = executor.execute(&call).await;
            history.push(Message::ToolResult {
                call_id: call.id.clone(),
                content: outcome.content,
            });
            calls.push(call);
        }
    }

    // Out of rounds. Ask once more with no tools, so the model answers from
    // what it gathered instead of the run ending in silence.
    let final_turn = model.turn(&history, &[]).await?;
    usage.add(final_turn.usage);

    Ok(AgentRun {
        answer: final_turn.content,
        calls,
        usage,
        stop: StopReason::ToolRoundsExhausted,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ModelTurn, ToolOutcome};
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// A model that replays a fixed script, recording what it was shown.
    struct ScriptedModel {
        turns: RefCell<VecDeque<ModelTurn>>,
        /// Number of tools offered on each turn, in order.
        tools_offered: RefCell<Vec<usize>>,
        /// The history as it stood at the last turn.
        last_history: RefCell<Vec<Message>>,
    }

    impl ScriptedModel {
        fn new(turns: Vec<ModelTurn>) -> Self {
            Self {
                turns: RefCell::new(turns.into()),
                tools_offered: RefCell::new(Vec::new()),
                last_history: RefCell::new(Vec::new()),
            }
        }
    }

    impl ChatModel for ScriptedModel {
        async fn turn(
            &self,
            messages: &[Message],
            tools: &[ToolDef],
        ) -> Result<ModelTurn, AgentError> {
            self.tools_offered.borrow_mut().push(tools.len());
            *self.last_history.borrow_mut() = messages.to_vec();
            self.turns
                .borrow_mut()
                .pop_front()
                .ok_or_else(|| AgentError::Model("script exhausted".to_string()))
        }
    }

    /// An executor that answers from a canned map and records what it ran.
    struct StubExecutor {
        replies: Vec<(&'static str, ToolOutcome)>,
        ran: RefCell<Vec<String>>,
    }

    impl StubExecutor {
        fn new(replies: Vec<(&'static str, ToolOutcome)>) -> Self {
            Self {
                replies,
                ran: RefCell::new(Vec::new()),
            }
        }
    }

    impl ToolExecutor for StubExecutor {
        async fn execute(&self, call: &ToolCall) -> ToolOutcome {
            self.ran.borrow_mut().push(call.name.clone());
            self.replies
                .iter()
                .find(|(name, _)| *name == call.name)
                .map(|(_, outcome)| outcome.clone())
                .unwrap_or_else(|| ToolOutcome::error(format!("no stub for {}", call.name)))
        }
    }

    fn call(id: &str, name: &str, arguments: &str) -> ToolCall {
        ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: arguments.to_string(),
        }
    }

    fn tool(name: &str) -> ToolDef {
        // `from_str` rather than the `json!` macro — this codebase builds JSON
        // from typed structs, and a literal is the honest way to write a
        // schema fixture.
        let schema = serde_json::from_str(r#"{"type":"object","properties":{}}"#).unwrap();
        ToolDef::new(name, format!("the {name} tool"), schema)
    }

    fn seed(question: &str) -> Vec<Message> {
        vec![
            Message::System("you are a helpful bot".to_string()),
            Message::User(question.to_string()),
        ]
    }

    #[tokio::test]
    async fn answers_without_calling_anything() {
        let model = ScriptedModel::new(vec![ModelTurn::answer("hello")]);
        let executor = StubExecutor::new(vec![]);

        let run = run(
            &model,
            &executor,
            &[tool("find_assets")],
            seed("hi"),
            Limits::default(),
        )
        .await
        .unwrap();

        assert_eq!(run.answer.as_deref(), Some("hello"));
        assert_eq!(run.stop, StopReason::Answered);
        assert!(run.calls.is_empty());
        assert!(executor.ran.borrow().is_empty());
    }

    /// The scenario this was designed around: "show me all the ghoul pirates"
    /// resolves the trait vocabulary first, then queries with what it learned.
    #[tokio::test]
    async fn resolves_a_vocabulary_then_queries_with_it() {
        let model = ScriptedModel::new(vec![
            ModelTurn::calling(vec![call("c1", "resolve_traits", r#"{"query":"ghoul"}"#)]),
            ModelTurn::calling(vec![call("c2", "find_assets", r#"{"trait_bits":[47]}"#)]),
            ModelTurn::answer("112 ghoul pirates"),
        ]);
        let executor = StubExecutor::new(vec![
            ("resolve_traits", ToolOutcome::ok(r#"[{"bit":47}]"#)),
            ("find_assets", ToolOutcome::ok(r#"{"total":112}"#)),
        ]);

        let run = run(
            &model,
            &executor,
            &[tool("resolve_traits"), tool("find_assets")],
            seed("show me all the ghoul pirates"),
            Limits::default(),
        )
        .await
        .unwrap();

        assert_eq!(run.answer.as_deref(), Some("112 ghoul pirates"));
        assert_eq!(run.stop, StopReason::Answered);
        assert_eq!(*executor.ran.borrow(), vec!["resolve_traits", "find_assets"]);

        // The recorded calls are what makes a result refinable later.
        assert_eq!(run.calls.len(), 2);
        assert_eq!(run.calls[1].name, "find_assets");

        // Both results reached the model before it answered.
        let history = model.last_history.borrow();
        let results: Vec<&str> = history
            .iter()
            .filter_map(|m| match m {
                Message::ToolResult { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(results, vec![r#"[{"bit":47}]"#, r#"{"total":112}"#]);
    }

    #[tokio::test]
    async fn every_call_in_a_turn_runs_and_each_result_is_tagged_with_its_call() {
        let model = ScriptedModel::new(vec![
            ModelTurn::calling(vec![
                call("c1", "find_assets", "{}"),
                call("c2", "collection_stats", "{}"),
            ]),
            ModelTurn::answer("done"),
        ]);
        let executor = StubExecutor::new(vec![
            ("find_assets", ToolOutcome::ok("assets")),
            ("collection_stats", ToolOutcome::ok("stats")),
        ]);

        let run = run(
            &model,
            &executor,
            &[tool("find_assets"), tool("collection_stats")],
            seed("two things"),
            Limits::default(),
        )
        .await
        .unwrap();

        assert_eq!(run.calls.len(), 2);
        assert_eq!(
            *executor.ran.borrow(),
            vec!["find_assets", "collection_stats"]
        );

        // A result that answers the wrong call is rejected by the provider, so
        // the pairing matters more than the content.
        let history = model.last_history.borrow();
        let pairs: Vec<(&str, &str)> = history
            .iter()
            .filter_map(|m| match m {
                Message::ToolResult { call_id, content } => {
                    Some((call_id.as_str(), content.as_str()))
                }
                _ => None,
            })
            .collect();
        assert_eq!(pairs, vec![("c1", "assets"), ("c2", "stats")]);
    }

    /// The cost gate. A model that never stops asking must still be stopped,
    /// and must still say something.
    #[tokio::test]
    async fn caps_tool_rounds_then_forces_an_answer_with_no_tools() {
        let model = ScriptedModel::new(vec![
            ModelTurn::calling(vec![call("c1", "find_assets", "{}")]),
            ModelTurn::calling(vec![call("c2", "find_assets", "{}")]),
            // Would keep going, but the cap lands first.
            ModelTurn::answer("here's what I found so far"),
        ]);
        let executor = StubExecutor::new(vec![("find_assets", ToolOutcome::ok("some"))]);

        let run = run(
            &model,
            &executor,
            &[tool("find_assets")],
            seed("loop forever"),
            Limits { max_tool_rounds: 2 },
        )
        .await
        .unwrap();

        assert_eq!(run.stop, StopReason::ToolRoundsExhausted);
        assert_eq!(run.answer.as_deref(), Some("here's what I found so far"));
        assert_eq!(run.calls.len(), 2);

        // Two rounds saw the tool; the forced final turn saw none.
        assert_eq!(*model.tools_offered.borrow(), vec![1, 1, 0]);
    }

    /// A tool failure is information, not a dead end.
    #[tokio::test]
    async fn a_failing_tool_reaches_the_model_as_content() {
        let model = ScriptedModel::new(vec![
            ModelTurn::calling(vec![call("c1", "resolve_traits", r#"{"query":"ghoul"}"#)]),
            ModelTurn::answer("no such trait — did you mean Ghoul Skin?"),
        ]);
        let executor = StubExecutor::new(vec![(
            "resolve_traits",
            ToolOutcome::error("no trait value matching 'ghoul'"),
        )]);

        let run = run(
            &model,
            &executor,
            &[tool("resolve_traits")],
            seed("show me the ghouls"),
            Limits::default(),
        )
        .await
        .unwrap();

        assert_eq!(run.stop, StopReason::Answered);
        let history = model.last_history.borrow();
        assert!(history.iter().any(|m| matches!(
            m,
            Message::ToolResult { content, .. } if content.contains("no trait value")
        )));
    }

    #[tokio::test]
    async fn usage_accumulates_across_every_turn() {
        let model = ScriptedModel::new(vec![
            ModelTurn::calling(vec![call("c1", "find_assets", "{}")]).with_usage(Usage::new(100, 10)),
            ModelTurn::answer("done").with_usage(Usage::new(250, 40)),
        ]);
        let executor = StubExecutor::new(vec![("find_assets", ToolOutcome::ok("ok"))]);

        let run = run(
            &model,
            &executor,
            &[tool("find_assets")],
            seed("count it"),
            Limits::default(),
        )
        .await
        .unwrap();

        assert_eq!(run.usage, Usage::new(350, 50));
        assert_eq!(run.usage.total(), 400);
    }

    #[tokio::test]
    async fn a_model_failure_ends_the_run() {
        let model = ScriptedModel::new(vec![]); // script exhausted immediately
        let executor = StubExecutor::new(vec![]);

        let outcome = run(
            &model,
            &executor,
            &[tool("find_assets")],
            seed("anything"),
            Limits::default(),
        )
        .await;

        assert!(matches!(outcome, Err(AgentError::Model(_))));
    }
}
