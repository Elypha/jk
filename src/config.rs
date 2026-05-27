use crate::error::{JkError, JkResult};
use crate::render::{aggregate_shape, analyze};
use crate::shell::Shell;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Config {
    pub tree: CommandTree,
}

#[derive(Debug)]
pub struct CommandTree {
    pub(crate) root: BTreeMap<String, CommandNode>,
}

#[derive(Debug, Clone)]
pub enum CommandNode {
    Leaf(LeafCommand),
    Namespace(BTreeMap<String, CommandNode>),
}

/// Origin of a leaf in the merged command tree, used by listing to render
/// `(g)` / `(o)` / no marker. `parse_str` defaults to `LocalOnly`; callers
/// apply `tag_all_origins` after loading the global config, and `merge`
/// sets `Override` on colliding leaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    LocalOnly,
    GlobalOnly,
    Override,
}

#[derive(Debug, Clone)]
pub struct LeafCommand {
    pub desc: Option<String>,
    pub shell: Shell,
    pub cmd: CmdBody,
    pub origin: Origin,
}

#[derive(Debug, Clone)]
pub enum CmdBody {
    Single(String),
    Sequence(Vec<String>),
}

impl LeafCommand {
    pub fn cmd_strings(&self) -> Vec<String> {
        match &self.cmd {
            CmdBody::Single(s) => vec![s.clone()],
            CmdBody::Sequence(v) => v.clone(),
        }
    }
}

impl CommandTree {
    pub fn lookup(&self, path: &[String]) -> Option<&CommandNode> {
        if path.is_empty() {
            return None;
        }
        let mut node = self.root.get(&path[0])?;
        for seg in &path[1..] {
            match node {
                CommandNode::Namespace(m) => node = m.get(seg)?,
                CommandNode::Leaf(_) => return None,
            }
        }
        Some(node)
    }

    pub fn root(&self) -> &BTreeMap<String, CommandNode> {
        &self.root
    }
}

#[derive(Deserialize)]
struct RawConfig {
    shell: Option<String>,
    #[serde(flatten)]
    rest: BTreeMap<String, toml::Value>,
}

pub fn parse_str(s: &str) -> JkResult<Config> {
    let raw: RawConfig = toml::from_str(s).map_err(|e| JkError::ConfigParse(e.to_string()))?;

    let file_shell: Option<Shell> = raw.shell.as_deref().map(Shell::parse).transpose()?;

    let mut tree = BTreeMap::new();
    for (key, value) in raw.rest {
        let node = build_node(&key, value, file_shell)?;
        tree.insert(key, node);
    }

    Ok(Config {
        tree: CommandTree { root: tree },
    })
}

/// Locate the local config file.
///
/// - `Ok(Some(path))` — found via explicit `++config`, `JK_CONFIG`, or cwd walk-up.
/// - `Ok(None)` — walk-up exhausted with no `.jk` found; caller decides whether to
///   fall back to global-only mode or return `ConfigNotFound`.
/// - `Err(ConfigPathInvalid)` — explicit path (`++config` or non-empty `JK_CONFIG`)
///   points to a file that does not exist; explicit paths never fall back to global-only.
pub fn discover(start: &Path, explicit: Option<String>) -> JkResult<Option<PathBuf>> {
    if let Some(p) = explicit {
        let path = PathBuf::from(&p);
        if path.is_file() {
            return Ok(Some(path));
        }
        return Err(JkError::ConfigPathInvalid(p));
    }

    if let Ok(env_path) = std::env::var("JK_CONFIG") {
        if !env_path.is_empty() {
            let path = PathBuf::from(&env_path);
            if path.is_file() {
                return Ok(Some(path));
            }
            return Err(JkError::ConfigPathInvalid(env_path));
        }
        // empty string: treat as unset, fall through to cwd walk
    }

    let mut cur = start.to_path_buf();
    loop {
        let candidate = cur.join(".jk");
        if candidate.is_file() {
            return Ok(Some(candidate));
        }
        if !cur.pop() {
            break;
        }
    }
    Ok(None)
}

