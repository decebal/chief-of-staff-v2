//! The shipped library: every entry under `templates/roles/` is compiled in,
//! parses, validates, cites a public source and carries a specialist's full
//! definition.

use cos::agents::policy;
use cos::agents::roles::{self, LibraryEntry};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/templates/roles");

#[test]
fn every_file_under_templates_roles_is_compiled_in() {
    let mut on_disk: Vec<String> = std::fs::read_dir(DIR)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".json"))
        .collect();
    on_disk.sort();
    let mut listed: Vec<String> = roles::LIBRARY_FILES
        .iter()
        .map(|(n, _)| (*n).to_owned())
        .collect();
    listed.sort();
    assert_eq!(on_disk, listed);
    for (name, text) in roles::LIBRARY_FILES {
        assert_eq!(
            *text,
            std::fs::read_to_string(format!("{DIR}/{name}")).unwrap(),
            "{name}"
        );
    }
}

#[test]
fn every_entry_parses_and_none_is_dropped() {
    for (name, text) in roles::LIBRARY_FILES {
        let e: Result<LibraryEntry, _> = serde_json::from_str(text);
        assert!(e.is_ok(), "{name}: {:?}", e.err());
    }
    assert_eq!(roles::library().len(), roles::LIBRARY_FILES.len());
    assert_eq!(roles::library().len(), 12);
}

#[test]
fn every_entry_is_a_full_specialist_definition() {
    for e in roles::library() {
        let r = &e.role;
        let at = &e.id;
        assert_eq!(r.id, e.id, "{at}: id");
        assert!(r.principles.len() >= 5, "{at}: principles");
        assert!(r.method.len() >= 5, "{at}: method");
        assert!(r.anti_patterns.len() >= 4, "{at}: anti_patterns");
        assert!(r.escalation.len() >= 3, "{at}: escalation");
        assert!(
            r.output_format
                .as_deref()
                .is_some_and(|f| !f.trim().is_empty()),
            "{at}: output_format"
        );
        assert!(!e.sources.is_empty(), "{at}: sources");
        for s in &e.sources {
            assert!(s.url.starts_with("https://"), "{at}: {}", s.url);
            assert!(!s.title.trim().is_empty(), "{at}: source title");
        }
        assert!(
            !r.head && !r.retired && r.from.is_none(),
            "{at}: head, retired or from"
        );
    }
}

#[test]
fn every_entry_validates_inside_the_template_staff() {
    let mut p = policy::seed();
    for e in roles::library() {
        p.roles.push(e.role.clone());
    }
    let problems = roles::validate(&p, &roles::vocab(&p));
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_new_kinds_are_in_the_template_and_match_each_entrys_declaration() {
    let kinds = policy::seed().fit.unwrap().kinds;
    for k in [
        "security",
        "performance",
        "accessibility",
        "analysis",
        "reliability",
    ] {
        assert!(kinds.contains_key(k), "template lacks {k}");
    }
    for e in roles::library() {
        for (k, terms) in &e.fit_kinds {
            assert_eq!(kinds.get(k), Some(terms), "{}: {k}", e.id);
        }
    }
}
