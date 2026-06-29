//! Managed `.env.local` block injection. Replaces a marker-delimited region without touching
//! anything the user wrote outside it. Markers default to midflow's (`# >>> midflow >>>`).

use super::config::FlowConfig;
use super::git;
use anyhow::Result;
use std::path::PathBuf;

fn markers(cfg: &FlowConfig) -> (String, String) {
    (
        format!("# >>> {} >>>", cfg.env_marker),
        format!("# <<< {} <<<", cfg.env_marker),
    )
}

fn api_env_path(cfg: &FlowConfig) -> Result<PathBuf> {
    Ok(git::repo_root()?.join(&cfg.api_env_local))
}

fn block(cfg: &FlowConfig) -> String {
    let (begin, end) = markers(cfg);
    format!(
        "{begin}\nMYSQL_DATABASE_URL={}\n{end}\n",
        cfg.local_db_url()
    )
}

/// Find the byte range of the managed block (markers inclusive, plus a trailing newline) in `text`.
fn block_span(cfg: &FlowConfig, text: &str) -> Option<(usize, usize)> {
    let (begin, end) = markers(cfg);
    let start = text.find(&begin)?;
    let end_marker = text[start..].find(&end)? + start;
    let mut stop = end_marker + end.len();
    if text[stop..].starts_with('\n') {
        stop += 1;
    }
    Some((start, stop))
}

/// Replace (or append) the managed block with a `MYSQL_DATABASE_URL` pointing at the local tunnel.
pub fn write_api_env_local(cfg: &FlowConfig) -> Result<()> {
    let p = api_env_path(cfg)?;
    let new_block = block(cfg);

    let existing = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(dir) = p.parent() {
                std::fs::create_dir_all(dir)?;
            }
            std::fs::write(&p, new_block)?;
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    let next = if let Some((s, e)) = block_span(cfg, &existing) {
        format!("{}{}{}", &existing[..s], new_block, &existing[e..])
    } else {
        let trimmed = existing.trim_end_matches([' ', '\t', '\r', '\n']);
        format!("{trimmed}\n{new_block}")
    };
    std::fs::write(&p, next)?;
    Ok(())
}

/// Remove the managed block, leaving everything else; collapse 3+ blank lines to 2.
pub fn clear_api_env_local(cfg: &FlowConfig) -> Result<()> {
    let p = api_env_path(cfg)?;
    let existing = match std::fs::read_to_string(&p) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    let stripped = match block_span(cfg, &existing) {
        Some((s, e)) => format!("{}{}", &existing[..s], &existing[e..]),
        None => existing,
    };
    let collapsed = collapse_blank_lines(&stripped);
    std::fs::write(&p, collapsed)?;
    Ok(())
}

fn collapse_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut newline_run = 0;
    for c in s.chars() {
        if c == '\n' {
            newline_run += 1;
            if newline_run <= 2 {
                out.push(c);
            }
        } else {
            newline_run = 0;
            out.push(c);
        }
    }
    out
}
