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

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== QuestionOption Tests ====================

    #[test]
    fn test_question_option_creation() {
        let opt = QuestionOption {
            index: 1,
            label: "Option A".to_string(),
            description: Some("Description of A".to_string()),
        };

        assert_eq!(opt.index, 1);
        assert_eq!(opt.label, "Option A");
        assert_eq!(opt.description, Some("Description of A".to_string()));
    }

    #[test]
    fn test_question_option_without_description() {
        let opt = QuestionOption {
            index: 2,
            label: "Simple option".to_string(),
            description: None,
        };

        assert_eq!(opt.index, 2);
        assert!(opt.description.is_none());
    }

    #[test]
    fn test_question_option_serialization() {
        let opt = QuestionOption {
            index: 1,
            label: "Test".to_string(),
            description: Some("Desc".to_string()),
        };

        let json = serde_json::to_string(&opt).unwrap();
        assert!(json.contains(r#""index":1"#));
        assert!(json.contains(r#""label":"Test""#));
        assert!(json.contains(r#""description":"Desc""#));
    }

    #[test]
    fn test_question_option_deserialization() {
        let json = r#"{"index":3,"label":"Choice","description":null}"#;
        let opt: QuestionOption = serde_json::from_str(json).unwrap();

        assert_eq!(opt.index, 3);
        assert_eq!(opt.label, "Choice");
        assert!(opt.description.is_none());
    }

    #[test]
    fn test_question_option_indices_are_one_based() {
        // Verify that option indices should start at 1 (for terminal navigation)
        let options = vec![
            QuestionOption { index: 1, label: "First".to_string(), description: None },
            QuestionOption { index: 2, label: "Second".to_string(), description: None },
            QuestionOption { index: 3, label: "Third".to_string(), description: None },
        ];

        // All indices should be positive and start from 1
        for (i, opt) in options.iter().enumerate() {
            assert_eq!(opt.index, i + 1, "Option indices should be 1-based");
        }
    }

    // ==================== InteractiveQuestion Tests ====================

    #[test]
    fn test_interactive_question_new() {
        let options = vec![
            QuestionOption { index: 1, label: "Yes".to_string(), description: None },
            QuestionOption { index: 2, label: "No".to_string(), description: None },
        ];

        let q = InteractiveQuestion::new(
            "Confirm".to_string(),
            "Are you sure?".to_string(),
            options,
            false,
        );

        assert_eq!(q.header, "Confirm");
        assert_eq!(q.question, "Are you sure?");
        assert_eq!(q.options.len(), 2);
        assert!(!q.multi_select);
        // ID should be a valid UUID
        assert!(!q.id.is_empty());
        assert!(Uuid::parse_str(&q.id).is_ok());
    }

    #[test]
    fn test_interactive_question_multi_select() {
        let options = vec![
            QuestionOption { index: 1, label: "Feature A".to_string(), description: None },
            QuestionOption { index: 2, label: "Feature B".to_string(), description: None },
            QuestionOption { index: 3, label: "Feature C".to_string(), description: None },
        ];

        let q = InteractiveQuestion::new(
            "Features".to_string(),
            "Select features to enable:".to_string(),
            options,
            true,
        );

        assert!(q.multi_select);
    }

    #[test]
    fn test_interactive_question_unique_ids() {
        let q1 = InteractiveQuestion::new("H1".to_string(), "Q1?".to_string(), vec![], false);
        let q2 = InteractiveQuestion::new("H2".to_string(), "Q2?".to_string(), vec![], false);

        assert_ne!(q1.id, q2.id, "Each question should have a unique ID");
    }

    #[test]
    fn test_interactive_question_serialization() {
        let q = InteractiveQuestion {
            id: "test-id-123".to_string(),
            header: "Model".to_string(),
            question: "Which model?".to_string(),
            options: vec![
                QuestionOption { index: 1, label: "Opus".to_string(), description: Some("Best".to_string()) },
                QuestionOption { index: 2, label: "Sonnet".to_string(), description: None },
            ],
            multi_select: false,
        };

        let json = serde_json::to_string(&q).unwrap();
        assert!(json.contains(r#""id":"test-id-123""#));
        assert!(json.contains(r#""header":"Model""#));
        assert!(json.contains(r#""question":"Which model?""#));
        assert!(json.contains(r#""multi_select":false"#));
        assert!(json.contains(r#""label":"Opus""#));
    }

    #[test]
    fn test_interactive_question_deserialization() {
        let json = r#"{
            "id": "q-001",
            "header": "Auth",
            "question": "Select auth method:",
            "options": [
                {"index": 1, "label": "OAuth", "description": "Use OAuth 2.0"},
                {"index": 2, "label": "API Key", "description": null}
            ],
            "multi_select": false
        }"#;

        let q: InteractiveQuestion = serde_json::from_str(json).unwrap();

        assert_eq!(q.id, "q-001");
        assert_eq!(q.header, "Auth");
        assert_eq!(q.question, "Select auth method:");
        assert_eq!(q.options.len(), 2);
        assert_eq!(q.options[0].label, "OAuth");
        assert_eq!(q.options[0].description, Some("Use OAuth 2.0".to_string()));
        assert!(q.options[1].description.is_none());
    }

    #[test]
    fn test_interactive_question_with_empty_options() {
        // Edge case: question with no options
        let q = InteractiveQuestion::new(
            "Empty".to_string(),
            "No choices available".to_string(),
            vec![],
            false,
        );

        assert!(q.options.is_empty());
    }

    #[test]
    fn test_interactive_question_clone() {
        let q = InteractiveQuestion::new(
            "Header".to_string(),
            "Question?".to_string(),
            vec![QuestionOption { index: 1, label: "A".to_string(), description: None }],
            false,
        );

        let cloned = q.clone();
        assert_eq!(q.id, cloned.id);
        assert_eq!(q.header, cloned.header);
        assert_eq!(q.options.len(), cloned.options.len());
    }

    // ==================== InteractivePrompt Tests ====================

    #[test]
    fn test_interactive_prompt_new_single_question() {
        let questions = vec![
            InteractiveQuestion::new("Q1".to_string(), "First?".to_string(), vec![], false),
        ];

        let prompt = InteractivePrompt::new(questions);

        assert!(!prompt.id.is_empty());
        assert!(Uuid::parse_str(&prompt.id).is_ok());
        assert_eq!(prompt.questions.len(), 1);
    }

    #[test]
    fn test_interactive_prompt_new_multiple_questions() {
        let questions = vec![
            InteractiveQuestion::new("Q1".to_string(), "First?".to_string(), vec![], false),
            InteractiveQuestion::new("Q2".to_string(), "Second?".to_string(), vec![], false),
            InteractiveQuestion::new("Q3".to_string(), "Third?".to_string(), vec![], true),
        ];

        let prompt = InteractivePrompt::new(questions);

        assert_eq!(prompt.questions.len(), 3);
        assert_eq!(prompt.questions[0].header, "Q1");
        assert_eq!(prompt.questions[2].header, "Q3");
        assert!(prompt.questions[2].multi_select);
    }

    #[test]
    fn test_interactive_prompt_unique_ids() {
        let p1 = InteractivePrompt::new(vec![]);
        let p2 = InteractivePrompt::new(vec![]);

        assert_ne!(p1.id, p2.id, "Each prompt should have a unique ID");
    }

    #[test]
    fn test_interactive_prompt_serialization() {
        let prompt = InteractivePrompt {
            id: "prompt-123".to_string(),
            questions: vec![
                InteractiveQuestion {
                    id: "q1".to_string(),
                    header: "H1".to_string(),
                    question: "Q1?".to_string(),
                    options: vec![],
                    multi_select: false,
                },
            ],
        };

        let json = serde_json::to_string(&prompt).unwrap();
        assert!(json.contains(r#""id":"prompt-123""#));
        assert!(json.contains(r#""questions":"#));
    }

    #[test]
    fn test_interactive_prompt_deserialization() {
        let json = r#"{
            "id": "p-001",
            "questions": [
                {"id": "q1", "header": "Test", "question": "Q?", "options": [], "multi_select": false}
            ]
        }"#;

        let prompt: InteractivePrompt = serde_json::from_str(json).unwrap();

        assert_eq!(prompt.id, "p-001");
        assert_eq!(prompt.questions.len(), 1);
        assert_eq!(prompt.questions[0].header, "Test");
    }

    #[test]
    fn test_interactive_prompt_max_questions() {
        // Claude Code allows up to 4 questions in a single AskUserQuestion call
        let questions = (1..=4).map(|i| {
            InteractiveQuestion::new(
                format!("Q{}", i),
                format!("Question {}?", i),
                vec![],
                false,
            )
        }).collect();

        let prompt = InteractivePrompt::new(questions);

        assert_eq!(prompt.questions.len(), 4);
    }

    #[test]
    fn test_interactive_prompt_empty() {
        // Edge case: prompt with no questions
        let prompt = InteractivePrompt::new(vec![]);

        assert!(prompt.questions.is_empty());
    }

    // ==================== InteractiveEvent Tests ====================

    #[test]
    fn test_interactive_event_prompt_presented() {
        let session_id = Uuid::new_v4();
        let prompt = InteractivePrompt::new(vec![]);

        let event = InteractiveEvent::PromptPresented {
            session_id,
            prompt: prompt.clone(),
        };

        match event {
            InteractiveEvent::PromptPresented { session_id: sid, prompt: p } => {
                assert_eq!(sid, session_id);
                assert_eq!(p.id, prompt.id);
            }
            _ => panic!("Expected PromptPresented"),
        }
    }

    #[test]
    fn test_interactive_event_interaction_complete() {
        let session_id = Uuid::new_v4();

        let event = InteractiveEvent::InteractionComplete { session_id };

        match event {
            InteractiveEvent::InteractionComplete { session_id: sid } => {
                assert_eq!(sid, session_id);
            }
            _ => panic!("Expected InteractionComplete"),
        }
    }

    #[test]
    fn test_interactive_event_serialization_prompt_presented() {
        let session_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440000").unwrap();
        let prompt = InteractivePrompt {
            id: "p1".to_string(),
            questions: vec![],
        };

        let event = InteractiveEvent::PromptPresented { session_id, prompt };
        let json = serde_json::to_string(&event).unwrap();

        // Should have snake_case type tag
        assert!(json.contains(r#""type":"prompt_presented""#));
        assert!(json.contains(r#""session_id":"550e8400-e29b-41d4-a716-446655440000""#));
    }

    #[test]
    fn test_interactive_event_serialization_interaction_complete() {
        let session_id = Uuid::parse_str("550e8400-e29b-41d4-a716-446655440001").unwrap();

        let event = InteractiveEvent::InteractionComplete { session_id };
        let json = serde_json::to_string(&event).unwrap();

        assert!(json.contains(r#""type":"interaction_complete""#));
        assert!(json.contains(r#""session_id":"550e8400-e29b-41d4-a716-446655440001""#));
    }

    #[test]
    fn test_interactive_event_deserialization_prompt_presented() {
        let json = r#"{
            "type": "prompt_presented",
            "session_id": "550e8400-e29b-41d4-a716-446655440000",
            "prompt": {"id": "p1", "questions": []}
        }"#;

        let event: InteractiveEvent = serde_json::from_str(json).unwrap();

        match event {
            InteractiveEvent::PromptPresented { session_id, prompt } => {
                assert_eq!(session_id.to_string(), "550e8400-e29b-41d4-a716-446655440000");
                assert_eq!(prompt.id, "p1");
            }
            _ => panic!("Expected PromptPresented"),
        }
    }

    #[test]
    fn test_interactive_event_deserialization_interaction_complete() {
        let json = r#"{
            "type": "interaction_complete",
            "session_id": "550e8400-e29b-41d4-a716-446655440001"
        }"#;

        let event: InteractiveEvent = serde_json::from_str(json).unwrap();

        match event {
            InteractiveEvent::InteractionComplete { session_id } => {
                assert_eq!(session_id.to_string(), "550e8400-e29b-41d4-a716-446655440001");
            }
            _ => panic!("Expected InteractionComplete"),
        }
    }

    #[test]
    fn test_interactive_event_roundtrip() {
        let session_id = Uuid::new_v4();
        let prompt = InteractivePrompt::new(vec![
            InteractiveQuestion::new(
                "Test".to_string(),
                "Question?".to_string(),
                vec![
                    QuestionOption { index: 1, label: "A".to_string(), description: None },
                    QuestionOption { index: 2, label: "B".to_string(), description: Some("Desc".to_string()) },
                ],
                false,
            ),
        ]);

        let event = InteractiveEvent::PromptPresented {
            session_id,
            prompt: prompt.clone(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let deserialized: InteractiveEvent = serde_json::from_str(&json).unwrap();

        match deserialized {
            InteractiveEvent::PromptPresented { session_id: sid, prompt: p } => {
                assert_eq!(sid, session_id);
                assert_eq!(p.id, prompt.id);
                assert_eq!(p.questions.len(), 1);
                assert_eq!(p.questions[0].options.len(), 2);
                assert_eq!(p.questions[0].options[1].description, Some("Desc".to_string()));
            }
            _ => panic!("Expected PromptPresented"),
        }
    }

    #[test]
    fn test_interactive_event_clone() {
        let event = InteractiveEvent::InteractionComplete {
            session_id: Uuid::new_v4(),
        };

        let cloned = event.clone();

        match (&event, &cloned) {
            (
                InteractiveEvent::InteractionComplete { session_id: s1 },
                InteractiveEvent::InteractionComplete { session_id: s2 },
            ) => {
                assert_eq!(s1, s2);
            }
            _ => panic!("Clone should preserve variant"),
        }
    }

    // ==================== Integration-style Tests ====================

    #[test]
    fn test_typical_model_selection_prompt() {
        // Simulates the /model command interactive prompt
        let options = vec![
            QuestionOption { index: 1, label: "opus".to_string(), description: Some("Claude Opus 4.5 (Recommended)".to_string()) },
            QuestionOption { index: 2, label: "sonnet".to_string(), description: Some("Claude Sonnet 4".to_string()) },
            QuestionOption { index: 3, label: "haiku".to_string(), description: Some("Claude Haiku 3.5".to_string()) },
        ];

        let question = InteractiveQuestion::new(
            "Model".to_string(),
            "Which model would you like to use?".to_string(),
            options,
            false,
        );

        let prompt = InteractivePrompt::new(vec![question]);

        assert_eq!(prompt.questions.len(), 1);
        assert_eq!(prompt.questions[0].options.len(), 3);
        assert!(!prompt.questions[0].multi_select);
    }

    #[test]
    fn test_multi_question_config_prompt() {
        // Simulates a complex /config command with multiple questions
        let q1 = InteractiveQuestion::new(
            "Theme".to_string(),
            "Select theme:".to_string(),
            vec![
                QuestionOption { index: 1, label: "Dark".to_string(), description: None },
                QuestionOption { index: 2, label: "Light".to_string(), description: None },
            ],
            false,
        );

        let q2 = InteractiveQuestion::new(
            "Features".to_string(),
            "Enable features:".to_string(),
            vec![
                QuestionOption { index: 1, label: "Auto-save".to_string(), description: None },
                QuestionOption { index: 2, label: "Notifications".to_string(), description: None },
                QuestionOption { index: 3, label: "Analytics".to_string(), description: None },
            ],
            true, // multi-select for features
        );

        let prompt = InteractivePrompt::new(vec![q1, q2]);

        assert_eq!(prompt.questions.len(), 2);
        assert!(!prompt.questions[0].multi_select);
        assert!(prompt.questions[1].multi_select);
        assert_eq!(prompt.questions[1].options.len(), 3);
    }
}
