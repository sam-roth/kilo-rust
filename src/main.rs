#![feature(question_mark,
           range_contains,
           inclusive_range_syntax,
           unicode,
           slice_patterns,
           type_ascription)]
#![warn(trivial_numeric_casts)]

extern crate libc;

mod editor;
mod low_level;
mod read_key;
mod syntax;

use std::path::Path;
use std::{io, env, process};
use std::borrow::Borrow;

use editor::Editor;

fn usage() {
    let prog_name = env::args()
        .nth(0)
        .unwrap_or("kilo_rust".to_owned());

    println!("Usage: {} FILENAME", prog_name);
}

fn main() {
    let syntax_db = syntax::make_syntax_db();

    let file_name =
        if let Some(file_name) = env::args().nth(1) {
            file_name
        } else {
            usage();
            process::exit(1);
        };

    let file_path = Path::new(&file_name);
    let syntax = file_path.extension()
        .map(|s| s.to_string_lossy())
        .and_then(|s| syntax_db.get(s.borrow(): &str));

    let stdin = io::stdin();
    let mut editor = Editor::new().unwrap();

    editor.enable_raw_mode()
        .expect("Failed to enable raw mode");

    editor.open(Path::new(&file_name)).unwrap();

    editor.set_syntax(syntax.map(|s| (**s).clone()));

    loop {
        editor.refresh_screen().unwrap();
        let opt_k = read_key::read_escape(&mut stdin.lock()).ok()
            .and_then(|k| k.interpret());

        if let Some(k) = opt_k {
            if !editor.handle_keypress(k) {
                break;
            }
        }
    }
}
