#[macro_use]
extern crate failure;
extern crate term;
extern crate difference;
extern crate clap;

use clap::{Arg, App};
use dialoguer::Confirmation;
use difference::{Difference, Changeset};
use failure::Error;
use itertools::Itertools;
use regex::{escape, Regex};
use rnix::parser::{Node, NodeType};
use rnix::tokenizer::Token;
use rnix::types::*;
use rowan::SmolStr;
use rowan::WalkEvent;
use serde_json;
use serde_json::json;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::process::exit;

// Based on the panic! macro.
macro_rules! issue {
    ($msg:expr) => ({
        issue($msg);
    });
    ($fmt:expr, $($arg:tt)+) => ({
        issue(format!($fmt, $($arg)+));
    });
}

// The type is necessary to satisfy the compiler, even though we exit.
fn issue(msg: String) -> ! {
    eprintln!("Something unexpected happened:");
    eprintln!("error: {}", msg);
    eprintln!("Please report an issue at: https://github.com/msteen/nix-update-fetch/issues");
    exit(1);
}

#[derive(Debug, Deserialize)]
struct FetcherArg<'a> {
    position: Pos<'a>,
    value: &'a str,
}

#[derive(Debug, Deserialize)]
struct Pos<'a> {
    file: &'a str,
    line: usize,
    column: usize,
}

struct EditSetEntry<'a> {
    set_entry: &'a SetEntry,
    name: String,
    value: String,
    derived: bool,
}

impl<'a> EditSetEntry<'a> {
    // Automatically convert `&str` to `String`.
    fn new<S: Into<String>, T: Into<String>>(set_entry: &'a SetEntry, name: S, value: T, derived: bool) -> EditSetEntry<'a> {
        EditSetEntry {
            set_entry,
            name: name.into(),
            value: value.into(),
            derived,
        }
    }
}

// https://codereview.stackexchange.com/questions/165393/single-element-from-iterator
trait SingleItem {
    type Item;

    fn single(&mut self) -> Option<Self::Item>;
}

impl<I> SingleItem for I where I: Iterator {
    type Item = I::Item;

    fn single(&mut self) -> Option<Self::Item> {
        if let (x@Some(_), None) = (self.next(), self.next()) { x } else { None }
    }
}

trait NodeExt {
    fn is_token(&self) -> bool;

    fn as_str(&self) -> &str;

    fn debug(&self) -> String;
}

impl NodeExt for Node {
    fn is_token(&self) -> bool {
        if let NodeType::Token(_) = self.kind() { true } else { false }
    }

    fn as_str(&self) -> &str {
        self.leaf_text().map(SmolStr::as_str).unwrap_or("")
    }

    fn debug(&self) -> String {
        format!("{:?}'{}'", self, self)
    }
}

// Based on `escapeNixString`:
// https://github.com/NixOS/nixpkgs/blob/d4224f05074b6b8b44fd9bd68e12d4f55341b872/lib/strings.nix#L316
fn escape_nix_string(s: &str) -> String {
    json!(s).to_string().replace("$", "\\$")
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        exit(1)
    }
}

