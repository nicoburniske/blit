use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{self, BufWriter, Read, Write},
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

use rustix::{
    event::{PollFd, PollFlags, Timespec, poll},
    termios::{self, OptionalActions, Termios},
};

use crate::protocol::{self, Parser};

pub struct Terminal {
    input: File,
    output: BufWriter<File>,
    original: Termios,
    resize: UnixStream,
    signal: signal_hook::SigId,
    parser: Parser,
    events: VecDeque<protocol::Event>,
    active: bool,
}

pub enum Event {
    Protocol(protocol::Event),
    Resize(Size),
}

pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

impl Terminal {
    pub fn new() -> io::Result<Self> {
        let input = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        let output = BufWriter::new(input.try_clone()?);
        let original = termios::tcgetattr(&input)?;
        let (resize, writer) = UnixStream::pair()?;
        resize.set_nonblocking(true)?;
        let signal = signal_hook::low_level::pipe::register(signal_hook::consts::SIGWINCH, writer)?;
        let mut terminal = Self {
            input,
            output,
            original,
            resize,
            signal,
            parser: Parser::default(),
            events: VecDeque::new(),
            active: false,
        };
        let mut raw = terminal.original.clone();
        raw.make_raw();
        termios::tcsetattr(&terminal.input, OptionalActions::Now, &raw)?;
        terminal.active = true;
        terminal.write_all(protocol::ENTER)?;
        terminal.flush()?;
        Ok(terminal)
    }

    pub fn size(&self) -> io::Result<Size> {
        let size = termios::tcgetwinsize(&self.input)?;
        Ok(Size {
            cols: size.ws_col,
            rows: size.ws_row,
        })
    }

    pub fn read(&mut self, timeout: Option<Duration>) -> io::Result<Option<Event>> {
        let start = Instant::now();
        loop {
            if let Some(event) = self.events.pop_front() {
                return Ok(Some(Event::Protocol(event)));
            }
            let remaining = timeout.map(|timeout| {
                Timespec::try_from(timeout.saturating_sub(start.elapsed())).unwrap_or(Timespec {
                    tv_sec: i64::MAX,
                    tv_nsec: 0,
                })
            });
            let mut fds = [
                PollFd::new(&self.input, PollFlags::IN),
                PollFd::new(&self.resize, PollFlags::IN),
            ];
            match poll(&mut fds, remaining.as_ref()) {
                Ok(0) => return Ok(None),
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
            let [input, resize] = fds.map(|fd| fd.revents());
            if resize.contains(PollFlags::IN) {
                let mut bytes = [0; 128];
                loop {
                    match self.resize.read(&mut bytes) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => return Err(error),
                    }
                }
                return self.size().map(|size| Some(Event::Resize(size)));
            }
            if input.intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR) {
                let mut bytes = [0; 4096];
                match self.input.read(&mut bytes) {
                    Ok(0) => {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "terminal closed",
                        ));
                    }
                    Ok(count) => self
                        .parser
                        .parse(&bytes[..count], |event| self.events.push_back(event)),
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => return Err(error),
                }
            }
            if self.events.is_empty() && timeout.is_some_and(|timeout| start.elapsed() >= timeout) {
                return Ok(None);
            }
        }
    }

    pub fn finish(&mut self) -> io::Result<()> {
        if !self.active {
            return Ok(());
        }
        self.active = false;
        let leave = self.write_all(protocol::LEAVE);
        let flush = self.flush();
        let restore = termios::tcsetattr(&self.input, OptionalActions::Now, &self.original);
        leave.and(flush).and(restore.map_err(Into::into))
    }
}

impl Write for Terminal {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.output.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.finish();
        signal_hook::low_level::unregister(self.signal);
    }
}
