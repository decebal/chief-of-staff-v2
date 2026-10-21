//! The brains' read-only surface: no file tools, `context/` inlined instead,
//! and a child that leaves a pipe open cannot hang a run.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use cos::READ_ONLY;
use cos::claude::{BRAIN_TOOLS, Run, Toolset, command};

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("cos-safety-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn brain(denied: Vec<String>) -> Run<'static> {
    Run {
        agent: "tactician",
        model: "sonnet",
        prompt: "p".into(),
        json_schema: None,
        max_budget_usd: 1.0,
        tools: Toolset::Brain(READ_ONLY.to_vec()),
        deadline: Duration::from_secs(1),
        denied,
    }
}

#[test]
fn brains_get_no_file_or_shell_tools() {
    assert_eq!(BRAIN_TOOLS, "ToolSearch");
    for banned in ["Read", "Glob", "Grep", "Bash", "Write", "Edit"] {
        assert!(
            !READ_ONLY
                .iter()
                .any(|t| *t == banned || t.starts_with(&format!("{banned}("))),
            "{banned} is in READ_ONLY"
        );
    }
    let cmd = command(Path::new("/tmp"), &brain(vec![]));
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(args.windows(2).any(|w| w == ["--tools", "ToolSearch"]));
    assert!(
        args.windows(2)
            .any(|w| w == ["--permission-mode", "dontAsk"])
    );
    assert!(
        !args.iter().any(|a| a == "--disallowedTools"),
        "no deny list configured"
    );
}

#[test]
fn denied_servers_come_from_context() {
    let home = scratch("deny");
    std::fs::create_dir_all(home.join("context")).unwrap();
    std::fs::write(
        home.join("context/deny-mcp.txt"),
        "# unused connectors\nmcp__claude_ai_Looker\n\n  mcp__prime  # local graph\n",
    )
    .unwrap();
    let denied = cos::context::denied_servers(&home);
    assert_eq!(denied, ["mcp__claude_ai_Looker", "mcp__prime"]);

    let cmd = command(&home, &brain(denied));
    let args: Vec<_> = cmd
        .get_args()
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    assert!(
        args.windows(2)
            .any(|w| w == ["--disallowedTools", "mcp__claude_ai_Looker,mcp__prime"])
    );
}

#[test]
fn context_is_inlined_newest_decision_first_and_capped() {
    let home = scratch("context");
    let ctx = home.join("context");
    std::fs::create_dir_all(ctx.join("decisions")).unwrap();
    std::fs::write(ctx.join("goals.md"), "Ship the parser.").unwrap();
    std::fs::write(ctx.join("decisions/2026-01-01-old.md"), "old call").unwrap();
    std::fs::write(ctx.join("decisions/2026-09-01-new.md"), "new call").unwrap();
    std::fs::write(ctx.join("decisions/README.md"), "how to write one").unwrap();

    let block = cos::context::block(&home, &home);
    assert!(block.contains("### context/goals.md\n\nShip the parser."));
    assert!(block.contains("### context/profile.md\n\n(missing"));
    assert!(!block.contains("how to write one"));
    let (new, old) = (
        block.find("new call").unwrap(),
        block.find("old call").unwrap(),
    );
    assert!(new < old, "newest decision first");

    let huge = "y".repeat(cos::context::LIMIT);
    std::fs::write(ctx.join("decisions/2026-10-01-huge.md"), huge).unwrap();
    let block = cos::context::block(&home, &home);
    assert!(block.len() <= cos::context::LIMIT + 512);
    assert!(block.contains("Not inlined") && block.contains("2026-10-01-huge.md"));
}

#[test]
fn a_grandchild_holding_the_pipe_cannot_hang_a_run() {
    let started = Instant::now();
    let out = cos::proc::run(
        Command::new("sh").args(["-c", "/bin/sleep 60 & echo $!"]),
        Duration::from_secs(30),
    )
    .unwrap();
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "{:?}",
        started.elapsed()
    );
    let pid: i32 = out.stdout.trim().parse().unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while cos::proc::alive(pid) {
        assert!(Instant::now() < until, "grandchild {pid} survived");
        std::thread::sleep(Duration::from_millis(50));
    }
}
