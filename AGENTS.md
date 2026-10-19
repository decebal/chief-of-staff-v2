# Chief of Staff starter workspace

Use `plugins/chief-of-staff-starter/skills/chief-of-staff-starter/SKILL.md` for setup, planning, priorities, delegation, and status. Read the required mode references, not every reference on every run.

Keep personal data under `.chief-of-staff/`, outside the plugin. Never treat examples, another person's permissions, or installation as authority to act. This project starts with no external-write authority or enabled automation.

When working on the kit itself, keep fixtures fictional and update the explicit package manifest and integrity checks. Never add live runtime data or credentials to a release. Do not overwrite an existing marketplace, skill, schedule, or profile during setup.

This folder is also the source of `cos`, a Rust engine that runs the same skill's review and planning on a schedule against the same `.chief-of-staff/` files. Its instructions for coding agents are in `CLAUDE.md`; `cos state` follows the same rules as `scripts/state.py`.
