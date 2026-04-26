use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandAction {
    Move(String),
    DeleteOrphaned,
    Quit,
    ImportDesktop,
    Import(Option<String>),
    ImportJex(String),
    ExportJex(String),
    Read(String),
    TagAdd(String),
    TagRemove(String),
    TagList,
    MkNote(String),
    MkTodo(String),
    MkBook(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandDescriptor {
    pub name: &'static str,
    pub usage: &'static str,
    pub description: &'static str,
    pub hidden_from_completion: bool,
}

pub const COMMANDS: &[CommandDescriptor] = &[
    CommandDescriptor {
        name: "move",
        usage: ":move <notebook>",
        description: "Move the selected note or notebook to a notebook",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "delete-orphaned",
        usage: ":delete-orphaned",
        description: "Delete notes that are no longer connected to a notebook",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "quit",
        usage: ":quit",
        description: "Quit NeoJoplin",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "q",
        usage: ":q",
        description: "Alias for :quit",
        hidden_from_completion: true,
    },
    CommandDescriptor {
        name: "import-desktop",
        usage: ":import-desktop",
        description: "Import notes, notebooks, and tags from Joplin Desktop",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "import",
        usage: ":import [database.sqlite]",
        description: "Import from the Joplin CLI database or an explicit SQLite file",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "import-jex",
        usage: ":import-jex <file.jex>",
        description: "Import notes, notebooks, and tags from a JEX archive",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "export-jex",
        usage: ":export-jex <file.jex>",
        description: "Export notes, notebooks, and tags to a JEX archive",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "read",
        usage: ":read <file>",
        description: "Create a note from a file",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "tag",
        usage: ":tag add <tag> | :tag remove <tag> | :tag list",
        description: "Add, remove, or list tags for the selected note",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "mknote",
        usage: ":mknote <title>",
        description: "Create a new note with the given title",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "mktodo",
        usage: ":mktodo <title>",
        description: "Create a new to-do with the given title",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "mkbook",
        usage: ":mkbook <title>",
        description: "Create a new notebook with the given title",
        hidden_from_completion: false,
    },
    CommandDescriptor {
        name: "mv",
        usage: ":mv <notebook>",
        description: "Alias for :move",
        hidden_from_completion: false,
    },
];

#[derive(Debug, Clone, Default)]
pub struct CompletionState {
    pub items: Vec<String>,
    pub index: usize,
}

impl CompletionState {
    pub fn current(&self) -> Option<&str> {
        self.items.get(self.index).map(String::as_str)
    }

    pub fn advance(&mut self, backwards: bool) {
        if self.items.is_empty() {
            return;
        }

        if backwards {
            self.index = if self.index == 0 {
                self.items.len() - 1
            } else {
                self.index - 1
            };
        } else {
            self.index = (self.index + 1) % self.items.len();
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CommandPromptState {
    pub visible: bool,
    pub input: String,
    pub completion: Option<CompletionState>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommandPreview {
    pub left: &'static str,
    pub right: &'static str,
}

const TAG_COMMAND_PREVIEWS: &[CommandPreview] = &[
    CommandPreview {
        left: ":tag add <tag>",
        right: "Attach a tag to the selected note, creating it if needed",
    },
    CommandPreview {
        left: ":tag remove <tag>",
        right: "Detach a tag from the selected note",
    },
    CommandPreview {
        left: ":tag list",
        right: "List the selected note's tags in the status bar",
    },
];

impl CommandPromptState {
    pub fn open(&mut self, initial_input: impl Into<String>) {
        self.visible = true;
        self.input = initial_input.into();
        self.completion = None;
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.input.clear();
        self.completion = None;
    }

    pub fn set_input(&mut self, input: String) {
        self.input = input;
        self.completion = None;
    }

    pub fn push_char(&mut self, c: char) {
        self.input.push(c);
        self.completion = None;
    }

    pub fn pop_char(&mut self) {
        self.input.pop();
        self.completion = None;
    }
}

pub fn parse_command(input: &str) -> Result<CommandAction, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("Enter a command after :".to_string());
    }

    let (name, arg) = split_command(trimmed);
    match name {
        "move" | "mv" => required_arg(arg, "move <notebook>").map(CommandAction::Move),
        "delete-orphaned" => no_arg(arg, "delete-orphaned").map(|_| CommandAction::DeleteOrphaned),
        "quit" | "q" => no_arg(arg, name).map(|_| CommandAction::Quit),
        "import-desktop" => no_arg(arg, "import-desktop").map(|_| CommandAction::ImportDesktop),
        "import" => Ok(CommandAction::Import(optional_arg(arg))),
        "import-jex" => required_arg(arg, "import-jex <file.jex>").map(CommandAction::ImportJex),
        "export-jex" => required_arg(arg, "export-jex <file.jex>").map(CommandAction::ExportJex),
        "read" => required_arg(arg, "read <file>").map(CommandAction::Read),
        "tag" => parse_tag_command(arg),
        "mknote" => required_arg(arg, "mknote <title>").map(CommandAction::MkNote),
        "mktodo" => required_arg(arg, "mktodo <title>").map(CommandAction::MkTodo),
        "mkbook" => required_arg(arg, "mkbook <title>").map(CommandAction::MkBook),
        _ => Err(format!("Unknown command: {}", name)),
    }
}

pub fn complete_path_input(command_name: &str, raw_arg: &str) -> Vec<String> {
    let trimmed_arg = raw_arg.trim_start();
    let expanded = expand_path(trimmed_arg);
    let (directory, name_prefix, display_prefix) = split_path_completion(trimmed_arg, &expanded);

    let Ok(entries) = std::fs::read_dir(&directory) else {
        return Vec::new();
    };

    let mut suggestions: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_name = entry.file_name();
            let file_name = file_name.to_string_lossy().to_string();
            if !starts_with_ignore_case(&file_name, &name_prefix) {
                return None;
            }

            let metadata = entry.metadata().ok()?;
            let mut completion = format!("{}{}", display_prefix, file_name);
            if metadata.is_dir() {
                completion.push('/');
            }

            Some(format!("{} {}", command_name, completion))
        })
        .collect();

    suggestions.sort_by_key(|item| item.to_lowercase());
    suggestions
}

pub fn command_previews(input: &str) -> Vec<CommandPreview> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return COMMANDS
            .iter()
            .filter(|command| !command.hidden_from_completion)
            .map(|command| CommandPreview {
                left: command.usage,
                right: command.description,
            })
            .collect();
    }

    let (name, arg, has_argument_context) = split_command_input(trimmed);
    if !has_argument_context {
        return COMMANDS
            .iter()
            .filter(|command| {
                !command.hidden_from_completion && starts_with_ignore_case(command.name, name)
            })
            .map(|command| CommandPreview {
                left: command.usage,
                right: command.description,
            })
            .collect();
    }

    if name == "tag" {
        let (subcommand, subarg) = split_command(arg.trim_start());
        if subarg.is_none() {
            return TAG_COMMAND_PREVIEWS
                .iter()
                .copied()
                .filter(|preview| {
                    let sub = preview
                        .left
                        .strip_prefix(":tag ")
                        .unwrap_or(preview.left)
                        .split_whitespace()
                        .next()
                        .unwrap_or_default();
                    starts_with_ignore_case(sub, subcommand)
                })
                .collect();
        }
    }

    COMMANDS
        .iter()
        .find(|command| command.name == name)
        .map(|command| {
            vec![CommandPreview {
                left: command.usage,
                right: command.description,
            }]
        })
        .unwrap_or_default()
}

