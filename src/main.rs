use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use cos::agents::{self, Harness};
use cos::business::{self, Business, Dirs, Inputs, Sources, Triple};
use cos::claude::Run;
use cos::mcp::{self, Mode};
use cos::pipeline::Config;

/// Chief of staff: one daily brief over headless Claude Code, from the goals
/// down to today.
#[derive(Parser)]
#[command(version)]
struct Cli {
    /// The registered business to run on: its home, workspace and projects together.
    /// Env: COS_BUSINESS.
    #[arg(long)]
    business: Option<String>,
    /// Repo holding CLAUDE.md, .claude/agents and context/. Env: COS_HOME.
    #[arg(long)]
    home: Option<PathBuf>,
    /// Directory holding `.chief-of-staff/` (profile and state), as the kit's
    /// `state.py --workspace`. Default: `--home`. Env: COS_WORKSPACE.
    #[arg(long)]
    workspace: Option<PathBuf>,
    /// Directory scanned for chronis repos. Env: COS_PROJECTS.
    #[arg(long)]
    projects: Option<PathBuf>,
    /// Spending cap per Claude run of the brains (brief, ask), in USD; hires
    /// in Herdr have none.
    #[arg(long)]
    budget: Option<f64>,
    /// Print the prompt instead of calling Claude; for `agents spawn`, the
    /// hire's plan (repo, fit, contract) instead of starting it.
    #[arg(long)]
    dry_run: bool,
    /// Write the HTML report but do not open it.
    #[arg(long, global = true)]
    no_open: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Tactician: goals, this week's deliverables, then today's calendar,
    /// inbox, Jira, chronis and PRs → one brief.
    Brief,
    /// Ask the chief of staff a question; the tactician answers it.
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
    /// The documentation room: repo docs and Notion pages as a graph you organise.
    Docs {
        #[command(subcommand)]
        cmd: Option<DocsCmd>,
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
    /// Spin off coding agents on a brief's prompts, and watch them.
    Agents {
        #[command(subcommand)]
        cmd: AgentCmd,
    },
    /// The staff agents are hired into: list, propose from the library, add,
    /// edit, retire and restore roles in context/agents.json.
    Roles {
        #[command(subcommand)]
        cmd: RolesCmd,
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
    /// Refresh a staff role from an enrolled AllStack coordinator. Starts no work.
    StaffSync {
        #[arg(long)]
        coordinator: PathBuf,
        #[arg(long)]
        run: String,
        #[arg(long)]
        role: String,
    },
    /// Every ritual flow under --projects: one Chronis epic each, with its
    /// step. Read-only; needs an approved `tasks` source in the profile.
    Flows {
        #[arg(long)]
        json: bool,
        /// The chronis binary.
        #[arg(long, env = "COS_CN", default_value = "cn")]
        cn: PathBuf,
    },
    /// The ritual's gate: a person's approval of a design, on the flow's epic.
    Ritual {
        #[command(subcommand)]
        cmd: RitualCmd,
    },
    /// The registered businesses, each with its home, workspace and projects.
    Business {
        #[command(subcommand)]
        cmd: BusinessCmd,
    },
    /// Run a program in this process's group and end the group at --secs,
    /// even if whoever started the guard is gone.
    #[command(hide = true)]
    ProcGuard {
        #[arg(long)]
        secs: u64,
        program: std::ffi::OsString,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<std::ffi::OsString>,
    },
    /// Independent design review of a commitment's draft, then your approval.
    Review {
        #[command(subcommand)]
        cmd: ReviewCmd,
    },
}

#[derive(Subcommand)]
enum ReviewCmd {
    /// Attach a design draft, inside a Git checkout, to a commitment.
    Bind {
        commitment: String,
        /// The Git checkout root holding the draft.
        #[arg(long)]
        repo: PathBuf,
        #[arg(long)]
        draft: PathBuf,
        /// Who implements the design once approved.
        #[arg(long)]
        owner: String,
    },
    /// Pin the reviewer program by its sha256. Asks for your login through
    /// the OS; without --sha256 it prints the digest to pin.
    Enroll {
        commitment: String,
        #[arg(long)]
        reviewer: PathBuf,
        #[arg(long)]
        sha256: Option<String>,
        /// Retry an earlier request with its action ID.
        #[arg(long)]
        action_id: Option<String>,
    },
    /// Run the pinned reviewer over the draft: five fresh networkless processes.
    Run {
        commitment: String,
        #[arg(long)]
        action_id: Option<String>,
    },
    /// The bound draft, its digests, each finding by index, and what blocks approval.
    Show { commitment: String },
    /// Record how a finding is resolved.
    Resolve {
        commitment: String,
        index: usize,
        /// The review digest `cos review show` printed.
        #[arg(long)]
        review: String,
        #[arg(long)]
        action_id: Option<String>,
        resolution: Vec<String>,
    },
    /// Approve the reviewed draft. Asks for your login through the OS.
    Approve {
        commitment: String,
        /// The draft digest `cos review show` printed.
        #[arg(long)]
        digest: String,
        #[arg(long)]
        action_id: Option<String>,
    },
}

#[derive(Subcommand)]
enum BusinessCmd {
    /// One line per business: `*` on the active one, directory and profile status,
    /// then the triple this invocation runs on.
    List,
    /// Register a business. Its three directories must exist. A new home gets
    /// `context/` from the install's templates.
    Add {
        name: String,
        home_dir: PathBuf,
        workspace_dir: PathBuf,
        projects_dir: PathBuf,
        /// Overwrite the business of this name.
        #[arg(long)]
        replace: bool,
    },
    /// Make a business the active one.
    Use { name: String },
    /// Print a business's home, and exit 0 only when its directories exist.
    /// Default: the business this invocation runs on.
    Home { name: Option<String> },
    /// Unregister a business. Refuses the active one; deletes no directory.
    Remove { name: String },
    /// Each dashboard in `dashboards.toml` (`COS_DASHBOARDS`, else beside the registry),
    /// its server, and whether its URL answers its probe now. Starts and writes nothing.
    Dashboards { name: Option<String> },
}

#[derive(Subcommand)]
enum RitualCmd {
    /// Exit 0 when the epic is approved and its design is unchanged since.
    Check {
        epic: String,
        /// The repo holding the epic's `.chronis/` store. Default: the main
        /// checkout of the current directory.
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long, env = "COS_CN", default_value = "cn")]
        cn: PathBuf,
    },
    /// Record your approval of a design: its sha256 on the epic, then `cn approve`.
    /// Asks on the terminal and refuses without one; `--yes` records it with no prompt.
    Approve {
        epic: String,
        /// The design, relative to the repo.
        design: String,
        /// Your name, as recorded on the epic.
        #[arg(long)]
        by: String,
        /// A review file whose front matter names this design, its current
        /// sha256, and `verdict: clean`.
        #[arg(long, conflicts_with = "without_review")]
        review: Option<PathBuf>,
        /// Approve with no review; the epic records that.
        #[arg(long)]
        without_review: bool,
        /// Skip the terminal prompt. Only when the user has told you, in this
        /// conversation, to approve this design; never on your own initiative.
        #[arg(long)]
        yes: bool,
        /// The repo holding the epic's `.chronis/` store. Default: the main
        /// checkout of the current directory.
        #[arg(long)]
        repo: Option<PathBuf>,
        #[arg(long, env = "COS_CN", default_value = "cn")]
        cn: PathBuf,
    },
}

