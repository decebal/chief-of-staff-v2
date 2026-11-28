//! The one write path to `context/agents.json` (`roles::apply`): approval,
//! lock, digest, validation, round trip, atomic replace and one level of
//! undo. Every refusal leaves the file's bytes as they were. Fixtures are
//! fictional.

use std::path::{Path, PathBuf};

use cos::agents::policy::{self, Role};
use cos::agents::roles::{self, Change, ContractApproval};

fn home(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-roles-write-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

fn analyst() -> Role {
    Role {
        id: "analyst".into(),
        title: "Analyst".into(),
        mandate: "Answer the question from the data you are given.".into(),
        principles: vec!["Every number names its source.".into()],
        ..Role::default()
    }
}

fn bytes(home: &Path) -> Option<Vec<u8>> {
    std::fs::read(policy::path(home)).ok()
}

fn put(home: &Path, text: &str) {
    std::fs::create_dir_all(home.join("context")).unwrap();
    std::fs::write(policy::path(home), text).unwrap();
}

fn digest(home: &Path) -> String {
    roles::read(home).unwrap().1
}

fn add(home: &Path, r: Role) -> anyhow::Result<roles::Written> {
    roles::apply(home, Change::Add(r), &digest(home), ContractApproval::None)
}

/// Asserts `f` is refused with a message containing `want` and the file is
/// byte-for-byte what it was.
fn refused(home: &Path, want: &str, f: impl FnOnce() -> anyhow::Result<roles::Written>) {
    let before = bytes(home);
    let e = match f() {
        Ok(w) => panic!("written, wanted a refusal naming {want:?}: {:?}", w.notes),
        Err(e) => format!("{e:#}"),
    };
    assert!(e.contains(want), "{want:?} not in: {e}");
    assert_eq!(bytes(home), before, "a refusal changed the file");
}

#[test]
fn the_first_write_on_a_fresh_home_keeps_the_template_roles_and_says_so() {
    let h = home("fresh");
    let (_, d) = roles::read(&h).unwrap();
    assert_eq!(d, "absent");
    let w = add(&h, analyst()).unwrap();
    let p = policy::load(&h);
    assert_eq!(p.roles.len(), policy::seed().roles.len() + 1);
    assert!(p.role("analyst").is_some() && p.role("builder").is_some());
    assert!(
        w.notes.iter().any(|n| n.contains("template")),
        "{:?}",
        w.notes
    );
    assert!(
        !h.join("context/agents.json.bak").exists(),
        "no previous file, no .bak"
    );
    assert!(
        !h.join("context/agents.json.lock").exists(),
        "the lock is released"
    );
    let raw: serde_json::Value = serde_json::from_slice(&bytes(&h).unwrap()).unwrap();
    assert!(raw.get("fit").is_none(), "fit is not materialised");
    assert_eq!(w.digest, digest(&h));
}

#[test]
fn a_stale_digest_an_unparseable_file_and_a_held_lock_change_nothing() {
    let h = home("refusals");
    add(&h, analyst()).unwrap();
    let mut other = analyst();
    other.id = "second".into();
    refused(&h, "changed since you read it", || {
        roles::apply(
            &h,
            Change::Add(other.clone()),
            "absent",
            ContractApproval::None,
        )
    });

    std::fs::write(h.join("context/agents.json.lock"), "4242").unwrap();
    refused(&h, "agents.json.lock", || add(&h, other.clone()));
    let e = format!("{:#}", add(&h, other.clone()).unwrap_err());
    assert!(e.contains("4242"), "{e}");
    std::fs::remove_file(h.join("context/agents.json.lock")).unwrap();

    put(&h, "{ not json");
    refused(&h, "does not parse", || {
        roles::apply(
            &h,
            Change::Retire("builder".into()),
            &digest(&h),
            ContractApproval::None,
        )
    });
}

#[test]
fn a_write_that_adds_a_refuse_problem_is_refused() {
    let h = home("invalid");
    add(&h, analyst()).unwrap();
    let mut bad = analyst();
    bad.id = "escape".into();
    bad.output = Some("../outside.md".into());
    refused(&h, "output", || add(&h, bad.clone()));
    let mut two = analyst();
    two.id = "lines".into();
    two.principles = vec!["one\ntwo".into()];
    refused(&h, "principles", || add(&h, two.clone()));
    let mut long = analyst();
    long.id = "long".into();
    long.method = (0..11).map(|i| format!("step {i}")).collect();
    refused(&h, "method", || add(&h, long.clone()));
    let mut head = analyst();
    head.id = "boss".into();
    head.head = true;
    refused(&h, "head", || add(&h, head.clone()));
    refused(&h, "already", || add(&h, analyst()));
}

#[test]
fn the_head_is_never_replaced_retired_or_restored() {
    let h = home("head");
    add(&h, analyst()).unwrap();
    refused(&h, "head", || {
        roles::apply(
            &h,
            Change::Retire("chief-of-staff".into()),
            &digest(&h),
            ContractApproval::Human,
        )
    });
    let head = policy::load(&h).role("chief-of-staff").unwrap().clone();
    refused(&h, "head", || {
        roles::apply(
            &h,
            Change::Replace {
                id: "chief-of-staff".into(),
                role: head.clone(),
            },
            &digest(&h),
            ContractApproval::Human,
        )
    });
}

#[test]
fn replace_keeps_the_id_and_retire_restore_flip_with_one_level_of_undo() {
    let h = home("flip");
    add(&h, analyst()).unwrap();
    let mut renamed = analyst();
    renamed.id = "other".into();
    refused(&h, "id", || {
        roles::apply(
            &h,
            Change::Replace {
                id: "analyst".into(),
                role: renamed.clone(),
            },
            &digest(&h),
            ContractApproval::Human,
        )
    });

    let before = bytes(&h).unwrap();
    roles::apply(
        &h,
        Change::Retire("analyst".into()),
        &digest(&h),
        ContractApproval::None,
    )
    .unwrap();
    assert!(policy::load(&h).role("analyst").unwrap().retired);
    assert_eq!(
        std::fs::read(h.join("context/agents.json.bak")).unwrap(),
        before
    );
    roles::apply(
        &h,
        Change::Restore("analyst".into()),
        &digest(&h),
        ContractApproval::None,
    )
    .unwrap();
    assert!(!policy::load(&h).role("analyst").unwrap().retired);

    let mut sharper = analyst();
    sharper.mandate = "Answer the question; show the query.".into();
    roles::apply(
        &h,
        Change::Replace {
            id: "analyst".into(),
            role: sharper,
        },
        &digest(&h),
        ContractApproval::None,
    )
    .expect("a descriptive change needs no contract approval");
    assert!(
        policy::load(&h)
            .role("analyst")
            .unwrap()
            .mandate
            .contains("query")
    );
}

#[test]
fn a_contract_change_needs_a_person() {
    let h = home("contract");
    add(&h, analyst()).unwrap();
    let mut checked = analyst();
    checked.id = "checked".into();
    checked.checks = vec!["test -s ANSWER.md".into()];
    refused(&h, "checks", || add(&h, checked.clone()));
    roles::apply(
        &h,
        Change::Add(checked),
        &digest(&h),
        ContractApproval::Human,
    )
    .unwrap();

    let mut wider = policy::load(&h).role("analyst").unwrap().clone();
    wider.scope = vec!["src".into()];
    refused(&h, "scope", || {
        roles::apply(
            &h,
            Change::Replace {
                id: "analyst".into(),
                role: wider.clone(),
            },
            &digest(&h),
            ContractApproval::None,
        )
    });
    let d = roles::contract_diff(&analyst(), &wider);
    assert_eq!(
        d.iter().map(|f| f.field.as_str()).collect::<Vec<_>>(),
        ["scope"]
    );
}

#[test]
fn a_legacy_file_writes_with_its_retired_keys_dropped_and_counted() {
    let h = home("legacy");
    put(
        &h,
        r#"{
          "defaults": { "deadline_secs": 900, "budget_usd": 5, "stall_secs": 600, "repeat": 5 },
          "roles": [{ "id": "chief-of-staff", "title": "Chief of staff", "head": true, "mandate": "Brief." },
                    { "id": "reviewer", "title": "Reviewer", "mandate": "Review.",
                      "budget_usd": 2, "stall_secs": 60, "repeat": 3 }],
          "repos": { "demo-repo": { "budget_usd": 9 } }
        }"#,
    );
    let w = add(&h, analyst()).unwrap();
    assert!(w.notes.iter().any(|n| n.contains("7")), "{:?}", w.notes);
    let text = String::from_utf8(bytes(&h).unwrap()).unwrap();
    assert!(
        !text.contains("budget_usd") && !text.contains("stall_secs"),
        "{text}"
    );
    assert_eq!(policy::load(&h).defaults.deadline_secs, Some(900));
}

