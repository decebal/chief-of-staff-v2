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
