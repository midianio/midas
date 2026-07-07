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

/// A fixture that conforms to the mechanized checks (state dir present, no banned calls, agent
/// docs synced — AGT-0001).
fn clean_fixture(root: &Path) {
    fs::create_dir_all(root.join("app/web/src/lib/state")).unwrap();
    write(root, "app/web/src/lib/utils.ts", "export const x = 1;\n");
    write(root, "app/api/src/main.rs", "fn main() {}\n");
    midas().current_dir(root).arg("sync").assert().success();
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

    let out = midas()
        .args(["--json", "check", "--root"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "exit 0 — no mechanical violations");

    // The gate must not be vacuously clean: real checks fire on a conformant tree.
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let passed = v["mechanical"]["passed"].as_u64().unwrap();
    assert!(passed >= 3, "expected real checks to pass, got {passed}");
}

#[test]
fn check_deviation_against_hard_rule_is_an_error() {
    // SPEC §7: a `hard` rule can't be ledgered away — the ledger entry itself fails the gate,
    // even when the rule currently passes.
    let dir = tempfile::tempdir().unwrap();
    clean_fixture(dir.path());
    write(
        dir.path(),
        "midas.toml",
        "[standard]\nversion = \"0.2.0\"\n[deviations]\n\"BE-0010\" = \"we like bare clients\"\n",
    );
    midas()
        .args(["check", "--root"])
        .arg(dir.path())
        .assert()
        .code(2)
        .stderr(predicate::str::contains("mechanical violation"));
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
fn prompt_without_tty_is_a_usage_error() {
    // CLI-0001/CLI-0008: a command that would prompt must exit 3 under no TTY instead of hanging —
    // `touch state` with no name would ask for one.
    let dir = tempfile::tempdir().unwrap();
    midas()
        .current_dir(dir.path())
        .args(["touch", "state"])
        .assert()
        .code(3)
        .stderr(predicate::str::is_empty().not());
}

#[test]
fn expected_failure_is_never_silent() {
    // Exit 2 must always carry a human message on stderr (CLI-0003) — a clean "no" with empty
    // output is indistinguishable from a crash to a user or agent.
    let dir = tempfile::tempdir().unwrap();
    midas()
        .current_dir(dir.path())
        .args(["touch", "state", "foo"])
        .assert()
        .success();
    midas()
        .current_dir(dir.path())
        .args(["touch", "state", "foo"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains("already exists"));
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
fn flow_help_is_flat() {
    // The cleaned-up flow group: verbs are direct, not nested under `db`, and the redundant
    // `hotfix`/`doctor` are gone.
    midas()
        .args(["flow", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("start"))
        .stdout(predicate::str::contains("end"))
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("hotfix").not())
        .stdout(predicate::str::contains("Operate on the active pscale").not());
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
fn touch_state_scaffolds_singleton() {
    let dir = tempfile::tempdir().unwrap();
    let out = midas()
        .args([
            "--json",
            "touch",
            "state",
            "notes-pane",
            "--dir",
            "lib/state",
        ])
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
fn touch_state_refuses_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    midas()
        .args(["touch", "state", "x", "--dir", "lib/state"])
        .current_dir(dir.path())
        .assert()
        .success();
    // second time without --force → expected-negative exit 2
    midas()
        .args(["touch", "state", "x", "--dir", "lib/state"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .code(2);
}

#[test]
fn touch_component_pascal_filename() {
    let dir = tempfile::tempdir().unwrap();
    let out = midas()
        .args([
            "--json",
            "touch",
            "component",
            "notes-toolbar",
            "--dir",
            "c",
        ])
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
fn touch_migration_numbers_sequentially() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("db/migrations")).unwrap();
    fs::write(dir.path().join("db/migrations/018_existing.sql"), "").unwrap();
    let out = midas()
        .args(["--json", "touch", "migration", "add-notes-index"])
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
fn touch_module_scaffolds_and_wires() {
    let dir = tempfile::tempdir().unwrap();
    let modules = dir.path().join("m");
    fs::create_dir_all(&modules).unwrap();
    fs::write(modules.join("mod.rs"), "//! mods\npub mod notes;\n").unwrap();

    let out = midas()
        .args(["--json", "touch", "module", "billing", "--dir", "m"])
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
        .args(["touch", "module", "billing", "--dir", "m", "--force"])
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
fn touch_project_scaffolds() {
    // The canonical project path is `touch project`; `new` is a hidden back-compat alias.
    let dir = tempfile::tempdir().unwrap();
    let out = midas()
        .args(["--json", "touch", "project", "acme", "--profile", "service"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["profile"], "service");
    assert!(dir.path().join("acme/midas.toml").exists());
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

    // the service profile lays down the runnable rust-service skeleton under app/api/
    for f in [
        "app/api/Cargo.toml",
        "app/api/src/main.rs",
        "app/api/src/lib.rs",
        "app/api/src/response.rs",
        "app/api/src/error.rs",
        "app/api/src/ids.rs",
        "app/api/src/routes.rs",
    ] {
        assert!(proj.join(f).exists(), "missing {f}");
    }
    // project tokens are substituted: package name + crate path derive from the project name
    let cargo = fs::read_to_string(proj.join("app/api/Cargo.toml")).unwrap();
    assert!(
        cargo.contains("name = \"acme-api\""),
        "PKG token unsubstituted"
    );
    assert!(
        !cargo.contains("{{"),
        "left an unsubstituted token in Cargo.toml"
    );
    let main = fs::read_to_string(proj.join("app/api/src/main.rs")).unwrap();
    assert!(main.contains("acme_api::"), "CRATE token unsubstituted");
    // uuid lives only in ids.rs (BE-0016 allow_in) — the gate below would fail otherwise
    assert!(fs::read_to_string(proj.join("app/api/src/ids.rs"))
        .unwrap()
        .contains("uuid::Uuid::new_v4"));

    // the freshly-scaffolded project — code skeleton included — passes its own gate
    midas()
        .args(["check", "--root"])
        .arg(&proj)
        .assert()
        .success();
}

#[test]
fn new_app_scaffolds_backend_and_frontend() {
    let dir = tempfile::tempdir().unwrap();
    midas()
        .args(["new", "shop", "--profile", "app"])
        .current_dir(dir.path())
        .assert()
        .success();
    let proj = dir.path().join("shop");

    // app profile lays down BOTH the rust-service backend and the svelte-app frontend
    for f in [
        "app/api/src/main.rs",
        "app/api/src/modules/items/handler.rs", // BE-0001 modules pattern
        "app/api/src/auth/mod.rs",              // BE-0004 RequireAuth seam
        "app/web/package.json",
        "app/web/svelte.config.js",
        "app/web/src/routes/(public)/+page.svelte", // SSR'd marketing group
        "app/web/src/routes/app/+page.svelte",      // SPA app group
        "app/web/src/lib/utils.ts",
        "app/web/src/lib/api.ts",
        "app/web/src/lib/state/app.svelte.ts", // FE-0001 state dir must exist or the gate fails
        "app/web/src/lib/state/auth.svelte.ts", // auth singleton + token provider
        "app/web/src/lib/components/ui/Button.svelte", // FE-0011 component
    ] {
        assert!(proj.join(f).exists(), "missing {f}");
    }
    // {{NAME}} token substituted into the frontend package + page
    let pkg = fs::read_to_string(proj.join("app/web/package.json")).unwrap();
    assert!(
        pkg.contains("\"name\": \"shop\""),
        "NAME token unsubstituted"
    );
    assert!(!pkg.contains("{{"), "left an unsubstituted token");
    // crypto.randomUUID lives only in utils.ts (FE-0010 allow_in) — the gate would fail otherwise
    assert!(fs::read_to_string(proj.join("app/web/src/lib/utils.ts"))
        .unwrap()
        .contains("crypto.randomUUID"));

    // both layers' mechanical checks pass on the freshly-scaffolded app
    midas()
        .args(["check", "--root"])
        .arg(&proj)
        .assert()
        .success();
}

#[test]
fn dev_runs_processes_with_prefixed_output() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("midas.toml"),
        "[standard]\nversion = \"0.1.0\"\n[dev]\nprocesses = [\n\
         { name = \"api\", cmd = \"echo hi-from-api\" },\n\
         { name = \"web\", cmd = \"echo hi-from-web\" },\n]\n",
    )
    .unwrap();
    let out = midas()
        .args(["--no-color", "dev"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "dev exits 0 when processes finish");
    // process stdout is streamed to our stdout, one prefixed line per process
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("api │ hi-from-api"),
        "missing api prefix: {stdout}"
    );
    assert!(
        stdout.contains("web │ hi-from-web"),
        "missing web prefix: {stdout}"
    );
}

#[test]
fn dev_fails_fast_when_a_declared_port_is_busy() {
    // Hold a port in-process; `midas dev` must refuse to start (exit 2, the clean "answer is no")
    // before spawning anything, and name the port + process in the message.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();

    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("midas.toml"),
        format!(
            "[standard]\nversion = \"0.1.0\"\n[dev]\nprocesses = [\n\
             {{ name = \"api\", cmd = \"echo should-not-run\", port = {port} }},\n]\n"
        ),
    )
    .unwrap();
    let out = midas()
        .args(["--no-color", "dev"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "busy port is an expected failure"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains(&format!(":{port} (api)")),
        "names the busy port and process: {stderr}"
    );
    assert!(
        stderr.contains("--kill-ports"),
        "points at the reclaim flag: {stderr}"
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("should-not-run"),
        "no process spawns when preflight fails: {stdout}"
    );
}

#[test]
fn dev_with_free_declared_port_runs_normally() {
    // Bind-then-drop to get a port that is actually free right now.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        l.local_addr().unwrap().port()
    };
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("midas.toml"),
        format!(
            "[standard]\nversion = \"0.1.0\"\n[dev]\nprocesses = [\n\
             {{ name = \"api\", cmd = \"echo hi-from-api\", port = {port} }},\n]\n"
        ),
    )
    .unwrap();
    let out = midas()
        .args(["--no-color", "dev"])
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "free declared port doesn't block");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("api │ hi-from-api"), "{stdout}");
}

#[test]
fn dev_without_config_is_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("midas.toml"),
        "[standard]\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    midas()
        .args(["dev"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .code(3); // no [dev] processes → usage error
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

#[test]
fn drift_same_version_is_clean_and_exits_zero() {
    // No midas.toml → pinned falls back to the embedded version, so from == to: `drift` degrades to
    // the (B) standing-drift pass on a clean tree, reports `same`, and never gates (exit 0).
    let dir = tempfile::tempdir().unwrap();
    clean_fixture(dir.path());
    let out = midas()
        .args(["--json", "drift", "--root"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success(), "drift is a report, never a gate");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["direction"], "same");
    assert!(v["transitions"].as_array().unwrap().is_empty());
    assert!(v.get("summary").is_some());
}

#[test]
fn drift_unknown_version_is_usage_error() {
    let dir = tempfile::tempdir().unwrap();
    clean_fixture(dir.path());
    midas()
        .args(["drift", "0.9.9", "--root"])
        .arg(dir.path())
        .assert()
        .failure()
        .code(3); // not embedded → usage error listing available versions
}

#[test]
fn drift_outcome_diff_blocks_and_cleans_ledger() {
    // The headline deep diff: a new `hard` convention the repo violates is `blocking`/fix_required
    // with the file:line worklist, and a convention removed at the target that the repo still ledgers
    // is `ledger_cleanup`/remove_dead_deviation. Both registries are supplied as files so the diff is
    // independent of whatever the binary embeds.
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "src/main.rs",
        "fn main() { println!(\"x\"); }\n",
    );
    write(
        dir.path(),
        "midas.toml",
        "[standard]\nversion = \"0.1.0\"\n[deviations]\n\"X-0014\" = \"legacy\"\n",
    );
    // `from`: only the soon-to-be-removed X-0014. `to`: drops X-0014, adds the hard X-9001 println ban.
    write(
        dir.path(),
        "from.json",
        r#"{ "version": "0.1.0", "conventions": [
            { "id": "X-0014", "title": "Legacy rule.", "layer": "cli", "tier": "check", "escape": "ledgered" }
        ] }"#,
    );
    write(
        dir.path(),
        "to.json",
        r#"{ "version": "0.2.0", "conventions": [
            { "id": "X-9001", "title": "No bare println!.", "layer": "cli", "tier": "check", "escape": "hard",
              "check": { "kind": "banned-call", "pattern": "\\bprintln!", "globs": ["**/*.rs"] },
              "doc": "cli/conventions.md" }
        ] }"#,
    );

    let out = midas()
        .args(["--json", "drift", "--from-file"])
        .arg(dir.path().join("from.json"))
        .arg("--to-file")
        .arg(dir.path().join("to.json"))
        .arg("--root")
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "drift exits 0 even with blocking drift"
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["direction"], "upgrade");

    let ts = v["transitions"].as_array().unwrap();
    let added = ts
        .iter()
        .find(|t| t["id"] == "X-9001")
        .expect("X-9001 present");
    assert_eq!(added["class"], "blocking");
    assert_eq!(added["action"], "fix_required");
    assert_eq!(added["new_outcome"], "fail");
    assert!(
        !added["findings"].as_array().unwrap().is_empty(),
        "blocking transition carries the file:line worklist"
    );

    let removed = ts
        .iter()
        .find(|t| t["id"] == "X-0014")
        .expect("X-0014 present");
    assert_eq!(removed["class"], "ledger_cleanup");
    assert_eq!(removed["action"], "remove_dead_deviation");

    assert_eq!(v["summary"]["blocking"], 1);
    assert_eq!(v["summary"]["ledger_cleanup"], 1);
}