// We call `run()` in `main()`, which handles the errors.
fn run() -> Result<(), Error> {
    let matches = App::new("nix-fetch-update")
        .version("0.1.0")
        .about("Update a fetcher call")
        .arg(Arg::with_name("context")
            .short("C")
            .long("context")
            .value_name("CONTEXT")
            .help("How much lines of context should be shown at the diff"))
        .arg(Arg::with_name("assume_yes")
            .short("y")
            .long("yes")
            .help("Assume that, yes, you want the changes applied"))
        .arg(Arg::with_name("version")
            .short("v")
            .long("version")
            .value_name("VERSION")
            .help("Change the version regardless of it being used in the fetcher arguments"))
        .arg(Arg::with_name("fetcher_args")
            .value_name("FETCHER_ARGS")
            .help("The fetcher arguments to change")
            .required(true)
            .index(1))
        .get_matches();

    let context = matches.value_of("context").map(str::parse).unwrap_or(Ok(2))?;

    let assume_yes = matches.occurrences_of("assume_yes") > 0;

    let version = matches.value_of("version");

    let mut fetcher_args = match serde_json::from_str::<HashMap<&str, FetcherArg>>(matches.value_of("fetcher_args").unwrap()) {
        Ok(fetcher_args) => fetcher_args.into_iter().collect::<Vec<_>>(),
        Err(e) => bail!("Failed to parse the JSON containing the fetcher arguments:\n  error:{}", e)
    };
    fetcher_args.sort_unstable_by(|(_, arg1), (_, arg2)| {
        let pos1 = &arg1.position;
        let pos2 = &arg2.position;
        (pos1.file, pos1.line).cmp(&(pos2.file, pos2.line))
    });

    let mut file_with_version: Option<&str> = None;

    let groups = fetcher_args.into_iter().group_by(|(_, arg)| arg.position.file);
    for (file, group) in &groups {
        let fetcher_args = group.collect::<Vec<_>>();

        let content = fs::read_to_string(&file)?;
        let mut new_content_len = content.len();
        for (name, arg) in &fetcher_args {
            if arg.value.contains('\n') {
                bail!("Multiline string values are unsupported, yet fetcher argument '{}' has one for its value:\n  value: {:?}", name, arg.value);
            }
            new_content_len += arg.value.len();
        }
        let mut new_content = String::with_capacity(new_content_len);
        let ast = rnix::parse(&content);

        let mut iter = fetcher_args.into_iter();
        let (mut name, mut arg) = iter.next().unwrap();

        let mut edit_set_entries = Vec::new();

        let mut done = false;
        for event in ast.node().preorder() {
            if let WalkEvent::Enter(node) = event {
                let start = node.range().start().to_usize();
                let line = content[..start].lines().count();

                let start_line = content[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let column = content[start_line..start].chars().count() + 1;

                if !(line == arg.position.line && column == arg.position.column) {
                    continue;
                }
                if let Some(set_entry) = to_set_entry(&node)? {
                    resolve_bindings(&mut edit_set_entries, EditSetEntry::new(set_entry, name, arg.value, false))?;

                    if let (Some(version), Some(set_entry)) = (version, lookup_set_entry("version", &node)) {
                        if let Some(prev_file) = file_with_version {
                            if file != prev_file {
                                bail!("A version binding was already found in a previous file:\n  previous: {}\n   current: {}", prev_file, file);
                            } else if edit_set_entries.iter().any(|x| x.name == "version" && x.set_entry.node() != set_entry.node()) {
                                bail!("Different version bindings found in file '{}'.", file);
                            }
                        } else {
                            resolve_bindings(&mut edit_set_entries, EditSetEntry::new(set_entry, "version", version, false))?;
                            file_with_version = Some(file);
                        }
                    }

                    // https://github.com/rust-lang/rfcs/issues/372
                    if let Some((new_name, new_arg)) = iter.next() {
                        name = new_name;
                        arg = new_arg;
                    } else {
                        done = true;
                        break;
                    }
                }
            }
        }

        if !done {
            let pos = arg.position;
            bail!("Fetcher argument '{}' has not been found, while searching from position '{}:{}:{}'.", name, pos.file, pos.line, pos.column);
        }

        // We want to use the reverse insertion order (last binding wins), so we need to reverse the vector first.
        edit_set_entries.reverse();
        edit_set_entries.sort_by(|x, y| {
            let f = |x: &EditSetEntry| x.set_entry.node().range().start().to_usize();
            (f(x), x.derived).cmp(&(f(y), y.derived))
        });

        let mut min_end = 0;
        for event in ast.node().preorder() {
            if let WalkEvent::Enter(node) = event {
                let end = node.range().end().to_usize();
                if let Some(EditSetEntry { name, value, .. }) = edit_set_entries.iter().find(|x| x.set_entry.node() == node) {
                    // TODO: Test if we should only replace the value instead.
                    new_content.push_str(&format!("{} = {};", name, escape_nix_string(&value)));
                    min_end = end;
                } else if end > min_end {
                    new_content.push_str(node.as_str());
                }
            }
        }

        if diff(context, &content, &new_content)? && (assume_yes || Confirmation::new().with_text("Do you want to apply these changes?").show_default(true).interact()?) {
            fs::write(file, new_content)?;
        }
    }

    Ok(())
}

fn to_set_entry(node: &Node) -> Result<Option<&SetEntry>, Error> {
    if let x @ Some(_) = SetEntry::cast(node) {
        Ok(x)
    } else if let Some(inherit) = node.parent().and_then(Inherit::cast) {
        if inherit.from().is_some() {
            bail!("There is no support yet for inherited attributes from an expression {}.", inherit.node().debug());
        }
        if let Some(ident) = node.next_sibling().and_then(Ident::cast) {
            Ok(lookup_set_entry(ident.as_str(), ident.node()))
        } else {
            issue!("Expected identifier node, yet found {:?} in inherit node {}.", node.next_sibling().map(Node::debug), inherit.node().debug());
        }
    } else {
        Ok(None)
    }
}

fn lookup_set_entry<'a>(name: &str, mut node: &'a Node) -> Option<&'a SetEntry> {
    loop {
        if let Some(new_node) = node.prev_sibling() {
            node = new_node;
        } else if let Some(new_node) = node.parent() {
            node = new_node;
        } else {
            return None;
        }
        if let Some(set_entry) = SetEntry::cast(node).and_then(|set_entry|
                set_entry.key().path().single().and_then(Ident::cast).filter(|ident| ident.as_str() == name).map(|_| set_entry)) {
            return Some(set_entry);
        }
    }
}

fn checked_lookup_set_entry<'a>(name: &str, node: &'a Node) -> Result<&'a SetEntry, Error> {
    if let Some(set_entry) = lookup_set_entry(name, node) {
        Ok(set_entry)
    } else {
        bail!("Could not find binding '{}'.", name);
    }
}