#[derive(Subcommand)]
enum RolesCmd {
    /// The staff (or, with --library, the shipped specialist definitions).
    List {
        #[arg(long)]
        library: bool,
        /// Include retired roles.
        #[arg(long)]
        retired: bool,
        #[arg(long)]
        json: bool,
    },
    /// One role (or library entry) as JSON, ready to edit and pass to `edit --file`.
    Show {
        id: String,
        #[arg(long)]
        library: bool,
    },
    /// Specialise a library entry to this business (a paid run, no tools).
    /// Writes only --out; add it with `new --file`. With --dry-run, prints
    /// exactly what would be sent.
    Propose {
        library_id: String,
        /// The id the role gets on the staff; default the entry's.
        #[arg(long)]
        id: Option<String>,
        /// What to focus on, in your words (at most 1,000 characters).
        #[arg(long)]
        brief: Option<String>,
        /// A new file to save the proposed role to.
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        json: bool,
    },
    /// Add a role: a library entry as it is, a role file, or a blank role.
    New {
        library_id: Option<String>,
        #[arg(long, conflicts_with_all = ["library_id", "blank"])]
        file: Option<PathBuf>,
        #[arg(long, conflicts_with = "library_id", requires_all = ["title", "mandate"])]
        blank: Option<String>,
        #[arg(long)]
        title: Option<String>,
        #[arg(long)]
        mandate: Option<String>,
        /// The id for a library entry's role; default the entry's.
        #[arg(long, requires = "library_id")]
        id: Option<String>,
        /// Write without the terminal prompt. A contract change still needs a
        /// person typing `contract` at a terminal.
        #[arg(long)]
        yes: bool,
    },
    /// Replace a role with the one in a file (see `show`).
    Edit {
        id: String,
        #[arg(long)]
        file: PathBuf,
        #[arg(long)]
        yes: bool,
    },
    /// Keep a role for the record but stop hiring it.
    Retire {
        id: String,
        #[arg(long)]
        yes: bool,
    },
    /// Hire a retired role again.
    Restore {
        id: String,
        #[arg(long)]
        yes: bool,
    },
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
    /// Write the latest brief's proposed deliverables for the week into
    /// `.chief-of-staff/candidate.json` for you to read and commit.
    Propose,
}

#[derive(Subcommand)]
enum DocsCmd {
    /// List the Notion pages about the monitored projects (a paid run with your
    /// Notion connector) into out/docs/notion.json.
    Notion,
    /// Print the room's inventory as JSON: documents, Notion pages, your topics.
    Json,
}

