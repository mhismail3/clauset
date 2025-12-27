//! Types for TUI menu detection and native rendering.
//!
//! These types represent terminal-based selection menus from Claude Code's
//! built-in commands (like /model, /config) that are detected from ANSI
//! terminal output and rendered natively in the chat UI.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A single option in a TUI selection menu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TuiMenuOption {
    /// 0-based index of this option
    pub index: usize,
    /// Display label (e.g., "Claude Sonnet 4")
    pub label: String,
    /// Optional description (e.g., "Balanced performance and speed")
    pub description: Option<String>,
    /// Whether this option is currently selected (has checkmark)
    pub is_selected: bool,
}

/// Types of TUI menus Claude Code presents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TuiMenuType {
    /// Model selection (/model command)
    ModelSelect,
    /// Configuration options (/config)
    Config,
    /// Permission settings
    Permissions,
    /// Mode selection (/mode)
    Mode,
    /// Generic/unknown menu type
    #[default]
    Generic,
}

/// Represents a detected TUI selection menu.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TuiMenu {
    /// Unique identifier for this menu instance
    pub id: String,
    /// Menu title (e.g., "Select model")
    pub title: String,
    /// Subtitle/description (e.g., "Switch between Claude models...")
    pub description: Option<String>,
    /// Available options
    pub options: Vec<TuiMenuOption>,
    /// Type of menu (for specialized handling)
    pub menu_type: TuiMenuType,
    /// Index of currently highlighted option (with cursor/arrow)
    pub highlighted_index: usize,
}

/// Events for TUI menu lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TuiMenuEvent {
    /// A TUI menu has been detected and parsed from terminal output
    MenuPresented {
        session_id: Uuid,
        menu: TuiMenu,
    },
    /// The menu was dismissed (user cancelled or made selection)
    MenuDismissed {
        session_id: Uuid,
        menu_id: String,
    },
}

impl TuiMenuOption {
    /// Create a new TUI menu option.
    pub fn new(index: usize, label: String, description: Option<String>, is_selected: bool) -> Self {
        Self {
            index,
            label,
            description,
            is_selected,
        }
    }
}

