use std::path::{Path, PathBuf};

use super::*;

fn path_of(len: usize) -> PathBuf {
    let name = "x".repeat(len - "/daemon.sock".len() - 1);
    PathBuf::from(format!("/{name}")).join("daemon.sock")
}

#[test]
fn a_socket_path_that_fits_sun_path_is_accepted() {
    let p = path_of(103);
    assert_eq!(p.as_os_str().len(), 103);
    check_socket_path(&p).unwrap();
}

#[test]
fn a_socket_path_too_long_for_sun_path_is_refused_with_its_length() {
    let p = path_of(104);
    let msg = format!("{:#}", check_socket_path(&p).unwrap_err());
    assert!(msg.contains("104 bytes"), "{msg}");
    assert!(msg.contains("103"), "{msg}");
}

#[test]
fn the_directory_is_the_override_when_set() {
    assert_eq!(
        dir_from(Some("/tmp/cos-test-dir".into())).unwrap(),
        Path::new("/tmp/cos-test-dir")
    );
}

#[cfg(target_os = "macos")]
#[test]
fn the_default_directory_is_cos_under_the_user_temp_dir() {
    let d = dir_from(None).unwrap();
    assert!(d.is_absolute(), "{}", d.display());
    assert!(d.ends_with("cos"), "{}", d.display());
    assert!(d.parent().unwrap().is_dir(), "{}", d.display());
    check_socket_path(&socket_path(&d)).unwrap();
}

#[test]
fn requests_parse_only_inside_the_binding() {
    let req = "0123456789abcdef";
    assert!(matches!(parse(r#"{"op":"hello"}"#), Ok(Request::Hello)));
    assert!(matches!(
        parse(r#"{"op":"subscribe"}"#),
        Ok(Request::Subscribe)
    ));
    let ask = format!(
        r#"{{"op":"start","kind":"ask","business":"acme","client_req":"{req}","question":"why?"}}"#
    );
    assert!(matches!(
        parse(&ask),
        Ok(Request::Start { kind: Kind::Ask, question: Some(q), .. }) if q == "why?"
    ));
    for bad in [
        "[]".to_owned(),
        r#"{"op":"hello","x":1}"#.to_owned(),
        format!(
            r#"{{"op":"start","kind":"brief","business":"acme","client_req":"{req}","question":"q"}}"#
        ),
        format!(r#"{{"op":"start","kind":"ask","business":"acme","client_req":"{req}"}}"#),
        format!(r#"{{"op":"start","kind":"brief","business":"-acme","client_req":"{req}"}}"#),
        r#"{"op":"start","kind":"brief","business":"acme","client_req":"0123456789ABCDEF"}"#
            .to_owned(),
        r#"{"op":"stop","run":"r-1","business":"acme"}"#.to_owned(),
    ] {
        assert!(parse(&bad).is_err(), "{bad}");
    }
}
