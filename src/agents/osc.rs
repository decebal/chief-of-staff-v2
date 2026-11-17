//! OSC 7501 rev 0.4 status records. A fleet runner (herdr) reads them off the
//! pty; the detached supervisor has none, so the sole emitter is the foreground
//! `agents watch`, mirroring the saved [`State`] the table already read — one
//! record per agent, status never recomputed here.
//! Wire: `ESC ] 7501 ; state=<v>[:id=seg/seg][:app=a][:title=b64][:msg=b64] ESC \`.
//! Spec: <https://www.superlogical.com/rex/docs/build/program-status>.

use std::io::{self, Write};

use super::{State, Status};

const APP: &str = "cos";

/// herdr has no status for "ran and was rejected", so a failed run is an
/// `error` record, not a `done` one.
fn state_str(s: Status) -> &'static str {
    match s {
        Status::Starting | Status::Queued | Status::Seeding | Status::Running => "working",
        Status::Done => "done",
        Status::Failed | Status::Killed | Status::TimedOut | Status::Lost => "error",
    }
}

/// An id segment is `[A-Za-z0-9_.+-]{1,32}`; anything else becomes `-`.
fn seg(id: &str) -> String {
    let mut s: String = id
        .chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '_' | '.' | '+' | '-' => c,
            _ => '-',
        })
        .collect();
    s.truncate(32);
    s
}

/// Standard base64 (RFC 4648) of UTF-8, which OSC 7501 requires for free text.
fn b64(input: &str) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [chunk[0], *chunk.get(1).unwrap_or(&0), *chunk.get(2).unwrap_or(&0)];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(A[((n >> 18) & 63) as usize] as char);
        out.push(A[((n >> 12) & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            A[((n >> 6) & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            A[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn truncate(s: &str, max: usize) -> &str {
    match s.char_indices().nth(max) {
        Some((i, _)) => &s[..i],
        None => s,
    }
}

/// One OSC 7501 record for an agent, addressed `cos/<id>`: its title, and the
/// final summary once there is one, else the newest activity line.
pub fn report(w: &mut impl Write, id: &str, title: &str, st: &State) -> io::Result<()> {
    write!(
        w,
        "\x1b]7501;state={}:id={APP}/{}:app={APP}",
        state_str(st.status),
        seg(id)
    )?;
    if !title.is_empty() {
        write!(w, ":title={}", b64(truncate(title, 120)))?;
    }
    let msg = if st.summary.is_empty() {
        &st.last
    } else {
        &st.summary
    };
    let msg = truncate(msg, 160);
    if !msg.is_empty() {
        write!(w, ":msg={}", b64(msg))?;
    }
    write!(w, "\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(status: Status, last: &str, summary: &str) -> State {
        State {
            status,
            last: last.into(),
            summary: summary.into(),
            ..Default::default()
        }
    }

    #[test]
    fn working_record_is_exact() {
        let st = state(Status::Running, "Compiling", "");
        let mut buf = Vec::new();
        report(&mut buf, "20261113-225320-f142", "Build", &st).unwrap();
        assert_eq!(
            buf,
            b"\x1b]7501;state=working:id=cos/20261113-225320-f142:app=cos:title=QnVpbGQ=:msg=Q29tcGlsaW5n\x1b\\"
        );
    }

    #[test]
    fn summary_wins_over_last_line() {
        let st = state(Status::Done, "tool Bash: ls", "Done");
        let mut buf = Vec::new();
        report(&mut buf, "a", "T", &st).unwrap();
        assert_eq!(
            buf,
            b"\x1b]7501;state=done:id=cos/a:app=cos:title=VA==:msg=RG9uZQ==\x1b\\"
        );
    }

    #[test]
    fn terminal_statuses_map_to_done_or_error() {
        assert_eq!(state_str(Status::Done), "done");
        for s in [
            Status::Failed,
            Status::Killed,
            Status::TimedOut,
            Status::Lost,
        ] {
            assert_eq!(state_str(s), "error");
        }
    }

    #[test]
    fn b64_matches_spec_examples() {
        assert_eq!(b64("Build"), "QnVpbGQ=");
        assert_eq!(b64("Compiling"), "Q29tcGlsaW5n");
        assert_eq!(b64("Done"), "RG9uZQ==");
    }

    #[test]
    fn id_is_sanitised_and_capped() {
        assert_eq!(seg("a/b c:d"), "a-b-c-d");
        assert_eq!(seg(&"x".repeat(40)).len(), 32);
    }
}