impl TuiMenu {
    /// Create a new TUI menu with the given title and options.
    pub fn new(title: String, options: Vec<TuiMenuOption>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            description: None,
            options,
            menu_type: TuiMenuType::Generic,
            highlighted_index: 0,
        }
    }

    /// Create a new TUI menu with all fields specified.
    pub fn with_details(
        title: String,
        description: Option<String>,
        options: Vec<TuiMenuOption>,
        menu_type: TuiMenuType,
        highlighted_index: usize,
    ) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            title,
            description,
            options,
            menu_type,
            highlighted_index,
        }
    }

    /// Infer menu type from title keywords.
    pub fn infer_menu_type(title: &str) -> TuiMenuType {
        let title_lower = title.to_lowercase();
        if title_lower.contains("model") {
            TuiMenuType::ModelSelect
        } else if title_lower.contains("config") {
            TuiMenuType::Config
        } else if title_lower.contains("permission") {
            TuiMenuType::Permissions
        } else if title_lower.contains("mode") {
            TuiMenuType::Mode
        } else {
            TuiMenuType::Generic
        }
    }

    /// Get the currently selected option (if any has checkmark).
    pub fn selected_option(&self) -> Option<&TuiMenuOption> {
        self.options.iter().find(|opt| opt.is_selected)
    }

    /// Get the currently highlighted option.
    pub fn highlighted_option(&self) -> Option<&TuiMenuOption> {
        self.options.get(self.highlighted_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ==================== TuiMenuOption Tests ====================

    #[test]
    fn test_menu_option_creation() {
        let opt = TuiMenuOption::new(
            0,
            "Claude Sonnet".to_string(),
            Some("Balanced performance".to_string()),
            false,
        );

        assert_eq!(opt.index, 0);
        assert_eq!(opt.label, "Claude Sonnet");
        assert_eq!(opt.description, Some("Balanced performance".to_string()));
        assert!(!opt.is_selected);
    }

    #[test]
    fn test_menu_option_without_description() {
        let opt = TuiMenuOption::new(1, "Option".to_string(), None, true);

        assert_eq!(opt.index, 1);
        assert!(opt.description.is_none());
        assert!(opt.is_selected);
    }

    #[test]
    fn test_menu_option_serialization() {
        let opt = TuiMenuOption::new(0, "Test".to_string(), Some("Desc".to_string()), true);

        let json = serde_json::to_string(&opt).unwrap();
        assert!(json.contains(r#""index":0"#));
        assert!(json.contains(r#""label":"Test""#));
        assert!(json.contains(r#""description":"Desc""#));
        assert!(json.contains(r#""is_selected":true"#));
    }

    #[test]
    fn test_menu_option_deserialization() {
        let json = r#"{"index":2,"label":"Choice","description":null,"is_selected":false}"#;
        let opt: TuiMenuOption = serde_json::from_str(json).unwrap();

        assert_eq!(opt.index, 2);
        assert_eq!(opt.label, "Choice");
        assert!(opt.description.is_none());
        assert!(!opt.is_selected);
    }

    // ==================== TuiMenuType Tests ====================

    #[test]
    fn test_menu_type_serialization() {
        assert_eq!(
            serde_json::to_string(&TuiMenuType::ModelSelect).unwrap(),
            r#""model_select""#
        );
        assert_eq!(
            serde_json::to_string(&TuiMenuType::Config).unwrap(),
            r#""config""#
        );
        assert_eq!(
            serde_json::to_string(&TuiMenuType::Generic).unwrap(),
            r#""generic""#
        );
    }

    #[test]
    fn test_menu_type_deserialization() {
        assert_eq!(
            serde_json::from_str::<TuiMenuType>(r#""model_select""#).unwrap(),
            TuiMenuType::ModelSelect
        );
        assert_eq!(
            serde_json::from_str::<TuiMenuType>(r#""permissions""#).unwrap(),
            TuiMenuType::Permissions
        );
    }

    #[test]
    fn test_menu_type_default() {
        assert_eq!(TuiMenuType::default(), TuiMenuType::Generic);
    }

    // ==================== TuiMenu Tests ====================

    #[test]
    fn test_menu_creation() {
        let options = vec![
            TuiMenuOption::new(0, "Option A".to_string(), None, false),
            TuiMenuOption::new(1, "Option B".to_string(), None, true),
        ];

        let menu = TuiMenu::new("Select option".to_string(), options);

        assert!(!menu.id.is_empty());
        assert_eq!(menu.title, "Select option");
        assert!(menu.description.is_none());
        assert_eq!(menu.options.len(), 2);
        assert_eq!(menu.menu_type, TuiMenuType::Generic);
        assert_eq!(menu.highlighted_index, 0);
    }

    #[test]
    fn test_menu_with_details() {
        let options = vec![
            TuiMenuOption::new(0, "Sonnet".to_string(), Some("Fast".to_string()), false),
            TuiMenuOption::new(1, "Opus".to_string(), Some("Smart".to_string()), true),
        ];

        let menu = TuiMenu::with_details(
            "Select model".to_string(),
            Some("Choose your model".to_string()),
            options,
            TuiMenuType::ModelSelect,
            1,
        );

        assert_eq!(menu.title, "Select model");
        assert_eq!(menu.description, Some("Choose your model".to_string()));
        assert_eq!(menu.menu_type, TuiMenuType::ModelSelect);
        assert_eq!(menu.highlighted_index, 1);
    }

    #[test]
    fn test_menu_infer_type_model() {
        assert_eq!(TuiMenu::infer_menu_type("Select model"), TuiMenuType::ModelSelect);
        assert_eq!(TuiMenu::infer_menu_type("Model Selection"), TuiMenuType::ModelSelect);
    }

    #[test]
    fn test_menu_infer_type_config() {
        assert_eq!(TuiMenu::infer_menu_type("Configuration"), TuiMenuType::Config);
        assert_eq!(TuiMenu::infer_menu_type("Select config option"), TuiMenuType::Config);
    }

    #[test]
    fn test_menu_infer_type_permissions() {
        assert_eq!(TuiMenu::infer_menu_type("Permission settings"), TuiMenuType::Permissions);
    }

    #[test]
    fn test_menu_infer_type_mode() {
        assert_eq!(TuiMenu::infer_menu_type("Select mode"), TuiMenuType::Mode);
    }

    #[test]
    fn test_menu_infer_type_generic() {
        assert_eq!(TuiMenu::infer_menu_type("Choose an option"), TuiMenuType::Generic);
        assert_eq!(TuiMenu::infer_menu_type("Select something"), TuiMenuType::Generic);
    }

    #[test]
    fn test_menu_selected_option() {
        let options = vec![
            TuiMenuOption::new(0, "A".to_string(), None, false),
            TuiMenuOption::new(1, "B".to_string(), None, true),
            TuiMenuOption::new(2, "C".to_string(), None, false),
        ];

        let menu = TuiMenu::new("Test".to_string(), options);

        let selected = menu.selected_option().unwrap();
        assert_eq!(selected.index, 1);
        assert_eq!(selected.label, "B");
    }

    #[test]
    fn test_menu_no_selected_option() {
        let options = vec![
            TuiMenuOption::new(0, "A".to_string(), None, false),
            TuiMenuOption::new(1, "B".to_string(), None, false),
        ];

        let menu = TuiMenu::new("Test".to_string(), options);

        assert!(menu.selected_option().is_none());
    }

    #[test]
    fn test_menu_highlighted_option() {
        let options = vec![
            TuiMenuOption::new(0, "A".to_string(), None, false),
            TuiMenuOption::new(1, "B".to_string(), None, false),
            TuiMenuOption::new(2, "C".to_string(), None, false),
        ];

        let mut menu = TuiMenu::new("Test".to_string(), options);
        menu.highlighted_index = 2;

        let highlighted = menu.highlighted_option().unwrap();
        assert_eq!(highlighted.index, 2);
        assert_eq!(highlighted.label, "C");
    }

    #[test]
    fn test_menu_serialization() {
        let options = vec![TuiMenuOption::new(0, "Option".to_string(), None, false)];
        let menu = TuiMenu::new("Test Menu".to_string(), options);

        let json = serde_json::to_string(&menu).unwrap();
        assert!(json.contains(r#""title":"Test Menu""#));
        assert!(json.contains(r#""menu_type":"generic""#));
        assert!(json.contains(r#""highlighted_index":0"#));
    }

    #[test]
    fn test_menu_deserialization() {
        let json = r#"{
            "id": "test-id",
            "title": "Select model",
            "description": "Choose a model",
            "options": [
                {"index": 0, "label": "Sonnet", "description": null, "is_selected": true}
            ],
            "menu_type": "model_select",
            "highlighted_index": 0
        }"#;

        let menu: TuiMenu = serde_json::from_str(json).unwrap();
        assert_eq!(menu.id, "test-id");
        assert_eq!(menu.title, "Select model");
        assert_eq!(menu.description, Some("Choose a model".to_string()));
        assert_eq!(menu.options.len(), 1);
        assert_eq!(menu.menu_type, TuiMenuType::ModelSelect);
    }

    // ==================== TuiMenuEvent Tests ====================

    #[test]
    fn test_menu_presented_event_serialization() {
        let session_id = Uuid::nil();
        let menu = TuiMenu::new("Test".to_string(), vec![]);
        let event = TuiMenuEvent::MenuPresented {
            session_id,
            menu: menu.clone(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"menu_presented""#));
        assert!(json.contains(r#""session_id""#));
        assert!(json.contains(r#""menu""#));
    }

    #[test]
    fn test_menu_dismissed_event_serialization() {
        let session_id = Uuid::nil();
        let event = TuiMenuEvent::MenuDismissed {
            session_id,
            menu_id: "menu-123".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"menu_dismissed""#));
        assert!(json.contains(r#""menu_id":"menu-123""#));
    }

    #[test]
    fn test_menu_event_deserialization() {
        let json = r#"{
            "type": "menu_presented",
            "session_id": "00000000-0000-0000-0000-000000000000",
            "menu": {
                "id": "m1",
                "title": "Test",
                "description": null,
                "options": [],
                "menu_type": "generic",
                "highlighted_index": 0
            }
        }"#;

        let event: TuiMenuEvent = serde_json::from_str(json).unwrap();
        match event {
            TuiMenuEvent::MenuPresented { session_id, menu } => {
                assert_eq!(session_id, Uuid::nil());
                assert_eq!(menu.title, "Test");
            }
            _ => panic!("Expected MenuPresented event"),
        }
    }

    #[test]
    fn test_menu_event_roundtrip() {
        let session_id = Uuid::new_v4();
        let options = vec![
            TuiMenuOption::new(0, "Opus".to_string(), Some("Powerful".to_string()), false),
            TuiMenuOption::new(1, "Sonnet".to_string(), Some("Balanced".to_string()), true),
        ];
        let menu = TuiMenu::with_details(
            "Select model".to_string(),
            Some("Choose your model".to_string()),
            options,
            TuiMenuType::ModelSelect,
            1,
        );

        let event = TuiMenuEvent::MenuPresented {
            session_id,
            menu: menu.clone(),
        };

        let json = serde_json::to_string(&event).unwrap();
        let parsed: TuiMenuEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(event, parsed);
    }
}
