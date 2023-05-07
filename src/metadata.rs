/*!
CSV metadata types.
*/
use std::fmt;
use std::fs::File;
use std::io::{Read, Seek, Write};
use std::path::Path;

use csv::{Reader, ReaderBuilder};
use tabwriter::TabWriter;

use crate::{error::Result, field_type::Type, snip::snip_preamble};

/// Primary CSV metadata. Generated by
/// [`Sniffer::sniff_path`](../struct.Sniffer.html#method.sniff_path) or
/// [`Sniffer::sniff_reader`](../struct.Sniffer.html#method.sniff_reader) after examining a CSV
/// file.
#[derive(Debug, Clone, PartialEq)]
pub struct Metadata {
    /// [`Dialect`](struct.Dialect.html) subtype.
    pub dialect: Dialect,
    /// Average record length (in bytes).
    pub avg_record_len: usize,
    /// (Maximum) number of fields per record.
    pub num_fields: usize,
    /// field/column names
    pub fields: Vec<String>,
    /// Inferred field types.
    pub types: Vec<Type>,
}
impl fmt::Display for Metadata {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Metadata")?;
        writeln!(f, "========")?;
        writeln!(f, "{}", self.dialect)?;
        writeln!(f, "Average record length (bytes): {}", self.avg_record_len)?;
        writeln!(f, "Number of fields: {}", self.num_fields)?;
        writeln!(f, "Fields:")?;

        let mut tabwtr = TabWriter::new(vec![]);

        for (i, ty) in self.types.iter().enumerate() {
            writeln!(
                &mut tabwtr,
                "\t{}:\t{}\t{}",
                i,
                ty,
                self.fields.get(i).unwrap_or(&String::new())
            )
            .unwrap_or_default();
        }
        // safety: we just wrote to the tabwriter, so it should be ok to unwrap
        tabwtr.flush().unwrap();

        // safety: we just flushed the tabwriter, so it should be ok to unwrap the inner vec
        // the second unwrap is to convert the vec<u8> to a String, so its also safe.
        let tabbed_field_list = simdutf8::basic::from_utf8(&tabwtr.into_inner().unwrap())
            .unwrap()
            .to_string();
        writeln!(f, "{tabbed_field_list}")?;

        Ok(())
    }
}

/// Dialect-level metadata. This type encapsulates the details to be used to derive a
/// `ReaderBuilder` object (in the [`csv`](https://docs.rs/csv) crate).
#[derive(Clone)]
pub struct Dialect {
    /// CSV delimiter (field separator).
    pub delimiter: u8,
    /// [`Header`](struct.Header.html) subtype (header row boolean and number of preamble rows).
    pub header: Header,
    /// Record quoting details.
    pub quote: Quote,
    /// Whether or not the number of fields in a record is allowed to change.
    pub flexible: bool,
    /// Whether the file is utf-8 encoded.
    pub is_utf8: bool,
}
impl PartialEq for Dialect {
    fn eq(&self, other: &Dialect) -> bool {
        self.delimiter == other.delimiter
            && self.header == other.header
            && self.quote == other.quote
            && self.flexible == other.flexible
            && self.is_utf8 == other.is_utf8
    }
}
impl fmt::Debug for Dialect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("Dialect")
            .field("delimiter", &char::from(self.delimiter))
            .field("header", &self.header)
            .field("quote", &self.quote)
            .field("flexible", &self.flexible)
            .field("is_utf8", &self.is_utf8)
            .finish()
    }
}
impl fmt::Display for Dialect {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        writeln!(f, "Dialect:")?;
        writeln!(f, "\tDelimiter: {}", char::from(self.delimiter))?;
        writeln!(f, "\tHas header row?: {}", self.header.has_header_row)?;
        writeln!(
            f,
            "\tNumber of preamble rows: {}",
            self.header.num_preamble_rows
        )?;
        writeln!(
            f,
            "\tQuote character: {}",
            match self.quote {
                Quote::Some(chr) => format!("{}", char::from(chr)),
                Quote::None => "none".into(),
            }
        )?;
        writeln!(f, "\tFlexible: {}", self.flexible)?;
        writeln!(f, "\tIs utf-8 encoded?: {}", self.is_utf8)
    }
}
impl Dialect {
    /// Use this `Dialect` to open a file specified by provided path. Returns a `Reader` (from the
    /// [`csv`](https://docs.rs/csv) crate). Fails on file opening or reading errors.
    pub fn open_path<P: AsRef<Path>>(&self, path: P) -> Result<Reader<File>> {
        self.open_reader(File::open(path)?)
    }

