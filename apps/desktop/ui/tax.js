const taxFields = ["partner-salary", "partner-property", "partner-other", "partner-dividend", "owner-income", "loan-balance", "repayment", "rent"];
const taxSteps = [
  ["shares", "Set up alphabet shares", "Confirm both shareholdings, prepare rights and resolutions, then file applicable changes before any class-specific dividend."],
  ["rent", "Document the office arrangement", "Record actual use from incorporation, including your partner’s office needs. Review the rent, tenancy consent and personal tax before posting or offsetting."],
  ["loan", "Decide on the director’s loan", "Reconcile the loan and interest, then compare retaining personal cash against the refundable Section 455 charge."],
  ["books", "Reconcile the company records", "Match bank transactions and confirm tax provisions and distributable profits before declaring dividends."],
  ["dividend", "Plan your partner’s dividend", "After share rights are effective, use projected income and verified reserves to decide the amount."],
];
let taxData = { values: {}, steps: {} };
let taxLoaded = false;
let taxLoading = false;
const taxPounds = (n) => new Intl.NumberFormat("en-GB", { style: "currency", currency: "GBP" }).format(n);
const taxNumber = (key) => {
  const raw = $(`tax-${key}`).value.trim();
  if (raw === "") return null;
  const n = Number(raw);
  return Number.isFinite(n) && n >= 0 ? n : null;
};
const taxValue = (text, cls = "") => el("span", { className: cls, textContent: text });
function taxMetric(title, value, detail) {
  return el("article", { className: "card tax-metric" }, taxValue(title, "muted small"), taxValue(value, "tax-number num"), taxValue(detail, "muted small"));
}
function taxBar(label, value, limit, context) {
  const complete = value !== null;
  const remaining = complete ? limit - value : null;
  const bar = el("div", { className: "tax-progress", role: "progressbar" });
  bar.setAttribute("aria-label", label);
  bar.setAttribute("aria-valuemin", "0");
  bar.setAttribute("aria-valuemax", String(limit));
  if (complete) bar.setAttribute("aria-valuenow", String(Math.min(value, limit)));
  const inner = el("div", { className: "tax-progress-fill" });
  inner.style.width = `${complete ? Math.min(100, value / limit * 100) : 0}%`;
  if (complete && value >= limit) inner.dataset.tone = "bad";
  bar.append(inner);
  return el("div", { className: "tax-chart-row" },
    el("div", { className: "tax-chart-heading" }, taxValue(label), taxValue(complete ? `${taxPounds(value)} / ${taxPounds(limit)}` : "Add projections", "num")),
    bar,
    taxValue(complete ? `${remaining < 0 ? `${taxPounds(-remaining)} over` : `${taxPounds(remaining)} remaining`} · ${context}` : context, "muted small"));
}
function renderTax() {
  const salary = taxNumber("partner-salary"), property = taxNumber("partner-property"), other = taxNumber("partner-other"), dividends = taxNumber("partner-dividend");
  const owner = taxNumber("owner-income"), loan = taxNumber("loan-balance"), repayment = taxNumber("repayment"), rent = taxNumber("rent");
  const base = [salary, property, other].every((n) => n !== null) ? salary + property + other : null;
  const partner = base !== null && dividends !== null ? base + dividends : null;
  const headroom = partner === null ? null : Math.max(0, 50000 - partner);
  const left = loan !== null && repayment !== null && repayment <= loan ? loan - repayment : null;
  const estimate = left === null ? null : left * 0.3575;
  $("tax-metrics").replaceChildren(
    taxMetric("Your partner’s remaining headroom", headroom === null ? "—" : taxPounds(headroom), "Additional income before £50,000 target"),
    taxMetric("Your allowance taper", owner === null ? "—" : owner > 100000 ? `${taxPounds(owner - 100000)} over` : `${taxPounds(100000 - owner)} left`, "£100,000 adjusted net income"),
    taxMetric("Loan after repayment", left === null ? "—" : taxPounds(left), "Principal · excludes accrued interest"),
    taxMetric("Proposed office rent", rent === null ? "—" : `${taxPounds(rent)} / mo`, "Requires supported agreement"));
  $("tax-charts").replaceChildren(
    taxBar("Partner · personal target", partner, 50000, "£50,000 target; income includes planned dividends"),
    taxBar("Partner · higher-rate threshold", partner, 50270, "Assumes standard personal allowance"),
    taxBar("Your personal allowance", owner, 100000, "Allowance reduces above £100,000 adjusted net income"));
  if (partner !== null && partner > 50000) $("tax-charts").append(el("p", { className: "tax-warning", textContent: `Projected income exceeds your £50,000 target by ${taxPounds(partner - 50000)}. Review planned dividends or other income.` }));
  if (owner !== null && owner > 100000) $("tax-charts").append(el("p", { className: "tax-warning", textContent: "Your projected adjusted net income exceeds £100,000. Review eligible pension contributions and other reliefs before deciding on extraction." }));
  const loanNote = loan === null || repayment === null ? "Enter both amounts to compare scenarios." : repayment > loan ? "Repayment exceeds the outstanding principal. Check the figures." : `If the entire balance is from loans made on or after 6 April 2026 and remains chargeable, a 35.75% Section 455 provision would be ${taxPounds(estimate)}. Earlier loans can have different rates; actual liability depends on accounting periods and qualifying repayments. Tax may be reclaimed after repayment under HMRC rules.`;
  $("tax-loan").replaceChildren(el("p", { textContent: loanNote }), el("p", { className: "muted small", textContent: "A repayment moves cash to the company; it is not an operating expense. Interest, anti-avoidance and payment deadlines need separate review. Loans over £10,000 may trigger beneficial-loan reporting and tax." }));
  const actions = taxSteps.map(([id, title, detail]) => {
    const select = el("select", { className: "select tax-status", ariaLabel: `Status: ${title}` });
    for (const [value, label] of [["not_started", "Not started"], ["in_progress", "In progress"], ["blocked", "Blocked"], ["done", "Done"]]) select.append(el("option", { value, textContent: label }));
    select.value = taxData.steps[id] ?? "not_started";
    select.addEventListener("change", () => { taxData.steps[id] = select.value; $("tax-save-status").textContent = "Unsaved changes"; });
    return el("div", { className: "tax-action" }, el("div", {}, el("h3", { textContent: title }), el("p", { className: "muted small", textContent: detail })), select);
  });
  $("tax-actions").replaceChildren(...actions);
}
async function loadTax() {
  if (taxLoading) return;
  taxLoading = true;
  try {
    const saved = await invoke("tax_read");
    taxData = { values: saved?.values ?? {}, steps: saved?.steps ?? {} };
    for (const field of taxFields) $(`tax-${field}`).value = taxData.values[field] ?? "";
    taxLoaded = true;
    $("tax-save-status").textContent = "Saved privately in this workspace.";
    renderTax();
  } catch (e) { logDev("tax_read", e); $("tax-save-status").textContent = "Could not load projections. Check workspace access and select Tax to retry."; }
  finally { taxLoading = false; }
}
document.querySelector('.app-tab[data-view="tax"]').addEventListener("click", () => { if (!taxLoaded) loadTax(); });
$("tax-form").addEventListener("input", () => { $("tax-save-status").textContent = "Unsaved changes"; renderTax(); });
$("tax-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  if (!taxLoaded || !$("tax-form").reportValidity()) return;
  taxData.values = Object.fromEntries(taxFields.map((field) => [field, taxNumber(field)]));
  try { await invoke("tax_save", { data: taxData }); $("tax-save-status").textContent = "Saved privately in this workspace."; }
  catch (e) { logDev("tax_save", e); $("tax-save-status").textContent = "Could not save projections. Check workspace access and try again."; }
});
