//! Discovery of Claude Code slash commands from multiple sources.

use crate::Result;
use clauset_types::{Command, CommandCategory, CommandCounts, CommandFrontmatter, CommandsResponse};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tracing::debug;

/// Cache TTL for discovered commands.
const CACHE_TTL: Duration = Duration::from_secs(30);

/// Discovers Claude Code commands from all sources.
pub struct CommandDiscovery {
    claude_dir: PathBuf,
    cache: Option<CachedCommands>,
}

struct CachedCommands {
    commands: Vec<Command>,
    discovered_at: Instant,
}

impl CommandDiscovery {
    /// Create a new command discovery instance.
    pub fn new() -> Self {
        let claude_dir = dirs::home_dir().unwrap_or_default().join(".claude");
        Self {
            claude_dir,
            cache: None,
        }
    }

    /// Discover all commands, using cache if available and fresh.
    pub fn discover_all(&mut self) -> Result<CommandsResponse> {
        // Check cache freshness
        if let Some(ref cached) = self.cache {
            if cached.discovered_at.elapsed() < CACHE_TTL {
                return Ok(self.build_response(&cached.commands));
            }
        }

        // Discover fresh
        let commands = self.discover_fresh();

        // Update cache
        self.cache = Some(CachedCommands {
            commands: commands.clone(),
            discovered_at: Instant::now(),
        });

        Ok(self.build_response(&commands))
    }

    fn discover_fresh(&self) -> Vec<Command> {
        let mut commands = Vec::new();

        // 1. Built-in commands
        commands.extend(self.built_in_commands());

        // 2. User commands
        if let Ok(user_cmds) = self.discover_user_commands() {
            commands.extend(user_cmds);
        }

        // 3. Skills
        if let Ok(skills) = self.discover_skills() {
            commands.extend(skills);
        }

        // 4. Plugin commands
        if let Ok(plugin_cmds) = self.discover_plugin_commands() {
            commands.extend(plugin_cmds);
        }

        // Sort by category then name
        commands.sort_by(|a, b| {
            let cat_order = |c: &CommandCategory| match c {
                CommandCategory::BuiltIn => 0,
                CommandCategory::User => 1,
                CommandCategory::Skill => 2,
                CommandCategory::Plugin => 3,
            };
            match cat_order(&a.category).cmp(&cat_order(&b.category)) {
                std::cmp::Ordering::Equal => a.name.cmp(&b.name),
                other => other,
            }
        });

        commands
    }

    fn build_response(&self, commands: &[Command]) -> CommandsResponse {
        let counts = CommandCounts {
            built_in: commands
                .iter()
                .filter(|c| c.category == CommandCategory::BuiltIn)
                .count(),
            user: commands
                .iter()
                .filter(|c| c.category == CommandCategory::User)
                .count(),
            skill: commands
                .iter()
                .filter(|c| c.category == CommandCategory::Skill)
                .count(),
            plugin: commands
                .iter()
                .filter(|c| c.category == CommandCategory::Plugin)
                .count(),
            total: commands.len(),
        };

        CommandsResponse {
            commands: commands.to_vec(),
            counts,
        }
    }

