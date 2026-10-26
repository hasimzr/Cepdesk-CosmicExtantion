//! Keep bundled image-loader configuration out of host programs we launch.
use std::ffi::OsStr;

pub fn command(program: impl AsRef<OsStr>) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(program);
    if std::env::var_os("CEPDESK_BUNDLED_SERVICE").is_some() {
        command.env_remove("GDK_PIXBUF_MODULE_FILE");
        command.env_remove("CEPDESK_BUNDLED_SERVICE");
    }
    command
}

pub fn std_command(program: impl AsRef<OsStr>) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    if std::env::var_os("CEPDESK_BUNDLED_SERVICE").is_some() {
        command.env_remove("GDK_PIXBUF_MODULE_FILE");
        command.env_remove("CEPDESK_BUNDLED_SERVICE");
    }
    command
}