#[derive(Subcommand)]
enum AgentCmd {
    /// Hire an agent into a Herdr terminal (`cos-<id>`); prints its id once
    /// Herdr has it. Refused when herdr-core is not running.
    Spawn {
        /// Take the prompt from the latest saved brief: its actions, or its week's goals.
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
        /// The finish line in words; a deliverable brings its own `doneWhen`.
        #[arg(long)]
        done_when: Option<String>,
        /// A staff role from `context/agents.json` (builder, reviewer, …): its
        /// contract and mandate fill what the other flags leave empty.
        #[arg(long)]
        role: Option<String>,
        /// The program the terminal runs instead of the harness's own
        /// (a stand-in agent for a proof that costs nothing).
        #[arg(long, hide = true)]
        program: Option<String>,
    },
    /// Send a message into a run's Herdr terminal (`hc send`).
    Say { id: String, message: Vec<String> },
    /// Press the agent's stop key (`hc interrupt`): its turn ends, it stays.
    Pause { id: String },
    /// End a run's Herdr terminal (`hc kill`) and judge it, once.
    Done { id: String },
    /// Show a run's terminal in the Herdr window.
    Open { id: String },
    /// A run's terminal screen and recent history (`hc read`).
    Screen {
        id: String,
        #[arg(long, default_value_t = 80)]
        lines: usize,
    },
    /// Judge an agent again: its diff, its checks, its scope.
    Judge { id: String },
    /// Record what you found when you ran what a judged agent drafted (a
    /// tester's script). Stored on the run; nothing is sent.
    Record {
        id: String,
        #[arg(long, value_enum)]
        outcome: agents::Tested,
        #[arg(long, default_value = "")]
        notes: String,
        /// A link or path that shows it; repeatable.
        #[arg(long)]
        evidence: Vec<String>,
    },
    /// How agents did, by harness, model and repo, from every verdict.
    Scorecard,
    /// Every agent: its status, what Herdr says, its verdict.
    Ls,
    /// Follow every run in Herdr, redrawing the table every 2 s: stops runs
    /// at their deadline and judges each once, until none is live.
    Watch {
        /// Emit one OSC 7501 status record per agent instead of the table, so
        /// the Herdr pane it runs in shows every hire.
        #[arg(long)]
        osc: bool,
    },
    /// One agent in full, with how to reach its Herdr terminal.
    Show { id: String },
    /// Stop an agent: its Herdr terminal is ended, or its preparation.
    Kill { id: String },
    /// Remove finished agents' directories and worktrees. Refuses one whose
    /// worktree holds work, unless --force.
    Clean {
        id: Option<String>,
        #[arg(long)]
        force: bool,
    },
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

/// `EX_TEMPFAIL`: the run was put off (dark wake), not failed; nothing spent.
const DEFERRED_EXIT: i32 = 75;

fn main() -> Result<()> {
    let cli = Cli::parse();
    let auth = if matches!(&cli.cmd, Cmd::Dash { .. }) {
        cos::auth::Source::Interactive
    } else {
        cos::auth::load()
    };
    let result = run(&cli, auth);
    if let Err(e) = &result
        && e.is::<cos::pipeline::Deferred>()
    {
        eprintln!("{e}");
        std::process::exit(DEFERRED_EXIT);
    }
    // launchd runs with stderr redirected to a file; a failure there must
    // still reach the user the morning it happens.
    if let Err(e) = &result {
        use std::io::IsTerminal;
        if !std::io::stderr().is_terminal() && matches!(cli.cmd, Cmd::Brief) {
            cos::auth::notify("cos brief failed", &format!("{e:#}"));
        }
    }
    result
}

fn run(cli: &Cli, auth: cos::auth::Source) -> Result<()> {
    match &cli.cmd {
        Cmd::Ritual { cmd } => ritual_cmd(cmd),
        Cmd::Business { cmd } => business_cmd(cli, cmd),
        Cmd::ProcGuard {
            secs,
            program,
            args,
        } => {
            let code = cos::proc::guard(program, args, Duration::from_secs(*secs))?;
            std::process::exit(code)
        }
        // This takes its paths as arguments and never resolves a triple.
        Cmd::Agents {
            cmd:
                AgentCmd::BuildSlot {
                    lock,
                    program,
                    args,
                },
        } => agents::slots::exec_holding_build_slot(lock, program, args),
        cmd => {
            let t = resolve_triple(cli)?;
            if matches!(
                cmd,
                Cmd::Dash { .. } | Cmd::Mcp | Cmd::Serve { .. } | Cmd::Tunnel { .. }
            ) {
                eprintln!("business: {} {t}", t.label());
            }
            run_on(cli, &t, auth, cmd)
        }
    }
}

fn run_on(cli: &Cli, t: &Triple, auth: cos::auth::Source, cmd: &Cmd) -> Result<()> {
    let ws = t.workspace.clone();
    let cfg = Config {
        home: t.home.clone(),
        install: install_dir(),
        workspace: ws.clone(),
        projects: t.projects.clone(),
        budget: cli.budget,
        bins: Default::default(),
    };
    match cmd {
        Cmd::Brief if cli.dry_run => print_run(&cfg.brief_run(&cfg.gather()?)),
        Cmd::Ask { question } if cli.dry_run => print_run(&cfg.ask_run(&question.join(" "))),
        Cmd::Brief => {
            let done = cfg.brief()?;
            finish(&done.markdown, done.cost_usd, done.saved.as_deref())?;
            tracked(done.tracked.as_ref());
            if !done.value.week.proposed.is_empty() {
                eprintln!("proposals: `cos state propose` writes them as a candidate to review");
            }
            show_report(&t.home, cli.no_open)
        }
        Cmd::Auth => auth_check(&t.home, auth),
        Cmd::State { cmd } => state_cmd(&t.home, &ws, cmd),
        Cmd::StaffSync {
            coordinator,
            run,
            role,
        } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&cos::staff::sync(&ws, coordinator, run, role)?)?
            );
            Ok(())
        }
        Cmd::Handoff { id } => {
            let (st, profile) = workspace(&ws)?;
            print!(
                "{}",
                cos::handoff::deliverable(&st, profile.as_ref(), id, cos::handoff::Receiver::Chat)?
            );
            Ok(())
        }
        Cmd::Report => {
            fold_latest_run(&t.home)?;
            cos::report::build(&t.home, &ws)?;
            show_report(&t.home, cli.no_open)
        }
        Cmd::Dash { port } => {
            if !cli.no_open {
                let url = format!("http://127.0.0.1:{port}/");
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(400));
                    let _ = cos::report::open(std::path::Path::new(&url));
                });
            }
            runtime()?.block_on(cos::dash::serve(
                t.home.clone(),
                ws,
                t.projects.clone(),
                *port,
            ))
        }
        Cmd::Docs { cmd } => match cmd {
            Some(DocsCmd::Notion) => {
                let (list, cost) = cfg.notion_pages()?;
                cos::docs::save_notion(&t.home, &list)?;
                eprintln!(
                    "cost ${cost:.2}\nlisted {} Notion pages into {}",
                    list.pages.len(),
                    cos::docs::notion_path(&t.home).display()
                );
                Ok(())
            }
            Some(DocsCmd::Json) => {
                let data = cos::docs::library(&t.home, &ws, &t.projects)?;
                println!("{}", serde_json::to_string_pretty(&data)?);
                Ok(())
            }
            None => {
                let url = "http://127.0.0.1:8790/docs";
                eprintln!(
                    "the room is served by `cos dash` at {url}, and in the desktop app's Docs tab"
                );
                if !cli.no_open {
                    cos::report::open(std::path::Path::new(url))?;
                }
                Ok(())
            }
        },
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
        Cmd::Agents { cmd } => agents_cmd(cli, t, cmd),
        Cmd::Roles { cmd } => roles_cmd(cli, t, &cfg, cmd),
        Cmd::Flows { json, cn } => flows_cmd(&ws, &t.projects, &t.home, cn, *json),
        Cmd::Review { cmd } => review_cmd(&ws, cmd),
        Cmd::Ritual { .. } => unreachable!("run dispatches ritual before a triple resolves"),
        Cmd::Business { .. } => unreachable!("run dispatches business before a triple resolves"),
        Cmd::ProcGuard { .. } => unreachable!("run dispatches proc-guard before a triple resolves"),
    }
}

