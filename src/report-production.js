if (workspace?.production && !workspace.error) {
  const production = workspace.production;
  const platforms = production.platforms || [];
  const evidenceLink = (item) => {
    const href = String(item?.url || '');
    return /^https?:\/\/[^\s"'<>]+$/.test(href) ? `<a href="${esc(href)}" target="_blank" rel="noopener noreferrer">${esc(item.label || 'Evidence')} <span aria-hidden="true">↗</span></a>` : `<span>${esc(item?.label || 'Evidence unavailable')}</span>`;
  };
  const tone = (status) => ['verified', 'partial', 'blocked', 'unknown'].includes(status) ? status : 'unknown';
  const proof = (row, platform) => row.platforms?.[platform.id] || {status:'unknown', label:'Not verified', detail:'No platform-specific outcome evidence is saved.'};
  const priorNodes = [...$('plan').childNodes];
  $('plan').innerHTML = `<section class="production-board" aria-labelledby="production-heading"><header class="production-heading"><div><div class="dash-eyebrow">Production / whole picture</div><h2 id="production-heading">What is live. What is proved.</h2><p>Longhand delivery, payment readiness and traffic evidence. Select any state to inspect its proof below.</p></div><span class="production-date">Checked ${esc(savedTime(production.assessedAt))}</span></header><div class="production-toolbar"><label for="production-scope">Show</label><select id="production-scope"><option value="bets">Confirmed bets</option><option value="all">Bets + monitored workstreams</option></select><label for="production-search">Find</label><input id="production-search" type="search" placeholder="Bet, workstream or platform"><span id="production-count" role="status"></span></div><div class="production-scroll" role="region" aria-label="Longhand production matrix; scroll horizontally for all platforms" tabindex="0"><table class="production-table"><caption>Production availability is separate from proof of a useful customer outcome.</caption><thead><tr><th scope="col">Bet / workstream</th>${platforms.map(p=>`<th scope="col"><span>${esc(p.name)}</span><small>${esc(p.deployment.label)}</small></th>`).join('')}<th scope="col">Payments</th><th scope="col">Traffic monitoring</th><th scope="col">Customer outcome</th></tr></thead><tbody id="production-rows"></tbody></table></div><p class="production-key"><span data-tone="verified">Verified</span><span data-tone="partial">Partial proof</span><span data-tone="blocked">Blocked</span><span data-tone="unknown">Unknown</span><span>Unknown does not mean zero. Select a platform for version, date and evidence.</span></p><section class="production-detail" id="production-detail" aria-labelledby="production-detail-title" tabindex="-1"></section><details class="production-coverage"><summary>Coverage, freshness and missing evidence</summary><p>${esc(production.coverage)}</p><p>${esc(production.maintenance)}</p></details></section><details class="card production-existing"><summary>Outcome records, workstreams and active inventory</summary><div id="production-existing-body"></div></details>`;
  priorNodes.forEach(node => $('production-existing-body').append(node));
  const rows = production.rows || [];
  let selected = null;
  const cell = (row, field, label, status) => `<button type="button" class="production-cell" data-row="${esc(row.id)}" data-field="${esc(field)}" data-tone="${tone(status)}" aria-label="${esc(`${row.title} · ${field} · ${label}`)}"><span class="production-dot" aria-hidden="true"></span><span>${esc(label)}</span><span class="production-open" aria-hidden="true">↗</span></button>`;
  const showDetail = (row, field, focus = false) => {
    selected = {row:row.id, field};
    const platform = platforms.find(p=>p.id===field);
    const item = platform ? proof(row, platform) : row[field] || {status:'unknown',label:'Unknown',detail:'No evidence saved.'};
    const panel = $('production-detail');
    panel.innerHTML = `<div class="production-detail-heading"><div><div class="dash-eyebrow">${esc(row.kind === 'bet' ? row.priority || 'Confirmed bet' : 'Monitored workstream · not a new bet')}</div><h3 id="production-detail-title">${esc(row.title)} <span>/ ${esc(platform?.name || ({payment:'Payments',traffic:'Traffic monitoring',outcome:'Customer outcome'}[field] || field))}</span></h3></div><span class="production-status" data-tone="${tone(item.status)}">${esc(item.label)}</span></div><p>${esc(item.detail)}</p>${platform ? `<div class="production-facts"><div><b>Production availability</b><p>${esc(platform.deployment.label)}</p><p>${esc(platform.deployment.detail)}</p><small>Evidence: ${esc(platform.deployment.evidenceAt || 'Unknown date')}</small></div><div><b>Payments on ${esc(platform.name)}</b><p>${esc(platform.payment.label)}</p><p>${esc(platform.payment.detail)}</p></div><div><b>Traffic on ${esc(platform.name)}</b><p>${esc(platform.traffic.label)}</p><p>${esc(platform.traffic.detail)}</p></div></div>` : ''}<div class="production-next"><b>Next proof</b><p>${esc(item.nextAction || row.nextAction || 'Record dated evidence before changing this status.')}</p></div><nav class="production-links" aria-label="Evidence and destinations">${[...(item.links || []),...(platform?.links || []),...(row.links || [])].map(evidenceLink).join('')}</nav><p class="ids">${esc(row.boundary || 'Saved source evidence; customer acceptance remains separate.')} ${esc(item.evidenceAt ? `Observed ${item.evidenceAt}.` : '')}</p>`;
    document.querySelectorAll('.production-cell').forEach(button=>button.setAttribute('aria-pressed', String(button.dataset.row===row.id && button.dataset.field===field)));
    if (focus) panel.focus({preventScroll:true});
  };
  const renderRows = () => {
    const query = $('production-search').value.trim().toLowerCase();
    const visible = rows.filter(row=>($('production-scope').value==='all' || row.kind==='bet') && [row.title,row.priority,...platforms.map(p=>`${p.name} ${proof(row,p).label}`)].join(' ').toLowerCase().includes(query));
    $('production-rows').innerHTML = visible.map(row=>`<tr><th scope="row"><b>${esc(row.title)}</b><small>${esc(row.kind==='bet' ? row.priority || 'Confirmed bet' : 'Monitored · not a bet')}</small></th>${platforms.map(p=>{const value=proof(row,p);return `<td>${cell(row,p.id,value.label,value.status)}</td>`;}).join('')}${['payment','traffic','outcome'].map(field=>`<td>${cell(row,field,row[field]?.label || 'Unknown',row[field]?.status)}</td>`).join('')}</tr>`).join('') || '<tr><td colspan="9">No matching bets or workstreams.</td></tr>';
    $('production-count').textContent = `${visible.length} ${$('production-scope').value==='bets' ? 'confirmed bets' : 'bets and monitored workstreams'}`;
    $('production-rows').querySelectorAll('button').forEach(button=>{button.onclick=()=>showDetail(rows.find(row=>row.id===button.dataset.row),button.dataset.field,true);});
    const current = visible.find(row=>row.id===selected?.row) || visible[0];
    if (current) showDetail(current,selected?.field || platforms[0]?.id || 'outcome');
    else $('production-detail').innerHTML = '<h3 id="production-detail-title">No matching evidence</h3><p>Clear the search or select another view.</p>';
  };
  $('production-search').oninput = renderRows;
  $('production-scope').onchange = renderRows;
  renderRows();
}
