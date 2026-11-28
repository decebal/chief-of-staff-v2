//! The role library: the caps every role field is held to, the text a hire
//! reads, and (in later parts) the shipped library entries, validation and
//! the only writer of `context/agents.json`.

/// A list's bound: at most `items`, each at most `chars` characters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ListCap {
    pub items: usize,
    pub chars: usize,
}

/// One table for the validator, the proposal schema and the renderer, so
/// the three never disagree on a bound.
#[derive(Debug, Clone, Copy)]
pub struct Caps {
    pub title: usize,
    pub mandate: usize,
    /// `serves`, `sandbox`, `judged`, `done_when`.
    pub text: usize,
    pub principles: ListCap,
    pub method: ListCap,
    pub anti_patterns: ListCap,
    pub escalation: ListCap,
    pub output_format: usize,
    /// The file a role leaves, relative to its workdir.
    pub output: usize,
    pub scope: ListCap,
    pub rationale: usize,
    pub brief: usize,
}

pub const CAPS: Caps = Caps {
    title: 60,
    mandate: 600,
    text: 300,
    principles: ListCap {
        items: 8,
        chars: 240,
    },
    method: ListCap {
        items: 10,
        chars: 240,
    },
    anti_patterns: ListCap {
        items: 8,
        chars: 240,
    },
    escalation: ListCap {
        items: 6,
        chars: 240,
    },
    output_format: 1200,
    output: 200,
    scope: ListCap {
        items: 40,
        chars: 200,
    },
    rationale: 800,
    brief: 1000,
};

/// `s` on one line: every run of whitespace, newlines included, becomes one
/// space, trimmed, then cut to `cap` characters. The preamble renders every
/// role value through this, so a hand-edited role cannot open a section of
/// its own or outgrow the bound.
pub fn clean(s: &str, cap: usize) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(cap)
        .collect()
}

/// `items` cleaned and cut to `cap`, empty ones dropped.
pub fn clean_list(items: &[String], cap: ListCap) -> Vec<String> {
    items
        .iter()
        .map(|i| clean(i, cap.chars))
        .filter(|i| !i.is_empty())
        .take(cap.items)
        .collect()
}