/// `cos business`. Dispatched before a triple resolves, so a corrupt registry fails only the
/// commands that read it.
fn business_cmd(cli: &Cli, cmd: &BusinessCmd) -> Result<()> {
    let path = business::location()?;
    if let BusinessCmd::Dashboards { name } = cmd {
        return business_dashboards(&path, name.as_deref());
    }
    let inputs = inputs(cli);
    let default = business::builtin(&inputs)?;
    match cmd {
        BusinessCmd::List => {
            announce_first_run(business::first_run(&path, &default)?, &path, &default);
            let registry = business::load(&path)?;
            for (name, b) in &registry.businesses {
                let star = if registry.active.as_deref() == Some(name) {
                    "*"
                } else {
                    " "
                };
                println!(
                    "{star} {name:<12} {}  home={} workspace={} projects={}",
                    business::status(b),
                    b.home.display(),
                    b.workspace.display(),
                    b.projects.display()
                );
            }
            let t = resolve_triple(cli)?;
            println!("current: {} {t}", t.label());
            Ok(())
        }
        BusinessCmd::Add {
            name,
            home_dir,
            workspace_dir,
            projects_dir,
            replace,
        } => {
            let business = Business {
                home: home_dir.clone(),
                workspace: workspace_dir.clone(),
                projects: projects_dir.clone(),
            };
            let canon = business::canonical(&business).context("cos business add refused")?;
            announce_first_run(business::first_run(&path, &default)?, &path, &default);
            let nested = business::add(&path, name, canon.clone(), *replace)?;
            if !nested.is_empty() {
                let registry = business::load(&path)?;
                for other in &nested {
                    let Some(other_biz) = registry.businesses.get(other) else {
                        continue;
                    };
                    let relation = if canon.home.starts_with(&other_biz.home) {
                        "is inside"
                    } else {
                        "contains"
                    };
                    eprintln!(
                        "warning: {} {relation} {other}'s home",
                        canon.home.display()
                    );
                }
            }
            let install = default_home();
            if business::seed_context(&install.join("templates/context"), &canon.home)? {
                eprintln!(
                    "seeded {}/context from the install's templates",
                    canon.home.display()
                );
            }
            eprintln!("added {name} to {}", path.display());
            if cos::state::profile(&canon.workspace).is_none() {
                eprintln!(
                    "no profile in {}; run `cos --workspace {} state init`",
                    canon.workspace.display(),
                    canon.workspace.display()
                );
            }
            Ok(())
        }
        BusinessCmd::Use { name } => {
            announce_first_run(business::first_run(&path, &default)?, &path, &default);
            business::set_active(&path, name)?;
            eprintln!("active business: {name}");
            Ok(())
        }
        BusinessCmd::Home { name } => {
            announce_first_run(business::first_run(&path, &default)?, &path, &default);
            let registry = business::load(&path)?;
            let name = match name {
                Some(name) => name.clone(),
                None => resolve_triple(cli)?
                    .business
                    .context("no active business; name one: `cos business home <name>`")?,
            };
            let b = registry
                .businesses
                .get(&name)
                .with_context(|| format!("business {name:?} is not registered"))?;
            for (label, dir) in [
                ("home", &b.home),
                ("workspace", &b.workspace),
                ("projects", &b.projects),
            ] {
                if !dir.is_dir() {
                    anyhow::bail!("{label} {} of business {name:?} is missing", dir.display());
                }
            }
            println!("{}", b.home.display());
            Ok(())
        }
        BusinessCmd::Remove { name } => {
            announce_first_run(business::first_run(&path, &default)?, &path, &default);
            business::remove(&path, name)?;
            eprintln!("removed {name} from {}", path.display());
            Ok(())
        }
        BusinessCmd::Dashboards { .. } => unreachable!("handled before the built-in defaults"),
    }
}

