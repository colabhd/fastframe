//! Open an egui window on the graphics a computer has, and say why when it
//! has none.
//!
//! eframe's glow renderer needs OpenGL 2 or later. A Windows computer
//! without its graphics driver (the Microsoft Basic Display Adapter, many
//! virtual machines, some Remote Desktop sessions) offers only OpenGL 1.1, so
//! `run_native` returns `egui_glow requires opengl 2.0+` before the window
//! draws anything. An app started from Explorer has no console, so it seemed
//! to quit without a word (Solco's Windows first-run test, 2026-10-27).
//!
//! [`Graphics::run_native`] opens the window with glow. When glow cannot draw
//! it and the `wgpu` feature is on, it opens it again with wgpu, which on
//! Windows draws through Direct3D 12 and, without a vendor driver, through
//! WARP, Windows' own software renderer. When no renderer can draw, it shows
//! a native dialog that names the cause and what to do, with a link to the
//! app's help page, and returns the error.

#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
#[allow(unsafe_code)] // NSAlert and NSWorkspace, with safety notes at each call.
mod macos;
#[cfg(windows)]
#[allow(unsafe_code)] // MessageBoxW and ShellExecuteW, with safety notes.
mod windows;

use std::cell::Cell;
use std::fmt::Display;
use std::panic::AssertUnwindSafe;
use std::rc::Rc;
use std::sync::Mutex;

pub use eframe::Renderer;

/// The renderers [`Graphics::run_native`] tries, in order: glow, which every
/// app has drawn with so far, then wgpu when the `wgpu` feature is on.
pub const RENDERERS: &[Renderer] = &[
    Renderer::Glow,
    #[cfg(feature = "wgpu")]
    Renderer::Wgpu,
];

/// The renderer that last opened a window in this process.
static WORKED: Mutex<Option<Renderer>> = Mutex::new(None);

/// The app's name and help page, for opening its windows.
#[derive(Clone, Debug)]
pub struct Graphics {
    app_name: String,
    help_url: Option<String>,
}

impl Graphics {
    /// For the app called `app_name`, which is also the name eframe gets.
    pub fn new(app_name: impl Into<String>) -> Self {
        Self {
            app_name: app_name.into(),
            help_url: None,
        }
    }

    /// The page the dialog offers to open, explaining what to install.
    #[must_use]
    pub fn help(mut self, url: impl Into<String>) -> Self {
        self.help_url = Some(url.into());
        self
    }

    /// Opens a window with `eframe::run_native`, as many times as it takes
    /// to find a renderer that draws it, and returns when it closes.
    ///
    /// `options` and `app_creator` are called once for each try, since a
    /// failed try consumes both. The renderer in `options` is replaced: the
    /// first window tries [`RENDERERS`] in order, and later windows use the
    /// one that worked. Another renderer is tried only when the window could
    /// not start (the app creator never ran) and the error is a graphics one
    /// ([`is_graphics_failure`]).
    ///
    /// # Errors
    ///
    /// The last try's error. When it is a graphics failure, the dialog has
    /// been shown and closed by then.
    pub fn run_native<'app>(
        &self,
        mut options: impl FnMut() -> eframe::NativeOptions,
        mut app_creator: impl FnMut() -> eframe::AppCreator<'app>,
    ) -> eframe::Result {
        let remembered = *WORKED
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = try_renderers(
            remembered,
            RENDERERS,
            |renderer| {
                let started = Rc::new(Cell::new(false));
                let creator = app_creator();
                let flag = Rc::clone(&started);
                let creator: eframe::AppCreator<'app> = Box::new(move |context| {
                    // The renderer drew the window: the app can read it now,
                    // for its error reports.
                    flag.set(true);
                    *WORKED
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(renderer);
                    creator(context)
                });
                let mut options = options();
                options.renderer = renderer;
                // wgpu treats a surface it cannot configure as fatal and
                // panics: Direct3D 12 on WARP did in a Windows session with
                // no desktop (over SSH). Before the window started, that is
                // one more renderer that cannot draw; after, the app's own
                // panic goes on unchanged.
                let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
                    eframe::run_native(&self.app_name, options, creator)
                }));
                let result = match outcome {
                    Ok(result) => result.map_err(Tried::Error),
                    Err(payload) if !started.get() => Err(Tried::Panicked(panic_text(&*payload))),
                    Err(payload) => std::panic::resume_unwind(payload),
                };
                (result, started.get())
            },
            Tried::is_graphics_failure,
        );
        match result {
            Ok(_) => Ok(()),
            Err(Failure { error, graphics }) => {
                if graphics {
                    self.explain(&error);
                }
                Err(error.into())
            }
        }
    }

    /// Shows the dialog for a window that could not open, naming `error`
    /// under the explanation, and returns when it closes. If the person asks
    /// for the help page, it opens in their browser.
    ///
    /// [`Graphics::run_native`] calls this itself. It is public for an app
    /// that opens its window another way.
    pub fn explain(&self, error: &dyn Display) {
        let message = Message::new(&self.app_name, self.help_url.is_some(), &error.to_string());
        log::error!("{}: {}", message.title, error);
        if show(&message)
            && let Some(url) = &self.help_url
        {
            open_url(url);
        }
    }
}

