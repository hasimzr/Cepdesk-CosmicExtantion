mod backend;
mod i18n;
mod ui;
fn main() -> cosmic::iced::Result {
    let standalone = std::env::args().any(|argument| argument == "--settings");
    if standalone {
        cosmic::app::run::<ui::DesktopManager>(
            cosmic::app::Settings::default().size(cosmic::iced::Size::new(980.0, 760.0)),
            (),
        )
    } else {
        cosmic::applet::run::<ui::Manager>(false)
    }
}