/// `cos business dashboards`. Calls no `first_run`: the registry is read only for the orphan
/// report, and a missing one is empty.
fn business_dashboards(registry: &std::path::Path, name: Option<&str>) -> Result<()> {
    use cos::dashboards::{self, ProbeResult};

    let explicit = std::env::var_os("COS_DASHBOARDS").filter(|v| !v.is_empty());
    let file = dashboards::location(registry, explicit.as_deref().map(std::path::Path::new));
    let loaded = dashboards::load(&file)?;
    if let Some(name) = name
        && loaded.for_business(name).is_empty()
    {
        println!("no dashboards for {name}");
        return Ok(());
    }
    let reg = business::load(registry)?;
    let selected = |business: &str| name.is_none_or(|n| n == business);
    for (business, list) in loaded.businesses.iter().filter(|(b, _)| selected(b)) {
        for d in list {
            let addr = std::net::SocketAddrV4::new(std::net::Ipv4Addr::LOCALHOST, d.port);
            let result = match dashboards::probe(addr, &d.probe, Duration::from_secs(3)) {
                ProbeResult::Response { matched: true, .. } => "answers",
                ProbeResult::Response { matched: false, .. } => "answers, marker missing",
                ProbeResult::NoResponse => "no response",
                ProbeResult::NoListener => "no listener",
            };
            println!(
                "{business}  {}  {}  {}  {result}",
                d.name,
                d.url,
                d.server.as_deref().unwrap_or("unmanaged")
            );
        }
    }
    for orphan in loaded.orphans(&reg).iter().filter(|b| selected(b)) {
        println!("orphaned: {orphan}");
    }
    Ok(())
}

fn announce_first_run(fresh: bool, path: &std::path::Path, default: &Business) {
    if fresh {
        eprintln!(
            "wrote default (active) from the built-in defaults to {}: home={} workspace={} projects={}",
            path.display(),
            default.home.display(),
            default.workspace.display(),
            default.projects.display()
        );
    }
}

/// `cos flows`: read-only, and only when the profile approves the `tasks`
/// source, the same gate as the brief's `cn` facts.
fn flows_cmd(
    ws: &std::path::Path,
    projects: &std::path::Path,
    home: &std::path::Path,
    cn: &std::path::Path,
    json: bool,
) -> Result<()> {
    if !cos::context::local_reads(ws).tasks {
        anyhow::bail!(
            "the profile does not approve a tasks source, so cos reads no task store; approve one in .chief-of-staff/profile.json"
        );
    }
    let tz = cos::digest::zone(ws);
    let scan = cos::flows::scan(
        projects,
        cn,
        cos::flows::SCAN_BUDGET,
        Some(cos::flows::Hires { home, tz: &tz }),
    );
    if json {
        println!("{}", serde_json::to_string_pretty(&scan)?);
    } else {
        println!("{}", cos::flows::render(&scan));
    }
    Ok(())
}

