//! The interface copy table: every string a surface shows, keyed, with the
//! placeholders that text carries.
//!
//! The player's surfaces read their text from here instead of holding literals,
//! so one owner decides what each screen says and a later ticket can check a
//! pack's UI-plugin `copy` overrides against [`KEYS`] ([spec §5], R6–R9). The
//! values are the player's current text, byte for byte: this table arrived as a
//! prefactor, so nothing a player sees changed.
//!
//! Three surfaces stay literal on purpose — the error screen, the Notice and the
//! Monitor — because a plugin never restyles them (R7). Engine-supplied reasons
//! (an unreadable slot's reason, for one) and pack content (say-lines, option
//! text, the pack name, the tagline, NPC names) are not interface copy either;
//! they reach a surface as engine slots, never as keys.
//!
//! An id is dotted and borrows the [Keiko control ids]' own vocabulary, so the
//! text a player reads about a control sits beside that control's name. Where a
//! label would collide with its control's id — `title.new-game` is a control —
//! the key names the text role instead: `title.row.new-game`.
//!
//! [`text`] is a `const fn` on purpose. The player's own copy is `const` and
//! `&'static str` typed, and a prefactor may not force a type change through
//! every caller; a plain function would have.
//!
//! [spec §5]: https://github.com/SairaDev02/unmei-vn/blob/main/docs/ui-plugin-spec.md#5-surfaces-slots-controls-and-copy-keys-r6r9-r25
//! [Keiko control ids]: crate::keiko::CONTROL_IDS

mod table;

pub use self::table::KEYS;

/// One keyed interface string: its id, the placeholders its text carries, and
/// that text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CopyKey {
    /// The dotted id, for example `confirm.overwrite.prompt`.
    pub id: &'static str,
    /// The placeholders [`Self::text`] carries, in the order it carries them,
    /// for example `&["slot"]`.
    pub placeholders: &'static [&'static str],
    /// The default text, with a `{name}` for each placeholder.
    pub text: &'static str,
}

/// The key `id` names, or `None` when the table has no such id.
pub fn key(id: &str) -> Option<&'static CopyKey> {
    KEYS.iter().find(|key| key.id == id)
}

/// The text `id` names.
///
/// # Panics
///
/// Panics on an id the table does not hold. A `const` caller fails to compile
/// instead, at the constant that names the unknown id.
pub const fn text(id: &str) -> &'static str {
    let mut index = 0;
    while index < KEYS.len() {
        if str_eq(KEYS[index].id, id) {
            return KEYS[index].text;
        }
        index += 1;
    }
    panic!("unknown copy key")
}

/// The text of `id` with each placeholder `values` supplies replaced by its
/// value.
///
/// # Panics
///
/// Panics on an id the table does not hold.
pub fn render(id: &str, values: &[(&str, &str)]) -> String {
    let key = key(id).unwrap_or_else(|| panic!("unknown copy key: {id}"));
    fill(key.text, values)
}

/// `text` with each placeholder `values` supplies replaced by its value.
///
/// [`render`] is this function over the table's text. A plugin's copy override
/// is the other caller: the override stands in for [`CopyKey::text`] and keeps
/// the same placeholders (spec §5), so it fills the same way.
///
/// One pass over the source text, so a substituted value is never read again: a
/// copied path may hold `{`, and it must survive verbatim. A declared
/// placeholder that `values` omits keeps its own `{name}`, which lets a caller
/// fill a line in stages.
pub fn fill(text: &str, values: &[(&str, &str)]) -> String {
    let mut rendered = String::with_capacity(text.len());
    let mut rest = text;
    while let Some((before, after)) = rest.split_once('{') {
        rendered.push_str(before);
        let Some((name, tail)) = after.split_once('}') else {
            rendered.push('{');
            rest = after;
            continue;
        };
        match values.iter().find(|(placeholder, _)| *placeholder == name) {
            Some((_, value)) => rendered.push_str(value),
            None => {
                rendered.push('{');
                rendered.push_str(name);
                rendered.push('}');
            }
        }
        rest = tail;
    }
    rendered.push_str(rest);
    rendered
}

/// Whether two strings are equal, for the `const` lookup above: `==` on `str` is
/// not usable in a `const fn`.
const fn str_eq(left: &str, right: &str) -> bool {
    let (left, right) = (left.as_bytes(), right.as_bytes());
    if left.len() != right.len() {
        return false;
    }
    let mut index = 0;
    while index < left.len() {
        if left[index] != right[index] {
            return false;
        }
        index += 1;
    }
    true
}

#[cfg(test)]
mod tests;
