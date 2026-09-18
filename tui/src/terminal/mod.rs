use std::{
    collections::VecDeque,
    fs::{File, OpenOptions},
    io::{self, BufWriter, Write},
    os::fd::AsFd,
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

use blit_arrayvec::ArrayVec;
use rustix::{
    event::Timespec,
    termios::{self, OptionalActions, Termios},
};

use crate::protocol::{self, Parser};

pub struct Terminal {
    input: File,
    output: BufWriter<File>,
    original: Termios,
    resize: UnixStream,
    wake: UnixStream,
    signal: signal_hook::SigId,
    parser: Parser,
    events: VecDeque<protocol::Event>,
    poll: poll::Poll,
    active: bool,
}

pub enum Event {
    Protocol(protocol::Event),
    Resize(Size),
    Wake,
}

pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

impl Terminal {
    pub fn new(wake: UnixStream) -> io::Result<Self> {
        let input = OpenOptions::new().read(true).write(true).open("/dev/tty")?;
        let output = BufWriter::new(input.try_clone()?);
        let original = termios::tcgetattr(&input)?;
        let (resize, writer) = UnixStream::pair()?;
        resize.set_nonblocking(true)?;
        let signal = signal_hook::low_level::pipe::register(signal_hook::consts::SIGWINCH, writer)?;
        let poll = poll::Poll::new([input.as_fd(), resize.as_fd(), wake.as_fd()]);
        let mut terminal = Self {
            input,
            output,
            original,
            resize,
            wake,
            signal,
            parser: Parser::default(),
            events: VecDeque::new(),
            poll,
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
            let [input, resize, wake] = match self.poll.wait(
                [self.input.as_fd(), self.resize.as_fd(), self.wake.as_fd()],
                remaining.as_ref(),
            ) {
                Ok([false, false, false]) => return Ok(None),
                Ok(ready) => ready,
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            };
            if wake {
                let mut bytes = ArrayVec::<u8, 128>::new();
                loop {
                    match read_into(&self.wake, &mut bytes) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => return Err(error),
                    }
                }
                return Ok(Some(Event::Wake));
            }
            if resize {
                let mut bytes = ArrayVec::<u8, 128>::new();
                loop {
                    match read_into(&self.resize, &mut bytes) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => return Err(error),
                    }
                }
                return self.size().map(|size| Some(Event::Resize(size)));
            }
            if input {
                let mut bytes = ArrayVec::<u8, 4096>::new();
                match read_into(&self.input, &mut bytes) {
                    Ok(0) => {
                        return Err(io::Error::new(
                            io::ErrorKind::UnexpectedEof,
                            "terminal closed",
                        ));
                    }
                    Ok(_) => self
                        .parser
                        .parse(&bytes, |event| self.events.push_back(event)),
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

fn read_into<const N: usize>(fd: impl AsFd, bytes: &mut ArrayVec<u8, N>) -> io::Result<usize> {
    bytes.clear();
    let count = {
        let (initialized, _) = rustix::io::read(fd, bytes.spare_capacity_mut())?;
        initialized.len()
    };
    // safety: rustix returns the initialized prefix
    unsafe { bytes.set_len(count) };
    Ok(count)
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

#[cfg(not(target_os = "macos"))]
mod poll {
    use std::os::fd::BorrowedFd;

    use rustix::event::{PollFd, PollFlags, Timespec, poll};

    pub type Fd<'a> = BorrowedFd<'a>;

    pub struct Poll;

    impl Poll {
        pub fn new(_: [Fd<'_>; 3]) -> Self {
            Self
        }

        pub fn wait(
            &mut self,
            fds: [Fd<'_>; 3],
            timeout: Option<&Timespec>,
        ) -> rustix::io::Result<[bool; 3]> {
            let mut pollfds = fds.each_ref().map(|fd| PollFd::new(fd, PollFlags::IN));
            poll(&mut pollfds, timeout)?;
            Ok(pollfds.map(|fd| {
                fd.revents()
                    .intersects(PollFlags::IN | PollFlags::HUP | PollFlags::ERR)
            }))
        }
    }
}

#[cfg(target_os = "macos")]
mod poll {
    //! `poll` does not reliably report `/dev/tty` readiness on macos, so use `select`

    use std::os::fd::{AsRawFd as _, BorrowedFd, RawFd};

    use rustix::event::{
        FdSetElement, FdSetIter, Timespec, fd_set_insert, fd_set_num_elements, select,
    };

    pub type Fd<'a> = BorrowedFd<'a>;

    pub struct Poll {
        readfds: Vec<FdSetElement>,
        nfds: RawFd,
    }

    impl Poll {
        pub fn new(fds: [Fd<'_>; 3]) -> Self {
            let nfds = fds.map(|fd| fd.as_raw_fd()).into_iter().max().unwrap() + 1;
            Self {
                readfds: vec![FdSetElement::default(); fd_set_num_elements(fds.len(), nfds)],
                nfds,
            }
        }

        pub fn wait(
            &mut self,
            fds: [Fd<'_>; 3],
            timeout: Option<&Timespec>,
        ) -> rustix::io::Result<[bool; 3]> {
            let fds = fds.map(|fd| fd.as_raw_fd());
            self.readfds.fill(FdSetElement::default());
            for fd in fds {
                fd_set_insert(&mut self.readfds, fd);
            }
            unsafe { select(self.nfds, Some(&mut self.readfds), None, None, timeout) }?;
            Ok(fds.map(|fd| FdSetIter::new(&self.readfds).any(|ready| ready == fd)))
        }
    }
}
