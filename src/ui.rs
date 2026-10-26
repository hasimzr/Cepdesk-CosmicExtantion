use crate::backend;
use cosmic::iced::platform_specific::shell::wayland::commands::popup::{destroy_popup, get_popup};
use cosmic::{
    iced::{Length, Limits, Subscription, window::Id},
    prelude::*,
    widget,
};
use serde_json::{Value, json};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Page {
    Status,
    Devices,
    Commands,
    Jobs,
    Settings,
    Api,
}
#[derive(Debug, Clone)]
pub enum Message {
    TogglePopup,
    Closed(Id),
    Refresh,
    Snapshot(Result<Value, String>),
    Action(Value),
    Result(Result<Value, String>),
    Service(&'static str),
    OpenManager,
    Page(Page),
    Field(&'static str, String),
    Edit(Value),
    Save,
    Run,
    Password,
    Api,
    Confirm(Value, String),
    Confirmed,
    CancelConfirm,
    Job(Value),
    JobDetail(Result<Value, String>),
    SendInput,
    Language(usize),
    OpenLink(backend::Link),
    LinkOpened(Result<(), String>),
}
pub struct Manager {
    core: cosmic::Core,
    popup: Option<Id>,
    standalone: bool,
    page: Page,
    data: Value,
    busy: bool,
    refreshing: bool,
    error: String,
    notice: String,
    result: String,
    editing: String,
    name: String,
    command: String,
    shell: String,
    cwd: String,
    timeout: String,
    password: String,
    password_repeat: String,
    api_method: String,
    api_params: String,
    job: Value,
    answer: String,
    confirmation: Option<(Value, String)>,
    language: crate::i18n::Preference,
    icon: widget::icon::Handle,
}
impl cosmic::Application for Manager {
    type Executor = cosmic::executor::Default;
    type Flags = bool;
    type Message = Message;
    const APP_ID: &'static str = "io.github.hasimzr.CepDesk";
    fn core(&self) -> &cosmic::Core {
        &self.core
    }
    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.core
    }
    fn init(mut core: cosmic::Core, standalone: bool) -> (Self, Task<cosmic::Action<Message>>) {
        let language = crate::i18n::Preference::load();
        core.window.header_title = language.resolve().tr("CepDesk Yönetimi").into();
        let app = Self {
            core,
            language,
            popup: None,
            standalone,
            page: Page::Status,
            data: Value::Null,
            busy: false,
            refreshing: true,
            error: String::new(),
            notice: String::new(),
            result: String::new(),
            editing: String::new(),
            name: String::new(),
            command: String::new(),
            shell: "/bin/bash".into(),
            cwd: std::env::var("HOME").unwrap_or_else(|_| "/tmp".into()),
            timeout: "60".into(),
            password: String::new(),
            password_repeat: String::new(),
            api_method: "system.controls".into(),
            api_params: "{}".into(),
            job: Value::Null,
            answer: String::new(),
            confirmation: None,
            icon: widget::icon::from_raster_bytes(
                include_bytes!("../resources/io.github.hasimzr.CepDesk.png").as_slice(),
            ),
        };
        (
            app,
            Task::perform(backend::startup_snapshot(), |result| {
                cosmic::Action::App(Message::Snapshot(result))
            }),
        )
    }
    fn subscription(&self) -> Subscription<Message> {
        cosmic::iced::time::every(std::time::Duration::from_secs(3)).map(|_| Message::Refresh)
    }
    fn on_close_requested(&self, id: Id) -> Option<Message> {
        if self.popup == Some(id) {
            Some(Message::Closed(id))
        } else {
            None
        }
    }
    fn view(&self) -> Element<'_, Message> {
        if self.standalone {
            return self.content();
        }
        self.core
            .applet
            .icon_button_from_handle(self.icon.clone())
            .on_press(Message::TogglePopup)
            .into()
    }
    fn view_window(&self, _: Id) -> Element<'_, Message> {
        self.core.applet.popup_container(self.content()).into()
    }
    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        if self.standalone {
            None
        } else {
            Some(cosmic::applet::style())
        }
    }
    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        match message {
            Message::Language(index) => {
                let preference = crate::i18n::Preference::from_index(index);
                match preference.save() {
                    Ok(()) => {
                        self.language = preference;
                        self.core.window.header_title = self.tr("CepDesk Yönetimi").into();
                        self.notice.clear();
                        self.error.clear();
                    }
                    Err(error) => {
                        self.error = format!("{}: {error}", self.tr("Dil tercihi kaydedilemedi"))
                    }
                }
            }
            Message::OpenLink(link) => {
                return Task::perform(backend::open_link(link), |result| {
                    cosmic::Action::App(Message::LinkOpened(result))
                });
            }
            Message::LinkOpened(result) => {
                if let Err(error) = result {
                    self.error = format!("{}: {error}", self.tr("Bağlantı açılamadı"));
                }
            }
            Message::TogglePopup => {
                return if let Some(id) = self.popup.take() {
                    destroy_popup(id)
                } else {
                    let id = Id::unique();
                    self.popup = Some(id);
                    let mut settings = self.core.applet.get_popup_settings(
                        self.core.main_window_id().unwrap(),
                        id,
                        None,
                        None,
                        None,
                    );
                    settings.positioner.size_limits = Limits::NONE
                        .min_width(360.0)
                        .max_width(600.0)
                        .min_height(220.0)
                        .max_height(760.0);
                    get_popup(settings)
                };
            }
            Message::Closed(id) => {
                if self.popup == Some(id) {
                    self.popup = None;
                }
            }
            Message::OpenManager => {
                if let Ok(path) = std::env::current_exe() {
                    match std::process::Command::new(path)
                        .arg("--settings")
                        .env_remove("WAYLAND_SOCKET")
                        .spawn()
                    {
                        Ok(mut child) => {
                            std::thread::spawn(move || {
                                let _ = child.wait();
                            });
                        }
                        Err(error) => {
                            self.error =
                                format!("{}{error}", self.tr("Yönetim penceresi açılamadı: "))
                        }
                    }
                }
            }
            Message::Page(page) => self.page = page,
            Message::Refresh => {
                self.language = crate::i18n::Preference::load();
                self.core.window.header_title = self.tr("CepDesk Yönetimi").into();
                if !self.refreshing && !self.busy {
                    self.refreshing = true;
                    let snapshot = Task::perform(backend::snapshot(), |result| {
                        cosmic::Action::App(Message::Snapshot(result))
                    });
                    if self.page == Page::Jobs && self.job["status"] == "running" {
                        return Task::batch([
                            snapshot,
                            Task::perform(
                                backend::request(
                                    json!({"method":"jobs.get","id":self.job["id"],"owner":self.job["owner"]}),
                                ),
                                |result| cosmic::Action::App(Message::JobDetail(result)),
                            ),
                        ]);
                    }
                    return snapshot;
                }
            }
            Message::Snapshot(result) => {
                self.refreshing = false;
                match result {
                    Ok(value) => {
                        self.data = value;
                        if self.error.starts_with("CepDesk servisi") {
                            self.error.clear();
                        }
                    }
                    Err(error) => {
                        self.data = Value::Null;
                        self.error = error;
                    }
                }
            }
            Message::Action(value) => {
                if !self.busy {
                    self.busy = true;
                    self.error.clear();
                    self.notice.clear();
                    return Task::perform(backend::request(value), |result| {
                        cosmic::Action::App(Message::Result(result))
                    });
                }
            }
            Message::Service(action) => {
                if !self.busy {
                    self.busy = true;
                    self.error.clear();
                    return Task::perform(backend::service(action), |result| {
                        cosmic::Action::App(Message::Result(result))
                    });
                }
            }
            Message::Result(result) => {
                self.busy = false;
                match result {
                    Ok(value) => {
                        self.notice = "İşlem tamamlandı.".into();
                        self.result = serde_json::to_string_pretty(&value).unwrap_or_default();
                    }
                    Err(error) => self.error = error,
                };
                return self.update(Message::Refresh);
            }
            Message::Confirm(request, label) => self.confirmation = Some((request, label)),
            Message::CancelConfirm => self.confirmation = None,
            Message::Confirmed => {
                if let Some((request, _)) = self.confirmation.take() {
                    if let Some(action) = request["service"].as_str() {
                        return self.update(Message::Service(match action {
                            "stop" => "stop",
                            _ => "restart",
                        }));
                    }
                    return self.update(Message::Action(request));
                }
            }
            Message::Field(field, value) => match field {
                "name" => self.name = value,
                "command" => self.command = value,
                "shell" => self.shell = value,
                "cwd" => self.cwd = value,
                "timeout" => self.timeout = value,
                "password" => self.password = value,
                "password_repeat" => self.password_repeat = value,
                "api_method" => self.api_method = value,
                "api_params" => self.api_params = value,
                "answer" => self.answer = value,
                _ => {}
            },
            Message::Edit(value) => {
                self.editing = value["id"].as_str().unwrap_or("").into();
                self.name = value["name"].as_str().unwrap_or("").into();
                self.command = value["command"].as_str().unwrap_or("").into();
                self.shell = value["shell"].as_str().unwrap_or("/bin/bash").into();
                self.cwd = value["cwd"].as_str().unwrap_or("/tmp").into();
                self.timeout = value["timeout_seconds"].as_u64().unwrap_or(60).to_string();
            }
            Message::Save => {
                let timeout = match self.timeout.parse::<u64>() {
                    Ok(value) if (1..=3600).contains(&value) => value,
                    _ => {
                        self.error = "Zaman aşımı 1–3600 saniye olmalı.".into();
                        return Task::none();
                    }
                };
                if self.name.trim().is_empty() || self.command.trim().is_empty() {
                    self.error = "Komut adı ve metni gerekli.".into();
                    return Task::none();
                }
                let id = if self.editing.is_empty() {
                    format!(
                        "desktop-{}",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_nanos()
                    )
                } else {
                    self.editing.clone()
                };
                self.editing = id.clone();
                return self.update(Message::Action(json!({"method":"saved.add","value":{"id":id,"name":self.name,"command":self.command,"shell":self.shell,"cwd":self.cwd,"timeout_seconds":timeout}})));
            }
            Message::Run => {
                let timeout = self.timeout.parse::<u64>().unwrap_or(0);
                return self.update(Message::Action(backend::rpc("commands.run", json!({"command":self.command,"shell":self.shell,"cwd":self.cwd,"timeout_seconds":timeout,"keep_history":false}))));
            }
            Message::Password => {
                if self.password != self.password_repeat
                    || self.password.chars().count() < 12
                    || self.password.len() > 256
                {
                    self.error = "Parolalar aynı ve 12–256 karakter olmalı.".into();
                    return Task::none();
                }
                let value = std::mem::take(&mut self.password);
                self.password_repeat.clear();
                return self.update(Message::Action(
                    json!({"method":"password.set","password":value}),
                ));
            }
            Message::Api => match serde_json::from_str(&self.api_params) {
                Ok(params) => {
                    return self.update(Message::Action(backend::rpc(&self.api_method, params)));
                }
                Err(error) => self.error = format!("{}{error}", self.tr("JSON geçersiz: ")),
            },
            Message::Job(job) => {
                self.job = job.clone();
                self.answer.clear();
                return Task::perform(
                    backend::request(
                        json!({"method":"jobs.get","id":job["id"],"owner":job["owner"]}),
                    ),
                    |result| cosmic::Action::App(Message::JobDetail(result)),
                );
            }
            Message::JobDetail(result) => match result {
                Ok(value) => {
                    if value["id"] == self.job["id"] {
                        self.job = value;
                    }
                }
                Err(error) => self.error = error,
            },
            Message::SendInput => {
                let data = format!("{}\n", std::mem::take(&mut self.answer));
                return self.update(Message::Action(json!({"method":"jobs.input","id":self.job["id"],"owner":self.job["owner"],"data":data})));
            }
        }
        Task::none()
    }
}
impl Manager {
    fn tr<'a>(&self, text: &'a str) -> &'a str {
        self.language.resolve().tr(text)
    }
    fn button<'a>(&self, label: &'a str, message: Message) -> Element<'a, Message> {
        let button = match &message {
            Message::Save | Message::Password | Message::Run | Message::Service("start") => {
                widget::button::suggested(label)
            }
            Message::Confirmed => widget::button::destructive(label),
            Message::Confirm(_, _) => widget::button::destructive(label),
            _ => widget::button::standard(label),
        };
        if self.busy {
            button.into()
        } else {
            button.on_press(message).into()
        }
    }
    fn field<'a>(
        &'a self,
        label: &'a str,
        value: &'a str,
        key: &'static str,
    ) -> Element<'a, Message> {
        widget::column::with_capacity(32)
            .spacing(4)
            .push(widget::text::caption(label))
            .push(
                widget::text_input(label, value).on_input(move |value| Message::Field(key, value)),
            )
            .into()
    }
    fn content(&self) -> Element<'_, Message> {
        let title = match self.page {
            Page::Status => "Durum",
            Page::Devices => "Cihazlar",
            Page::Commands => "Komutlar",
            Page::Jobs => "İşler",
            Page::Settings => "Ayarlar",
            Page::Api => "Servis API",
        };
        let mut column = widget::column::with_capacity(32)
            .spacing(16)
            .padding(20)
            .push(widget::text::title3(self.tr(title)));
        if !self.error.is_empty() {
            column = column.push(widget::text(self.language.resolve().error(&self.error)));
        }
        if !self.notice.is_empty() {
            column = column.push(widget::text(self.tr(&self.notice)));
        }
        if let Some((_, label)) = &self.confirmation {
            column = column.push(widget::text(self.tr(label))).push(
                widget::row::with_capacity(8)
                    .spacing(8)
                    .push(self.button(self.tr("Onayla"), Message::Confirmed))
                    .push(self.button(self.tr("Vazgeç"), Message::CancelConfirm)),
            );
        }
        if self.busy {
            column = column.push(widget::text::caption(self.tr("İşlem sürüyor…")));
        }
        match self.page {
            Page::Status => {
                let devices = self.data["status"]["devices"].as_array();
                let online = devices
                    .map(|devices| {
                        devices
                            .iter()
                            .filter(|device| device["online"] == true)
                            .count()
                    })
                    .unwrap_or(0);
                column = column.push(widget::text(if self.data.is_null() { self.tr("Servis bağlı değil") } else { self.tr("Servis çalışıyor") })).push(widget::text(format!("{online} {} · {} {}", self.tr("bağlı cihaz"), devices.map(Vec::len).unwrap_or(0), self.tr("kayıtlı cihaz"))))
                    .push(widget::text(format!("Port: {} · {}: {}", self.data["status"]["port"], self.tr("Keşif"), match self.data["status"]["discovery"].as_bool() { Some(true) => self.tr("Açık"), Some(false) => self.tr("Kapalı"), None => "—" })))
                    .push(widget::row::with_capacity(8).spacing(8).push(self.button(self.tr("Başlat"), Message::Service("start"))).push(self.button(self.tr("Durdur"), Message::Confirm(json!({"service":"stop"}), "Servis duracak; cihaz bağlantıları ve çalışan işler kapanacak.".into()))).push(self.button(self.tr("Yeniden başlat"), Message::Confirm(json!({"service":"restart"}), "Servis yeniden başlayacak; mevcut bağlantılar kapanacak.".into()))))
                    .push(widget::row::with_capacity(8).spacing(8).push(self.button(self.tr("Oturumda otomatik başlat"), Message::Service("enable"))).push(self.button(self.tr("Otomatik başlatmayı kapat"), Message::Service("disable"))));
                if !self.data.is_null() && self.data["status"]["password_configured"] != true {
                    column = column.push(widget::text(
                        self.tr("İlk bağlantı için Ayarlar bölümünde bir parola belirleyin."),
                    ));
                }
                if let Some(fingerprint) = self.data["status"]["fingerprint_sha256"].as_str() {
                    column = column
                        .push(widget::text::caption(self.tr("Sertifika")))
                        .push(widget::text::caption(fingerprint));
                }
            }
            Page::Devices => {
                if self.data["status"]["devices"]
                    .as_array()
                    .is_some_and(Vec::is_empty)
                {
                    column = column.push(widget::text(self.tr(
                        "Henüz kayıtlı cihaz yok. Telefonunuzu Ayarlar bölümünden eşleştirin.",
                    )));
                }
                if let Some(devices) = self.data["status"]["devices"].as_array() {
                    for device in devices {
                        column = column
                            .push(widget::text::heading(format!(
                                "{} · {}",
                                device["name"].as_str().unwrap_or(self.tr("Cihaz")),
                                if device["online"] == true {
                                    self.tr("Bağlı")
                                } else {
                                    self.tr("Çevrimdışı")
                                }
                            )))
                            .push(widget::text::caption(device["id"].as_str().unwrap_or("")));
                        if let Some(sessions) = device["sessions"].as_array() {
                            for session in sessions {
                                column = column.push(widget::text::caption(format!(
                                    "{}: {}",
                                    self.tr("Bağlantı"),
                                    session["peer"].as_str().unwrap_or("")
                                )));
                            }
                        }
                        let enabled = device["free_commands"] != true;
                        column = column.push(widget::column::with_capacity(4).spacing(8).push(self.button(if enabled { self.tr("Serbest komut izni ver") } else { self.tr("Serbest komut iznini kaldır") }, Message::Confirm(json!({"method":"permission","id":device["id"],"enabled":enabled}), if enabled { "Bu cihaz bilgisayarda kendi shell komutlarını çalıştırabilecek.".into() } else { "Yeni serbest komutlar engellenecek; mevcut işler çalışmaya devam eder.".into() })))
                    .push(self.button(self.tr("Bağlantıyı kes"), Message::Confirm(json!({"method":"disconnect","id":device["id"]}), "Mevcut bağlantı kesilir; cihaz tekrar bağlanabilir. Kalıcı engelleme için yetkiyi iptal edin.".into())))
                    .push(self.button(self.tr("Yetkiyi iptal et"), Message::Confirm(json!({"method":"revoke","id":device["id"]}), "Cihazın anahtarı iptal edilecek ve işleri durdurulacak; yeniden eşleştirme gerekir.".into()))));
                        column = column.push(widget::divider::horizontal::default());
                    }
                }
            }
            Page::Commands => {
                if self.data["saved"].as_array().is_some_and(Vec::is_empty) {
                    column = column.push(widget::text(
                        self.tr("Henüz kayıtlı komut yok. Aşağıdan ilk komutunuzu ekleyin."),
                    ));
                }
                if let Some(commands) = self.data["saved"].as_array() {
                    for command in commands {
                        column = column
                            .push(widget::text::heading(
                                command["name"].as_str().unwrap_or(self.tr("Komut")),
                            ))
                            .push(widget::text::caption(
                                command["command"].as_str().unwrap_or(""),
                            ))
                            .push(
                                widget::row::with_capacity(8)
                                    .spacing(8)
                                    .push(self.button(
                                        self.tr("Çalıştır"),
                                        Message::Action(backend::rpc(
                                            "commands.run",
                                            json!({"saved_id":command["id"],"keep_history":false}),
                                        )),
                                    ))
                                    .push(
                                        self.button(
                                            self.tr("Düzenle"),
                                            Message::Edit(command.clone()),
                                        ),
                                    )
                                    .push(self.button(
                                        self.tr("Sil"),
                                        Message::Confirm(
                                            json!({"method":"saved.delete","id":command["id"]}),
                                            "Kayıtlı komut silinecek.".into(),
                                        ),
                                    )),
                            );
                    }
                }
                column = column
                    .push(widget::divider::horizontal::default())
                    .push(widget::text::heading(self.tr("Komut düzenleyici")))
                    .push(self.field(self.tr("Komut adı"), &self.name, "name"))
                    .push(self.field(self.tr("Komut"), &self.command, "command"))
                    .push(self.field("Shell", &self.shell, "shell"))
                    .push(self.field(self.tr("Çalışma dizini"), &self.cwd, "cwd"))
                    .push(self.field(self.tr("Zaman aşımı (saniye)"), &self.timeout, "timeout"))
                    .push(
                        widget::row::with_capacity(8)
                            .spacing(8)
                            .push(self.button(self.tr("Kaydet"), Message::Save))
                            .push(self.button(self.tr("Kaydetmeden çalıştır"), Message::Run))
                            .push(self.button(self.tr("Yeni komut"), Message::Edit(Value::Null))),
                    );
            }
            Page::Jobs => {
                if self.data["jobs"].as_array().is_some_and(Vec::is_empty) {
                    column = column.push(widget::text(
                        self.tr("Henüz iş yok. Çalıştırdığınız komutlar burada görünür."),
                    ));
                }
                if let Some(jobs) = self.data["jobs"].as_array() {
                    for job in jobs {
                        column = column.push(widget::text(format!("{} · {}", job["spec"]["command"].as_str().unwrap_or(self.tr("Komut")), self.tr(match job["status"].as_str().unwrap_or("") {
                            "running" => "Çalışıyor", "finished" | "completed" => "Tamamlandı", "failed" => "Başarısız",
                            "cancelled" | "canceled" => "İptal edildi", "timed_out" => "Zaman aşımı", other => other,
                        }))))
                    .push(widget::row::with_capacity(8).spacing(8).push(self.button(self.tr("Çıktı"), Message::Job(job.clone()))).push(self.button(if job["status"] == "running" { self.tr("İptal et") } else { self.tr("Geçmişten sil") }, Message::Confirm(json!({"method":if job["status"] == "running" {"jobs.cancel"} else {"jobs.delete"},"id":job["id"],"owner":job["owner"]}), "Seçilen iş üzerinde bu işlem uygulansın mı?".into()))));
                    }
                }
                if !self.job.is_null() {
                    column = column
                        .push(widget::text(self.job["stdout"].as_str().unwrap_or("")))
                        .push(widget::text(self.job["stderr"].as_str().unwrap_or("")))
                        .push(
                            self.button(self.tr("Çıktıyı yenile"), Message::Job(self.job.clone())),
                        );
                    if self.job["status"] == "running" {
                        column = column
                            .push(
                                widget::text_input::secure_input(
                                    self.tr("Terminal yanıtı / parola"),
                                    &self.answer,
                                    None,
                                    true,
                                )
                                .on_input(|value| Message::Field("answer", value)),
                            )
                            .push(self.button(self.tr("Yanıtı gönder ↵"), Message::SendInput));
                    }
                }
            }
            Page::Settings => {
                column = column
                    .push(widget::text::heading(self.tr("Dil")))
                    .push(widget::dropdown(
                        vec![self.tr("Cihaz dilini kullan"), "Türkçe", "English"],
                        Some(self.language.index()),
                        Message::Language,
                    ))
                    .push(widget::divider::horizontal::default());
                column = column
                    .push(widget::text::heading(self.tr("Bağlantı parolası")))
                    .push(
                        widget::text_input::secure_input(
                            self.tr("Yeni parola (en az 12 karakter)"),
                            &self.password,
                            None,
                            true,
                        )
                        .on_input(|value| Message::Field("password", value)),
                    )
                    .push(
                        widget::text_input::secure_input(
                            self.tr("Parolayı tekrar girin"),
                            &self.password_repeat,
                            None,
                            true,
                        )
                        .on_input(|value| Message::Field("password_repeat", value)),
                    )
                    .push(self.button(self.tr("Parolayı belirle / değiştir"), Message::Password))
                    .push(widget::text::caption(self.tr(
                        "Parola değişikliği kayıtlı cihaz anahtarlarını iptal etmez.",
                    )));
                if let Some(settings) = self.data["status"]["settings"].as_object() {
                    let mut next = Value::Object(settings.clone());
                    next["paused"] = json!(next["paused"] != true);
                    column = column.push(self.button(
                        if next["paused"] == true {
                            self.tr("Pano takibini duraklat")
                        } else {
                            self.tr("Pano takibini sürdür")
                        },
                        Message::Action(json!({"method":"settings.set","value":next})),
                    ));
                }
                column = column
                    .push(widget::text::heading(self.tr("Kodla eşleştirme")))
                    .push(self.button(
                        self.tr("Geçici eşleştirme kodu oluştur"),
                        Message::Action(json!({"method":"pair"})),
                    ));
                if let Some(pending) = self.data["pending"].as_array() {
                    for device in pending.iter().filter(|device| device["approved"] != true) {
                        column = column
                            .push(widget::text(
                                device["name"].as_str().unwrap_or(self.tr("Cihaz")),
                            ))
                            .push(
                                widget::row::with_capacity(8)
                                    .spacing(8)
                                    .push(self.button(
                                        self.tr("Onayla"),
                                        Message::Action(
                                            json!({"method":"approve","id":device["id"]}),
                                        ),
                                    ))
                                    .push(self.button(
                                        self.tr("Reddet"),
                                        Message::Action(json!({"method":"deny","id":device["id"]})),
                                    )),
                            );
                    }
                }
                if !self.result.is_empty() {
                    column = column.push(widget::text(&self.result));
                }
            }
            Page::Api => {
                column = column.push(widget::text::heading(self.tr("Servis işlemleri")))
                .push(widget::text::caption(self.tr("Pano, pencere, medya, sistem ve komut işlemleri. Örn: clipboard.list, windows.list, system.controls, media.list, settings.set. Akışa bağlı monitor/input/ses işlemleri telefon oturumunda yapılır.")))
                .push(self.field(self.tr("Yöntem"), &self.api_method, "api_method")).push(self.field(self.tr("JSON parametreleri"), &self.api_params, "api_params"))
                .push(self.button(self.tr("İsteği gönder"), Message::Api)).push(widget::text(&self.result));
            }
        }
        if matches!(self.page, Page::Status | Page::Settings) {
            column = column
                .push(widget::divider::horizontal::default())
                .push(widget::text::heading(self.tr("Mobil uygulama ve proje")))
                .push(widget::text(self.tr("Mobil uygulamayı indirmek veya proje detaylarını öğrenmek için web sitemizi ziyaret edin.")))
                .push(widget::text::caption(backend::Link::Project.url()))
                .push(widget::button::suggested(self.tr("Web sitesini ziyaret et")).on_press(Message::OpenLink(backend::Link::Project)))
                .push(widget::text::caption(self.tr("Geliştirici · Haşim Özer")))
                .push(widget::text::caption(backend::Link::Developer.url()))
                .push(widget::button::text(self.tr("GitHub profilini aç")).on_press(Message::OpenLink(backend::Link::Developer)));
        }
        let body = widget::scrollable(column).height(Length::Fill);
        let mut header = widget::row::with_capacity(4)
            .spacing(12)
            .padding(16)
            .push(widget::text::title3("CepDesk"))
            .push(widget::Space::new().width(Length::Fill))
            .push(self.button(self.tr("Yenile"), Message::Refresh));
        if !self.standalone {
            header = header.push(
                widget::button::suggested(self.tr("Yönetim merkezi"))
                    .on_press(Message::OpenManager),
            );
        }
        let mut layout = widget::column::with_capacity(4)
            .push(header)
            .push(widget::divider::horizontal::default());
        if self.standalone {
            let mut navigation = widget::column::with_capacity(8)
                .spacing(8)
                .padding(12)
                .width(Length::Fixed(170.0));
            for (page, label) in [
                (Page::Status, "Durum"),
                (Page::Devices, "Cihazlar"),
                (Page::Commands, "Komutlar"),
                (Page::Jobs, "İşler"),
                (Page::Settings, "Ayarlar"),
                (Page::Api, "Servis API"),
            ] {
                let button = if self.page == page {
                    widget::button::suggested(self.tr(label))
                } else {
                    widget::button::text(self.tr(label))
                };
                navigation =
                    navigation.push(button.width(Length::Fill).on_press(Message::Page(page)));
            }
            layout = layout.push(
                widget::row::with_capacity(2)
                    .push(widget::container(navigation).class(cosmic::theme::Container::Secondary))
                    .push(
                        widget::container(body)
                            .class(cosmic::theme::Container::Card)
                            .width(Length::Fill),
                    )
                    .height(Length::Fill),
            );
        } else {
            let mut navigation = widget::row::with_capacity(2).spacing(8).padding([8, 16]);
            for (page, label) in [(Page::Status, "Durum"), (Page::Settings, "Ayarlar")] {
                let button = if self.page == page {
                    widget::button::suggested(self.tr(label))
                } else {
                    widget::button::text(self.tr(label))
                };
                navigation = navigation.push(button.on_press(Message::Page(page)));
            }
            layout = layout
                .push(navigation)
                .push(body)
                .width(Length::Fixed(480.0));
        }
        layout.into()
    }
}

/// A separate application ID lets launchers and Flatpak exports distinguish the
/// normal management window from the panel entry, while sharing the same model.
pub struct DesktopManager(Manager);
impl cosmic::Application for DesktopManager {
    type Executor = cosmic::executor::Default;
    type Flags = ();
    type Message = Message;
    const APP_ID: &'static str = "io.github.hasimzr.CepDesk.Manager";
    fn core(&self) -> &cosmic::Core {
        &self.0.core
    }
    fn core_mut(&mut self) -> &mut cosmic::Core {
        &mut self.0.core
    }
    fn init(core: cosmic::Core, _: ()) -> (Self, Task<cosmic::Action<Message>>) {
        let (model, task) = <Manager as cosmic::Application>::init(core, true);
        (Self(model), task)
    }
    fn view(&self) -> Element<'_, Message> {
        self.0.content()
    }
    fn update(&mut self, message: Message) -> Task<cosmic::Action<Message>> {
        self.0.update(message)
    }
    fn subscription(&self) -> Subscription<Message> {
        self.0.subscription()
    }
}