/// Expected path for the global config (`<home>/.jk/config.toml`), without
/// checking whether the file exists.
///
/// Home is read from `HOME` on Unix and `USERPROFILE` on Windows.
/// Returns `None` if the environment variable is absent (treated as no global config).
pub fn global_config_path() -> Option<PathBuf> {
    let home = home_dir()?;
    Some(home.join(".jk").join("config.toml"))
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(unix)]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
}

pub fn tag_all_origins(cfg: &mut Config, origin: Origin) {
    fn walk(map: &mut BTreeMap<String, CommandNode>, origin: Origin) {
        for node in map.values_mut() {
            match node {
                CommandNode::Leaf(l) => l.origin = origin,
                CommandNode::Namespace(m) => walk(m, origin),
            }
        }
    }
    walk(&mut cfg.tree.root, origin);
}

pub fn load_from_path(p: &Path) -> JkResult<Config> {
    let s = std::fs::read_to_string(p)?;
    parse_str(&s)
}

pub fn merge(global: Config, local: Config) -> JkResult<Config> {
    let mut tree_map = global.tree.root.clone();
    merge_map(&mut tree_map, local.tree.root, "")?;
    Ok(Config {
        tree: CommandTree { root: tree_map },
    })
}

fn merge_map(
    target: &mut BTreeMap<String, CommandNode>,
    source: BTreeMap<String, CommandNode>,
    path_prefix: &str,
) -> JkResult<()> {
    for (key, src_node) in source {
        let full_path = if path_prefix.is_empty() {
            key.clone()
        } else {
            format!("{}.{}", path_prefix, key)
        };
        match (target.get_mut(&key), src_node) {
            (None, src) => {
                target.insert(key, src);
            }
            (Some(CommandNode::Leaf(_)), CommandNode::Leaf(mut src_leaf)) => {
                src_leaf.origin = Origin::Override;
                target.insert(key, CommandNode::Leaf(src_leaf));
            }
            (Some(CommandNode::Namespace(t_map)), CommandNode::Namespace(s_map)) => {
                merge_map(t_map, s_map, &full_path)?;
            }
            (Some(_), _) => {
                return Err(JkError::ConfigSchema(format!(
                    "merge conflict at '{}': namespace and leaf cannot share path",
                    full_path
                )));
            }
        }
    }
    Ok(())
}

