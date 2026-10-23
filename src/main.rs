use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use cos::agents::{self, Harness};
use cos::claude::Run;
use cos::mcp::{self, Mode};
use cos::pipeline::Config;

/// Chief of staff: a tactical and a strategic brain over headless Claude Code.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// Repo holding CLAUDE.md, .claude/agents and context/.
    #[arg(long, env = "COS_HOME", default_value_os_t = default_home())]
    home: PathBuf,
    /// Directory holding `.chief-of-staff/` (profile and state), as the kit's
    /// `state.py --workspace`. Default: `--home`.
    #[arg(long, env = "COS_WORKSPACE")]
    workspace: Option<PathBuf>,
    /// Directory scanned for chronis repos.
    #[arg(long, env = "COS_PROJECTS", default_value_os_t = home_dir().join("Projects"))]
    projects: PathBuf,
    /// Spending cap per Claude run, in USD.
    #[arg(long)]
    budget: Option<f64>,
    /// Print the prompt instead of calling Claude.
    #[arg(long)]
    dry_run: bool,
    /// Write the HTML report but do not open it.
    #[arg(long)]
    no_open: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

impl Cli {
    fn ws(&self) -> PathBuf {
        self.workspace.clone().unwrap_or_else(|| self.home.clone())
    }
}

#[derive(Subcommand)]
enum Cmd {
    /// Tactician: today's calendar, inbox, Jira, chronis and PRs → one brief.
    Brief,
    /// Strategist: this week against your goals and bets.
    Review,
    /// Ask the chief of staff; it routes to one or both brains and merges.
    Ask { question: Vec<String> },
    /// Rebuild the HTML report from saved runs and open it. Free; calls nothing.
    Report,
    /// Check that unattended runs can log in: which credential they use, and a
    /// one-word Claude call to prove it (about $0.01).
    Auth,
    /// Serve the dashboard live on localhost; an open tab reloads after every run.
    Dash {
        #[arg(long, default_value_t = 8790)]
        port: u16,
    },
    /// MCP server on stdio, for Claude Desktop and Codex. All tools.
    Mcp,
    /// MCP server over streamable HTTP at /mcp/<token>. Read-only tools only.
    Serve {
        #[arg(long, default_value = "127.0.0.1:8787")]
        addr: String,
        /// Path secret; the URL is the credential, so keep it long and private.
        #[arg(long, env = "COS_MCP_TOKEN", hide_env_values = true)]
        token: String,
        /// Public hostname in front of the server (a tunnel); its Host header is accepted.
        #[arg(long, env = "COS_PUBLIC_HOST")]
        public_host: Option<String>,
    },
    /// Read-only MCP server behind a Cloudflare quick tunnel, for claude.ai.
    /// Prints the connector URL; Ctrl-C stops both.
    Tunnel {
        #[arg(long, default_value_t = 8787)]
        port: u16,
    },
    /// Spin off coding agents on a brief's or review's prompts, and watch them.
    Agents {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
    /// The private workspace state in `<workspace>/.chief-of-staff/`: the same
    /// files and rules as the starter kit's `state.py`.
    State {
        #[command(subcommand)]
        cmd: StateCmd,
    },
    /// "Delegate this": print the handoff for a tracked deliverable. Starts
    /// nothing; `cos agents spawn --deliverable` runs it in a worktree.
    Handoff { id: String },
}

#[derive(Subcommand)]
enum StateCmd {
    /// Create `.chief-of-staff/` with a blank profile and empty state. Never overwrites.
    Init,
    /// Print the validated state.
    Read,
    /// Validate a candidate state file.
    Check {
        #[arg(long)]
        file: PathBuf,
    },
    /// Commit a candidate: lock, revision check, transition rules, journal entry.
    Commit {
        #[arg(long)]
        candidate: PathBuf,
        #[arg(long)]
        expected_revision: i64,
        #[arg(long)]
        reason: String,
    },
    /// Write the latest review's proposed deliverables into
    /// `.chief-of-staff/candidate.json` for you to read and commit.
    Propose,
}

#[derive(Subcommand)]
enum AgentCmd {
    /// Start an agent in the background; prints its id. `--budget` caps a
    /// Claude agent's spend (default $5; Codex has no budget flag).
    Spawn {
        /// Take the prompt from the latest saved brief (actions) or review (goals).
        #[arg(long, value_enum, conflicts_with_all = ["prompt", "deliverable"])]
        from: Option<agents::From>,
        /// Hand off a tracked deliverable from `.chief-of-staff/state.json`.
        #[arg(long, conflicts_with = "prompt")]
        deliverable: Option<String>,
        /// Which item, numbered as the report numbers it.
        #[arg(long, requires = "from")]
        item: Option<usize>,
        /// Every item with a prompt, one agent each.
        #[arg(long, requires = "from", conflicts_with = "item")]
        all: bool,
        /// A prompt of your own instead.
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long, value_enum, default_value_t = Harness::Claude)]
        harness: Harness,
        /// Repo to work in; default: the first ~/Projects/<repo> the prompt names.
        #[arg(long)]
        repo: Option<PathBuf>,
        /// Work in the repo's own checkout instead of a detached worktree.
        #[arg(long)]
        in_place: bool,
        #[arg(long)]
        model: Option<String>,
        /// Seconds before the agent is stopped; capped at 7200. Default:
        /// `context/agents.json`, else 1800.
        #[arg(long)]
        deadline: Option<u64>,
        /// A command that must exit 0 in the agent's workdir once it ends;
        /// repeatable. cos runs it itself to judge the agent.
        #[arg(long = "check")]
        checks: Vec<String>,
        /// A path the agent may change, relative to the repo; repeatable.
        /// Changes elsewhere make the verdict `needs_you`.
        #[arg(long)]
        scope: Vec<String>,
        /// The finish line in words; a deliverable brings its own `done_when`.
        #[arg(long)]
        done_when: Option<String>,
    },
    /// Judge an ended agent again: its diff, its checks, its scope.
    Judge { id: String },
    /// How agents did, by harness, model and repo, from every verdict.
    Scorecard,
    /// Every agent: status, tool calls, cost, latest activity.
    Ls,
    /// The table, redrawn every 2 s until no agent is running.
    Watch,
    /// One agent in full, with the command to take it over interactively.
    Show { id: String },
    /// Its activity, one line per tool call or message.
    Tail {
        id: String,
        /// Keep following until it ends.
        #[arg(short, long)]
        follow: bool,
    },
    /// Stop an agent and its whole process group.
    Kill { id: String },
    /// Remove finished agents' directories and worktrees. Refuses one whose
    /// worktree holds work, unless --force.
    Clean {
        id: Option<String>,
        #[arg(long)]
        force: bool,
    },
    #[command(hide = true)]
    Supervise { id: String },
    /// The build-tool shim: wait for the machine-wide build lock, then exec.
    #[command(hide = true)]
    BuildSlot {
        #[arg(long)]
        lock: PathBuf,
        program: PathBuf,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
}

