//! The loop.

use std::collections::HashSet;

use crate::{AgentError, ChatModel, Message, ToolCall, ToolDef, ToolExecutor, ToolOutcome, Usage};

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
    history: Vec<Message>,
    limits: Limits,
) -> Result<AgentRun, AgentError>
where
    M: ChatModel,
    E: ToolExecutor,
{
    run_with_progress(model, executor, tools, history, limits, |_| async {}).await
}

/// [`run`], with a hook fired before every model turn.
///
/// Exists for one concrete reason: a run takes long enough that the caller
/// needs to keep saying so. Discord's typing indicator lasts about ten seconds
/// and a two-round run comfortably outlives it, so a single "typing…" before
/// the loop leaves the user watching nothing for the rest of it.
///
/// The hook is deliberately *before the turn* rather than after: the point is
/// to cover the wait that is about to happen. It gets the round number (0-based)
/// so a caller can distinguish "starting" from "still going".
pub async fn run_with_progress<M, E, F, Fut>(
    model: &M,
    executor: &E,
    tools: &[ToolDef],
    mut history: Vec<Message>,
    limits: Limits,
    mut on_turn: F,
) -> Result<AgentRun, AgentError>
where
    M: ChatModel,
    E: ToolExecutor,
    F: FnMut(u8) -> Fut,
    Fut: core::future::Future<Output = ()>,
{
    let mut calls = Vec::new();
    let mut usage = Usage::default();
    // Calls that already failed, by (name, arguments). A model that can't
    // express what it wants will often reach for the same wrong thing again,
    // and each retry costs a full round — the whole prompt back through the
    // model — for a result we already know.
    let mut failed: HashSet<(String, String)> = HashSet::new();

    for round in 0..limits.max_tool_rounds {
        on_turn(round).await;
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
        let mut fresh = 0usize;
        for call in turn.calls {
            let signature = (call.name.clone(), call.arguments.clone());

            let outcome = if failed.contains(&signature) {
                // Answered from memory. Saying so is more useful than silently
                // repeating the same error, and it costs nothing.
                ToolOutcome::error(format!(
                    "`{}` already failed with exactly these arguments. Try different \
                     arguments, a different tool, or answer without it.",
                    call.name
                ))
            } else {
                fresh += 1;
                executor.execute(&call).await
            };

            if outcome.is_error {
                failed.insert(signature);
            }
            history.push(Message::ToolResult {
                call_id: call.id.clone(),
                content: outcome.content,
            });
            calls.push(call);
        }

        // A whole round of nothing but repeats means the model is stuck rather
        // than working. Stop spending rounds on it and go straight to the
        // forced answer, which is where this was heading anyway.
        if fresh == 0 {
            break;
        }
    }

    // Out of rounds. Ask once more with no tools, so the model answers from
    // what it gathered instead of the run ending in silence.
    on_turn(limits.max_tool_rounds).await;
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
    use crate::ModelTurn;
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
        ToolDef::new(name, format!("the {name} tool"), vec![])
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

    /// The typing-indicator contract: one hook per model turn, *including* the
    /// forced final one. A run whose last turn wasn't covered would drop the
    /// indicator right before the answer lands, which is the worst moment.
    #[tokio::test]
    async fn progress_fires_once_before_every_model_turn() {
        let model = ScriptedModel::new(vec![
            ModelTurn::calling(vec![call("c1", "find_assets", "{}")]),
            ModelTurn::calling(vec![call("c2", "find_assets", "{}")]),
            ModelTurn::answer("done"),
        ]);
        let executor = StubExecutor::new(vec![("find_assets", ToolOutcome::ok("ok"))]);
        let seen = RefCell::new(Vec::new());

        let run = run_with_progress(
            &model,
            &executor,
            &[tool("find_assets")],
            seed("keep going"),
            Limits { max_tool_rounds: 2 },
            |round| {
                seen.borrow_mut().push(round);
                async {}
            },
        )
        .await
        .unwrap();

        assert_eq!(run.stop, StopReason::ToolRoundsExhausted);
        // Two rounds plus the forced answer.
        assert_eq!(*seen.borrow(), vec![0, 1, 2]);
    }

    /// The live waste this guards: a model with no way to express what it
    /// wants reaches for the same wrong call every round, and each retry costs
    /// a full pass of the prompt through the model for a known answer.
    #[tokio::test]
    async fn an_identical_failing_call_is_not_run_twice() {
        let repeat = || call("c", "find_assets", r#"{"q":"x"}"#);
        // Round 1 runs it, round 2 repeats it — which ends the loop, so the
        // third turn is the forced answer rather than another attempt.
        let model = ScriptedModel::new(vec![
            ModelTurn::calling(vec![repeat()]),
            ModelTurn::calling(vec![repeat()]),
            ModelTurn::answer("giving up"),
        ]);
        let executor = StubExecutor::new(vec![("find_assets", ToolOutcome::error("nope"))]);

        let run = run(
            &model,
            &executor,
            &[tool("find_assets")],
            seed("keep trying"),
            Limits::default(),
        )
        .await
        .unwrap();

        // Executed once; the repeat short-circuits and ends the loop.
        assert_eq!(*executor.ran.borrow(), vec!["find_assets"]);
        assert_eq!(run.stop, StopReason::ToolRoundsExhausted);
        assert_eq!(run.answer.as_deref(), Some("giving up"));
    }

    /// Different arguments are a genuine retry, not a repeat.
    #[tokio::test]
    async fn a_retry_with_different_arguments_still_runs() {
        let model = ScriptedModel::new(vec![
            ModelTurn::calling(vec![call("c1", "find_assets", r#"{"q":"x"}"#)]),
            ModelTurn::calling(vec![call("c2", "find_assets", r#"{"q":"y"}"#)]),
            ModelTurn::answer("found it"),
        ]);
        let executor = StubExecutor::new(vec![("find_assets", ToolOutcome::error("nope"))]);

        let run = run(
            &model,
            &executor,
            &[tool("find_assets")],
            seed("try again"),
            Limits::default(),
        )
        .await
        .unwrap();

        assert_eq!(*executor.ran.borrow(), vec!["find_assets", "find_assets"]);
        assert_eq!(run.stop, StopReason::Answered);
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