/// The renderer drawing the window, or that drew the last one, for error
/// reports. Set before the app creator runs; `None` until a window opened
/// through [`Graphics::run_native`].
pub fn renderer() -> Option<Renderer> {
    *WORKED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Whether `error` says the computer's graphics cannot draw the window: no
/// usable OpenGL (glutin found no configuration or context, or the version
/// is below 2), or wgpu found no adapter or device. Window-system and app
/// errors are not.
pub fn is_graphics_failure(error: &eframe::Error) -> bool {
    match error {
        eframe::Error::Glutin(_)
        | eframe::Error::NoGlutinConfigs(..)
        | eframe::Error::OpenGL(_) => true,
        #[cfg(feature = "wgpu")]
        eframe::Error::Wgpu(_) => true,
        _ => false,
    }
}

/// How one try failed: eframe's error, or a panic before the window started.
#[derive(Debug)]
enum Tried {
    Error(eframe::Error),
    Panicked(String),
}

impl Tried {
    fn is_graphics_failure(&self) -> bool {
        match self {
            Self::Error(error) => is_graphics_failure(error),
            Self::Panicked(_) => true,
        }
    }
}

impl Display for Tried {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error(error) => error.fmt(f),
            Self::Panicked(text) => {
                write!(f, "the renderer failed while opening the window: {text}")
            }
        }
    }
}

/// A panic before the window started is returned as the app creation
/// error eframe would give, since eframe has no variant of its own for it.
impl From<Tried> for eframe::Error {
    fn from(tried: Tried) -> Self {
        match tried {
            Tried::Error(error) => error,
            Tried::Panicked(_) => Self::AppCreation(tried.to_string().into()),
        }
    }
}

fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "a panic without a message".to_owned())
}

/// A try that failed, and whether it was a graphics failure.
#[derive(Debug)]
struct Failure<E> {
    error: E,
    graphics: bool,
}

/// Tries `order` (or only the `remembered` renderer that worked before),
/// moving to the next renderer only while the window had not started and
/// the error was a graphics one. Returns the renderer that worked.
fn try_renderers<R: Copy + std::fmt::Debug, E: Display>(
    remembered: Option<R>,
    order: &[R],
    mut open: impl FnMut(R) -> (Result<(), E>, bool),
    is_graphics: impl Fn(&E) -> bool,
) -> Result<R, Failure<E>> {
    let order = match remembered {
        Some(renderer) => std::slice::from_ref(&renderer).to_vec(),
        None => order.to_vec(),
    };
    let mut renderers = order.iter().peekable();
    while let Some(&renderer) = renderers.next() {
        let (result, started) = open(renderer);
        let Err(error) = result else {
            return Ok(renderer);
        };
        let graphics = !started && is_graphics(&error);
        match renderers.peek() {
            Some(next) if graphics => {
                log::warn!(
                    "the {renderer:?} renderer cannot draw the window ({error}); trying {next:?}"
                );
            }
            _ => return Err(Failure { error, graphics }),
        }
    }
    unreachable!("there is always at least one renderer to try")
}

/// The dialog's words.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Message {
    title: String,
    body: String,
    /// Whether the dialog asks to open the help page (Yes and No) rather
    /// than only closing (OK).
    asks_for_help: bool,
}

impl Message {
    fn new(app: &str, help: bool, details: &str) -> Self {
        let remedy = if cfg!(windows) {
            format!(
                "Install your graphics card's driver, from its maker or from Windows Update, then open {app} again. Remote Desktop and virtual machines need 3D acceleration turned on."
            )
        } else if cfg!(target_os = "macos") {
            format!("Update macOS, then open {app} again.")
        } else {
            format!(
                "Install your graphics card's driver or Mesa, then open {app} again. Remote sessions and virtual machines need 3D acceleration turned on."
            )
        };
        let mut body = format!(
            "{app} needs your graphics driver to draw its window (OpenGL 2 or later), and this computer only has basic graphics.\n\n{remedy}\n\nDetails: {}",
            details.trim()
        );
        if help {
            body.push_str("\n\nOpen the help page?");
        }
        Self {
            title: format!("{app} cannot open its window"),
            body,
            asks_for_help: help,
        }
    }
}

/// Shows the dialog and returns whether the person asked for the help page.
fn show(message: &Message) -> bool {
    #[cfg(windows)]
    return windows::show(message);
    #[cfg(target_os = "macos")]
    return macos::show(message);
    #[cfg(target_os = "linux")]
    return linux::show(message);
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    {
        let _ = message;
        false
    }
}