// ---- the standards family: explain · conventions · deviate ----

#[test]
fn explain_known_id_renders_and_json_is_stable() {
    midas()
        .args(["explain", "BE-0010"])
        .assert()
        .success()
        .stdout(predicate::str::contains("BE-0010"))
        .stdout(predicate::str::contains("hard"));

    let out = midas()
        .args(["--json", "explain", "BE-0010"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["convention"]["id"], "BE-0010");
    assert_eq!(v["convention"]["tier"], "check");
    assert_eq!(v["convention"]["escape"], "hard");
    assert!(v["version"].as_str().is_some());
}

#[test]
fn explain_is_case_insensitive_and_unknown_id_is_usage_error() {
    midas().args(["explain", "be-0010"]).assert().success();
    midas()
        .args(["explain", "XX-9999"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("unknown convention id"));
}

#[test]
fn conventions_lists_and_filters() {
    let out = midas().args(["--json", "conventions"]).output().unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let all = v["count"].as_u64().unwrap();
    assert!(all >= 50, "the whole catalog lists, got {all}");

    let out = midas()
        .args([
            "--json",
            "conventions",
            "--tier",
            "check",
            "--layer",
            "backend",
        ])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["count"].as_u64().unwrap() < all,
        "filters narrow the list"
    );
    for c in v["conventions"].as_array().unwrap() {
        assert_eq!(c["tier"], "check");
        assert_eq!(c["layer"], "backend");
    }

    midas()
        .args(["conventions", "--tier", "bogus"])
        .assert()
        .code(3);
}

#[test]
fn deviate_writes_ledger_and_refuses_hard_and_advisory() {
    let dir = tempfile::tempdir().unwrap();
    write(
        dir.path(),
        "midas.toml",
        "[standard]\nversion = \"0.2.0\"\n",
    );

    // ledgered-escape rule → entry written, comments/sections preserved.
    midas()
        .args([
            "deviate",
            "BE-0014",
            "--reason",
            "no TS client yet",
            "--root",
        ])
        .arg(dir.path())
        .assert()
        .success();
    let toml = fs::read_to_string(dir.path().join("midas.toml")).unwrap();
    assert!(toml.contains("[deviations]"));
    assert!(toml.contains("\"BE-0014\" = \"no TS client yet\""));
    assert!(toml.starts_with("[standard]"), "existing content preserved");

    // hard → refused (exit 3); advisory → refused (exit 3).
    midas()
        .args(["deviate", "BE-0010", "--reason", "x", "--root"])
        .arg(dir.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("hard"));
    midas()
        .args(["deviate", "FE-0002", "--reason", "x", "--root"])
        .arg(dir.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("advisory"));

    // no midas.toml → usage error pointing at adopt.
    let bare = tempfile::tempdir().unwrap();
    midas()
        .args(["deviate", "BE-0014", "--reason", "x", "--root"])
        .arg(bare.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("midas adopt"));
}

#[test]
fn deviate_prune_drops_dead_entries_and_keeps_live_ones() {
    let dir = tempfile::tempdir().unwrap();
    clean_fixture(dir.path());
    // ZZ-0001 no longer exists in the standard (dead); FE-0004 is review-tier (kept).
    write(
        dir.path(),
        "midas.toml",
        "[standard]\nversion = \"0.2.0\"\n[deviations]\n\"ZZ-0001\" = \"from an old standard\"\n\"FE-0004\" = \"web-only\"\n",
    );
    let out = midas()
        .args(["--json", "deviate", "--prune", "--root"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert!(out.status.success());
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["pruned"], serde_json::json!(["ZZ-0001"]));
    assert_eq!(v["kept"], serde_json::json!(["FE-0004"]));
    let toml = fs::read_to_string(dir.path().join("midas.toml")).unwrap();
    assert!(!toml.contains("ZZ-0001"));
    assert!(toml.contains("FE-0004"));
}

// ---- adopt · doctor --fix · completions · check --changed ----

#[test]
fn adopt_brownfield_writes_manifest_docs_and_runs_the_gate() {
    let dir = tempfile::tempdir().unwrap();
    // An existing repo with a standing hard violation (FE-0010) and no state dir (FE-0001).
    write(
        dir.path(),
        "app/web/src/lib/thing.ts",
        "export const id = () => crypto.randomUUID();\n",
    );
    midas()
        .args(["adopt", "--profile", "app", "-y", "--root"])
        .arg(dir.path())
        .assert()
        .code(2) // hard violations can't be ledgered — adopt ends on the honest gate
        .stderr(predicate::str::contains("mechanical violation"));

    let toml = fs::read_to_string(dir.path().join("midas.toml")).unwrap();
    assert!(toml.contains("[standard]"));
    assert!(toml.contains("profile = \"app\""));
    let doc = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert!(doc.contains("<!-- midas:"), "agent docs synced");

    // Idempotent: a second adopt keeps the manifest and still just re-checks.
    midas()
        .args(["adopt", "--profile", "app", "-y", "--root"])
        .arg(dir.path())
        .assert()
        .code(2);
}

#[test]
fn adopt_clean_tree_exits_zero() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("app/web/src/lib/state")).unwrap();
    write(dir.path(), "app/api/src/main.rs", "fn main() {}\n");
    midas()
        .args(["adopt", "--profile", "app", "-y", "--root"])
        .arg(dir.path())
        .assert()
        .success();
}

