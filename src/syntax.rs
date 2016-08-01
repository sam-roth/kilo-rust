use std::collections::HashSet;
use std::iter::{Iterator, Peekable};

#[derive(Debug, Copy, Clone)]
pub enum Highlight {
    Normal,
    Nonprint,
    Comment,
    MultiLineComment,
    PrimaryKeyword,
    SecondaryKeyword,
    String,
    Number,
    Match,
}

impl Highlight {
    pub fn color(self) -> u8 {
        use syntax::Highlight::*;
        match self {
            Comment | MultiLineComment => 36, // cyan
            PrimaryKeyword => 33,             // yellow
            SecondaryKeyword => 32,           // green
            String => 35,                     // magenta
            Number => 31,                     // red
            Match => 34,                      // blue
            Normal | Nonprint => 0,           // white
        }
    }
}

#[derive(Debug)]
pub struct Syntax {
    pub file_extensions: HashSet<String>,
    pub primary_keywords: HashSet<String>,
    pub secondary_keywords: HashSet<String>,
}

fn peek<Iter, Item>(iter: &Iter, count: usize) -> Vec<Item>
        where Iter: Iterator<Item=Item> + Clone, Item: Copy {
    let iter = iter.clone();
    iter.take(count).collect()
}

impl Syntax {
    pub fn highlight(&self, s: &str) -> Vec<Highlight> {
        let mut result = vec![];
        let mut it = s.chars().peekable();

        macro_rules! classify {
            ($token_len:expr, $highlight:expr) => {{
                for _ in 0..$token_len {
                    result.push($highlight);
                }
            }};
        }

        loop {
            let det = peek(&it, 2);

            match &det[..] {
                &[ch, ..] if ch.is_whitespace() => {
                    it.next();
                    classify!(1, Highlight::Normal);
                },
                &[ch, ..] if ch.is_numeric() => {
                    it.next();
                    classify!(1, Highlight::Number);
                },
                &['.', ch] if ch.is_numeric() => {
                    it.next();
                    classify!(1, Highlight::Number);
                },
                &[ch, ..] if ch.is_alphabetic() => {
                    let token = read_pred(&mut it,
                                          |ch| ch.is_alphanumeric() || ch == '_');

                    let classification =
                        if self.primary_keywords.contains(&token) {
                            Highlight::PrimaryKeyword
                        } else if self.secondary_keywords.contains(&token) {
                            Highlight::SecondaryKeyword
                        } else {
                            Highlight::Normal
                        };

                    classify!(token.chars().count(), classification);
                },
                &[quote_char, ..] if quote_char == '\'' || quote_char == '"' => {
                    it.next();
                    let mut count = 1;

                    loop {
                        if let Some(ch) = it.next() {
                            count += 1;
                            match ch {
                                '\\' => {
                                    if let Some(_) = it.next() {
                                        count += 1;
                                    }
                                },
                                c if c == quote_char => {
                                    break;
                                },
                                _ => ()
                            }
                        } else {
                            break;
                        }
                    }

                    classify!(count, Highlight::String);
                },
                &['/', '/'] => {
                    classify!(it.by_ref().count(), Highlight::Comment);
                },
                // &['/', '*'] => {
                // },
                &[] => {
                    break;
                },
                _ => {
                    it.next();
                    classify!(1, Highlight::Normal);
                },
            }
        }
        result
    }
}

macro_rules! string_set_helper {
    ($($x:expr),*) => {{
        let mut result: HashSet<String> = HashSet::new();
        $(result.insert($x.to_owned());)*
        result
    }};
}

macro_rules! string_set {
    ($($x:expr),*) => { string_set_helper![$($x),*] };
    ($($x:expr,)*) => { string_set_helper![$($x),*] };
}

pub fn make_rust_syntax() -> Syntax {
    let mut result = Syntax {
        file_extensions: string_set![".rs"],
        primary_keywords: string_set![
            "as", "break", "const", "continue", "crate", "else",
            "enum", "extern", "false", "fn", "for", "if", "impl",
            "in", "let", "loop", "match", "mod", "move", "mut", "pub",
            "ref", "return", "Self", "self", "static", "struct",
            "trait", "true", "type", "unsafe", "use", "where", "while",
        ],
        secondary_keywords: string_set![
            "float", "str", "char", "bool", "f32", "f64",
        ],
    };

    for prefix in &["u", "i"] {
        for suffix in &["8", "16", "32", "64", "size"] {
            result.secondary_keywords.insert(format!("{}{}", prefix, suffix));
        }
    }

    result
}

fn read_pred<I, F>(it: &mut Peekable<I>, pred: F) -> String
        where I: Iterator<Item=char>, F: Fn(char) -> bool {
    let mut result = String::new();

    loop {
        match it.peek() {
            Some(ch) if pred(*ch) => {
                result.push(*ch);
            },
            _ => break,
        }

        it.next();
    }

    result
}
