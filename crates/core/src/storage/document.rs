//! A TOML file as the core reads it: toml parses its text, and the core reads each table's keys
//! and each value's type itself, so a problem names the key, what its value is, and its line.

use std::ops::Range;

use nightcap_protocol::{Position, Problem, ValueType};
use toml::Spanned;
use toml::de::{DeInteger, DeTable, DeValue};

/// Why toml can't parse a file: it isn't valid TOML, where toml says. It reads after the file's
/// name, as the log may say it: toml's description names what it expected, never the file's text
/// (ADR-0009).
#[derive(Debug, thiserror::Error)]
#[error("isn't valid TOML{}: {}", found_at(*.at), .error.message())]
pub(crate) struct Malformed {
    pub(crate) at: Option<Position>,
    #[source]
    error: Box<toml::de::Error>,
}

/// Where toml found an error, as it reads after what's wrong: " at line 4, column 12".
fn found_at(at: Option<Position>) -> String {
    match at {
        Some(at) => format!(" at line {}, column {}", at.line, at.column),
        None => String::new(),
    }
}

/// `text` as a TOML document, each of its keys and values with where it is.
pub(crate) fn parse(text: &str) -> Result<Spanned<DeTable<'_>>, Malformed> {
    DeTable::parse(text).map_err(|error| Malformed {
        at: error.span().map(|span| position(text, span.start)),
        error: Box::new(error),
    })
}

/// Where the byte at `offset` in `text` is: its line, and its column in characters, each
/// counted from 1, as toml's own descriptions count them. A file past `u32::MAX` lines, or a
/// line past as many characters, is past any the core reads, so its last ones all count as
/// that one.
pub(crate) fn position(text: &str, offset: usize) -> Position {
    let before = || text.as_bytes().iter().take(offset);
    let breaks = before().filter(|&&byte| byte == b'\n').count();
    // A UTF-8 continuation byte, 0b10xx_xxxx, goes on with the character before it.
    let characters = before()
        .rev()
        .take_while(|&&byte| byte != b'\n')
        .filter(|&&byte| byte & 0b1100_0000 != 0b1000_0000)
        .count();
    Position {
        line: counted(breaks),
        column: counted(characters),
    }
}

/// The number, counted from 1, of what comes after `before` others.
fn counted(before: usize) -> u32 {
    u32::try_from(before).map_or(u32::MAX, |before| before.saturating_add(1))
}

/// What reading a key gives: its value with where it is, None where its table lacks it, or the
/// problem where the value isn't of the type read.
pub(crate) type Read<T> = Result<Option<Spanned<T>>, Spanned<Problem>>;

/// A table in a document, with the keys that lead to it from the top.
#[derive(Debug)]
pub(crate) struct Table<'a, 'i> {
    entries: &'a DeTable<'i>,
    path: Vec<String>,
    span: Range<usize>,
}

impl<'a, 'i> Table<'a, 'i> {
    /// The document's top table.
    pub(crate) fn top(document: &'a Spanned<DeTable<'i>>) -> Self {
        Self {
            entries: document.get_ref(),
            path: Vec::new(),
            span: document.span(),
        }
    }

    /// The table, once each of its keys is one of `keys`, else the first other one there.
    pub(crate) fn taking(self, keys: &[&str]) -> Result<Self, Spanned<Problem>> {
        let unknown = self
            .entries
            .iter()
            .map(|(key, _)| key)
            .filter(|key| !keys.contains(&key.get_ref().as_ref()))
            .min_by_key(|key| key.span().start);
        match unknown {
            None => Ok(self),
            Some(key) => Err(Spanned::new(
                key.span(),
                Problem::UnknownKey {
                    table: self.path,
                    key: key.get_ref().to_string(),
                    expected: keys.iter().map(|&key| key.to_owned()).collect(),
                },
            )),
        }
    }

    pub(crate) fn boolean(&self, key: &str) -> Read<bool> {
        self.read(key, ValueType::Boolean, DeValue::as_bool)
    }

    pub(crate) fn integer(&self, key: &str) -> Read<i64> {
        self.read(key, ValueType::Integer, |value| {
            value.as_integer().map(integer)
        })
    }

    pub(crate) fn string(&self, key: &str) -> Read<String> {
        self.read(key, ValueType::String, |value| {
            value.as_str().map(str::to_owned)
        })
    }

    /// A list of strings, each with where it is.
    pub(crate) fn strings(&self, key: &str) -> Read<Vec<Spanned<String>>> {
        self.read(key, ValueType::Strings, strings)
    }

    pub(crate) fn table(&self, key: &str) -> Result<Option<Self>, Spanned<Problem>> {
        let read = self.read(key, ValueType::Table, DeValue::as_table)?;
        Ok(read.map(|table| Table {
            span: table.span(),
            entries: table.into_inner(),
            path: self.path.iter().cloned().chain([key.to_owned()]).collect(),
        }))
    }

    /// `read`, the value of `key`, which the table must have.
    pub(crate) fn required<T>(&self, key: &str, read: Option<T>) -> Result<T, Spanned<Problem>> {
        read.ok_or_else(|| {
            Spanned::new(
                self.span.clone(),
                Problem::MissingKey {
                    table: self.path.clone(),
                    key: key.to_owned(),
                },
            )
        })
    }

    fn read<T>(
        &self,
        key: &str,
        expected: ValueType,
        read: impl FnOnce(&'a DeValue<'i>) -> Option<T>,
    ) -> Read<T> {
        let Some(value) = self.entries.get(key) else {
            return Ok(None);
        };
        match read(value.get_ref()) {
            Some(read) => Ok(Some(Spanned::new(value.span(), read))),
            None => Err(Spanned::new(
                value.span(),
                Problem::WrongType {
                    table: self.path.clone(),
                    key: key.to_owned(),
                    found: type_of(value.get_ref()),
                    expected,
                },
            )),
        }
    }
}

