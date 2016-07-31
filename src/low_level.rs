use std::io;
use std::mem::zeroed;
use libc::*;

pub type Fd = c_int;

pub fn get_termios(fd: Fd) -> Result<termios, io::Error> {
    let mut result: termios = unsafe { zeroed() };

    if unsafe { tcgetattr(fd, &mut result) } == -1 {
        return Err(io::Error::last_os_error());
    }

    Ok(result)
}

pub fn set_termios(fd: Fd, optional_actions: c_int, termios_: &termios) -> Result<(), io::Error> {
    if unsafe { tcsetattr(fd, optional_actions, termios_) } < 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}
