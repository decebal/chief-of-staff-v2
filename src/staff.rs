use crate::{proc, state};
use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

pub fn project(evidence: &Value, history: &Value, coordinator: &Path) -> Result<Value> {
    let run = &evidence["run"];
    let events = history
        .as_array()
        .context("AllStack history must be an array")?;
    let last = events.last().context("AllStack history is empty")?;
    ensure!(
        last["run"] == *run,
        "AllStack changed during read; preserve prior staff evidence"
    );
    ensure!(
        run["repository"].as_str() == coordinator.to_str(),
        "AllStack coordinator mismatch"
    );
    let id = run["id"].as_str().context("AllStack run identity absent")?;
    let revision = run["revision"]
        .as_u64()
        .context("AllStack revision absent")?;
    ensure!(
        !id.is_empty() && run["task"].is_string(),
        "AllStack task binding absent"
    );
    let now = jiff::Timestamp::now().to_string();
    let timeline: Vec<Value> = events
        .iter()
        .map(|event| {
            json!({
                "kind": event["kind"], "revision": event["run"]["revision"],
                "epoch": event["run"]["epoch"], "host": event["run"]["host"],
                "stage": event["run"]["stage"], "checkpoint": event["run"]["checkpoint"],
                "pending": event["run"]["pending"]
            })
        })
        .collect();
    Ok(json!({
        "id":id, "revision":revision, "host":run["host"], "epoch":run["epoch"],
        "stage":run["stage"], "status":if !run["pending"].is_null() { "needs_reconciliation" } else if run["stage"] == "shipped" { "shipped" } else { "active" },
        "pending":run["pending"], "taskRef":run["task"], "candidate":run["prepared_commit"],
        "workspace":run["workspace"], "coordinator":coordinator, "checkpoint":run["checkpoint"],
        "plan":run["plan"], "verification":run["verification"], "publication":run["publication"],
        "allowCommit":run["allow_commit"], "allowPublish":run["allow_publish"],
        "receiptRef":format!("allstack:{id}:revision:{revision}"), "verifiedAt":now,
        "timeline":timeline, "gateReceipts":evidence["gate_receipts"],
        "boundary":"Engine history readback proves run identity and ownership. It does not prove host completion, passed gates, publication or customer acceptance."
    }))
}

/// Verify a run against an explicit project and ownership binding.
pub fn project_bound_run(
    evidence: &Value,
    history: &Value,
    ownership: Option<&Value>,
    coordinator: &Path,
    run_id: &str,
    task_id: &str,
    epoch: u64,
) -> Result<Value> {
    ensure!(
        !run_id.is_empty() && !task_id.is_empty(),
        "Run and task bindings are required"
    );
    let run = &evidence["run"];
    ensure!(run["id"].as_str() == Some(run_id), "AllStack run mismatch");
    ensure!(
        run["task"].as_str() == Some(task_id),
        "AllStack task mismatch"
    );
    ensure!(
        run["epoch"].as_u64() == Some(epoch),
        "AllStack epoch mismatch"
    );
    let mut projection = project(evidence, history, coordinator)?;
    projection["ownershipVerified"] = json!(false);
    if run["pending"].is_null() {
        let ownership = ownership.context("AllStack ownership readback required")?;
        ensure!(
            ownership["run"] == *run,
            "AllStack changed during ownership readback"
        );
        projection["ownershipVerified"] = json!(true);
    }
    Ok(projection)
}

/// Read a bound run through AllStack, under one deadline for all readbacks.
pub fn read_bound_run(
    engine: &Path,
    coordinator: &Path,
    run_id: &str,
    task_id: &str,
    epoch: u64,
) -> Result<Value> {
    ensure!(
        !run_id.is_empty() && !task_id.is_empty(),
        "Run and task bindings are required"
    );
    let coordinator = coordinator.canonicalize()?;
    let deadline = Instant::now() + Duration::from_secs(45);
    let read = |operation: &str, ownership: bool| -> Result<Value> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        ensure!(
            !remaining.is_zero(),
            "AllStack evidence read deadline exceeded"
        );
        let mut command = Command::new(engine);
        command
            .arg("-C")
            .arg(&coordinator)
            .arg(operation)
            .arg(run_id);
        if ownership {
            command.args(["--epoch", &epoch.to_string()]);
        }
        let output = proc::run(&mut command, remaining)?;
        ensure!(output.status.success(), "AllStack {operation} failed");
        serde_json::from_str(&output.stdout).context("Invalid AllStack response")
    };
    let evidence = read("evidence", false)?;
    let history = read("history", false)?;
    // `resume` reads the task store, so reject mismatched evidence first.
    ensure!(
        evidence["run"]["id"].as_str() == Some(run_id),
        "AllStack run mismatch"
    );
    ensure!(
        evidence["run"]["task"].as_str() == Some(task_id),
        "AllStack task mismatch"
    );
    ensure!(
        evidence["run"]["epoch"].as_u64() == Some(epoch),
        "AllStack epoch mismatch"
    );
    project(&evidence, &history, &coordinator)?;
    let ownership = if evidence["run"]["pending"].is_null() {
        Some(read("resume", true)?)
    } else {
        None
    };
    project_bound_run(
        &evidence,
        &history,
        ownership.as_ref(),
        &coordinator,
        run_id,
        task_id,
        epoch,
    )
}