fn open_url(url: &str) {
    #[cfg(windows)]
    windows::open_url(url);
    #[cfg(target_os = "macos")]
    macos::open_url(url);
    #[cfg(target_os = "linux")]
    linux::open_url(url);
    #[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
    let _ = url;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Fake {
        Glow,
        Wgpu,
    }

    #[derive(Debug, PartialEq, Eq)]
    enum Error {
        OldOpenGl,
        Window,
    }

    impl Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "{self:?}")
        }
    }

    fn graphics(error: &Error) -> bool {
        *error == Error::OldOpenGl
    }

    #[test]
    fn falls_back_to_the_next_renderer_when_opengl_is_too_old() {
        let mut tried = Vec::new();
        let worked = try_renderers(
            None,
            &[Fake::Glow, Fake::Wgpu],
            |renderer| {
                tried.push(renderer);
                match renderer {
                    Fake::Glow => (Err(Error::OldOpenGl), false),
                    Fake::Wgpu => (Ok(()), true),
                }
            },
            graphics,
        );
        assert_eq!(worked.ok(), Some(Fake::Wgpu));
        assert_eq!(tried, [Fake::Glow, Fake::Wgpu]);
    }

    #[test]
    fn the_first_renderer_that_works_is_the_only_one_tried() {
        let mut tried = Vec::new();
        let worked = try_renderers(
            None,
            &[Fake::Glow, Fake::Wgpu],
            |renderer| {
                tried.push(renderer);
                (Ok::<(), Error>(()), true)
            },
            graphics,
        );
        assert_eq!(worked.ok(), Some(Fake::Glow));
        assert_eq!(tried, [Fake::Glow]);
    }

    #[test]
    fn a_failure_after_the_window_started_is_never_retried_or_explained() {
        let mut tried = Vec::new();
        let failure = try_renderers(
            None,
            &[Fake::Glow, Fake::Wgpu],
            |renderer| {
                tried.push(renderer);
                (Err(Error::OldOpenGl), true)
            },
            graphics,
        )
        .expect_err("the window failed");
        assert_eq!(tried, [Fake::Glow]);
        assert!(!failure.graphics, "no dialog for a window that opened");
    }

    #[test]
    fn other_errors_are_returned_without_a_retry_or_a_dialog() {
        let mut tried = Vec::new();
        let failure = try_renderers(
            None,
            &[Fake::Glow, Fake::Wgpu],
            |renderer| {
                tried.push(renderer);
                (Err(Error::Window), false)
            },
            graphics,
        )
        .expect_err("the window failed");
        assert_eq!(tried, [Fake::Glow]);
        assert_eq!(failure.error, Error::Window);
        assert!(!failure.graphics);
    }

    #[test]
    fn the_dialog_explains_when_every_renderer_fails() {
        let mut tried = Vec::new();
        let failure = try_renderers(
            None,
            &[Fake::Glow, Fake::Wgpu],
            |renderer| {
                tried.push(renderer);
                (Err(Error::OldOpenGl), false)
            },
            graphics,
        )
        .expect_err("no renderer works");
        assert_eq!(tried, [Fake::Glow, Fake::Wgpu]);
        assert!(failure.graphics);
    }

    #[test]
    fn later_windows_use_the_renderer_that_worked() {
        let mut tried = Vec::new();
        let worked = try_renderers(
            Some(Fake::Wgpu),
            &[Fake::Glow, Fake::Wgpu],
            |renderer| {
                tried.push(renderer);
                (Ok::<(), Error>(()), true)
            },
            graphics,
        );
        assert_eq!(worked.ok(), Some(Fake::Wgpu));
        assert_eq!(tried, [Fake::Wgpu]);
    }

    #[test]
    fn glow_is_always_tried_first() {
        assert_eq!(RENDERERS.first(), Some(&Renderer::Glow));
        assert_eq!(RENDERERS.len(), if cfg!(feature = "wgpu") { 2 } else { 1 });
    }

    #[test]
    fn the_message_names_the_app_the_cause_and_the_remedy() {
        let message = Message::new("Solco", true, "egui_glow requires opengl 2.0+. ");
        assert_eq!(message.title, "Solco cannot open its window");
        assert!(message.body.starts_with("Solco needs your graphics driver"));
        assert!(message.body.contains("OpenGL 2 or later"));
        assert!(message.body.contains("open Solco again"));
        assert!(
            message
                .body
                .contains("Details: egui_glow requires opengl 2.0+.")
        );
        assert!(message.body.ends_with("Open the help page?"));
        assert!(message.asks_for_help);

        let without_help = Message::new("Solco", false, "x");
        assert!(!without_help.body.contains("help page"));
        assert!(!without_help.asks_for_help);
    }

    #[test]
    fn a_renderer_that_panicked_before_the_window_is_a_graphics_failure() {
        let tried = Tried::Panicked(panic_text(&"wgpu error: Validation Error"));
        assert!(tried.is_graphics_failure());
        let error = eframe::Error::from(tried);
        assert!(error.to_string().contains("wgpu error: Validation Error"));
        assert!(
            !is_graphics_failure(&error),
            "eframe's own errors stay as they are"
        );
    }

    #[test]
    fn app_and_window_errors_are_not_graphics_failures() {
        assert!(!is_graphics_failure(&eframe::Error::AppCreation(
            "the library is locked".into()
        )));
    }
}
