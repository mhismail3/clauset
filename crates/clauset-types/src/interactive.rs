//! Types for interactive slash command prompts.
//!
//! These types represent interactive questions presented by Claude Code's
//! AskUserQuestion tool, enabling native chat UI rendering instead of
//! raw terminal output.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// An interactive question presented to the user.
///
/// Maps to Claude Code's AskUserQuestion tool input format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveQuestion {
    /// Unique identifier for this question instance
    pub id: String,
    /// Short label (max 12 chars) displayed as header
    pub header: String,
    /// Full question text
    pub question: String,
    /// Available options to choose from
    pub options: Vec<QuestionOption>,
    /// Whether multiple selections are allowed
    pub multi_select: bool,
}

/// A single option within an interactive question.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionOption {
    /// 1-based index (used for PTY response)
    pub index: usize,
    /// Display text for this option
    pub label: String,
    /// Optional longer description
    pub description: Option<String>,
}

/// A batch of questions from a single AskUserQuestion tool call.
///
/// Claude Code's AskUserQuestion can contain 1-4 questions that should
/// all be presented together and answered before continuing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractivePrompt {
    /// Unique identifier for this prompt batch
    pub id: String,
    /// All questions in this batch
    pub questions: Vec<InteractiveQuestion>,
}

/// Events for interactive prompt lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractiveEvent {
    /// A prompt with one or more questions has been presented
    PromptPresented {
        session_id: Uuid,
        prompt: InteractivePrompt,
    },
    /// The interactive prompt has completed (answered or cancelled)
    InteractionComplete {
        session_id: Uuid,
    },
}

impl InteractiveQuestion {
    /// Create a new interactive question from parsed tool input.
    pub fn new(
        header: String,
        question: String,
        options: Vec<QuestionOption>,
        multi_select: bool,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            header,
            question,
            options,
            multi_select,
        }
    }
}

impl InteractivePrompt {
    /// Create a new prompt batch from a list of questions.
    pub fn new(questions: Vec<InteractiveQuestion>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            questions,
        }
    }
}
