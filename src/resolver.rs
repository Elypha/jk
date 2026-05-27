use crate::config::{CommandNode, Config};
use crate::error::{JkError, JkResult};
use std::collections::BTreeMap;

pub struct Resolved<'a> {
    pub path: Vec<String>,
    pub args: Vec<String>,
    pub leaf: &'a crate::config::LeafCommand,
}

pub enum ResolveResult<'a> {
    Leaf(Resolved<'a>),
    Namespace {
        path: Vec<String>,
        children: &'a BTreeMap<String, CommandNode>,
    },
}

pub fn resolve<'a>(cfg: &'a Config, tokens: &[String]) -> JkResult<ResolveResult<'a>> {
    if tokens.is_empty() {
        return Ok(ResolveResult::Namespace {
            path: vec![],
            children: cfg.tree.root(),
        });
    }

    let mut current_map = cfg.tree.root();
    let mut consumed: Vec<String> = Vec::new();
    let mut iter = tokens.iter().enumerate();

    while let Some((idx, tok)) = iter.next() {
        let Some(node) = current_map.get(tok) else {
            return Err(JkError::UnknownCommand(tokens[..=idx].join(" ")));
        };
        consumed.push(tok.clone());
        match node {
            CommandNode::Leaf(leaf) => {
                let args = tokens[idx + 1..].to_vec();
                return Ok(ResolveResult::Leaf(Resolved {
                    path: consumed,
                    args,
                    leaf,
                }));
            }
            CommandNode::Namespace(map) => {
                current_map = map;
            }
        }
    }

    Ok(ResolveResult::Namespace {
        path: consumed,
        children: current_map,
    })
}