fn resolve_bindings<'a>(edit_set_entries: &mut Vec<EditSetEntry<'a>>, edit_set_entry: EditSetEntry<'a>) -> Result<(), Error> {
    let EditSetEntry { set_entry, name, value, derived } = edit_set_entry;

    let rhs = set_entry.value();
    match rhs.kind() {
        // Example: "${pname}-${version}";
        // Build a regular expression to match interpolated identifiers with the value.
        NodeType::Interpol => {
            let mut regex_format = String::with_capacity(value.len() + 16);
            let mut names = Vec::new();
            for node in rhs.children() {
                if let Some(child) = node.children().single() {
                    let s = child.to_string();
                    match node.kind() {
                        NodeType::InterpolLiteral => {
                            regex_format.push_str(&escape(match child.kind() {
                                NodeType::Token(Token::InterpolStart) => s.trim_end_matches("${"),
                                NodeType::Token(Token::InterpolEndStart) => s.trim_start_matches('}').trim_end_matches("${"),
                                NodeType::Token(Token::InterpolEnd) => s.trim_start_matches('}'),
                                _ => issue!("Expected an interpolation start, end/start, or end token, yet found token {}.", child.debug()),
                            }));
                        },
                        NodeType::InterpolAst => {
                            match child.kind() {
                                NodeType::Token(Token::Ident) => {
                                    regex_format.push_str(&format!(r#"(?P<{}>.+)"#, s));
                                    names.push(s);
                                },
                                NodeType::Apply => {
                                    let mut lambda = Apply::cast(child).unwrap().lambda();
                                    if let Some(index_set) = IndexSet::cast(lambda) {
                                        lambda = index_set.index();
                                    }
                                    if lambda.kind() == NodeType::Token(Token::Ident) && lambda.to_string() == "majorMinor" {
                                        regex_format.push_str(r#"[0-9]+\.[0-9]+"#);
                                    } else {
                                        bail!("Unsupported lambda application {}.", lambda.debug())
                                    }
                                },
                                _ => bail!("Unsupported interpolated token {}.", child.debug())
                            }
                        },
                        _ => issue!("Expected interpolation literal or AST, yet found node {}.", node.debug())
                    }
                } else {
                    bail!("Expected node containing a single child node, yet found node {}.", node.debug());
                }
            }
            if let Ok(re) = Regex::new(&regex_format) {
                let test_value = escape_nix_string(&value);
                if let Some(captures) = re.captures(&test_value) {
                    for name in names.into_iter() {
                        let set_entry = checked_lookup_set_entry(&name, set_entry.node())?;
                        let value = captures.name(&name).unwrap().as_str();
                        resolve_bindings(edit_set_entries, EditSetEntry::new(set_entry, name, value, true))?;
                    }
                } else {
                    bail!("The constructed regular expression failed to match:\n  regex: {}\n  value: {}.", regex_format, test_value);
                }
            } else {
                issue!("Failed to construct regular expression '{}'.", regex_format);
            }
        },

        // Example: version
        // Search for this binding instead.
        NodeType::Token(Token::Ident) => {
            let ident_name = Ident::cast(rhs).unwrap().as_str();
            // Do not consider `null` to be a variable needing to be looked up, consider it just like a string instead.
            if ident_name == "null" {
                edit_set_entries.push(EditSetEntry::new(set_entry, name, value, derived));
            } else {
                let set_entry = checked_lookup_set_entry(ident_name, set_entry.node())?;
                resolve_bindings(edit_set_entries, EditSetEntry::new(set_entry, ident_name, value, true))?;
            }
        },

        // Example: "0.1.0"
        // What we are looking for, a simple string binding.
        NodeType::Token(Token::String) => {
            edit_set_entries.push(EditSetEntry::new(set_entry, name, value, derived));
        },

        _ => bail!("Unsupported value node {}.", rhs.debug())
    }

    Ok(())
}

fn diff(context: usize, text1: &str, text2: &str) -> Result<bool, Error> {
    let Changeset { diffs, .. } = Changeset::new(text1, text2, "\n");

    let mut t = term::stdout().unwrap();

    let last_i = diffs.len() - 1;
    for (i, diff) in diffs.iter().enumerate() {
        match diff {
            Difference::Same(x) => {
                t.reset().unwrap();
                let mut lines = x.lines();
                if i != 0 {
                    for x in lines.by_ref().take(context) {
                        writeln!(t, " {}", x)?;
                    }
                }
                if i != last_i {
                    for x in lines.rev().take(context).collect::<Vec<&str>>().into_iter().rev() {
                        writeln!(t, " {}", x)?;
                    }
                }
            }
            Difference::Add(x) => {
                t.fg(term::color::GREEN).unwrap();
                for x in x.lines() {
                    writeln!(t, "+{}", x)?;
                }
            }
            Difference::Rem(x) => {
                t.fg(term::color::RED).unwrap();
                for x in x.lines() {
                    writeln!(t, "-{}", x)?;
                }
            }
        }
    }
    t.reset().unwrap();
    t.flush().unwrap();

    Ok(last_i != 0) // Was there a difference?
}