fn main() -> Result<()> {
    let auth = cos::auth::load();
    let cli = Cli::parse();
    let result = run(&cli, auth);
    // launchd runs with stderr redirected to a file; a failure there must
    // still reach the user the morning it happens.
    if let Err(e) = &result {
        use std::io::IsTerminal;
        if !std::io::stderr().is_terminal() && matches!(cli.cmd, Cmd::Brief | Cmd::Review) {
            let what = if matches!(cli.cmd, Cmd::Brief) {
                "brief"
            } else {
                "review"
            };
            cos::auth::notify(&format!("cos {what} failed"), &format!("{e:#}"));
        }
    }
    result
}

fn run(cli: &Cli, auth: cos::auth::Source) -> Result<()> {
    let ws = cli.ws();
    let cfg = Config {
        home: cli.home.clone(),
        workspace: ws.clone(),
        projects: cli.projects.clone(),
        budget: cli.budget,
    };
    match &cli.cmd {
        Cmd::Brief if cli.dry_run => print_run(&cfg.brief_run(&cfg.gather()?)),
        Cmd::Review if cli.dry_run => print_run(&cfg.review_run(&cfg.gather()?)),
        Cmd::Ask { question } if cli.dry_run => print_run(&Config::route_run(&question.join(" "))),
        Cmd::Brief => {
            let done = cfg.brief()?;
            finish(&done.markdown, done.cost_usd, done.saved.as_deref())?;
            tracked(done.tracked.as_ref());
            show_report(cli)
        }
        Cmd::Review => {
            let done = cfg.review()?;
            finish(&done.markdown, done.cost_usd, done.saved.as_deref())?;
            tracked(done.tracked.as_ref());
            if !done.value.proposed.is_empty() {
                eprintln!("proposals: `cos state propose` writes them as a candidate to review");
            }
            show_report(cli)
        }
        Cmd::Auth => auth_check(&cli.home, auth),
        Cmd::State { cmd } => state_cmd(&cli.home, &ws, cmd),
        Cmd::Handoff { id } => {
            let (st, profile) = workspace(&ws)?;
            print!(
                "{}",
                cos::handoff::deliverable(&st, profile.as_ref(), id, cos::handoff::Receiver::Chat)?
            );
            Ok(())
        }
        Cmd::Report => {
            fold_latest_run(&cli.home)?;
            cos::report::build(&cli.home, &ws)?;
            show_report(cli)
        }
        Cmd::Dash { port } => {
            if !cli.no_open {
                let url = format!("http://127.0.0.1:{port}/");
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    let _ = cos::report::open(std::path::Path::new(&url));
                });
            }
            runtime()?.block_on(cos::dash::serve(cli.home.clone(), ws, *port))
        }
        Cmd::Ask { question } => {
            let done = cfg.ask(&question.join(" "))?;
            finish(&done.markdown, done.cost_usd, None)
        }
        Cmd::Mcp => runtime()?.block_on(mcp::stdio(cfg, Mode::Local)),
        Cmd::Serve {
            addr,
            token,
            public_host,
        } => runtime()?.block_on(mcp::http(cfg, addr, token, public_host.clone())),
        Cmd::Tunnel { port } => runtime()?.block_on(cos::tunnel::run(cfg, *port)),
        Cmd::Agents { cmd } => agents_cmd(cli, cmd),
    }
}