#[test]
fn a_broken_role_already_in_the_file_does_not_block_retiring_it() {
    let h = home("broken");
    let mut p = policy::seed();
    let mut broken = analyst();
    broken.scope = vec!["../everything".into()];
    p.roles.push(broken);
    put(&h, &serde_json::to_string_pretty(&p).unwrap());
    roles::apply(
        &h,
        Change::Retire("analyst".into()),
        &digest(&h),
        ContractApproval::None,
    )
    .expect("retiring a hand-broken role adds no problem");
    roles::apply(
        &h,
        Change::Retire("builder".into()),
        &digest(&h),
        ContractApproval::None,
    )
    .expect("an unrelated role's problem blocks nothing");
}

#[test]
fn replacing_a_broken_role_must_leave_it_valid() {
    let h = home("replace-broken");
    let mut p = policy::seed();
    let mut broken = analyst();
    broken.scope = vec!["../everything".into()];
    p.roles.push(broken.clone());
    put(&h, &serde_json::to_string_pretty(&p).unwrap());

    let mut still = broken.clone();
    still.scope = vec!["/etc".into()];
    still.principles = vec!["one\ntwo".into()];
    refused(&h, "scope", || {
        roles::apply(
            &h,
            Change::Replace {
                id: "analyst".into(),
                role: still.clone(),
            },
            &digest(&h),
            ContractApproval::Human,
        )
    });
    let mut same_bad = broken.clone();
    same_bad.mandate = "A new mandate, the bad scope kept.".into();
    refused(&h, "scope", || {
        roles::apply(
            &h,
            Change::Replace {
                id: "analyst".into(),
                role: same_bad.clone(),
            },
            &digest(&h),
            ContractApproval::None,
        )
    });

    let mut fixed = broken;
    fixed.scope = vec!["notes".into()];
    roles::apply(
        &h,
        Change::Replace {
            id: "analyst".into(),
            role: fixed,
        },
        &digest(&h),
        ContractApproval::Human,
    )
    .expect("a replacement that leaves the role valid is written");
}