    /// Built-in Claude Code commands (hardcoded list).
    fn built_in_commands(&self) -> Vec<Command> {
        let builtins = [
            ("help", "Show available commands and shortcuts"),
            ("clear", "Clear conversation history"),
            ("compact", "Compact conversation to reduce context size"),
            ("context", "Show current context usage"),
            ("cost", "Show cost and token usage for this session"),
            ("export", "Export conversation to file"),
            ("model", "Show or change the current model"),
            ("agents", "List available agents"),
            ("mcp", "Manage MCP servers"),
            ("memory", "View or edit project memory"),
            ("resume", "Resume a previous session"),
            ("config", "View or edit configuration"),
            ("permissions", "Manage tool permissions"),
            ("plugin", "Manage plugins"),
            ("sandbox", "Toggle sandbox mode"),
            ("vim", "Toggle vim keybindings"),
            ("status", "Show current session status"),
            ("quit", "Exit Claude Code"),
            ("exit", "Exit Claude Code"),
            ("undo", "Undo the last file change"),
            ("diff", "Show diff of recent changes"),
            ("review", "Review current changes"),
            ("pr", "Create or manage pull requests"),
            ("test", "Run tests"),
            ("lint", "Run linting"),
            ("build", "Build the project"),
            ("run", "Run the project"),
            ("debug", "Debug the project"),
            ("log", "Show session logs"),
            ("settings", "Open settings"),
            ("theme", "Change theme"),
            ("login", "Log in to Anthropic"),
            ("logout", "Log out from Anthropic"),
            ("init", "Initialize Claude Code in a project"),
            ("doctor", "Diagnose common issues"),
            ("version", "Show Claude Code version"),
            ("update", "Check for updates"),
            ("feedback", "Send feedback to Anthropic"),
            ("bug", "Report a bug"),
            ("tasks", "Show running background tasks"),
        ];

        builtins
            .iter()
            .map(|(name, desc)| Command {
                name: name.to_string(),
                display_name: format!("/{}", name),
                description: desc.to_string(),
                category: CommandCategory::BuiltIn,
                argument_hint: None,
                source: "built-in".to_string(),
                plugin_name: None,
            })
            .collect()
    }

    /// Discover user commands from ~/.claude/commands/*.md
    fn discover_user_commands(&self) -> Result<Vec<Command>> {
        let commands_dir = self.claude_dir.join("commands");
        if !commands_dir.exists() {
            return Ok(Vec::new());
        }

        let mut commands = Vec::new();
        self.scan_commands_dir(&commands_dir, CommandCategory::User, None, &mut commands)?;
        Ok(commands)
    }

    /// Discover skills from ~/.claude/skills/*/SKILL.md
    fn discover_skills(&self) -> Result<Vec<Command>> {
        let skills_dir = self.claude_dir.join("skills");
        if !skills_dir.exists() {
            return Ok(Vec::new());
        }

        let mut commands = Vec::new();

        let entries = match fs::read_dir(&skills_dir) {
            Ok(e) => e,
            Err(_) => return Ok(Vec::new()),
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let skill_file = path.join("SKILL.md");
            if !skill_file.exists() {
                continue;
            }

            match self.parse_markdown_file(&skill_file) {
                Ok((frontmatter, first_line)) => {
                    let name = frontmatter
                        .name
                        .or_else(|| path.file_name().and_then(|n| n.to_str()).map(String::from))
                        .unwrap_or_else(|| "unknown".to_string());

                    let description = frontmatter
                        .description
                        .or(first_line)
                        .unwrap_or_else(|| "No description".to_string());

                    // Truncate long descriptions
                    let description = if description.len() > 100 {
                        format!("{}...", &description[..97])
                    } else {
                        description
                    };

                    commands.push(Command {
                        name: name.clone(),
                        display_name: format!("/{}", name),
                        description,
                        category: CommandCategory::Skill,
                        argument_hint: frontmatter.argument_hint,
                        source: skill_file.to_string_lossy().to_string(),
                        plugin_name: None,
                    });
                }
                Err(e) => {
                    debug!(
                        target: "clauset::command_discovery",
                        "Failed to parse skill {}: {}",
                        skill_file.display(),
                        e
                    );
                }
            }
        }

        Ok(commands)
    }