pub fn sync(workspace: &Path, coordinator: &Path, run_id: &str, role_id: &str) -> Result<Value> {
    let coordinator = coordinator.canonicalize()?;
    let mut candidate = state::read(workspace)?.context("Initialize cos state first")?;
    let revision = candidate["revision"]
        .as_i64()
        .context("State revision absent")?;
    let dashboard = candidate["commitments"]
        .as_array_mut()
        .context("Commitments absent")?
        .iter_mut()
        .find(|item| item["id"] == "C-longhand-dashboard")
        .context("Longhand dashboard commitment absent")?
        .get_mut("dashboard")
        .context("Dashboard absent")?;
    let staff = dashboard
        .get_mut("staff")
        .context("Staff configuration absent")?;
    ensure!(
        staff["integration"]["coordinator"].as_str() == coordinator.to_str(),
        "Coordinator not enrolled in staff configuration"
    );
    let executable = staff["integration"]["engineExecutable"]
        .as_str()
        .context("AllStack executable not configured")?
        .to_string();
    let until = Instant::now() + Duration::from_secs(45);
    let read = |operation: &str, epoch: Option<u64>| -> Result<Value> {
        let left = until.saturating_duration_since(Instant::now());
        ensure!(!left.is_zero(), "AllStack evidence read deadline exceeded");
        let mut command = Command::new(&executable);
        command.args([
            "-C",
            coordinator
                .to_str()
                .context("Coordinator path is not UTF-8")?,
            operation,
            run_id,
        ]);
        if let Some(epoch) = epoch {
            command.args(["--epoch", &epoch.to_string()]);
        }
        let output = proc::run(&mut command, left)?;
        ensure!(
            output.status.success(),
            "AllStack {operation} failed: {}",
            output.stderr
        );
        Ok(serde_json::from_str(&output.stdout)?)
    };
    let evidence = read("evidence", None)?;
    let history = read("history", None)?;
    let task = evidence["run"]["task"]
        .as_str()
        .context("AllStack task binding absent")?;
    let epoch = evidence["run"]["epoch"]
        .as_u64()
        .context("AllStack epoch absent")?;
    let ownership = if evidence["run"]["pending"].is_null() {
        Some(read("resume", Some(epoch))?)
    } else {
        None
    };
    let mut projection = project_bound_run(
        &evidence,
        &history,
        ownership.as_ref(),
        &coordinator,
        run_id,
        task,
        epoch,
    )?;
    let parked = staff["enrollment"]["status"] == "parked_by_owner"
        || staff["enrollment"]["parking"]["ownerRequested"] == true;
    if parked {
        projection["status"] = json!("parked");
    }
    let members = staff["members"]
        .as_array_mut()
        .context("Staff members absent")?;
    let member = members
        .iter_mut()
        .find(|item| item["id"] == role_id)
        .context("Unknown staff role")?;
    if member["run"]["id"] == projection["id"] {
        ensure!(
            member["run"]["revision"].as_u64().unwrap_or(0)
                <= projection["revision"].as_u64().unwrap_or(0),
            "AllStack history is older than saved staff evidence"
        );
    }
    member["run"] = projection.clone();
    member["taskRef"] = projection["taskRef"].clone();
    member["status"] = json!(if parked {
        "Parked by owner · no model calls"
    } else {
        "Managed execution · evidence linked"
    });
    member["evidenceBoundary"] = projection["boundary"].clone();
    for other in members
        .iter_mut()
        .filter(|item| item["run"]["id"] == projection["id"])
    {
        other["run"] = projection.clone();
    }
    if !parked {
        staff["enrollment"]["status"] = json!("managed_run_linked");
    }
    staff["integration"]["managedRunVerified"] = json!(true);
    staff["integration"]["label"] = json!(if parked {
        "Enrolled · parked by owner · no model calls"
    } else {
        "Enrolled · managed run history linked"
    });
    staff["integration"]["detail"] = json!(if parked {
        "Owner parking remains in force. Run and handoff evidence refreshed without launching workers or granting model access. Customer access and deployment remain gated."
    } else {
        "Run, Chronis task, epoch, checkpoints and handoffs read through AllStack. Customer access and deployment remain gated. Missing gate receipts and customer outcomes remain unknown."
    });
    staff["reviewedAt"] = projection["verifiedAt"].clone();
    state::commit(
        workspace,
        &candidate,
        revision,
        "Refresh staff from coherent AllStack engine evidence and history",
    )?;
    Ok(projection)
}
