# Role library: brief

Date: 2026-11-28.

## Problem

The user hires Claude Code and Codex agents through `cos agents` into roles
defined in `context/agents.json` (`src/agents/policy.rs` `Role`). Today:

- A role is a one-line `mandate` plus a contract (checks, scope, expects). The
  builder's whole brief is "Implement the deliverable in this repo, with
  tests." The hire's preamble (`policy::preamble`) carries only title, mandate
  and the output rule, so every hire starts from a generic agent, not a
  specialist.
- The only way to add, change or retire a role is to hand-edit JSON. The
  desktop Staff tab (`agents_roles`) and the CLI only read roles. A malformed
  edit drops the whole file to built-in defaults (`Policy::note`).
- There is no starting point for a role nobody has written yet. A new role
  (security reviewer, SRE, analyst) means writing its standards from memory.

## Goal

The user curates the staff from a library of world-class specialist role
definitions shipped with `cos`: each one states the standards a top
practitioner holds, the method they follow, the anti-patterns they refuse, the
exact shape of what they hand back and when they escalate. The user picks one,
a read-only brain specialises it to the business and its repos, the user
reviews the proposal and approves it into `context/agents.json`. From the
desktop Staff tab (New role, Edit, Retire) or the CLI
(`cos roles list|show|new|edit|retire`), through one validated write path.
Every hire in that role then reads the full definition in its preamble.

## Approaches

1. **Library + specialise, structured fields (chosen with the user).** Typed
   fields on `Role`: `principles[]`, `method[]`, `anti_patterns[]`,
   `output_format`, `escalation[]`. Library entries ship as data under
   `templates/roles/`, compiled in. Specialising is one headless run with no
   tools and a JSON schema; the output is a proposal, never written until the
   user approves it.
2. Curated library only, no model step. Lost: a generic library role does not
   know the business's repos, checks or vocabulary; the user would hand-adapt
   every one.
3. Model drafts from a short brief, no library. Lost: quality depends on the
   prompt each time; nothing curated to hold the bar or to diff against.

Markdown playbook instead of structured fields lost on the user's call:
structured fields validate, render as sections in the window and diff
field-by-field in the approval view.

## Constraints

- The brains stay read-only. The specialise run proposes; only the user's
  approval writes. The write goes through one function with a lock and an
  atomic replace, and refuses a policy that does not parse back.
- `context/agents.json` keeps loading old files: every new field is optional.
- The head (chief of staff) is never hired, retired or demoted.
- A retired role keeps its id so the ledger, scorecard and run history still
  name it; it is no longer offered for hire.
- The specialise run gets no tools and no connectors (`Toolset::None`); it is
  shown the library entry, the workspace goals and the repo names, nothing
  from mail, calendar or tickets.
- Every child process is owned and reaped (`proc`), one deadline.
- Library fixtures stay generic: no customer or client names.
- Desktop UI: every state through `stateView`, tokens only
  (`tests/design.rs`).

## Unknowns

- **Library content quality** (`sand`). What makes a definition world class
  is a judgement. Moves to rock: each entry cites the public standard it
  draws on (OWASP ASVS, Google SRE book, Diátaxis, and so on) and a test
  holds every entry to minimum counts per field.
- **Preamble length** (`rock`). Five structured fields add a few hundred
  tokens per hire; a cap per field keeps it bounded.
- **Specialise output drift** (`sand`). The model may rewrite the contract or
  widen scope. Moves to rock: the proposal schema excludes contract fields the
  user did not ask to change, and the approval view shows a field diff.
- **Editing in the desktop** (`rock`). A form over the same typed fields,
  saved through the same Tauri command the CLI's write path backs.
