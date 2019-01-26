// #[macro_use]
// extern crate clap;

#[macro_use] extern crate failure;
extern crate term;
extern crate difference;

extern crate clap;
use clap::{Arg, App};
use failure::Error;
use rnix::{
    parser::{Node, NodeType},
    tokenizer::Token,
    types::*,
};
use rowan::WalkEvent;
use std::fs;
use serde::{Serialize, Deserialize};
use serde_json;
use std::collections::HashMap;
use rowan::SmolStr;
use serde_json::json;
use itertools::Itertools;
use std::process::exit;
use regex::escape;
use regex::Regex;
use difference::{Difference, Changeset};
use std::io::Write;
use dialoguer::Confirmation;

#[derive(Debug, Serialize, Deserialize)]
struct FetcherArg {
    position: Pos,
    value: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Pos {
    file: String,
    line: usize,
    column: usize,
}

struct UpdateSetEntry<'a> {
    name: String,
    value: String,
    node: &'a Node,
}

// Based on `escapeNixString`:
// https://github.com/NixOS/nixpkgs/blob/d4224f05074b6b8b44fd9bd68e12d4f55341b872/lib/strings.nix#L316
fn escape_nix_string(s: &str) -> String {
    json!(s).to_string().replace("$", "\\$")
}

fn node_text(node: &Node) -> String {
    node.leaf_text().map(SmolStr::as_str).map(String::from).unwrap_or_else(|| "".to_string())
}

fn find_single<'a, T>(mut iter: impl Iterator<Item = &'a T>) -> Option<&'a T> {
    if let (x@Some(_), None) = (iter.next(), iter.next()) { x } else { None }
}

