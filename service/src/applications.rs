//! XDG application catalog and local icon rasterization. Launch only desktop files
//! resolved by the server; RPC clients cannot submit executable commands or paths.
use crate::protocol::{ApiError, Result};
use base64::Engine;
use serde::Serialize;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    env,
    ffi::{c_char, c_void, CString},
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

#[derive(Clone, Serialize)]
pub struct Application {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(skip)]
    pub path: PathBuf,
    #[serde(skip)]
    icon: String,
    #[serde(skip)]
    aliases: Vec<String>,
}
struct Catalog {
    scanned: Instant,
    apps: Vec<Application>,
}
static CATALOG: OnceLock<Mutex<Option<Catalog>>> = OnceLock::new();
struct IconIndex {
    scanned: Instant,
    paths: HashMap<String, PathBuf>,
}
static INDEX: OnceLock<Mutex<Option<IconIndex>>> = OnceLock::new();
type IconCache = HashMap<PathBuf, (std::time::SystemTime, Option<String>)>;
static ICONS: OnceLock<Mutex<IconCache>> = OnceLock::new();
fn data_roots() -> Vec<PathBuf> {
    let home = PathBuf::from(env::var_os("HOME").unwrap_or_default());
    let mut roots = vec![
        env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share")),
        home.join(".local/share/flatpak/exports/share"),
    ];
    roots.extend(
        env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".into())
            .split(':')
            .filter(|p| p.starts_with('/'))
            .map(PathBuf::from),
    );
    roots.push(PathBuf::from("/var/lib/flatpak/exports/share"));
    let mut seen = HashSet::new();
    roots.retain(|p| seen.insert(p.clone()));
    roots
}
fn desktop_fields(raw: &str) -> BTreeMap<String, String> {
    let mut fields = BTreeMap::new();
    let mut main = false;
    for line in raw.lines().map(str::trim) {
        if line.starts_with('[') {
            main = line == "[Desktop Entry]";
            continue;
        }
        if main && !line.starts_with('#') {
            if let Some((key, value)) = line.split_once('=') {
                fields.insert(
                    key.into(),
                    value
                        .replace("\\s", " ")
                        .replace("\\n", " ")
                        .replace("\\\\", "\\"),
                );
            }
        }
    }
    fields
}
fn localized(fields: &BTreeMap<String, String>, key: &str) -> String {
    let locale = env::var("LC_ALL")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| env::var("LC_MESSAGES").ok())
        .or_else(|| env::var("LANG").ok())
        .unwrap_or_default();
    let locale = locale.split('.').next().unwrap_or("");
    let language = locale.split('_').next().unwrap_or("");
    fields
        .get(&format!("{key}[{locale}]"))
        .or_else(|| fields.get(&format!("{key}[{language}]")))
        .or_else(|| fields.get(key))
        .cloned()
        .unwrap_or_default()
}
fn walk(root: &Path, depth: usize, files: &mut Vec<PathBuf>, limit: usize) {
    if depth == 0 || files.len() >= limit {
        return;
    }
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for path in entries {
        if files.len() >= limit {
            break;
        }
        if path.is_dir() {
            walk(&path, depth - 1, files, limit);
        } else {
            files.push(path);
        }
    }
}
fn scan_roots(roots: &[PathBuf]) -> Vec<Application> {
    let mut seen = HashSet::new();
    let mut apps = Vec::new();
    let desktops: Vec<_> = env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_else(|_| "COSMIC".into())
        .split(':')
        .map(str::to_string)
        .collect();
    for root in roots {
        let base = root.join("applications");
        let mut files = Vec::new();
        walk(&base, 4, &mut files, 4096);
        for path in files {
            if path.extension().is_none_or(|e| e != "desktop") {
                continue;
            }
            let Ok(relative) = path.strip_prefix(&base) else {
                continue;
            };
            let id = relative.to_string_lossy().replace('/', "-");
            if id.len() > 512 {
                continue;
            }
            // A hidden user entry masks a system entry with the same desktop ID.
            if !seen.insert(id.clone()) {
                continue;
            }
            if !fs::metadata(&path).is_ok_and(|m| m.len() <= 128 * 1024) {
                continue;
            }
            let Ok(raw) = fs::read_to_string(&path) else {
                continue;
            };
            let f = desktop_fields(&raw);
            if f.get("Type").is_none_or(|v| v != "Application")
                || ["Hidden", "NoDisplay"]
                    .iter()
                    .any(|k| f.get(*k).is_some_and(|v| v == "true"))
            {
                continue;
            }
            if f.get("OnlyShowIn").is_some_and(|v| {
                !v.split(';')
                    .any(|d| desktops.iter().any(|current| current == d))
            }) || f.get("NotShowIn").is_some_and(|v| {
                v.split(';')
                    .any(|d| desktops.iter().any(|current| current == d))
            }) {
                continue;
            }
            if !f.contains_key("Exec") && f.get("DBusActivatable").is_none_or(|v| v != "true") {
                continue;
            }
            if let Some(exec) = f.get("TryExec") {
                let found = if exec.starts_with('/') {
                    Path::new(exec).is_file()
                } else {
                    env::var("PATH")
                        .unwrap_or_default()
                        .split(':')
                        .any(|p| Path::new(p).join(exec).is_file())
                };
                if !found {
                    continue;
                }
            }
            let name = localized(&f, "Name");
            if name.is_empty() {
                continue;
            }
            let mut aliases = vec![id.trim_end_matches(".desktop").to_lowercase()];
            if let Some(class) = f.get("StartupWMClass") {
                aliases.push(class.to_lowercase());
            }
            apps.push(Application {
                id,
                name: name.chars().take(200).collect(),
                description: localized(&f, "Comment").chars().take(300).collect(),
                path,
                icon: f.get("Icon").cloned().unwrap_or_default(),
                aliases,
            });
        }
    }
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.truncate(2048);
    apps
}
pub fn list() -> Vec<Application> {
    let mut catalog = CATALOG.get_or_init(|| Mutex::new(None)).lock().unwrap();
    if catalog
        .as_ref()
        .is_none_or(|c| c.scanned.elapsed() >= Duration::from_secs(30))
    {
        *catalog = Some(Catalog {
            scanned: Instant::now(),
            apps: scan_roots(&data_roots()),
        });
    }
    catalog.as_ref().unwrap().apps.clone()
}
fn find(id: &str) -> Option<Application> {
    let lower = id.trim_end_matches(".desktop").to_lowercase();
    list()
        .into_iter()
        .find(|a| a.id == id || a.aliases.contains(&lower))
}
pub fn name(id: &str) -> String {
    find(id).map(|a| a.name).unwrap_or_else(|| id.into())
}
pub async fn launch(id: &str) -> Result<serde_json::Value> {
    let app = find(id).filter(|a| a.id == id).ok_or_else(|| {
        ApiError::new(
            "NOT_FOUND",
            "Uygulama artık bilgisayarda bulunmuyor. Listeyi yenileyin.",
        )
    })?;
    launch_entry(app).await
}
async fn launch_entry(app: Application) -> Result<serde_json::Value> {
    let result = tokio::time::timeout(
        Duration::from_secs(5),
        crate::host_process::command("gio")
            .arg("launch")
            .arg(&app.path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true)
            .status(),
    )
    .await
    .map_err(|_| ApiError::new("TIMEOUT", "Uygulama başlatıcısı yanıt vermedi."))?
    .map_err(|_| {
        ApiError::new(
            "UNSUPPORTED",
            "Uygulama açmak için masaüstünde gio gereklidir.",
        )
    })?;
    if !result.success() {
        return Err(ApiError::new(
            "LAUNCH_FAILED",
            "Uygulama açılamadı. Masaüstü oturumunu ve uygulamanın kurulumunu kontrol edin.",
        ));
    }
    Ok(serde_json::json!({"launched":app.id}))
}
fn icon_path(icon: &str) -> Option<PathBuf> {
    if icon.is_empty() {
        return None;
    }
    if Path::new(icon).is_absolute() {
        return Path::new(icon).is_file().then(|| PathBuf::from(icon));
    }
    if icon.contains('/') || icon.contains("..") {
        return None;
    }
    let mut index = INDEX.get_or_init(|| Mutex::new(None)).lock().unwrap();
    if index
        .as_ref()
        .is_none_or(|i| i.scanned.elapsed() > Duration::from_secs(300))
    {
        let mut paths = HashMap::new();
        let mut roots = data_roots()
            .into_iter()
            .map(|p| p.join("icons"))
            .collect::<Vec<_>>();
        if let Some(home) = env::var_os("HOME") {
            roots.insert(0, PathBuf::from(home).join(".icons"));
        }
        for root in roots {
            let mut themes = vec!["Cosmic".into(), "Pop".into(), "hicolor".into()];
            if let Ok(entries) = fs::read_dir(&root) {
                let mut others: Vec<String> = entries
                    .flatten()
                    .filter(|e| e.path().is_dir())
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect();
                others.sort();
                themes.extend(others);
            }
            let mut visited = HashSet::new();
            for theme in themes {
                if !visited.insert(theme.clone()) {
                    continue;
                }
                let mut files = Vec::new();
                walk(&root.join(theme), 5, &mut files, 40000);
                // Prefer full-color scalable and large application icons over tiny raster sizes.
                files.sort_by_key(|p| {
                    let s = p.to_string_lossy();
                    (
                        !s.contains("/apps/"),
                        !s.contains("/scalable/"),
                        !s.contains("/128x128/"),
                        !s.contains("/256x256/"),
                    )
                });
                for path in files {
                    if path.extension().is_some_and(|e| {
                        ["png", "svg", "xpm", "jpg", "webp"]
                            .iter()
                            .any(|ext| e == *ext)
                    }) {
                        if let Some(stem) = path.file_stem() {
                            paths
                                .entry(stem.to_string_lossy().into_owned())
                                .or_insert(path);
                        }
                    }
                }
            }
        }
        for root in data_roots() {
            if let Ok(entries) = fs::read_dir(root.join("pixmaps")) {
                for path in entries.flatten().map(|e| e.path()) {
                    if let Some(stem) = path.file_stem() {
                        paths
                            .entry(stem.to_string_lossy().into_owned())
                            .or_insert(path);
                    }
                }
            }
        }
        *index = Some(IconIndex {
            scanned: Instant::now(),
            paths,
        });
    }
    let stem = Path::new(icon).file_stem()?.to_string_lossy();
    index
        .as_ref()?
        .paths
        .get(icon)
        .or_else(|| index.as_ref()?.paths.get(stem.as_ref()))
        .cloned()
}
#[link(name = "libgdk_pixbuf-2.0.so.0", modifiers = "+verbatim")]
unsafe extern "C" {
    fn gdk_pixbuf_new_from_file_at_scale(
        filename: *const c_char,
        width: i32,
        height: i32,
        preserve: i32,
        error: *mut *mut c_void,
    ) -> *mut c_void;
    fn gdk_pixbuf_save_to_bufferv(
        pixbuf: *mut c_void,
        buffer: *mut *mut c_char,
        size: *mut usize,
        format: *const c_char,
        keys: *mut *mut c_char,
        values: *mut *mut c_char,
        error: *mut *mut c_void,
    ) -> i32;
}
#[link(name = "gobject-2.0")]
unsafe extern "C" {
    fn g_object_unref(object: *mut c_void);
}
#[link(name = "glib-2.0")]
unsafe extern "C" {
    fn g_free(mem: *mut c_void);
    fn g_error_free(error: *mut c_void);
}
fn rasterize(path: &Path) -> Option<String> {
    rasterize_at(path, 128).or_else(|| rasterize_at(path, 64))
}
fn rasterize_at(path: &Path, edge: i32) -> Option<String> {
    if !fs::metadata(path).is_ok_and(|m| m.len() <= 2 * 1024 * 1024) {
        return None;
    }
    let path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    // SAFETY: nul-terminated filename and PNG type; output/error pointers start
    // null. GLib owns returned allocations, released with the matching functions.
    unsafe {
        let mut error = std::ptr::null_mut();
        let pixbuf = gdk_pixbuf_new_from_file_at_scale(path.as_ptr(), edge, edge, 1, &mut error);
        if pixbuf.is_null() {
            if !error.is_null() {
                g_error_free(error);
            }
            return None;
        }
        let mut buffer = std::ptr::null_mut();
        let mut size = 0;
        let success = gdk_pixbuf_save_to_bufferv(
            pixbuf,
            &mut buffer,
            &mut size,
            c"png".as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut error,
        );
        g_object_unref(pixbuf);
        let result = if success != 0 && !buffer.is_null() && size <= 24 * 1024 {
            Some(format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD
                    .encode(std::slice::from_raw_parts(buffer as *const u8, size))
            ))
        } else {
            None
        };
        if !buffer.is_null() {
            g_free(buffer as *mut c_void);
        }
        if !error.is_null() {
            g_error_free(error);
        }
        result
    }
}
pub fn icon(id: &str) -> Option<String> {
    let app = find(id)?;
    let path = icon_path(&app.icon)?;
    let modified = fs::metadata(&path).ok()?.modified().ok()?;
    let cache = ICONS.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some((stamp, value)) = cache.lock().unwrap().get(&path) {
        if *stamp == modified {
            return value.clone();
        }
    }
    let image = rasterize(&path);
    let mut cache = cache.lock().unwrap();
    if cache.len() >= 512 {
        cache.clear();
    }
    cache.insert(path, (modified, image.clone()));
    image
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_honors_user_overrides_hidden_entries_and_desktop_group() {
        let root = env::temp_dir().join(format!("cepdesk-apps-{}", uuid::Uuid::new_v4()));
        let user = root.join("user");
        let system = root.join("system");
        for base in [&user, &system] {
            fs::create_dir_all(base.join("applications")).unwrap();
        }
        fs::write(
            user.join("applications/hidden.desktop"),
            "[Desktop Entry]\nType=Application\nHidden=true\n",
        )
        .unwrap();
        fs::write(
            system.join("applications/hidden.desktop"),
            "[Desktop Entry]\nType=Application\nName=Hidden\nExec=true\n",
        )
        .unwrap();
        fs::write(system.join("applications/editor.desktop"), "[Desktop Entry]\nType=Application\nName=Editor\nExec=true\nIcon=editor\n[Desktop Action Other]\nName=Wrong\nIcon=wrong\n").unwrap();
        fs::write(
            system.join("applications/helper.desktop"),
            "[Desktop Entry]\nType=Application\nName=Helper\nExec=true\nNoDisplay=true\n",
        )
        .unwrap();
        let apps = scan_roots(&[user, system]);
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].id, "editor.desktop");
        assert_eq!(apps[0].name, "Editor");
        assert_eq!(apps[0].icon, "editor");
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn gio_launches_a_catalog_entry_without_accepting_client_commands() {
        let root = env::temp_dir().join(format!("cepdesk-launch-test-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(root.join("applications")).unwrap();
        let marker = root.join("launched");
        fs::write(
            root.join("applications/fixture.desktop"),
            format!(
                "[Desktop Entry]\nType=Application\nName=Fixture\nExec=/usr/bin/touch {}\n",
                marker.display()
            ),
        )
        .unwrap();
        let app = scan_roots(std::slice::from_ref(&root)).remove(0);
        assert_eq!(
            launch_entry(app).await.unwrap()["launched"],
            "fixture.desktop"
        );
        tokio::time::timeout(Duration::from_secs(2), async {
            while !marker.exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        fs::remove_dir_all(root).unwrap();
    }
    #[tokio::test]
    async fn unknown_or_client_supplied_paths_cannot_be_launched() {
        for id in [
            "/tmp/untrusted.desktop",
            "../../bin/bash",
            "missing-launcher.desktop",
        ] {
            assert_eq!(launch(id).await.unwrap_err().code, "NOT_FOUND");
        }
    }
    #[test]
    fn installed_cosmic_icon_can_be_resolved_from_the_full_catalog() {
        if Path::new("/usr/share/applications/com.system76.CosmicFiles.desktop").exists() {
            assert!(icon("com.system76.CosmicFiles").is_some());
        }
    }
    #[test]
    fn svg_is_transferred_as_android_compatible_png_without_cropping() {
        let root = env::temp_dir().join(format!("cepdesk-icon-{}.svg", uuid::Uuid::new_v4()));
        fs::write(&root, "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"200\" height=\"100\"><rect width=\"200\" height=\"100\" fill=\"red\"/></svg>").unwrap();
        let image = rasterize(&root).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(image.trim_start_matches("data:image/png;base64,"))
            .unwrap();
        assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(u32::from_be_bytes(bytes[16..20].try_into().unwrap()), 128);
        assert_eq!(u32::from_be_bytes(bytes[20..24].try_into().unwrap()), 64);
        fs::remove_file(root).unwrap();
    }
}
