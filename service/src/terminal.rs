use std::{
    fs::File,
    io,
    os::fd::{AsRawFd, FromRawFd},
    process::Stdio,
    sync::Arc,
};
use tokio::{io::unix::AsyncFd, process::Child};

pub struct Terminal(AsyncFd<File>);
impl Terminal {
    pub fn spawn(shell: &str, command: &str, cwd: &str) -> io::Result<(Child, Arc<Self>)> {
        let mut master = -1;
        let mut slave = -1;
        let size = libc::winsize {
            ws_row: 24,
            ws_col: 80,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        // SAFETY: openpty initializes two owned descriptors; all pointers are valid.
        if unsafe {
            libc::openpty(
                &mut master,
                &mut slave,
                std::ptr::null_mut(),
                std::ptr::null(),
                &size,
            )
        } < 0
        {
            return Err(io::Error::last_os_error());
        }
        let master = unsafe { File::from_raw_fd(master) };
        let slave = unsafe { File::from_raw_fd(slave) };
        unsafe {
            if libc::fcntl(master.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) < 0
                || libc::fcntl(slave.as_raw_fd(), libc::F_SETFD, libc::FD_CLOEXEC) < 0
                || libc::fcntl(master.as_raw_fd(), libc::F_SETFL, libc::O_NONBLOCK) < 0
            {
                return Err(io::Error::last_os_error());
            }
            let mut settings = std::mem::zeroed();
            if libc::tcgetattr(slave.as_raw_fd(), &mut settings) < 0 {
                return Err(io::Error::last_os_error());
            }
            // Never echo answers/passwords into the output/history. Keep canonical input.
            settings.c_lflag &= !(libc::ECHO | libc::ECHONL);
            settings.c_oflag &= !libc::ONLCR;
            if libc::tcsetattr(slave.as_raw_fd(), libc::TCSANOW, &settings) < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        let terminal = Arc::new(Self(AsyncFd::new(master)?));
        let mut process = crate::host_process::command(shell);
        process
            .arg("-c")
            .arg(command)
            .current_dir(cwd)
            .env("TERM", "dumb")
            .stdin(Stdio::from(slave.try_clone()?))
            .stdout(Stdio::from(slave.try_clone()?))
            .stderr(Stdio::from(slave))
            .kill_on_drop(true);
        // SAFETY: only async-signal-safe syscalls run between fork and exec.
        unsafe {
            process.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY, 0) < 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }
        Ok((process.spawn()?, terminal))
    }
    pub async fn read(&self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            let mut ready = self.0.readable().await?;
            match ready.try_io(|fd| {
                let count = unsafe {
                    libc::read(
                        fd.get_ref().as_raw_fd(),
                        buffer.as_mut_ptr().cast(),
                        buffer.len(),
                    )
                };
                if count < 0 {
                    let error = io::Error::last_os_error();
                    if error.raw_os_error() == Some(libc::EIO) {
                        Ok(0)
                    } else {
                        Err(error)
                    }
                } else {
                    Ok(count as usize)
                }
            }) {
                Ok(result) => return result,
                Err(_) => continue,
            }
        }
    }
    pub async fn write(&self, mut bytes: &[u8]) -> io::Result<()> {
        while !bytes.is_empty() {
            let mut ready = self.0.writable().await?;
            match ready.try_io(|fd| {
                let count = unsafe {
                    libc::write(fd.get_ref().as_raw_fd(), bytes.as_ptr().cast(), bytes.len())
                };
                if count < 0 {
                    Err(io::Error::last_os_error())
                } else if count == 0 {
                    Err(io::ErrorKind::WriteZero.into())
                } else {
                    Ok(count as usize)
                }
            }) {
                Ok(result) => bytes = &bytes[result?..],
                Err(_) => continue,
            }
        }
        Ok(())
    }
}