#[test]
fn doctor_fix_repairs_stale_agent_docs() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("CLAUDE.md"),
        "# P\n\n<!-- midas:0.0.1 -->\nold\n<!-- /midas -->\n",
    )
    .unwrap();
    // Don't assert the exit code — gh/git env checks vary by machine; assert the fix happened.
    midas()
        .args(["doctor", "--fix", "--root"])
        .arg(dir.path())
        .assert()
        .stderr(predicate::str::contains("fixed: synced"));
    let doc = fs::read_to_string(dir.path().join("CLAUDE.md")).unwrap();
    assert!(!doc.contains("midas:0.0.1"), "stale block replaced");
}

#[test]
fn completions_emit_to_stdout() {
    midas()
        .args(["completions", "zsh"])
        .assert()
        .success()
        .stdout(predicate::str::contains("midas"));
    midas().args(["completions", "klingon"]).assert().code(2); // clap's invalid-value exit
}

#[test]
fn check_changed_scopes_content_scans_to_changed_files() {
    let dir = tempfile::tempdir().unwrap();
    clean_fixture(dir.path());
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .output()
            .unwrap()
    };
    git(&["init", "-q", "."]);
    // Commit a pre-existing violation, then add a fresh untracked one.
    write(
        dir.path(),
        "app/web/src/lib/old-bad.ts",
        "export const a = () => crypto.randomUUID();\n",
    );
    git(&["add", "-A"]);
    git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-qm",
        "init",
    ]);
    write(
        dir.path(),
        "app/web/src/lib/new-bad.ts",
        "export const b = () => crypto.randomUUID();\n",
    );

    let out = midas()
        .args(["--json", "check", "--changed", "--root"])
        .arg(dir.path())
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2), "the new violation still gates");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let fe0010 = v["mechanical"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "FE-0010")
        .unwrap();
    let files: Vec<&str> = fe0010["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["file"].as_str().unwrap())
        .collect();
    assert!(
        files.contains(&"app/web/src/lib/new-bad.ts"),
        "new file scanned"
    );
    assert!(
        !files.contains(&"app/web/src/lib/old-bad.ts"),
        "committed file skipped in --changed mode"
    );

    // Outside a git repo, --changed is a usage error.
    let bare = tempfile::tempdir().unwrap();
    clean_fixture(bare.path());
    midas()
        .args(["check", "--changed", "--root"])
        .arg(bare.path())
        .assert()
        .code(3);
}

