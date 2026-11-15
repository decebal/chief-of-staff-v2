use cos::gather::parse_toon;
use cos::report::render;
use serde_json::json;

#[test]
fn data_is_embedded_once_and_cannot_close_the_script() {
    let html = render(
        &json!({ "brief": { "report": { "headline": "</script><script>alert(1)</script>" } } }),
    )
    .unwrap();
    assert!(!html.contains("\"__COS_DATA__\""));
    assert!(!html.contains("</script><script>alert(1)"));
    assert!(html.contains("<\\/script>"));
    assert_eq!(html.matches("const DATA = ").count(), 1);
}

#[test]
fn saved_weekly_reviews_on_disk_are_ignored() {
    let home = std::env::temp_dir().join(format!("cos-old-reviews-{}", std::process::id()));
    std::fs::create_dir_all(home.join("out/reviews")).unwrap();
    std::fs::write(home.join("out/reviews/2030-04-05.json"), "{ not json").unwrap();
    let brief = json!({ "headline": "Saved before the week part.", "needs_you": [] });
    std::fs::create_dir_all(home.join("out/briefs")).unwrap();
    std::fs::write(home.join("out/briefs/2030-04-09.json"), brief.to_string()).unwrap();
    let data = cos::report::data(&home, &home, false).unwrap();
    assert!(data.get("review").is_none());
    assert_eq!(
        data["brief"]["report"]["headline"],
        json!("Saved before the week part.")
    );
    assert_eq!(data["brief"]["report"]["week"]["goals"], json!([]));
    std::fs::remove_dir_all(home).unwrap();
}

/// The data object a rendered page embeds.
fn embedded(html: &str) -> serde_json::Value {
    let at = html.find("const DATA = ").unwrap() + "const DATA = ".len();
    serde_json::Deserializer::from_str(&html[at..])
        .into_iter::<serde_json::Value>()
        .next()
        .unwrap()
        .unwrap()
}

#[test]
fn archived_render_names_its_brief_but_posts_none() {
    let home = std::env::temp_dir().join(format!("cos-brief-ref-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(home.join("out/briefs")).unwrap();
    let file = home.join("out/briefs/2030-04-09.json");
    std::fs::write(
        &file,
        json!({ "headline": "Rendered.", "needs_you": [] }).to_string(),
    )
    .unwrap();
    let mtime = cos::signals::mtime_ms(&std::fs::metadata(&file).unwrap());

    let archived = cos::report::build(&home, &home).unwrap();
    let html = std::fs::read_to_string(archived).unwrap();
    let data = embedded(&html);
    assert_eq!(
        data["briefRef"],
        json!({ "file": "2030-04-09.json", "mtime": mtime })
    );
    assert!(
        data.get("hostBrief").is_none(),
        "archived render has hostBrief"
    );

    let posts: Vec<&str> = html
        .lines()
        .map(str::trim)
        .filter(|l| l.contains("\"cos-report-ready\""))
        .collect();
    assert_eq!(
        posts,
        [
            "tellHost(hostRef ? { type: \"cos-report-ready\", brief: hostRef.file, mtime: hostRef.mtime } : { type: \"cos-report-ready\" });"
        ],
        "the ready message must carry brief only through hostRef"
    );
    assert_eq!(
        html.matches("const hostRef = DATA.hostBrief === true && DATA.briefRef;")
            .count(),
        1,
        "hostRef must be gated on hostBrief"
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn toon_rows_parse_even_with_a_pipe_in_the_title() {
    let out = "[id|type|title|pri|status|claimed|blocked_by|parent|archived]\n\
               t-1|task|Fix a|b split|p1|open||t-9|t-0|false\n\
               t-2|epic|Plain|p0|open||||false\n";
    let tasks = parse_toon("repo", out);
    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].title, "Fix a|b split");
    assert_eq!(tasks[0].priority, "p1");
    assert_eq!(tasks[0].blocked_by, "t-9");
    assert_eq!(tasks[0].parent, "t-0");
    assert_eq!(tasks[1].priority, "p0");
}