fn split_command_input(input: &str) -> (&str, &str, bool) {
    if let Some(index) = input.find(char::is_whitespace) {
        let command = &input[..index];
        let argument = &input[index + 1..];
        (command, argument, true)
    } else {
        (input, "", false)
    }
}

fn split_command(input: &str) -> (&str, Option<&str>) {
    if let Some(index) = input.find(char::is_whitespace) {
        let name = &input[..index];
        let rest = &input[index + 1..];
        (name, Some(rest))
    } else {
        (input, None)
    }
}

fn parse_tag_command(arg: Option<&str>) -> Result<CommandAction, String> {
    let raw = arg.unwrap_or("").trim();
    if raw.is_empty() {
        return Err("Usage: :tag add <tag> | :tag remove <tag> | :tag list".to_string());
    }

    let (subcommand, subarg) = split_command(raw);
    match subcommand {
        "add" => required_arg(subarg, "tag add <tag>").map(CommandAction::TagAdd),
        "remove" => required_arg(subarg, "tag remove <tag>").map(CommandAction::TagRemove),
        "list" => no_arg(subarg, "tag list").map(|_| CommandAction::TagList),
        _ => Err(format!(
            "Usage: :tag add <tag> | :tag remove <tag> | :tag list"
        )),
    }
}

fn optional_arg(arg: Option<&str>) -> Option<String> {
    arg.map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn required_arg(arg: Option<&str>, usage: &str) -> Result<String, String> {
    optional_arg(arg).ok_or_else(|| format!("Usage: :{}", usage))
}

fn no_arg(arg: Option<&str>, usage: &str) -> Result<(), String> {
    if optional_arg(arg).is_some() {
        Err(format!("Usage: :{}", usage))
    } else {
        Ok(())
    }
}

fn starts_with_ignore_case(text: &str, prefix: &str) -> bool {
    text.to_lowercase().starts_with(&prefix.to_lowercase())
}

fn expand_path(raw: &str) -> PathBuf {
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(rest);
        }
    } else if raw == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }

    PathBuf::from(raw)
}