#[test]
fn flow_rebase_replaces_sync_and_keeps_the_alias() {
    midas()
        .args(["flow", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rebase"))
        .stdout(predicate::str::contains("clean"));
    // The old spelling still parses (hidden alias) — outside a repo it fails with a typed code,
    // proving it routed to the rebase command rather than clap rejection (which would be 2).
    let dir = tempfile::tempdir().unwrap();
    midas()
        .args(["flow", "sync"])
        .current_dir(dir.path())
        .assert()
        .code(1)
        .stderr(predicate::str::contains("not inside a git repository"));
}

/// `[dev]` watch: a change to a watched path restarts the process (unix-only — the test drives
/// the long-running `midas dev` directly and reaps it with a kill).
#[cfg(unix)]
#[test]
fn dev_watch_restarts_process_on_change() {
    use std::io::Read as _;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "data.txt", "alpha\n");
    write(
        dir.path(),
        "midas.toml",
        "[standard]\nversion = \"0.3.0\"\n\n[dev]\nprocesses = [\n  { name = \"echoer\", cmd = \"cat data.txt\", watch = [\"data.txt\"] },\n]\n",
    );

    let mut child = std::process::Command::new(assert_cmd::cargo::cargo_bin("midas"))
        .arg("dev")
        .current_dir(dir.path())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    let out = Arc::new(Mutex::new(String::new()));
    let reader = {
        let out = out.clone();
        let mut stdout = child.stdout.take().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 512];
            while let Ok(n) = stdout.read(&mut buf) {
                if n == 0 {
                    break;
                }
                out.lock()
                    .unwrap()
                    .push_str(&String::from_utf8_lossy(&buf[..n]));
            }
        })
    };
    let wait_for = |needle: &str| {
        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            if out.lock().unwrap().contains(needle) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        false
    };

    assert!(wait_for("alpha"), "initial run streams the file");
    write(dir.path(), "data.txt", "bravo\n");
    let restarted = wait_for("bravo");

    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
    assert!(restarted, "change to a watched path restarts the process");
}