fn review_cmd(ws: &std::path::Path, cmd: &ReviewCmd) -> Result<()> {
    use cos::{approval_auth, reviewer, workflow};
    let revision = || -> Result<i64> {
        let st = cos::state::read(ws)?.context("Workspace state is missing")?;
        st["revision"].as_i64().context("State revision is missing")
    };
    let action = |given: &Option<String>| {
        let id = given.clone().unwrap_or_else(workflow::new_action_id);
        eprintln!("action: {id}");
        id
    };
    match cmd {
        ReviewCmd::Bind {
            commitment,
            repo,
            draft,
            owner,
        } => workflow::bind(ws, commitment, revision()?, repo, draft, owner)?,
        ReviewCmd::Enroll {
            commitment,
            reviewer,
            sha256,
            action_id,
        } => {
            let actual = reviewer::program_digest(reviewer)?;
            let Some(pinned) = sha256 else {
                anyhow::bail!(
                    "check the reviewer, then pin it: cos review enroll {commitment} --reviewer {} --sha256 {actual}",
                    reviewer.display()
                );
            };
            approval_auth::enroll_reviewer(
                ws,
                commitment,
                revision()?,
                &action(action_id),
                reviewer,
                pinned,
            )?;
        }
        ReviewCmd::Run {
            commitment,
            action_id,
        } => {
            let start = reviewer::run(ws, commitment, &action(action_id))?;
            if start == workflow::ReviewStart::AlreadyCompleted {
                eprintln!("this action's review is already recorded");
            }
        }
        ReviewCmd::Show { .. } => {}
        ReviewCmd::Resolve {
            commitment,
            index,
            review,
            action_id,
            resolution,
        } => workflow::resolve_finding(
            ws,
            commitment,
            revision()?,
            &action(action_id),
            review,
            *index,
            &resolution.join(" "),
        )?,
        ReviewCmd::Approve {
            commitment,
            digest,
            action_id,
        } => approval_auth::approve_owner(ws, commitment, revision()?, &action(action_id), digest)?,
    }
    let commitment = match cmd {
        ReviewCmd::Bind { commitment, .. }
        | ReviewCmd::Enroll { commitment, .. }
        | ReviewCmd::Run { commitment, .. }
        | ReviewCmd::Show { commitment }
        | ReviewCmd::Resolve { commitment, .. }
        | ReviewCmd::Approve { commitment, .. } => commitment,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&workflow::status(ws, commitment)?)?
    );
    Ok(())
}

fn repo_dir(repo: &Option<PathBuf>) -> Result<PathBuf> {
    cos::ritual::resolve_repo(repo.as_deref())
}

fn ritual_cmd(cmd: &RitualCmd) -> Result<()> {
    match cmd {
        RitualCmd::Check { epic, repo, cn } => {
            let outcome = cos::ritual::check(cn, &repo_dir(repo)?, epic)?;
            println!("{}", outcome.describe());
            if outcome.ok() {
                Ok(())
            } else {
                std::process::exit(1)
            }
        }
        RitualCmd::Approve {
            epic,
            design,
            by,
            review,
            without_review,
            yes,
            repo,
            cn,
        } => {
            let args = cos::ritual::ApproveArgs {
                epic,
                design,
                by,
                review: review.clone().map(cos::ritual::ReviewAt::File),
                without_review: *without_review,
                yes: *yes,
            };
            let line = cos::ritual::approve(cn, &repo_dir(repo)?, &args, confirm_on_terminal)?;
            println!("{line}\napproved {epic}");
            Ok(())
        }
    }
}

/// A person types `yes` at a terminal. A pipe or a file is refused, so an
/// unattended call cannot approve; `--yes` is the only way past this prompt.
fn confirm_on_terminal(line: &str) -> Result<bool> {
    use std::io::{BufRead, IsTerminal, Write};
    if !std::io::stdin().is_terminal() {
        anyhow::bail!(
            "cos ritual approve needs a terminal. Approve from the Flows tab in Chief of Staff, or an agent the user has told to approve this design passes --yes instead"
        );
    }
    eprint!("Append to the epic and approve it?\n  {line}\nType yes: ");
    std::io::stderr().flush()?;
    let mut answer = String::new();
    std::io::stdin().lock().read_line(&mut answer)?;
    Ok(answer.trim() == "yes")
}

fn roles_cmd(cli: &Cli, t: &Triple, cfg: &Config, cmd: &RolesCmd) -> Result<()> {
    use cos::agents::roles::cli;
    let home = &t.home;
    match cmd {
        RolesCmd::List {
            library,
            retired,
            json,
        } => cli::list(home, *library, *retired, *json),
        RolesCmd::Show { id, library } => cli::show(home, id, *library),
        RolesCmd::Propose {
            library_id,
            id,
            brief,
            out,
            json,
        } => cli::propose(
            cfg,
            &cli::ProposeArgs {
                library_id,
                id: id.as_deref(),
                brief: brief.as_deref(),
                out: out.as_deref(),
                json: *json,
                dry_run: cli.dry_run,
                business: t.label(),
            },
        ),
        RolesCmd::New {
            library_id,
            file,
            blank,
            title,
            mandate,
            id,
            yes,
        } => {
            let from = match (library_id, file, blank) {
                (Some(l), None, None) => cli::NewFrom::Library {
                    id: l,
                    new_id: id.as_deref(),
                },
                (None, Some(f), None) => cli::NewFrom::File(f),
                (None, None, Some(b)) => cli::NewFrom::Blank {
                    id: b,
                    title: title.as_deref().unwrap_or_default(),
                    mandate: mandate.as_deref().unwrap_or_default(),
                },
                _ => anyhow::bail!("name a library entry, --file <role.json> or --blank <id>"),
            };
            cli::new(home, from, *yes)
        }
        RolesCmd::Edit { id, file, yes } => cli::edit(home, id, file, *yes),
        RolesCmd::Retire { id, yes } => cli::set_retired(home, id, true, *yes),
        RolesCmd::Restore { id, yes } => cli::set_retired(home, id, false, *yes),
    }
}

