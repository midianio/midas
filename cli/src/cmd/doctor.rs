//! `midas doctor` — diagnose the local dev environment. Ported from midflow's doctor.

use crate::proc::{capture, on_path, try_capture};
use midian_cli::exit::{CliError, CliResult};
use midian_cli::Ctx;
use serde::Serialize;
use serde_json::json;

#[derive(Serialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Status {
    Ok,
    Warn,
    Fail,
}

#[derive(Serialize)]
struct Check {
    label: String,
    status: Status,
    #[serde(skip_serializing_if = "String::is_empty")]
    detail: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    hint: String,
}

fn ok(label: &str, detail: impl Into<String>) -> Check {
    Check {
        label: label.into(),
        status: Status::Ok,
        detail: detail.into(),
        hint: String::new(),
    }
}
fn warn(label: &str, hint: impl Into<String>) -> Check {
    Check {
        label: label.into(),
        status: Status::Warn,
        detail: String::new(),
        hint: hint.into(),
    }
}
fn fail(label: &str, hint: impl Into<String>) -> Check {
    Check {
        label: label.into(),
        status: Status::Fail,
        detail: String::new(),
        hint: hint.into(),
    }
}

fn check_bin(name: &str, hint: &str) -> Check {
    match on_path(name) {
        Some(p) => ok(&format!("{name} on PATH"), p.display().to_string()),
        None => fail(&format!("{name} on PATH"), hint),
    }
}

fn check_gh_auth() -> Check {
    if on_path("gh").is_none() {
        return fail("gh authenticated", "install gh first");
    }
    match capture("gh", &["api", "user", "--jq", ".login"]) {
        Ok(login) => ok("gh authenticated", format!("as {login}")),
        Err(_) => fail("gh authenticated", "run: gh auth login"),
    }
}

fn check_git_identity() -> Check {
    let name = capture("git", &["config", "--get", "user.name"]).unwrap_or_default();
    let email = capture("git", &["config", "--get", "user.email"]).unwrap_or_default();
    if name.is_empty() || email.is_empty() {
        fail(
            "git user.name / user.email set",
            "git config --global user.name \"Your Name\" && git config --global user.email you@example.com",
        )
    } else {
        ok(
            "git user.name / user.email set",
            format!("{name} <{email}>"),
        )
    }
}

fn check_inside_repo() -> Check {
    match capture("git", &["rev-parse", "--git-dir"]) {
        Ok(_) => ok("inside a git repo", ""),
        Err(_) => fail("inside a git repo", "run from a git repo"),
    }
}

fn check_pscale() -> Check {
    match on_path("pscale") {
        None => warn(
            "pscale on PATH (only needed for `midas flow`)",
            "install with: brew install planetscale/tap/pscale",
        ),
        Some(p) => {
            if try_capture("pscale", &["auth", "check"]).1 {
                ok("pscale authenticated", p.display().to_string())
            } else {
                warn("pscale authenticated", "run: pscale auth login")
            }
        }
    }
}

/// `flow_checks` adds the pscale tunnel checks (relevant only to `midas flow`).
pub fn run(ctx: &Ctx, flow_checks: bool) -> CliResult {
    ctx.out.banner("midas doctor");

    let mut checks = vec![
        check_bin("git", "install with: brew install git"),
        check_bin("gh", "install with: brew install gh && gh auth login"),
        check_gh_auth(),
        check_git_identity(),
        check_inside_repo(),
    ];
    if flow_checks {
        checks.push(check_pscale());
    }

    let failed = checks.iter().filter(|c| c.status == Status::Fail).count();

    ctx.out
        .data(&json!({ "checks": checks, "failed": failed }), |s| {
            let mut out = String::new();
            for c in &checks {
                let marker = match c.status {
                    Status::Ok => s.green("✓"),
                    Status::Warn => s.yellow("⚠"),
                    Status::Fail => s.red("✗"),
                };
                out.push_str(&format!("  {marker} {}\n", c.label));
                if !c.detail.is_empty() {
                    out.push_str(&format!("      {}\n", s.dim(&c.detail)));
                }
                if c.status != Status::Ok && !c.hint.is_empty() {
                    out.push_str(&format!("      {}\n", s.dim(&format!("→ {}", c.hint))));
                }
            }
            out.push_str(&if failed > 0 {
                s.red(&format!("{failed} check(s) failed"))
            } else {
                s.green("all checks passed")
            });
            out
        });

    if failed > 0 {
        Err(CliError::expected(format!("{failed} check(s) failed")))
    } else {
        Ok(())
    }
}