/// What `value` is, as TOML writes it.
fn type_of(value: &DeValue<'_>) -> ValueType {
    match value {
        DeValue::String(_) => ValueType::String,
        DeValue::Integer(_) => ValueType::Integer,
        DeValue::Float(_) => ValueType::Float,
        DeValue::Boolean(_) => ValueType::Boolean,
        DeValue::Datetime(_) => ValueType::Datetime,
        DeValue::Array(items) if items.iter().all(|item| item.get_ref().is_str()) => {
            ValueType::Strings
        }
        DeValue::Array(_) => ValueType::List,
        DeValue::Table(_) => ValueType::Table,
    }
}

/// `value` as a list of strings, each with where it is, or None where it isn't one.
fn strings(value: &DeValue<'_>) -> Option<Vec<Spanned<String>>> {
    value
        .as_array()?
        .iter()
        .map(|item| {
            let text = item.get_ref().as_str()?;
            Some(Spanned::new(item.span(), text.to_owned()))
        })
        .collect()
}

/// `integer`'s value. TOML's integers are 64-bit, so one past them is past any the core reads,
/// and reads as the nearest.
fn integer(integer: &DeInteger<'_>) -> i64 {
    let digits = integer.as_str();
    i64::from_str_radix(digits, integer.radix()).unwrap_or(if digits.starts_with('-') {
        i64::MIN
    } else {
        i64::MAX
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The problem `read` finds in `text`'s top table, and where it is.
    fn problem<T: std::fmt::Debug>(
        text: &str,
        read: impl for<'a, 'i> FnOnce(Table<'a, 'i>) -> Result<T, Spanned<Problem>>,
    ) -> (Position, Problem) {
        let document = parse(text).unwrap();
        let problem = read(Table::top(&document)).unwrap_err();
        (position(text, problem.span().start), problem.into_inner())
    }

    #[test]
    fn a_position_counts_lines_and_the_characters_before_it_on_its_line_from_1() {
        let text = "a = 1\nb = \"é\" x\n";

        assert_eq!(position(text, 0), Position { line: 1, column: 1 });
        assert_eq!(position(text, 6), Position { line: 2, column: 1 });
        assert_eq!(
            position(text, text.find('x').unwrap()),
            Position { line: 2, column: 9 }
        );
        assert_eq!(position(text, text.len()), Position { line: 3, column: 1 });
    }

    #[test]
    fn text_that_isnt_toml_is_malformed_where_toml_found_it() {
        let malformed = parse("websites = true\n[rules.off\n").unwrap_err();

        assert_eq!(
            malformed.at,
            Some(Position {
                line: 2,
                column: 11
            })
        );
    }

    #[test]
    fn a_key_its_table_doesnt_take_is_named_with_its_table_and_the_keys_it_takes() {
        let read = |top: Table<'_, '_>| {
            let rules = top.taking(&["rules"])?.table("rules")?.unwrap();
            rules.table("off")?.unwrap().taking(&["app", "domain"])?;
            Ok(())
        };

        assert_eq!(
            problem("[rules.off]\napp = []\n\ndomian = []\n", read),
            (
                Position { line: 4, column: 1 },
                Problem::UnknownKey {
                    table: vec!["rules".to_owned(), "off".to_owned()],
                    key: "domian".to_owned(),
                    expected: vec!["app".to_owned(), "domain".to_owned()],
                }
            )
        );
    }

    #[test]
    fn a_value_of_another_type_is_named_with_what_it_is_and_what_its_key_takes() {
        let cases = [
            ("app = \"x\"", ValueType::String),
            ("app = [\"x\", 1]", ValueType::List),
            ("app = 1", ValueType::Integer),
            ("app = 1.5", ValueType::Float),
            ("app = true", ValueType::Boolean),
            ("app = 2026-10-28", ValueType::Datetime),
            ("app = { x = 1 }", ValueType::Table),
        ];

        for (text, found) in cases {
            assert_eq!(
                problem(text, |top| top.strings("app")),
                (
                    Position { line: 1, column: 7 },
                    Problem::WrongType {
                        table: Vec::new(),
                        key: "app".to_owned(),
                        found,
                        expected: ValueType::Strings,
                    }
                ),
                "{text}"
            );
        }
        assert_eq!(
            problem("websites = [\"yes\"]", |top| top.boolean("websites")).1,
            Problem::WrongType {
                table: Vec::new(),
                key: "websites".to_owned(),
                found: ValueType::Strings,
                expected: ValueType::Boolean,
            }
        );
    }

    #[test]
    fn a_key_its_table_must_have_is_missing_from_where_the_table_starts() {
        let read = |top: Table<'_, '_>| top.required("version", top.integer("version")?);

        assert_eq!(
            problem("[rules.off]\napp = []\n", read),
            (
                Position { line: 1, column: 1 },
                Problem::MissingKey {
                    table: Vec::new(),
                    key: "version".to_owned(),
                }
            )
        );
    }

    #[test]
    fn an_integer_past_64_bits_reads_as_the_nearest() {
        let document =
            parse("big = 99999999999999999999\nsmall = -99999999999999999999\n").unwrap();
        let top = Table::top(&document);

        assert_eq!(
            top.integer("big").unwrap().map(Spanned::into_inner),
            Some(i64::MAX)
        );
        assert_eq!(
            top.integer("small").unwrap().map(Spanned::into_inner),
            Some(i64::MIN)
        );
    }
}