fn agents_cmd(cli: &Cli, t: &Triple, cmd: &AgentCmd) -> Result<()> {
    let home = &t.home;
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
            role,
            program,
        } => {
            use agents::hire::Ask;
            let picked: Vec<Ask> = match (from, prompt) {
                _ if deliverable.is_some() => {
                    vec![Ask::Deliverable(deliverable.clone().unwrap_or_default())]
                }
                (Some(from), _) => {
                    let tasks = agents::tasks(home, *from)?;
                    match item {
                        _ if *all => tasks.into_iter().map(Ask::Task).collect(),
                        Some(n) => vec![Ask::Task(
                            tasks
                                .into_iter()
                                .find(|t| t.n == *n)
                                .with_context(|| format!("item {n} has no agent prompt"))?,
                        )],
                        None => {
                            for t in &tasks {
                                println!("{:>2}  {}", t.n, t.title);
                            }
                            anyhow::bail!("pick one with --item <n>, or --all");
                        }
                    }
                }
                (None, Some(p)) => vec![Ask::Task(agents::Task {
                    n: 0,
                    title: p.lines().next().unwrap_or("").chars().take(80).collect(),
                    prompt: p.clone(),
                    source: "prompt".into(),
                })],
                (None, None) => {
                    anyhow::bail!("give --from brief|goals, --deliverable <id> or --prompt")
                }
            };
            if let Some(note) = agents::policy::load(home).note {
                eprintln!("{note}; hiring on the built-in defaults");
            }
            let ws = t.workspace.clone();
            let at = agents::hire::Where {
                home,
                workspace: &ws,
                projects: &t.projects,
            };
            let asked = agents::Contract {
                done_when: done_when.clone(),
                checks: checks.clone(),
                scope: scope.clone(),
                ..Default::default()
            };
            for ask in picked {
                let plan = agents::hire::plan(&at, role.as_deref(), ask, repo.clone(), &asked)?;
                if cli.dry_run {
                    println!("{}", serde_json::to_string_pretty(&plan)?);
                    continue;
                }
                for n in plan.fit.notes.iter().chain(&plan.warnings) {
                    eprintln!("note: {n}");
                }
                let req = plan.request(agents::hire::Run {
                    harness: *harness,
                    program: program.clone(),
                    model: model.clone(),
                    worktree: !in_place,
                    deadline: Duration::from_secs(deadline.unwrap_or(1800)),
                    slots: None,
                    explicit: agents::policy::Explicit {
                        deadline: deadline.is_some(),
                    },
                })?;
                let cos = std::env::current_exe().context("locate the cos binary")?;
                let spec = agents::spawn(t, req, &cos)?;
                println!("{}  {}  {}", spec.id, spec.workdir.display(), spec.title);
                let name = spec.terminal.clone().unwrap_or_default();
                eprintln!(
                    "in Herdr: {name} (hc attach {name})   ·   reply: cos agents say {} \"…\"   ·   end: cos agents done {}",
                    spec.id, spec.id
                );
            }
            if !cli.dry_run {
                eprintln!(
                    "follow: cos agents watch (stops at the deadline, judges once)   ·   live page: cos dash → /agents"
                );
            }
            Ok(())
        }
        AgentCmd::Ls => {
            let listing = agents::list(home)?;
            print!("{}", agents::view::table(&listing.runs));
            note_skipped(&listing.skipped);
            Ok(())
        }
        AgentCmd::Watch { osc } => loop {
            agents::tick(home)?;
            let listing = agents::list(home)?;
            if *osc {
                let mut out = std::io::stdout().lock();
                for (spec, state) in &listing.runs {
                    agents::osc::report(&mut out, &spec.id, &spec.title, state)?;
                }
                std::io::Write::flush(&mut out)?;
            } else {
                print!("\x1b[2J\x1b[H{}", agents::view::table(&listing.runs));
                note_skipped(&listing.skipped);
            }
            if listing.skipped.is_empty()
                && listing
                    .runs
                    .iter()
                    .all(|(spec, s)| spec.legacy() || s.status.terminal())
            {
                return Ok(());
            }
            std::thread::sleep(Duration::from_secs(2));
        },
        AgentCmd::Show { id } => {
            agents::follow::observe(home, id)?;
            let (spec, state) = agents::read(home, id)?;
            print!("{}", agents::view::show(&spec, &state));
            Ok(())
        }
        AgentCmd::Say { id, message } => {
            agents::say(home, id, &message.join(" "))?;
            println!("{id}: sent into its Herdr terminal");
            Ok(())
        }
        AgentCmd::Pause { id } => {
            let state = agents::pause(home, id)?;
            println!("{id}: its turn was stopped; it waits for your next message");
            let (spec, _) = agents::read(home, id)?;
            println!("in Herdr: {}", agents::view::herdr_word(&spec, &state));
            Ok(())
        }
        AgentCmd::Done { id } => {
            let state = agents::mark_done(home, id)?;
            let (spec, _) = agents::read(home, id)?;
            print!("{}", agents::view::show(&spec, &state));
            Ok(())
        }
        AgentCmd::Open { id } => {
            println!("{}", agents::open_in_herdr(home, id)?);
            Ok(())
        }
        AgentCmd::Screen { id, lines } => {
            print!("{}", agents::screen(home, id, *lines)?);
            Ok(())
        }
        AgentCmd::Scorecard => {
            let scores = agents::ledger::scorecard(&agents::ledger::read(home));
            print!("{}", agents::view::scorecard(&scores));
            Ok(())
        }
        AgentCmd::Record {
            id,
            outcome,
            notes,
            evidence,
        } => {
            let state = agents::record(
                home,
                id,
                agents::Recorded {
                    outcome: *outcome,
                    notes: notes.clone(),
                    evidence: evidence.clone(),
                    at: String::new(),
                },
            )?;
            let (spec, _) = agents::read(home, id)?;
            print!("{}", agents::view::show(&spec, &state));
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
                None => {
                    let listing = agents::list(home)?;
                    note_skipped(&listing.skipped);
                    listing
                        .runs
                        .into_iter()
                        .filter(|(_, s)| s.status.terminal())
                        .map(|(spec, _)| spec.id)
                        .collect()
                }
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
        AgentCmd::BuildSlot { .. } => {
            unreachable!("run dispatches it before a triple resolves")
        }
    }
}

