use crate::cli::ParsedCli;
use crate::config::{self, Config, Origin};
use crate::error::{JkError, JkResult};
use crate::execute::run_sequence;
use crate::output::Out;
use crate::render::{aggregate_shape, fold, substitute, validate_args};
use crate::resolver::{resolve, ResolveResult};
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run(cli: ParsedCli, out: &Out) -> JkResult<i32> {
    if cli.version {
        println!("jk {}", VERSION);
        return Ok(0);
    }

    let cwd = std::env::current_dir()?;

    let local_path: Option<PathBuf> = config::discover(&cwd, cli.config_path.clone())?;
    let local: Option<Config> = match &local_path {
        Some(p) => Some(config::load_from_path(p)?),
        None => None,
    };

    let global_path: Option<PathBuf> = config::global_config_path();
    let global: Option<Config> = match &global_path {
        Some(p) if p.is_file() => Some(load_global_from(p)?),
        _ => None,
    };

    let cfg = match (local, global) {
        (None, None) => {
            return Err(JkError::ConfigNotFound {
                cwd: cwd.display().to_string(),
                global_path: global_path.as_ref().map(|p| p.display().to_string()),
            });
        }
        (Some(l), None) => l,
        (None, Some(mut g)) => {
            config::tag_all_origins(&mut g, Origin::GlobalOnly);
            g
        }
        (Some(l), Some(mut g)) => {
            config::tag_all_origins(&mut g, Origin::GlobalOnly);
            config::merge(g, l)?
        }
    };

    let header_global: Option<&std::path::Path> = global_path.as_deref()
        .filter(|p| p.is_file());
    let header_local: Option<&std::path::Path> = local_path.as_deref();

    match resolve(&cfg, &cli.path)? {
        ResolveResult::Namespace { path, children } => {
            out.print_listing(&path, children, header_global, header_local);
            Ok(0)
        }
        ResolveResult::Leaf(resolved) => {
            let shell = resolved.leaf.shell;

            let raw_cmds = resolved.leaf.cmd_strings();

            let template_refs: Vec<&str> = raw_cmds.iter().map(|s| s.as_str()).collect();
            let leaf_shape = aggregate_shape(&template_refs)?;
            validate_args(&resolved.args, leaf_shape)?;

            let mut rendered: Vec<String> = Vec::with_capacity(raw_cmds.len());
            for c in &raw_cmds {
                let folded = fold(c);
                let s = substitute(&folded, &resolved.args, shell)?;
                rendered.push(s);
            }

            if cli.dry_run {
                for r in &rendered {
                    println!("{}", r);
                }
                return Ok(0);
            }

            let name = resolved.path.join(" ");
            run_sequence(&rendered, shell, out, &name)
        }
    }
}

fn load_global_from(p: &std::path::Path) -> JkResult<Config> {
    config::load_from_path(p).map_err(|e| match e {
        JkError::ConfigParse(m) => JkError::ConfigParse(format!("in global config {}: {}", p.display(), m)),
        JkError::ConfigSchema(m) => JkError::ConfigSchema(format!("in global config {}: {}", p.display(), m)),
        other => other,
    })
}
