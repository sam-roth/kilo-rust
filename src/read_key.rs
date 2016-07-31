
use std::io;

/// Poll stream once for input
fn maybe_read_byte(stream: &mut io::Read) -> io::Result<Option<u8>> {
    let mut buf = [0u8; 1];
    if stream.read(&mut buf)? == 0 {
        Ok(None)
    } else {
        Ok(Some(buf[0]))
    }
}

/// Poll stream until input shows up
fn read_byte(stream: &mut io::Read) -> io::Result<u8> {
    loop {
        if let Some(b) = maybe_read_byte(stream)? {
            return Ok(b);
        }
    }
}

#[allow(dead_code)]
pub mod key_codes {
    pub const CTRL_C: u8     = 3;
    pub const CTRL_D: u8     = 4;
    pub const CTRL_F: u8     = 6;
    pub const CTRL_H: u8     = 8;
    pub const CTRL_L: u8     = 12;
    pub const ENTER: u8      = 13;
    pub const CTRL_Q: u8     = 17;
    pub const CTRL_S: u8     = 19;
    pub const CTRL_U: u8     = 21;
    pub const BACKSPACE: u8 = 127;
}

#[derive(Debug, Copy, Clone)]
pub enum Key {
    Char(u8),
    Esc,
    Del,
    PageUp,
    PageDown,
    ArrowUp,
    ArrowDown,
    ArrowRight,
    ArrowLeft,
    Home,
    End,
}

#[derive(Debug)]
pub enum Escape {
    Char(u8),
    Esc,
    CSI(Vec<u8>),
    SS3(u8),
    InvalidEscape,
}

impl Escape {
    pub fn interpret(&self) -> Option<Key> {
        match self {
            &Escape::Char(ch) => Some(Key::Char(ch)),
            &Escape::Esc => Some(Key::Esc),
            &Escape::CSI(ref seq) => match &seq[..] {
                b"3~" => Some(Key::Del),
                b"5~" => Some(Key::PageUp),
                b"6~" => Some(Key::PageDown),
                b"A"  => Some(Key::ArrowUp),
                b"B"  => Some(Key::ArrowDown),
                b"C"  => Some(Key::ArrowRight),
                b"D"  => Some(Key::ArrowLeft),
                b"H"  => Some(Key::Home),
                b"F"  => Some(Key::End),
                _     => None,
            },
            &Escape::SS3(ch) => match ch {
                b'H'  => Some(Key::Home),
                b'F'  => Some(Key::End),
                _     => None,
            },
            &Escape::InvalidEscape => None,
        }
    }
}

fn read_csi(stream: &mut io::Read) -> io::Result<Escape> {
    let mut buf = vec![];

    loop {
        let byte = match maybe_read_byte(stream)? {
            Some(b) => b,
            None    => return Ok(Escape::Esc), // This wasn't a real escape sequence
        };

        buf.push(byte);

        if (64...126).contains(byte) {
            break;              // Final character
        }
    }

    Ok(Escape::CSI(buf))
}

fn read_ss3(stream: &mut io::Read) -> io::Result<Escape> {
    match maybe_read_byte(stream)? {
        Some(byte)  => Ok(Escape::SS3(byte)),
        None        => Ok(Escape::Esc), // Not a real escape sequence
    }
}

pub fn read_escape(stream: &mut io::Read) -> io::Result<Escape> {
    match read_byte(stream)? {
        // Escape sequence
        0x1b => match maybe_read_byte(stream)? {
            Some(b'[') => read_csi(stream), // Control sequence initiator
            Some(b'O') => read_ss3(stream), // Single shift three
            None => Ok(Escape::Esc),        // Plain old escape
            _ => Ok(Escape::InvalidEscape), // Invalid
        },
        // Normal character entry
        byte => Ok(Escape::Char(byte)),
    }
}