    /// Discover plugin commands from ~/.claude/plugins/cache/*/...
    fn discover_plugin_commands(&self) -> Result<Vec<Command>> {
        let plugins_cache = self.claude_dir.join("plugins").join("cache");
        if !plugins_cache.exists() {
            return Ok(Vec::new());
        }

        let mut commands = Vec::new();

        // Iterate through marketplace directories
        let marketplaces = match fs::read_dir(&plugins_cache) {
            Ok(e) => e,
            Err(_) => return Ok(Vec::new()),
        };

        for marketplace_entry in marketplaces.flatten() {
            let marketplace_path = marketplace_entry.path();

            if !marketplace_path.is_dir() {
                continue;
            }

            // Iterate through plugin directories
            let plugins = match fs::read_dir(&marketplace_path) {
                Ok(e) => e,
                Err(_) => continue,
            };

            for plugin_entry in plugins.flatten() {
                let plugin_path = plugin_entry.path();

                if !plugin_path.is_dir() {
                    continue;
                }

                let plugin_name = plugin_entry.file_name().to_string_lossy().to_string();
                self.scan_plugin_for_commands(&plugin_path, &plugin_name, &mut commands);
            }
        }

        Ok(commands)
    }

    fn scan_plugin_for_commands(
        &self,
        plugin_path: &Path,
        plugin_name: &str,
        commands: &mut Vec<Command>,
    ) {
        // Check for direct commands directory
        let commands_dir = plugin_path.join("commands");
        if commands_dir.exists() {
            let _ = self.scan_commands_dir(
                &commands_dir,
                CommandCategory::Plugin,
                Some(plugin_name),
                commands,
            );
            return;
        }

        // Check for versioned subdirectories
        let versions = match fs::read_dir(plugin_path) {
            Ok(e) => e,
            Err(_) => return,
        };

        for version_entry in versions.flatten() {
            let version_path = version_entry.path();

            if !version_path.is_dir() {
                continue;
            }

            let commands_dir = version_path.join("commands");
            if commands_dir.exists() {
                let _ = self.scan_commands_dir(
                    &commands_dir,
                    CommandCategory::Plugin,
                    Some(plugin_name),
                    commands,
                );
            }

            // Also check for skills in plugins
            let skills_dir = version_path.join("skills");
            if skills_dir.exists() {
                self.scan_plugin_skills(&skills_dir, plugin_name, commands);
            }
        }
    }

    fn scan_plugin_skills(&self, skills_dir: &Path, plugin_name: &str, commands: &mut Vec<Command>) {
        let entries = match fs::read_dir(skills_dir) {
            Ok(e) => e,
            Err(_) => return,
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let skill_file = path.join("SKILL.md");
            if !skill_file.exists() {
                continue;
            }

            match self.parse_markdown_file(&skill_file) {
                Ok((frontmatter, first_line)) => {
                    let name = frontmatter
                        .name
                        .or_else(|| path.file_name().and_then(|n| n.to_str()).map(String::from))
                        .unwrap_or_else(|| "unknown".to_string());

                    let description = frontmatter
                        .description
                        .or(first_line)
                        .unwrap_or_else(|| "No description".to_string());

                    // Truncate long descriptions
                    let description = if description.len() > 100 {
                        format!("{}...", &description[..97])
                    } else {
                        description
                    };

                    // Prefix with plugin name
                    let qualified_name = format!("{}:{}", plugin_name, name);

                    commands.push(Command {
                        name: qualified_name.clone(),
                        display_name: format!("/{}", qualified_name),
                        description,
                        category: CommandCategory::Plugin,
                        argument_hint: frontmatter.argument_hint,
                        source: skill_file.to_string_lossy().to_string(),
                        plugin_name: Some(plugin_name.to_string()),
                    });
                }
                Err(e) => {
                    debug!(
                        target: "clauset::command_discovery",
                        "Failed to parse plugin skill {}: {}",
                        skill_file.display(),
                        e
                    );
                }
            }
        }
    }