fn split_path_completion(raw: &str, expanded: &Path) -> (PathBuf, String, String) {
    if raw.is_empty() {
        return (
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            String::new(),
            String::new(),
        );
    }

    let raw_path = Path::new(raw);
    if raw.ends_with('/') || expanded.is_dir() {
        return (
            expanded.to_path_buf(),
            String::new(),
            ensure_trailing_slash(raw.to_string()),
        );
    }

    let parent = expanded
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));
    let prefix = expanded
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default();

    let display_prefix = raw_path
        .parent()
        .map(|path| {
            let prefix = path.to_string_lossy().to_string();
            if prefix.is_empty() {
                String::new()
            } else {
                ensure_trailing_slash(prefix)
            }
        })
        .unwrap_or_default();

    (parent, prefix, display_prefix)
}

fn ensure_trailing_slash(mut value: String) -> String {
    if !value.is_empty() && !value.ends_with('/') {
        value.push('/');
    }
    value
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_move_keeps_notebook_name_spaces() {
        assert_eq!(
            parse_command("move Personal Projects").unwrap(),
            CommandAction::Move("Personal Projects".to_string())
        );
    }

    #[test]
    fn parse_import_accepts_optional_path() {
        assert_eq!(
            parse_command("import").unwrap(),
            CommandAction::Import(None)
        );
        assert_eq!(
            parse_command("import /tmp/joplin.sqlite").unwrap(),
            CommandAction::Import(Some("/tmp/joplin.sqlite".to_string()))
        );
    }

    #[test]
    fn parse_unknown_command_fails() {
        assert!(parse_command("wat").is_err());
    }

    #[test]
    fn parse_tag_subcommands() {
        assert_eq!(
            parse_command("tag add urgent").unwrap(),
            CommandAction::TagAdd("urgent".to_string())
        );
        assert_eq!(
            parse_command("tag remove urgent").unwrap(),
            CommandAction::TagRemove("urgent".to_string())
        );
        assert_eq!(parse_command("tag list").unwrap(), CommandAction::TagList);
    }
}
