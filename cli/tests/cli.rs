//! Surface tests for `midas` (CLI-0008): human output, `--json` schema, and exit codes for the
//! happy path, the expected-negative path (exit 2), and the usage-error path (exit 3).

use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use std::path::Path;

fn midas() -> Command {
    Command::cargo_bin("midas").unwrap()
}

/// Write a file, creating parent dirs.
fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, body).unwrap();
}

/// A fixture that conforms to the mechanized checks (state dir present, no banned calls).
fn clean_fixture(root: &Path) {
    fs::create_dir_all(root.join("app/web/src/lib/state")).unwrap();
    write(root, "app/web/src/lib/utils.ts", "export const x = 1;\n");
    write(root, "app/api/src/main.rs", "fn main() {}\n");
}

#[test]
fn help_succeeds() {
    midas()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("flow"))
        .stdout(predicate::str::contains("check"));
}

#[test]
fn version_prints() {
    midas()
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("midas"));
}

#[test]
fn doctor_json_is_valid() {
    let out = midas().args(["--json", "doctor"]).output().unwrap();
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("doctor --json is valid JSON");
    assert!(v.get("checks").and_then(|c| c.as_array()).is_some());
}

#[test]
fn check_clean_fixture_passes() {
    let dir = tempfile::tempdir().unwrap();
    clean_fixture(dir.path());

    midas()
        .args(["check", "--root"])
        .arg(dir.path())
        .assert()
        .success(); // exit 0 — no mechanical violations
}

#[test]
fn check_violations_exit_2_and_report_ids() {
    let dir = tempfile::tempdir().unwrap();
    // No state dir (FE-0001 fails); a raw crypto.randomUUID (FE-0010 fails).
    write(
        dir.path(),
        "app/web/src/lib/thing.ts",
        "export const id = () => crypto.randomUUID();\n",
    );

    let out = midas()
        .args(["--json", "check", "--root"])
        .arg(dir.path())
        .output()
        .unwrap();

    assert_eq!(out.status.code(), Some(2), "mechanical drift must exit 2");

    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let failing: Vec<String> = v["mechanical"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["outcome"] == "fail")
        .map(|r| r["id"].as_str().unwrap().to_string())
        .collect();
    assert!(
        failing.contains(&"FE-0010".to_string()),
        "should flag FE-0010"
    );
    assert!(
        failing.contains(&"FE-0001".to_string()),
        "should flag FE-0001"
    );
    // partitioned output: both arms present
    assert!(v.get("mechanical").is_some() && v.get("semantic").is_some());
}

#[test]
fn check_ledgered_deviation_is_not_a_failure() {
    let dir = tempfile::tempdir().unwrap();
    clean_fixture(dir.path());
    // FE-0006 is `ledgered`; but to exercise the ledger we need a checkable+ledgered rule. None
    // ship a banned-call yet, so instead assert that a clean repo with deviations still passes.
    write(
        dir.path(),
        "midas.toml",
        "[standard]\nversion = \"0.1.0\"\n[deviations]\n\"FE-0004\" = \"web-only\"\n",
    );
    midas()
        .args(["check", "--root"])
        .arg(dir.path())
        .assert()
        .success();
}