    fn scan_commands_dir(
        &self,
        dir: &Path,
        category: CommandCategory,
        plugin_name: Option<&str>,
        commands: &mut Vec<Command>,
    ) -> Result<()> {
        if !dir.exists() {
            return Ok(());
        }

        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()),
        };

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_dir() {
                // Recurse into subdirectories (for namespaced commands)
                self.scan_commands_dir(&path, category, plugin_name, commands)?;
            } else if path.extension().map(|e| e == "md").unwrap_or(false) {
                match self.parse_markdown_file(&path) {
                    Ok((frontmatter, first_line)) => {
                        let name = frontmatter
                            .name
                            .or_else(|| {
                                path.file_stem().and_then(|n| n.to_str()).map(String::from)
                            })
                            .unwrap_or_else(|| "unknown".to_string());

                        let description = frontmatter
                            .description
                            .or(first_line)
                            .unwrap_or_else(|| "No description".to_string());

                        // Truncate long descriptions
                        let description = if description.len() > 100 {
                            format!("{}...", &description[..97])
                        } else {
                            description
                        };

                        let display_name = if let Some(plugin) = plugin_name {
                            format!("/{}:{}", plugin, name)
                        } else {
                            format!("/{}", name)
                        };

                        commands.push(Command {
                            name: name.clone(),
                            display_name,
                            description,
                            category,
                            argument_hint: frontmatter.argument_hint,
                            source: path.to_string_lossy().to_string(),
                            plugin_name: plugin_name.map(String::from),
                        });
                    }
                    Err(e) => {
                        debug!(
                            target: "clauset::command_discovery",
                            "Failed to parse command {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Parse a markdown file and extract YAML frontmatter and first content line.
    fn parse_markdown_file(&self, path: &Path) -> Result<(CommandFrontmatter, Option<String>)> {
        let content = fs::read_to_string(path)?;

        let (frontmatter, remaining) = parse_frontmatter(&content);

        // Extract first non-empty, non-heading line of content as fallback description
        let first_line = remaining
            .lines()
            .find(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty() && !trimmed.starts_with('#')
            })
            .map(|s| s.trim().to_string());

        Ok((frontmatter, first_line))
    }
}

impl Default for CommandDiscovery {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse YAML frontmatter from markdown content.
fn parse_frontmatter(content: &str) -> (CommandFrontmatter, &str) {
    let content = content.trim_start();

    if !content.starts_with("---") {
        return (CommandFrontmatter::default(), content);
    }

    // Find the closing ---
    let rest = &content[3..];
    if let Some(end_idx) = rest.find("\n---") {
        let yaml_content = &rest[..end_idx];
        let remaining = &rest[end_idx + 4..];

        match serde_yaml::from_str::<CommandFrontmatter>(yaml_content) {
            Ok(fm) => (fm, remaining),
            Err(e) => {
                debug!(
                    target: "clauset::command_discovery",
                    "Failed to parse YAML frontmatter: {}",
                    e
                );
                (CommandFrontmatter::default(), content)
            }
        }
    } else {
        (CommandFrontmatter::default(), content)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_with_yaml() {
        let content = r#"---
name: test-command
description: A test command
argument-hint: "[file]"
---

# Test Command

This is the body.
"#;
        let (fm, remaining) = parse_frontmatter(content);
        assert_eq!(fm.name, Some("test-command".to_string()));
        assert_eq!(fm.description, Some("A test command".to_string()));
        assert_eq!(fm.argument_hint, Some("[file]".to_string()));
        assert!(remaining.contains("# Test Command"));
    }

    #[test]
    fn test_parse_frontmatter_without_yaml() {
        let content = "# Just a heading\n\nSome content.";
        let (fm, remaining) = parse_frontmatter(content);
        assert_eq!(fm.name, None);
        assert_eq!(remaining, content);
    }

    #[test]
    fn test_built_in_commands() {
        let discovery = CommandDiscovery::new();
        let builtins = discovery.built_in_commands();

        assert!(!builtins.is_empty());
        assert!(builtins.iter().any(|c| c.name == "help"));
        assert!(builtins.iter().any(|c| c.name == "compact"));
        assert!(builtins
            .iter()
            .all(|c| c.category == CommandCategory::BuiltIn));
    }
}