fn build_node(path_for_err: &str, value: toml::Value, file_shell: Option<Shell>) -> JkResult<CommandNode> {
    let table = match value {
        toml::Value::Table(t) => t,
        other => {
            return Err(JkError::ConfigSchema(format!(
                "expected table at '{}', got {}",
                path_for_err,
                other.type_str()
            )));
        }
    };

    let has_cmd = table.contains_key("cmd");
    let has_children = table.iter().any(|(k, v)| {
        !matches!(k.as_str(), "cmd" | "desc" | "shell") && matches!(v, toml::Value::Table(_))
    });

    if has_cmd && has_children {
        return Err(JkError::ConfigSchema(format!(
            "'{}' has both `cmd` and child tables; a node must be leaf XOR namespace",
            path_for_err
        )));
    }

    if has_cmd {
        // leaf
        let desc = match table.get("desc") {
            Some(toml::Value::String(s)) => Some(s.clone()),
            None => None,
            _ => return Err(JkError::ConfigSchema(format!("'{}'.desc must be string", path_for_err))),
        };
        let shell: Option<Shell> = match table.get("shell") {
            Some(toml::Value::String(s)) => Some(
                Shell::parse(s).map_err(|e| match e {
                    JkError::ConfigSchema(msg) => JkError::ConfigSchema(format!("'{}'.shell: {}", path_for_err, msg)),
                    other => other,
                })?,
            ),
            None => None,
            _ => return Err(JkError::ConfigSchema(format!("'{}'.shell must be string", path_for_err))),
        };
        let cmd = match table.get("cmd").unwrap() {
            toml::Value::String(s) => CmdBody::Single(s.clone()),
            toml::Value::Array(arr) => {
                let mut v = Vec::new();
                for item in arr {
                    let toml::Value::String(s) = item else {
                        return Err(JkError::ConfigSchema(format!(
                            "'{}'.cmd array items must be strings",
                            path_for_err
                        )));
                    };
                    v.push(s.clone());
                }
                CmdBody::Sequence(v)
            }
            _ => {
                return Err(JkError::ConfigSchema(format!(
                    "'{}'.cmd must be string or array of strings",
                    path_for_err
                )));
            }
        };

        match &cmd {
            CmdBody::Single(s) if s.trim().is_empty() => {
                return Err(JkError::ConfigSchema(format!(
                    "'{}'.cmd must not be empty", path_for_err
                )));
            }
            CmdBody::Sequence(v) if v.is_empty() => {
                return Err(JkError::ConfigSchema(format!(
                    "'{}'.cmd array must not be empty", path_for_err
                )));
            }
            CmdBody::Sequence(v) => {
                for (i, s) in v.iter().enumerate() {
                    if s.trim().is_empty() {
                        return Err(JkError::ConfigSchema(format!(
                            "'{}'.cmd[{}] must not be empty", path_for_err, i
                        )));
                    }
                }
            }
            _ => {}
        }

        for (k, _) in table.iter() {
            if !matches!(k.as_str(), "cmd" | "desc" | "shell") {
                return Err(JkError::ConfigSchema(format!(
                    "'{}'.{} is not a known leaf field",
                    path_for_err, k
                )));
            }
        }

        let cmd_strings: Vec<&str> = match &cmd {
            CmdBody::Single(s) => vec![s.as_str()],
            CmdBody::Sequence(v) => v.iter().map(|s| s.as_str()).collect(),
        };
        for (idx, tmpl) in cmd_strings.iter().enumerate() {
            if let Err(e) = analyze(tmpl) {
                let where_in = if cmd_strings.len() > 1 {
                    format!("'{}'.cmd[{}]", path_for_err, idx)
                } else {
                    format!("'{}'.cmd", path_for_err)
                };
                let JkError::ConfigSchema(msg) = e else {
                    return Err(e);
                };
                return Err(JkError::ConfigSchema(format!("{}: {}", where_in, msg)));
            }
        }
        if let Err(e) = aggregate_shape(&cmd_strings) {
            let JkError::ConfigSchema(msg) = e else {
                return Err(e);
            };
            return Err(JkError::ConfigSchema(format!("'{}'.cmd: {}", path_for_err, msg)));
        }

        let Some(baked_shell) = shell.or(file_shell) else {
            return Err(JkError::ConfigSchema(format!(
                "'{}': no shell declared (file-level `shell` missing and leaf has no `shell` field of its own)",
                path_for_err
            )));
        };
        Ok(CommandNode::Leaf(LeafCommand { desc, shell: baked_shell, cmd, origin: Origin::LocalOnly }))
    } else if has_children {
        let mut children = BTreeMap::new();
        for (k, v) in table {
            if matches!(k.as_str(), "cmd" | "desc" | "shell") {
                return Err(JkError::ConfigSchema(format!(
                    "'{}' is a namespace; field '{}' is not allowed here",
                    path_for_err, k
                )));
            }
            let toml::Value::Table(_) = &v else {
                return Err(JkError::ConfigSchema(format!(
                    "'{}'.{} is not a known field; namespaces only contain child tables",
                    path_for_err, k
                )));
            };
            let sub_path = format!("{}.{}", path_for_err, k);
            children.insert(k, build_node(&sub_path, v, file_shell)?);
        }
        Ok(CommandNode::Namespace(children))
    } else {
        let unknown: Vec<String> = table.iter()
            .filter(|(k, _)| !matches!(k.as_str(), "cmd" | "desc" | "shell"))
            .map(|(k, _)| k.clone())
            .collect();
        if !unknown.is_empty() {
            Err(JkError::ConfigSchema(format!(
                "'{}' has neither `cmd` nor child tables; got unknown field(s): {}",
                path_for_err, unknown.join(", ")
            )))
        } else {
            Err(JkError::ConfigSchema(format!(
                "'{}' has neither `cmd` nor child tables",
                path_for_err
            )))
        }
    }
}