/// Names the run ids `agents::list` could not resolve, on stderr.
fn note_skipped(skipped: &[String]) {
    if !skipped.is_empty() {
        eprintln!("skipped: {}", skipped.join(" "));
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
            StateCmd::Read => state::read(&ws)?.with_context(|| {
                format!(
                    "no .chief-of-staff/state.json; run `cos --workspace {} state init`",
                    ws.display()
                )
            }),
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
                let r = cos::store::latest::<cos::reports::Brief>(home, cos::store::Kind::Briefs)?
                    .context("no saved brief; run `cos brief`")?;
                let c = cos::track::proposal_candidate(
                    &ws,
                    &r.value.week.proposed,
                    &format!("cos brief {}", r.day),
                )?;
                Ok(serde_json::json!({
                    "candidate": c.path,
                    "added": c.added,
                    "skippedNoTrackedGoal": c.skipped,
                    "commit": format!(
                        "cos state commit --candidate {} --expected-revision {} --reason \"Proposed deliverables from the {} brief\"",
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
fn workspace(
    workspace: &std::path::Path,
) -> Result<(serde_json::Value, Option<serde_json::Value>)> {
    let st = cos::state::read(workspace)?.with_context(|| {
        format!(
            "no .chief-of-staff/state.json; run `cos --workspace {} state init`, then set up goals",
            workspace.display()
        )
    })?;
    Ok((st, cos::state::profile(workspace)))
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

fn show_report(home: &std::path::Path, no_open: bool) -> Result<()> {
    let path = home.join("out/report.html");
    eprintln!("report {}", path.display());
    if no_open {
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

/// The triple this invocation runs on. Flags and env are passed separately, so a conflict
/// within one layer is an error and a higher layer wins over a lower one.
fn resolve_triple(cli: &Cli) -> Result<Triple> {
    let inputs = inputs(cli);
    business::resolve(&inputs, || business::load(&business::location()?))
}

fn inputs(cli: &Cli) -> Inputs {
    Inputs {
        flags: Sources {
            business: cli.business.clone(),
            dirs: Dirs {
                home: cli.home.clone(),
                workspace: cli.workspace.clone(),
                projects: cli.projects.clone(),
            },
        },
        env: Sources {
            business: env_value("COS_BUSINESS"),
            dirs: Dirs {
                home: env_value("COS_HOME").map(PathBuf::from),
                workspace: env_value("COS_WORKSPACE").map(PathBuf::from),
                projects: env_value("COS_PROJECTS").map(PathBuf::from),
            },
        },
        install: install_dir(),
        user_home: env_value("HOME").map(PathBuf::from),
    }
}

fn env_value(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

/// The clone `cos` was built from: `task install` builds in place, so an
/// installed binary finds its agents and context with no configuration.
fn default_home() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn install_dir() -> PathBuf {
    std::fs::canonicalize(default_home()).unwrap_or_else(|_| default_home())
}