fn run() -> Result<(), Error> {
    let matches = App::new("nix-fetch-update")
                          .version("0.1.0")
                          .about("Update a fetcher call")
                          .arg(Arg::with_name("version")
                               .short("v")
                               .long("version")
                               .value_name("VERSION")
                               .help("Change the version regardless of it being used in the fetcher arguments")
                               .takes_value(true))
                          .arg(Arg::with_name("FETCHER_ARGS")
                               .help("The fetcher arguments to change")
                               .required(true)
                               .index(1))
                          .get_matches();

    let version = matches.value_of("version");

    let fetcher_args: HashMap<String, FetcherArg> = serde_json::from_str(matches.value_of("FETCHER_ARGS").unwrap())?;
    let mut fetcher_args: Vec<(String, FetcherArg)> = fetcher_args.into_iter().collect();
    fetcher_args.sort_unstable_by(|(_, arg1), (_, arg2)| {
        let pos1 = &arg1.position;
        let pos2 = &arg2.position;
        (&pos1.file, &pos1.line).cmp(&(&pos2.file, &pos2.line))
    });

    let mut file_with_version: Option<String> = None;

    let groups = fetcher_args.into_iter().group_by(|(_, arg)| arg.position.file.clone());
    for (file, group) in &groups {
        let fetcher_args: Vec<(String, FetcherArg)> = group.collect();

        let content = fs::read_to_string(&file)?;
        let mut new_content_len = content.len();
        for (_, arg) in &fetcher_args {
            new_content_len += arg.value.len();
        }
        let mut new_content = String::with_capacity(new_content_len);
        let ast = rnix::parse(&content);

        let mut iter = fetcher_args.into_iter();
        let (mut name, mut arg) = iter.next().unwrap();

        let mut update_set_entries = Vec::new();

        let mut done = false;
        for event in ast.node().preorder() {
            if let WalkEvent::Enter(node) = event {
                let start = node.range().start().to_usize();
                let line = content[..start].lines().count();
                let start_line = content[..start].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let column = content[start_line..start].chars().count() + 1;
                if line == arg.position.line && column == arg.position.column {
                    if let Some(set_entry) = SetEntry::cast(node) {
                        // for child in Interpol::cast(set_entry.value()).iter().flatten().filter_map(InterpolAst::cast).filter_map(|x| find_single(x.children()).filter(|x| x.kind()== NodeType::Token(Token::Ident))
                        let value = set_entry.value();
                        match value.kind() {
                            NodeType::Interpol => {
                                let mut regex_format = String::with_capacity(arg.value.len() + 16);
                                let mut names = Vec::new();
                                for child in value.children() {
                                    if let Some(token) = find_single(child.children()) {
                                        let s = token.to_string();
                                        match child.kind() {
                                            NodeType::InterpolLiteral => {
                                                let lit = match token.kind() {
                                                    NodeType::Token(Token::InterpolStart) => s.trim_end_matches("${"),
                                                    NodeType::Token(Token::InterpolEndStart) => s.trim_start_matches('}').trim_end_matches("${"),
                                                    NodeType::Token(Token::InterpolEnd) => s.trim_start_matches('}'),
                                                    _ => return Err(format_err!("Unsupported kind: {:?}.", token)),
                                                };
                                                regex_format.push_str(&escape(lit));
                                            },
                                            NodeType::InterpolAst => {
                                                match token.kind() {
                                                    NodeType::Token(Token::Ident) => {
                                                        regex_format.push_str(&format!(r#"(?P<{}>.+)"#, s));
                                                        names.push(s);
                                                    },
                                                    NodeType::Apply => {
                                                        let mut lambda = Apply::cast(token).unwrap().lambda();
                                                        if let Some(index_set) = IndexSet::cast(lambda) {
                                                            lambda = index_set.index();
                                                        }
                                                        if lambda.kind() == NodeType::Token(Token::Ident) && lambda.to_string() == "majorMinor" {
                                                            regex_format.push_str(r#"[0-9]+\.[0-9]+"#);
                                                        } else {
                                                            return Err(format_err!("Unsupported application: {:?}.", token))
                                                        }
                                                    },
                                                    _ => return Err(format_err!("Unsupported kind: {:?}.", token)),
                                                }
                                            },
                                            _ => {
                                                return Err(format_err!("Unsupported kind: {:?}.", child));
                                            }
                                        }
                                    } else {
                                        return Err(format_err!("Expected single child: {:?}.", child));
                                    }
                                }
                                if let Ok(re) = Regex::new(&regex_format) {
                                    let test_value = escape_nix_string(&arg.value);
                                    if let Some(captures) = re.captures(&test_value) {
                                        for name in &names {
                                            if let Some(ident_node) = find_set_entry(name, &node) {
                                                update_set_entries.push(UpdateSetEntry { name: name.to_string(), value: captures.name(name).unwrap().as_str().to_string(), node: ident_node });
                                            } else {
                                                return Err(format_err!("Could not find binding: {:?}.", name));
                                            }
                                        }
                                    } else {
                                        return Err(format_err!("The constructed regex failed to match:\n  regex: {}\n  value: {}.", regex_format, test_value));
                                    }
                                } else {
                                    return Err(format_err!("Failed to construct the regex format based on the value: {:?}.", regex_format));
                                }
                            },
                            NodeType::Token(Token::Ident) => {
                                let name = value.to_string();
                                if let Some(ident_node) = find_set_entry(&name, &node) {
                                    update_set_entries.push(UpdateSetEntry { name, value: arg.value, node: ident_node });
                                }
                            },
                            NodeType::Token(Token::String) => update_set_entries.push(UpdateSetEntry { name: name.clone(), value: arg.value, node }),
                            _ => return Err(format_err!("Unsupported value token: {:?}.", value)),
                        }
                        if let Some(version) = version {
                            if let Some(version_node) = find_set_entry("version", &node) {
                                if let Some(prev_file) = file_with_version.clone() {
                                    if file != prev_file {
                                        return Err(format_err!("A version binding was found in both '{}' and '{}'.", prev_file, file));
                                    } else {
                                        for update_set_entry in &update_set_entries {
                                            if update_set_entry.name == "version" && update_set_entry.node != version_node {
                                                return Err(format_err!("Two different version bindings found in '{}'.", file));
                                            }
                                        }
                                    }
                                } else {
                                    update_set_entries.push(UpdateSetEntry { name: "version".to_string(), value: version.to_string(), node: version_node });
                                    file_with_version = Some(file.clone());
                                }
                            }
                        }
                        // FIXME: Is there a better way to destruct tuples?
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
        }
        if !done {
            // FIXME: Pass `name` to the formatter.
            return Err(format_err!("A fetcher argument {} was not found at its position", name));
        }

        update_set_entries.sort_unstable_by(|x, y| x.node.range().start().to_usize().cmp(&y.node.range().start().to_usize()));

        let mut min_end = 0;
        for event in ast.node().preorder() {
            if let WalkEvent::Enter(node) = event {
                let end = node.range().end().to_usize();
                if let Some(UpdateSetEntry { name, value, .. }) = update_set_entries.iter().find(|x| x.node == node) {
                    new_content.push_str(&format!("{} = {};", name, escape_nix_string(&value)));
                    min_end = end;
                } else if end > min_end {
                    new_content.push_str(&node_text(node));
                }
            }
        }

        if diff_github_style(&content, &new_content)? && Confirmation::new().with_text("Do you want to apply these changes?").show_default(true).interact()? {
            fs::write(file, new_content)?;
        }
    }

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {}", e);
        exit(1)
    }
}

fn find_set_entry<'a>(name: &str, mut node: &'a Node) -> Option<&'a Node> {
    loop {
        if let Some(new_node) = node.prev_sibling() {
            node = new_node;
        } else if let Some(new_node) = node.parent() {
            node = new_node;
        } else {
            return None;
        }
        // let set_entry = SetEntry::cast(node)?;
        // let mut path = set_entry.key().path();
        // let Some(ident_node) = path.next() {
        if let Some(set_entry) = SetEntry::cast(node) {
            let mut path = set_entry.key().path();
            if let Some(ident_node) = path.next() {
                if let Some(ident) = Ident::cast(&ident_node) {
                    if path.next().is_none() && ident.as_str() == name && set_entry.value().kind() == NodeType::Token(Token::String) {
                        return Some(node)
                    }
                }
            }
        }
    }
}

fn diff_github_style(text1: &str, text2: &str) -> Result<bool, Error> {
    let Changeset { diffs, .. } = Changeset::new(text1, text2, "\n");

    let mut t = term::stdout().unwrap();

    let last_i = diffs.len() - 1;
    for i in 0..=last_i {
        match diffs[i] {
            Difference::Same(ref x) => {
                t.reset().unwrap();
                let mut lines = x.lines();
                if i != 0 {
                    for x in lines.by_ref().take(2) {
                        writeln!(t, " {}", x)?;
                    }
                }
                if i != last_i {
                    for x in lines.rev().take(2).collect::<Vec<&str>>().into_iter().rev() {
                        writeln!(t, " {}", x)?;
                    }
                }
            }
            Difference::Add(ref x) => {
                match diffs[i - 1] {
                    Difference::Rem(ref y) => {
                        let mut x = x.lines();
                        let mut y = y.lines();
                        while let (Some(x), Some(y)) = (x.next(), y.next()) {
                            t.fg(term::color::GREEN).unwrap();
                            write!(t, "+")?;
                            let Changeset { diffs, .. } = Changeset::new(y, x, " ");
                            for c in diffs {
                                match c {
                                    Difference::Same(ref z) => {
                                        t.fg(term::color::GREEN).unwrap();
                                        write!(t, "{}", z)?;
                                        write!(t, " ")?;
                                    }
                                    Difference::Add(ref z) => {
                                        t.fg(term::color::WHITE).unwrap();
                                        t.bg(term::color::GREEN).unwrap();
                                        write!(t, "{}", z)?;
                                        t.reset().unwrap();
                                        write!(t, " ")?;
                                    }
                                    _ => (),
                                }
                            }
                            writeln!(t)?;
                        }
                    }
                    _ => {
                        t.fg(term::color::BRIGHT_GREEN).unwrap();
                        for x in x.lines() {
                            writeln!(t, "+{}", x)?;
                        }
                    }
                };
            }
            Difference::Rem(ref x) => {
                t.fg(term::color::RED).unwrap();
                for x in x.lines() {
                    writeln!(t, "-{}", x)?;
                }
            }
        }
    }
    t.reset().unwrap();
    t.flush().unwrap();
    Ok(last_i != 0)
}