    /// Use this `Dialect` to create a `Reader` (from the [`csv`](https://docs.rs/csv) crate) using
    /// the provided reader. Fails if unable to read from the reader.
    pub fn open_reader<R: Read + Seek>(&self, mut rdr: R) -> Result<Reader<R>> {
        snip_preamble(&mut rdr, self.header.num_preamble_rows)?;
        let bldr: ReaderBuilder = self.clone().into();
        Ok(bldr.from_reader(rdr))
    }
}
impl From<Dialect> for ReaderBuilder {
    fn from(dialect: Dialect) -> ReaderBuilder {
        let mut bldr = ReaderBuilder::new();
        bldr.delimiter(dialect.delimiter)
            .has_headers(dialect.header.has_header_row)
            .flexible(dialect.flexible);

        match dialect.quote {
            Quote::Some(character) => {
                bldr.quoting(true);
                bldr.quote(character);
            }
            Quote::None => {
                bldr.quoting(false);
            }
        }

        bldr
    }
}

/// Metadata about the header of the CSV file.
#[derive(Debug, Clone, PartialEq)]
pub struct Header {
    /// Whether or not this CSV file has a header row (a row containing column labels).
    pub has_header_row: bool,
    /// Number of rows that occur before either the header row (if `has_header_row` is `true), or
    /// the first data row.
    pub num_preamble_rows: usize,
}

/// Metadata about the quoting style of the CSV file.
#[derive(Clone, PartialEq)]
pub enum Quote {
    /// Quotes are not used in the CSV file.
    None,
    /// Quotes are enabled, with the provided character used as the quote character.
    Some(u8),
}
impl fmt::Debug for Quote {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Quote::Some(ref character) => f
                .debug_struct("Some")
                .field("character", &char::from(*character))
                .finish(),
            Quote::None => write!(f, "None"),
        }
    }
}

/// The escape character (or `Disabled` if escaping is disabled)
#[derive(Clone, PartialEq)]
pub enum Escape {
    /// Escapes are enabled, with the provided character as the escape character.
    Enabled(u8),
    /// Escapes are disabled.
    Disabled,
}
impl From<Escape> for Option<u8> {
    fn from(escape: Escape) -> Option<u8> {
        match escape {
            Escape::Enabled(chr) => Some(chr),
            Escape::Disabled => None,
        }
    }
}
impl fmt::Debug for Escape {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Escape::Enabled(chr) => write!(f, "Enabled({})", char::from(chr)),
            Escape::Disabled => write!(f, "Disabled"),
        }
    }
}

/// The comment character (or `Disabled` if commenting doesn't exist in this dialect)
#[derive(Clone, PartialEq)]
pub enum Comment {
    /// Comments are enabled, with the provided character as the comment character.
    Enabled(u8),
    /// Comments are disabled.
    Disabled,
}
impl From<Comment> for Option<u8> {
    fn from(comment: Comment) -> Option<u8> {
        match comment {
            Comment::Enabled(chr) => Some(chr),
            Comment::Disabled => None,
        }
    }
}
impl fmt::Debug for Comment {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Comment::Enabled(chr) => write!(f, "Enabled({})", char::from(chr)),
            Comment::Disabled => write!(f, "Disabled"),
        }
    }
}
