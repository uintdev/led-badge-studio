//! Wording about projects, for the app and `badgectl` to share.

use crate::protocol::{MAX_TOTAL_COLUMNS, message_columns};
use crate::{MESSAGE_COUNT, Project};

/// When the ticked messages need more than the badge's memory: how much
/// they need, and what sending them does about it (which messages are cut
/// short and which left out, by `message_columns`).
pub fn memory_overflow(project: &Project) -> Option<String> {
    let used = project.used_columns();
    let cut = cut_past(project, used)?;
    Some(format!(
        "The ticked messages need {} columns, more than the {} the badge holds: sending {cut}.",
        used * 8,
        MAX_TOTAL_COLUMNS * 8
    ))
}

/// What sending does to ticked messages that don't fit in the badge's
/// memory ("cuts message 6 short and leaves out messages 7 and 8"), by
/// `message_columns`; `None` when they fit.
pub fn memory_cut(project: &Project) -> Option<String> {
    cut_past(project, project.used_columns())
}

/// `memory_cut`, with the byte-columns the ticked messages need (`used`).
fn cut_past(project: &Project, used: usize) -> Option<String> {
    if used <= MAX_TOTAL_COLUMNS {
        return None;
    }
    let cols = message_columns(project);
    let (mut short, mut left) = (Vec::new(), Vec::new());
    for (i, m) in project.messages.iter().take(MESSAGE_COUNT).enumerate() {
        if m.selected && cols[i] < m.bitmap.used_width() / 8 {
            if cols[i] > 0 {
                short.push(i);
            } else {
                left.push(i);
            }
        }
    }
    let (s, l) = (
        messages_phrase(short.iter().copied()),
        messages_phrase(left.iter().copied()),
    );
    Some(match (short.is_empty(), left.is_empty()) {
        (false, false) => format!("cuts {s} short and leaves out {l}"),
        (false, true) => format!("cuts {s} short"),
        (true, false) => format!("leaves out {l}"),
        (true, true) => return None,
    })
}

/// Messages by index, counted from 1: "message 2", "messages 2 and 5",
/// "messages 1, 3 and 4".
pub fn messages_phrase(indices: impl Iterator<Item = usize>) -> String {
    let n: Vec<String> = indices.map(|i| (i + 1).to_string()).collect();
    match n.as_slice() {
        [one] => format!("message {one}"),
        [rest @ .., last] => format!("messages {} and {last}", rest.join(", ")),
        [] => "no messages".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CardType, LedColor};

    #[test]
    fn memory_overflow_names_whats_left_out() {
        let mut p = Project::new(CardType::Mono11);
        for m in &mut p.messages {
            m.selected = true;
            m.bitmap.set(4095, 0, LedColor::RED);
        }
        assert_eq!(
            memory_overflow(&p).as_deref(),
            Some(
                "The ticked messages need 32768 columns, more than the 20480 the badge holds: \
                 sending leaves out messages 6, 7 and 8."
            )
        );
    }

    #[test]
    fn memory_overflow_names_what_is_cut() {
        let mut p = Project::new(CardType::Mono11);
        assert_eq!(memory_overflow(&p), None);
        // 500 byte-columns each: five fit, the sixth gets 60, the rest none.
        for m in &mut p.messages {
            m.selected = true;
            m.bitmap.set(3999, 0, LedColor::RED);
        }
        p.messages[7].selected = false;
        assert_eq!(
            memory_overflow(&p).as_deref(),
            Some(
                "The ticked messages need 28000 columns, more than the 20480 the badge holds: \
                 sending cuts message 6 short and leaves out message 7."
            )
        );
        // Exactly full is fine.
        for m in &mut p.messages[5..] {
            m.selected = false;
        }
        p.messages[5].bitmap.clear();
        p.messages[5].bitmap.set(479, 0, LedColor::RED);
        p.messages[5].selected = true;
        assert_eq!(p.used_columns(), MAX_TOTAL_COLUMNS);
        assert_eq!(memory_overflow(&p), None);
    }
}
