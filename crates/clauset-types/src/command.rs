//! Types for Claude Code slash command discovery.

use serde::{Deserialize, Serialize};

/// Category of a slash command.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCategory {
    /// Built-in Claude Code commands
    BuiltIn,
    /// User-defined commands in ~/.claude/commands/
    User,
    /// Skills in ~/.claude/skills/
    Skill,
    /// Plugin commands in ~/.claude/plugins/cache/
    Plugin,
}

/// A discovered slash command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    /// Command name without leading slash (e.g., "optimize", "compact")
    pub name: String,
    /// Display name with leading slash (e.g., "/optimize", "/compact")
    pub display_name: String,
    /// Brief description of what the command does
    pub description: String,
    /// Source category
    pub category: CommandCategory,
    /// Hint for expected arguments (e.g., "[file-path]")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub argument_hint: Option<String>,
    /// Source location (file path or "built-in")
    pub source: String,
    /// Plugin name if category is Plugin
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plugin_name: Option<String>,
}

/// YAML frontmatter for commands and skills.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CommandFrontmatter {
    pub name: Option<String>,
    pub description: Option<String>,
    #[serde(rename = "argument-hint")]
    pub argument_hint: Option<String>,
    #[serde(rename = "allowed-tools")]
    pub allowed_tools: Option<String>,
}

/// Summary response for the commands list endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandsResponse {
    pub commands: Vec<Command>,
    pub counts: CommandCounts,
}

/// Count of commands by category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandCounts {
    pub built_in: usize,
    pub user: usize,
    pub skill: usize,
    pub plugin: usize,
    pub total: usize,
}
