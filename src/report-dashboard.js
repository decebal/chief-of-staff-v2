
if (workspace && !workspace.error && latestSaved) {
  const shell = document.querySelector('.viz-root');
  shell.classList.add('workspace-shell');
  const workspaceName = (workspace.dashboardTitle || 'Workspace').replace(/ chief of staff/i, '');
  shell.style.setProperty('--workspace-label', JSON.stringify(workspaceName.toUpperCase()));
  const labels = ['Overview', workspace.production ? 'Delivery proof' : 'Work & outcomes', 'Strategy', 'Evidence', 'Staff'];
  const navPaths = [
    '<rect x="3" y="3" width="7" height="7" rx="1.5"/><rect x="14" y="3" width="7" height="7" rx="1.5"/><rect x="3" y="14" width="7" height="7" rx="1.5"/><rect x="14" y="14" width="7" height="7" rx="1.5"/>',
    '<path d="M8 6h13M8 12h13M8 18h13M3 6h.01M3 12h.01M3 18h.01"/>',
    '<circle cx="12" cy="12" r="9"/><path d="m16 8-2.5 5.5L8 16l2.5-5.5Z"/>',
    '<path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8Z"/><path d="M14 2v6h6M8 13h8M8 17h5"/>',
    '<circle cx="9" cy="8" r="3"/><path d="M3 21v-3a6 6 0 0 1 12 0v3M16 5a3 3 0 0 1 0 6M21 21v-3a6 6 0 0 0-3-5.2"/>'
  ];
  tabs.forEach((tab, i) => {
    tab.innerHTML = `<svg class="nav-icon" viewBox="0 0 24 24" aria-hidden="true" focusable="false">${navPaths[i]}</svg><span>${esc(labels[i])}</span>`;
  });
  const active = (items) => (items || []).filter((x) => !['complete', 'resolved', 'retired', 'cancelled'].includes(x.status));
  const job = active(workspace.deliverables).find((d) => d.readinessEvidence);
  const blockers = active(job?.readinessBlockers);
  const visibility = job?.readinessEvidence?.visibilityDiagnosis || {};
  const portfolio = workspace.portfolio;
  const blockerLabel = (b) => ({'R-1':'Prove the shipped, installed artifact','R-2':'Name the customer and team','R-3':'Prove same-member Gmail execution','R-4':'Capture the accepted reply chain'}[b.id] || b.summary);
  const actionLine = (b) => ({'R-1':'Verify Replies in the shipped app.','R-2':'Choose a willing customer and confirm their team.','R-3':'Verify Gmail access in that member’s session.','R-4':'Retain the sent reply and customer acceptance.'}[b.id] || b.nextAction);
  const confirmedKpis = (workspace.kpis || []).filter((k) => k.status !== 'proposed' && k.targetBasis !== 'proposed');
  const outcome = confirmedKpis.find((k) => k.goalId === job?.goalId);
  const statistic = (label, value, note) => `<div class="dash-stat"><span>${esc(label)}</span><strong>${esc(value)}</strong><small>${esc(note)}</small></div>`;
  const installed = visibility.installedVerified === true;
  const gates = job?.evidenceGates || [];
  const summary = job ? 'Prove one complete customer job, through the installed app.' : 'Decisions and evidence from your saved assessments.';
  $('left').innerHTML = `
    <div class="dash-heading dash-hero"><svg class="dash-waves" viewBox="0 0 1100 420" preserveAspectRatio="xMidYMid slice" aria-hidden="true"><g fill="none" stroke="currentColor">${Array.from({length:18}, (_,i) => `<path d="M-100 ${130+i*13} C180 ${-130+i*13},390 ${490+i*4},640 ${200+i*8} S950 ${20+i*9},1250 ${180+i*11}"/>`).join('')}</g></svg><div class="dash-hero-copy"><div class="dash-eyebrow">${esc(workspaceName)} / Operating room</div><h2>From work<br>to <em>proof.</em></h2><p>${esc(summary)}</p><div class="row"><span class="dash-badge attention">${esc(savedAssessment.status)} evidence coverage</span><span class="dash-hero-note">Installed verification ${installed ? 'verified' : 'open'}</span></div></div><div class="dash-art" aria-hidden="true"><div class="dash-art-orbit"></div><img src="__COS_ART__" alt="" width="420" height="420" fetchpriority="high"><span class="dash-art-label">01 / THE REPLIES BET</span></div></div>
    <div class="dash-stats">
      ${statistic('Customer outcome', outcome?.current ?? 'Unknown', 'Accepted completion · saved evidence')}
      ${statistic('Installed proof', installed ? 'Verified' : 'Open', 'Requires proof in the installed app')}
      ${statistic('Readiness blockers', blockers.length, 'Open gates for the first customer job')}
      ${statistic('Active work', portfolio?.activeCount ?? 'Unknown', `${portfolio?.workstreams?.length ?? 0} monitored workstreams · not commitments`)}
    </div>
    <div class="dash-columns"><div>
      <section class="card dash-chart"><div class="dash-toolbar"><div><h3>Assessment activity</h3><span class="sub">Recorded evidence, by assessment date</span></div><div class="row"><label class="ids" for="dash-series">Metric</label><select id="dash-series"><option value="assessments">Assessments</option><option value="changes">Verified changes</option></select><label class="ids" for="dash-range">Period</label><select id="dash-range"><option value="7">Last 7 dates</option><option value="all">All dates</option></select></div></div><div id="dash-area"></div><p id="dash-chart-note" class="dash-chart-note" aria-live="polite">Saved activity measures review cadence, not customer progress.</p><details class="table"><summary>Chart data and limits</summary><div id="dash-area-table"></div><p>Publication receipts excluded. Missing dates are not inferred as zero. No customer-outcome trend is available.</p></details></section>
      ${job ? `<section class="card dash-proof"><header><div><div class="dash-eyebrow">Primary bet</div><h3>Replies · first customer job</h3></div><span class="dash-badge attention">${esc(job.status)}</span></header><p class="finish">Customer opens a genuine Gmail thread from Home, sends a reviewed reply and confirms it was useful.</p><details class="dash-finish"><summary>Full acceptance criteria</summary><p>${esc(job.doneWhen)}</p></details><div class="row"><span class="dash-badge">Participant: ${esc(job.customerIdentity || 'unconfirmed')}</span><span class="dash-badge">Team: ${esc(job.dashboardTeam || 'unconfirmed')}</span></div><div class="dash-gates">${gates.map((g) => `<details class="gate-stage" data-status="${esc(g.status)}"><summary><span class="gate-orb" aria-hidden="true">${gates.indexOf(g)+1}</span><span>${esc(human(g.id).replaceAll("-", " "))}<small>${esc(g.status)}</small></span></summary>${g.reference ? `<div class="ids">${link(g.reference)}</div>` : ''}</details>`).join('')}</div><p class="ids">Installed verification ${installed ? 'verified' : 'open'} · customer acceptance ${job.completionEvidence ? 'recorded' : 'unverified'}. Source, release and installed proof remain separate.</p></section>` : ''}
    </div><section class="card dash-next"><div class="dash-toolbar"><h3>Needs attention</h3><span class="dash-badge">${blockers.length} open</span></div>${blockers.length ? `<ol>${blockers.map((b) => `<li><b>${esc(b.id)} · ${esc(blockerLabel(b))}</b><p>${esc(actionLine(b))}</p><details><summary>Action and evidence</summary><p>${esc(b.nextAction)}</p><p>${esc(b.summary)}</p></details><div class="ids">${esc(b.owner || 'Owner unassigned')} · ${esc(b.status)}</div></li>`).join('')}</ol>` : '<p class="empty">No open readiness blockers recorded. Customer acceptance still requires evidence.</p>'}</section></div>
    <details class="card dash-evidence"><summary>Full readiness evidence and source coverage</summary>${savedReadiness()}${savedCoverage(workspace.sources || [])}</details>`;

  const proofPanel = $('left').querySelector('.dash-proof');
  if (proofPanel) proofPanel.parentElement.prepend(proofPanel);

  const drawArea = () => {
    const metric = $('dash-series').value;
    const byDate = new Map();
    for (const r of savedRuns.filter((r) => !['publication_receipt', 'dashboard_configuration'].includes(r.mode))) {
      const at = new Date(r.finishedAt || r.invokedAt);
      if (Number.isNaN(at.getTime())) continue;
      const date = new Intl.DateTimeFormat('en-CA', { timeZone: workspace.timezone || 'UTC', year:'numeric', month:'2-digit', day:'2-digit' }).format(at);
      const value = metric === 'changes' ? (r.verifiedChanges || []).length : 1;
      byDate.set(date, (byDate.get(date) || 0) + value);
    }
    const all = [...byDate].sort(([a], [b]) => a.localeCompare(b));
    const points = $('dash-range').value === '7' ? all.slice(-7) : all;
    if (!points.length) { $('dash-area').innerHTML = '<p class="empty">No dated assessments available.</p>'; return; }
    const max = Math.max(1, ...points.map((p) => p[1]));
    const x = (i) => 40 + (points.length === 1 ? 280 : i / (points.length - 1) * 560);
    const y = (v) => 174 - v / max * 144;
    const path = points.map((p, i) => `${i ? 'L' : 'M'}${x(i)},${y(p[1])}`).join(' ');
    const name = metric === 'changes' ? 'verified changes' : 'assessments';
    $('dash-area').innerHTML = `<svg viewBox="0 0 640 210" role="group" aria-label="${esc(name)} by recorded assessment date"><defs><linearGradient id="assessment-fill" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stop-color="var(--accent)" stop-opacity=".28"/><stop offset="100%" stop-color="var(--accent)" stop-opacity=".02"/></linearGradient></defs>${[0, .5, 1].map((n) => `<line x1="40" x2="600" y1="${y(n*max)}" y2="${y(n*max)}" stroke="var(--grid)" stroke-dasharray="3 5"/><text x="26" y="${y(n*max)+4}" text-anchor="end" class="axis-label">${Number((n*max).toFixed(1))}</text>`).join('')}<path class="area-fill" d="${path} L${x(points.length-1)},174 L${x(0)},174 Z" fill="url(#assessment-fill)"/><path d="${path}" fill="none" stroke="var(--accent)" stroke-width="2.5" stroke-linejoin="round"/>${points.map((p, i) => `<circle tabindex="0" cx="${x(i)}" cy="${y(p[1])}" r="4" data-dash-point="${i}" aria-label="${esc(p[0])}: ${p[1]} ${name}"><title>${esc(p[0])}: ${p[1]} ${name}</title></circle>`).join('')}${points.map((p, i) => i === 0 || i === points.length-1 || (points.length < 8 && i % 2 === 0) ? `<text x="${x(i)}" y="202" text-anchor="middle" class="axis-label">${esc(p[0].slice(5))}</text>` : '').join('')}</svg>`;
    $('dash-area-table').innerHTML = `<table><tr><th>Date</th><th>${esc(name)}</th></tr>${points.map((p) => `<tr><td>${esc(p[0])}</td><td>${p[1]}</td></tr>`).join('')}</table>`;
    $('dash-chart-note').textContent = `${points.length} recorded dates · ${points.reduce((sum,p) => sum+p[1],0)} ${name}. Review activity, not customer progress.`;
    $('dash-area').querySelectorAll('[data-dash-point]').forEach((point) => {
      const show = () => { const p = points[Number(point.dataset.dashPoint)]; $('dash-chart-note').textContent = `${p[0]} · ${p[1]} ${name} · ${workspace.timezone || 'UTC'}`; };
      point.addEventListener('mouseenter', show);
      point.addEventListener('focus', show);
    });
  };
  $('dash-series').onchange = drawArea;
  $('dash-range').onchange = drawArea;
  drawArea();

  const inventory = document.querySelector('[data-portfolio] details');
  if (inventory && portfolio?.activeWork) {
    const table = inventory.querySelector('table');
    inventory.insertAdjacentHTML('afterbegin', '<div class="dash-inventory-controls"><label for="dash-search" class="ids">Search work</label><input id="dash-search" type="search" placeholder="Title, owner or priority…"><label for="dash-status" class="ids">Status</label><select id="dash-status"><option value="all">All statuses</option></select></div>');
    const states = [...new Set(portfolio.activeWork.map((x) => x.status))].sort();
    $('dash-status').insertAdjacentHTML('beforeend', states.map((s) => `<option value="${esc(s)}">${esc(human(s))}</option>`).join(''));
    inventory.insertAdjacentHTML('beforeend', '<div class="dash-pagination"><span id="dash-page" role="status"></span><button type="button" id="dash-prev" aria-label="Previous work page">Previous</button><button type="button" id="dash-next">Next</button></div>');
    let page = 0;
    const renderInventory = () => {
      const q = $('dash-search').value.toLowerCase();
      const rows = portfolio.activeWork.filter((x) => ($('dash-status').value === 'all' || x.status === $('dash-status').value) && [x.title,x.owner,x.priority].join(' ').toLowerCase().includes(q));
      const pages = Math.max(1, Math.ceil(rows.length/12));
      page = Math.min(page, pages-1);
      table.innerHTML = `<thead><tr><th>Work</th><th>Status</th><th>Priority</th><th>Owner</th></tr></thead><tbody>${rows.slice(page*12,page*12+12).map((x) => `<tr><td>${esc(x.title)}</td><td><span class="dash-badge">${esc(human(x.status))}</span></td><td>${esc(x.priority)}</td><td>${esc(x.owner || 'unassigned')}</td></tr>`).join('') || '<tr><td colspan="4">No matching work.</td></tr>'}</tbody>`;
      $('dash-page').textContent = `${rows.length} records · Page ${page+1} of ${pages}`;
      $('dash-prev').disabled = page === 0;
      $('dash-next').disabled = page+1 >= pages;
    };
    $('dash-search').oninput = $('dash-status').onchange = () => { page=0; renderInventory(); };
    $('dash-prev').onclick = () => { page--; renderInventory(); };
    $('dash-next').onclick = () => { page++; renderInventory(); };
    renderInventory();
  }
}
