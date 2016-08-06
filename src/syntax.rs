use std::collections::HashSet;
use std::iter::{Iterator, Peekable};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Highlight {
    Normal,
    Comment,
    MultiLineComment,
    PrimaryKeyword,
    SecondaryKeyword,
    String,
    Number,
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
            Normal => 0,                      // white
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

#[derive(Debug)]
pub struct HighlightResult {
    pub highlight: Vec<Highlight>,
    pub initial_state: Highlight,
    pub ending_state: Highlight,
}

impl Syntax {
    pub fn highlight(&self, initial_state: Highlight, s: &str) -> HighlightResult {
        let mut ending_state = Highlight::Normal;
        let mut result = vec![];
        let mut it = s.chars().peekable();

        macro_rules! classify {
            ($token_len:expr, $highlight:expr) => {{
                for _ in 0..$token_len {
                    result.push($highlight);
                }
            }};
        }

        if initial_state == Highlight::MultiLineComment {
            let (count, continues) = multiline_comment_count(&mut it);
            if continues {
                ending_state = Highlight::MultiLineComment;
            }

            classify!(count, Highlight::MultiLineComment);
        } else if initial_state != Highlight::Normal {
            panic!("initial_state must be MultiLineComment or Normal");
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
                &['/', '*'] => {
                    it.next();
                    it.next();

                    let (rest_count, continues) = multiline_comment_count(&mut it);
                    let count = 2 + rest_count;

                    if continues {
                        ending_state = Highlight::MultiLineComment;
                    }

                    classify!(count, Highlight::MultiLineComment);
                },
                &[] => {
                    break;
                },
                _ => {
                    it.next();
                    classify!(1, Highlight::Normal);
                },
            }
        }

        HighlightResult{
            highlight: result,
            initial_state: initial_state,
            ending_state: ending_state,
        }
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

fn multiline_comment_count<I>(iter: &mut I) -> (usize, bool)
        where I: Iterator<Item=char> {
    let mut count = 0;

    loop {
        let ch = iter.next();
        match ch {
            Some('*') => match iter.next() {
                Some('/') => {
                    return (count + 2, false);
                },
                None => {
                    return (count + 1, true);
                },
                _ => {
                    count += 2;
                }
            },
            None => {
                return (count, true);
            },
            _ => {
                count += 1;
            },
        }
    }
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