fn agents_cmd(cli: &Cli, cmd: &AgentCmd) -> Result<()> {
    let home = &cli.home;
    match cmd {
        AgentCmd::Spawn {
            from,
            deliverable,
            item,
            all,
            prompt,
            harness,
            repo,
            in_place,
            model,
            deadline,
            checks,
            scope,
            done_when,
        } => {
            let mut done_when = done_when.clone();
            let picked: Vec<agents::Task> = match (from, prompt) {
                _ if deliverable.is_some() => {
                    let id = deliverable.as_deref().unwrap_or_default();
                    let (st, profile) = workspace(&cli.ws())?;
                    let prompt = cos::handoff::deliverable(
                        &st,
                        profile.as_ref(),
                        id,
                        cos::handoff::Receiver::Agent,
                    )?;
                    let found = cos::state::find(&st, "deliverables", id);
                    let title = found
                        .and_then(|d| d["title"].as_str())
                        .unwrap_or(id)
                        .to_owned();
                    if done_when.is_none() {
                        done_when = found
                            .and_then(|d| d["done_when"].as_str())
                            .filter(|s| !s.trim().is_empty())
                            .map(str::to_owned);
                    }
                    vec![agents::Task {
                        n: 0,
                        title,
                        prompt,
                        source: format!("deliverable {id}"),
                    }]
                }
                (Some(from), _) => {
                    let tasks = agents::tasks(home, *from)?;
                    match item {
                        _ if *all => tasks,
                        Some(n) => vec![
                            tasks
                                .into_iter()
                                .find(|t| t.n == *n)
                                .with_context(|| format!("item {n} has no agent prompt"))?,
                        ],
                        None => {
                            for t in &tasks {
                                println!("{:>2}  {}", t.n, t.title);
                            }
                            anyhow::bail!("pick one with --item <n>, or --all");
                        }
                    }
                }
                (None, Some(p)) => vec![agents::Task {
                    n: 0,
                    title: p.lines().next().unwrap_or("").chars().take(80).collect(),
                    prompt: p.clone(),
                    source: "prompt".into(),
                }],
                (None, None) => {
                    anyhow::bail!("give --from brief|review, --deliverable <id> or --prompt")
                }
            };
            let cos = std::env::current_exe().context("locate the cos binary")?;
            if let Some(note) = agents::policy::load(home).note {
                eprintln!("{note}; hiring on the built-in defaults");
            }
            for t in picked {
                let repo = repo
                    .clone()
                    .or_else(|| agents::infer_repo(&t.prompt, &cli.projects));
                let spec = agents::spawn(
                    home,
                    agents::Request {
                        harness: *harness,
                        program: None,
                        model: model.clone(),
                        worktree: !in_place,
                        repo,
                        title: t.title,
                        source: t.source,
                        prompt: t.prompt,
                        deadline: Duration::from_secs(deadline.unwrap_or(1800)),
                        budget_usd: cli.budget.unwrap_or(5.0),
                        slots: None,
                        contract: agents::Contract {
                            done_when: done_when.clone(),
                            checks: checks.clone(),
                            scope: scope.clone(),
                        },
                        explicit: agents::policy::Explicit {
                            deadline: deadline.is_some(),
                            budget: cli.budget.is_some(),
                        },
                    },
                    &cos,
                )?;
                println!("{}  {}  {}", spec.id, spec.workdir.display(), spec.title);
            }
            eprintln!("watch: cos agents watch   ·   live page: cos dash → /agents");
            Ok(())
        }
        AgentCmd::Ls => {
            print!("{}", agents::view::table(&agents::list(home)?));
            Ok(())
        }
        AgentCmd::Watch => loop {
            let list = agents::list(home)?;
            print!("\x1b[2J\x1b[H{}", agents::view::table(&list));
            if list.iter().all(|(_, s)| s.status.terminal()) {
                return Ok(());
            }
            std::thread::sleep(Duration::from_secs(2));
        },
        AgentCmd::Show { id } => {
            let (spec, state) = agents::read(home, id)?;
            print!("{}", agents::view::show(&spec, &state));
            Ok(())
        }
        AgentCmd::Tail { id, follow } => {
            let (spec, _) = agents::read(home, id)?;
            let events = agents::events_path(home, id);
            let mut at = 0;
            loop {
                let (lines, next) = agents::view::activity(&events, spec.harness, at);
                for l in lines {
                    println!("{l}");
                }
                at = next;
                let (_, state) = agents::read(home, id)?;
                if !follow || state.status.terminal() {
                    if *follow {
                        println!("— {}", state.status);
                    }
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        AgentCmd::Scorecard => {
            let scores = agents::ledger::scorecard(&agents::ledger::read(home));
            print!("{}", agents::view::scorecard(&scores));
            Ok(())
        }
        AgentCmd::Judge { id } => {
            agents::rejudge(home, id)?;
            let (spec, state) = agents::read(home, id)?;
            print!("{}", agents::view::show(&spec, &state));
            Ok(())
        }
        AgentCmd::Kill { id } => {
            let state = agents::stop(home, id, Duration::from_secs(10))?;
            println!("{id} {}", state.status);
            Ok(())
        }
        AgentCmd::Clean { id, force } => {
            let ids: Vec<String> = match id {
                Some(id) => vec![id.clone()],
                None => agents::list(home)?
                    .into_iter()
                    .filter(|(_, s)| s.status.terminal())
                    .map(|(spec, _)| spec.id)
                    .collect(),
            };
            let mut refused = 0;
            for id in ids {
                match agents::clean(home, &id, *force) {
                    Ok(()) => println!("removed {id}"),
                    Err(e) => {
                        refused += 1;
                        eprintln!("kept {id}: {e:#}");
                    }
                }
            }
            anyhow::ensure!(refused == 0, "{refused} agent(s) kept");
            Ok(())
        }
        AgentCmd::Supervise { id } => agents::supervise(home, id),
        AgentCmd::BuildSlot {
            lock,
            program,
            args,
        } => agents::slots::exec_holding_build_slot(lock, program, args),
    }
}

/// `cos state …`. Prints JSON like the kit's `state.py`, and stops with
/// `Stopped: <reason>` and exit 1 on any refusal.
fn state_cmd(home: &std::path::Path, ws: &std::path::Path, cmd: &StateCmd) -> Result<()> {
    use cos::state;
    let run = || -> Result<serde_json::Value> {
        let ws = std::path::absolute(ws)?;
        match cmd {
            StateCmd::Init => state::initialize(&ws),
            StateCmd::Read => {
                state::read(&ws)?.context("no .chief-of-staff/state.json; run `cos state init`")
            }
            StateCmd::Check { file } => {
                state::validate(&state::load(file)?)?;
                Ok(serde_json::json!({ "valid": true }))
            }
            StateCmd::Commit {
                candidate,
                expected_revision,
                reason,
            } => state::commit(&ws, &state::load(candidate)?, *expected_revision, reason),
            StateCmd::Propose => {
                let r =
                    cos::store::latest::<cos::reports::Review>(home, cos::store::Kind::Reviews)?
                        .context("no saved review; run `cos review`")?;
                let c = cos::track::proposal_candidate(
                    &ws,
                    &r.value.proposed,
                    &format!("cos review {}", r.day),
                )?;
                Ok(serde_json::json!({
                    "candidate": c.path,
                    "added": c.added,
                    "skippedNoTrackedGoal": c.skipped,
                    "commit": format!(
                        "cos state commit --candidate {} --expected-revision {} --reason \"Proposed deliverables from the {} review\"",
                        c.path.display(), c.revision, r.day
                    ),
                }))
            }
        }
    };
    match run() {
        Ok(v) => {
            println!("{}", serde_json::to_string_pretty(&v)?);
            Ok(())
        }
        Err(e) => {
            eprintln!("Stopped: {e:#}");
            std::process::exit(1);
        }
    }
}

/// The validated state and the profile, or why there are none.
fn workspace(home: &std::path::Path) -> Result<(serde_json::Value, Option<serde_json::Value>)> {
    let st = cos::state::read(home)?
        .context("no .chief-of-staff/state.json; run `cos state init`, then set up goals")?;
    Ok((st, cos::state::profile(home)))
}

fn tracked(r: Option<&cos::track::Recorded>) {
    if let Some(r) = r {
        eprintln!("{r}");
    }
}

/// Folds the newest saved facts (and that day's brief) into the archive;
/// re-recording a day replaces its row, so this is safe to repeat.
fn fold_latest_run(home: &std::path::Path) -> Result<()> {
    use cos::store::{Kind, latest};
    let Some(facts) = latest::<cos::gather::Facts>(home, Kind::Facts)? else {
        return Ok(());
    };
    let brief = latest::<cos::reports::Brief>(home, Kind::Briefs)?.filter(|b| b.day == facts.day);
    cos::history::record(
        home,
        &facts.day,
        &facts.value,
        brief.as_ref().map(|b| &b.value),
    )?;
    Ok(())
}

/// Proves the credential an unattended run would use: a one-word call with
/// the same project settings a brief uses.
fn auth_check(home: &std::path::Path, source: cos::auth::Source) -> Result<()> {
    println!("credential: {}", source.describe());
    let mut cmd = std::process::Command::new("claude");
    cmd.current_dir(home).args([
        "-p",
        "Reply with ok",
        "--output-format",
        "json",
        "--tools",
        "",
        "--strict-mcp-config",
        "--setting-sources",
        "project",
        "--no-session-persistence",
        "--model",
        "haiku",
        "--max-budget-usd",
        "0.1",
    ]);
    let out = cos::proc::run(&mut cmd, std::time::Duration::from_secs(60))?;
    let outcome: cos::claude::Outcome = serde_json::from_str(out.stdout.trim())
        .with_context(|| format!("claude gave no result: {}", out.stderr.trim()))?;
    if outcome.is_error {
        anyhow::bail!("{}", cos::claude::error_reason(&outcome));
    }
    println!("login works (cost ${:.3})", outcome.total_cost_usd);
    if source == cos::auth::Source::Interactive {
        println!(
            "warning: scheduled runs fail when this login expires. Fix: `claude setup-token`, then\n  \
             security add-generic-password -s {} -a \"$USER\" -w",
            cos::auth::KEYCHAIN_SERVICE
        );
    }
    Ok(())
}

fn show_report(cli: &Cli) -> Result<()> {
    let path = cli.home.join("out/report.html");
    eprintln!("report {}", path.display());
    if cli.no_open {
        return Ok(());
    }
    cos::report::open(&path)
}

fn finish(markdown: &str, cost: f64, saved: Option<&std::path::Path>) -> Result<()> {
    println!("{markdown}");
    eprintln!("cost ${cost:.2}");
    if let Some(p) = saved {
        eprintln!("wrote {}", p.display());
    }
    Ok(())
}

fn print_run(run: &Run) -> Result<()> {
    println!(
        "--agent {} --model {}\n\n{}",
        run.agent, run.model, run.prompt
    );
    Ok(())
}

fn runtime() -> Result<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("start tokio runtime")
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}

/// The clone `cos` was built from: `task install` builds in place, so an
/// installed binary finds its agents and context with no configuration.
fn default_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}