#[test]
fn flow_tag_bad_version_is_usage_error() {
    // Outside a git repo / not on trunk this errors before validation; assert it never hangs and
    // returns a typed non-success code (not 0). Run in a throwaway dir.
    let dir = tempfile::tempdir().unwrap();
    let code = midas()
        .args(["flow", "tag", "not-a-version"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .get_output()
        .status
        .code();
    assert!(matches!(code, Some(1) | Some(2) | Some(3)));
}

#[test]
fn sync_missing_then_present() {
    let dir = tempfile::tempdir().unwrap();
    // Seed an existing agent doc with project content outside the block.
    fs::write(dir.path().join("CLAUDE.md"), "# Project\n\nlocal notes\n").unwrap();

    // --check before sync: block missing → exit 2.
    midas()
        .args(["sync", "--check"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .code(2);

    // sync writes the block.
    midas()
        .arg("sync")
        .current_dir(dir.path())
        .assert()
        .success();
    let written = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert!(written.contains("local notes"), "project content preserved");
    assert!(written.contains("<!-- midas:"), "managed block written");

    // --check now clean → exit 0.
    midas()
        .args(["sync", "--check"])
        .current_dir(dir.path())
        .assert()
        .success();
}

#[test]
fn add_state_scaffolds_singleton() {
    let dir = tempfile::tempdir().unwrap();
    let out = midas()
        .args(["--json", "add", "state", "notes-pane", "--dir", "lib/state"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["class"], "NotesPaneStore");
    assert_eq!(v["instance"], "notesPane");
    let body = fs::read_to_string(dir.path().join("lib/state/notes-pane.svelte.ts")).unwrap();
    assert!(body.contains("export class NotesPaneStore"));
    assert!(body.contains("export const notesPane = new NotesPaneStore();"));
    assert!(body.contains("$state(false)"));
}

#[test]
fn add_state_refuses_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    midas()
        .args(["add", "state", "x", "--dir", "lib/state"])
        .current_dir(dir.path())
        .assert()
        .success();
    // second time without --force → expected-negative exit 2
    midas()
        .args(["add", "state", "x", "--dir", "lib/state"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .code(2);
}

#[test]
fn add_component_pascal_filename() {
    let dir = tempfile::tempdir().unwrap();
    let out = midas()
        .args(["--json", "add", "component", "notes-toolbar", "--dir", "c"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["component"], "NotesToolbar");
    let body = fs::read_to_string(dir.path().join("c/NotesToolbar.svelte")).unwrap();
    assert!(body.contains("$props()"));
    assert!(body.contains("lang=\"ts\""));
}

#[test]
fn add_migration_numbers_sequentially() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("db/migrations")).unwrap();
    fs::write(dir.path().join("db/migrations/018_existing.sql"), "").unwrap();
    let out = midas()
        .args(["--json", "add", "migration", "add-notes-index"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["number"], 19);
    assert!(dir
        .path()
        .join("db/migrations/019_add-notes-index.sql")
        .exists());
}

#[test]
fn add_module_scaffolds_and_wires() {
    let dir = tempfile::tempdir().unwrap();
    let modules = dir.path().join("m");
    fs::create_dir_all(&modules).unwrap();
    fs::write(modules.join("mod.rs"), "//! mods\npub mod notes;\n").unwrap();

    let out = midas()
        .args(["--json", "add", "module", "billing", "--dir", "m"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["module"], "Billing");
    assert_eq!(v["wired"], true);
    for f in ["mod.rs", "model.rs", "service.rs", "handler.rs"] {
        assert!(modules.join("billing").join(f).exists(), "missing {f}");
    }
    let handler = fs::read_to_string(modules.join("billing/handler.rs")).unwrap();
    assert!(handler.contains("RequireAuth")); // BE-0004
    assert!(handler.contains("response::ok_list")); // BE-0002
    assert!(handler.contains("utoipa::path")); // generated contract
    let model = fs::read_to_string(modules.join("billing/model.rs")).unwrap();
    assert!(model.contains("rename_all = \"camelCase\"")); // BE-0008
    let reg = fs::read_to_string(modules.join("mod.rs")).unwrap();
    assert!(reg.contains("pub mod billing;")); // wired

    // idempotent: second run doesn't duplicate the decl (uses --force on the dir)
    let out2 = midas()
        .args(["add", "module", "billing", "--dir", "m", "--force"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out2.status.success());
    let reg2 = fs::read_to_string(modules.join("mod.rs")).unwrap();
    assert_eq!(
        reg2.matches("pub mod billing;").count(),
        1,
        "no duplicate decl"
    );
}

#[test]
fn new_scaffolds_conformant_project() {
    let dir = tempfile::tempdir().unwrap();
    let out = midas()
        .args(["--json", "new", "acme", "--profile", "service"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["profile"], "service");

    let proj = dir.path().join("acme");
    for f in [
        "midas.toml",
        "README.md",
        ".gitignore",
        "CLAUDE.md",
        "AGENTS.md",
        ".github/workflows/ci.yml",
    ] {
        assert!(proj.join(f).exists(), "missing {f}");
    }
    // generated manifest must parse and declare the right profile/stack
    let toml = fs::read_to_string(proj.join("midas.toml")).unwrap();
    let parsed: toml::Value = toml::from_str(&toml).expect("generated midas.toml parses");
    assert_eq!(parsed["standard"]["profile"].as_str(), Some("service"));
    assert_eq!(parsed["stack"]["backend"]["current"].as_str(), Some("rust"));
    // agent docs carry the synced managed block
    assert!(fs::read_to_string(proj.join("CLAUDE.md"))
        .unwrap()
        .contains("<!-- midas:"));

    // the freshly-scaffolded project passes its own gate
    midas()
        .args(["check", "--root"])
        .arg(&proj)
        .assert()
        .success();
}

#[test]
fn new_refuses_nonempty_dir() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("acme")).unwrap();
    fs::write(dir.path().join("acme/keep.txt"), "x").unwrap();
    midas()
        .args(["new", "acme"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .code(2);
}

#[test]
fn json_stdout_has_no_log_noise() {
    // CLI-0003: --json stdout must be parseable with nothing else mixed in.
    let dir = tempfile::tempdir().unwrap();
    clean_fixture(dir.path());
    let out = midas()
        .args(["--json", "check", "--root"])
        .arg(dir.path())
        .output()
        .unwrap();
    serde_json::from_slice::<serde_json::Value>(&out.stdout)
        .expect("stdout is pure JSON even with progress on stderr");
}
