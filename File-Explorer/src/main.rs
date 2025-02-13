use druid::widget::{Button, Flex, Label, List, Scroll, TextBox};
use druid::{
    AppDelegate, AppLauncher, Command, Data, DelegateCtx, Env, Lens, Selector, Target,
    Widget, WidgetExt, WindowDesc, commands, FileDialogOptions, theme, Color,
};
use regex::Regex;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use walkdir::WalkDir;
use std::fs;

#[cfg(target_os = "macos")]
fn open_path(path: &str) {
    std::process::Command::new("open")
        .arg(path)
        .spawn()
        .expect("failed to open file");
}

#[cfg(target_os = "windows")]
fn open_path(path: &str) {
    std::process::Command::new("explorer")
        .arg(path)
        .spawn()
        .expect("failed to open file");
}

// A selector for updating search results from a background thread.
// Note: Now the payload is an Arc<Vec<String>>
const UPDATE_SEARCH_RESULTS: Selector<Arc<Vec<String>>> =
    Selector::new("update_search_results");

#[derive(Clone, Data, Lens)]
struct AppState {
    pub root_path: String,
    pub search_term: String,
    // Change from im::Vector<String> to Arc<Vec<String>> for compatibility with ListIter
    pub search_results: Arc<Vec<String>>,
}

fn build_ui() -> impl Widget<AppState> {
    // Use string literals for the buttons instead of Label::new(...)
    let choose_dir_btn = Button::new("Choose Directory")
        .padding(8.0)
        .background(Color::rgb8(0x44, 0x44, 0x44))
        .on_click(|ctx, _data, _env| {
            ctx.submit_command(Command::new(commands::SHOW_OPEN_PANEL, FileDialogOptions::default(), Target::Auto));
        });

    let search_btn = Button::new("Search")
        .padding(8.0)
        .background(Color::rgb8(0x44, 0x44, 0x44))
        .on_click(|ctx, data: &mut AppState, _env| {
            let root = data.root_path.clone();
            let term = data.search_term.clone();

            // Clear any previous search results.
            data.search_results = Arc::new(Vec::new());

            let sink = ctx.get_external_handle();

            thread::spawn(move || {
                let results = search_files(&root, &term);
                // Send the search results back to the UI thread.
                sink.submit_command(UPDATE_SEARCH_RESULTS, results, Target::Auto)
                    .expect("Failed to submit command");
            });
        });

    // TextBox: dark background and white text; uses lens for state binding
    let directory_box = TextBox::new()
        .with_placeholder("Enter directory path")
        .with_text_size(14.0)
        .with_text_color(Color::WHITE)
        .padding(8.0)
        .background(Color::rgb8(0x33, 0x33, 0x33))
        .lens(AppState::root_path);

    let search_box = TextBox::new()
        .with_placeholder("Enter search term")
        .with_text_size(14.0)
        .with_text_color(Color::WHITE)
        .padding(8.0)
        .background(Color::rgb8(0x33, 0x33, 0x33))
        .lens(AppState::search_term);

    // List: style each item with white text, padding, dark background, border, and rounded corners.
    let results_list = List::new(|| {
        Label::new(|item: &String, _env: &_| format!("{}", item))
            .with_text_size(14.0)
            .with_text_color(Color::WHITE)
            .padding(8.0)
            .background(Color::rgb8(0x33, 0x33, 0x33))
            .border(Color::rgb8(0x55, 0x55, 0x55), 1.0)
            .rounded(4.0)
            .on_click(|_ctx, item: &mut String, _env| {
                open_path(item);
            })
    })
    .with_spacing(4.0)
    .lens(AppState::search_results);

    let scroll = Scroll::new(results_list)
        .background(Color::BLACK)
        .expand();

    // Main layout with black background
    Flex::column()
        .with_child(choose_dir_btn)
        .with_child(directory_box)
        .with_child(search_box)
        .with_child(search_btn)
        .with_flex_child(scroll, 1.0)
        .padding(12.0)
        .background(Color::BLACK)
}

/// Searches files and directories under the given directory whose names match the search term (case-insensitive)
/// and returns an Arc<Vec<String>>.
fn search_files(root_path: &str, search_term: &str) -> Arc<Vec<String>> {
    let regex = Regex::new(&format!(r"(?i){}", search_term)).unwrap();
    let root = PathBuf::from(root_path);
    let results = search_files_recursive(&root, &regex);
    Arc::new(results)
}

fn search_files_recursive(dir: &Path, regex: &Regex) -> Vec<String> {
    let mut results = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir).expect("read_dir call failed") {
            if let Ok(entry) = entry {
                if entry.path().is_file() || entry.path().is_dir() {
                    if let Some(name) = entry.path().file_name().and_then(|n| n.to_str()) {
                        if regex.is_match(name) {
                            results.push(entry.path().display().to_string());
                        }
                    }
                }
            }
        }
    }
    results
}

/// A delegate to handle commands coming from the background thread.
struct Delegate;

impl AppDelegate<AppState> for Delegate {
    fn command(
        &mut self,
        _ctx: &mut DelegateCtx,
        _target: Target,
        cmd: &Command,
        data: &mut AppState,
        _env: &Env,
    ) -> druid::Handled {
        if let Some(results) = cmd.get(UPDATE_SEARCH_RESULTS) {
            data.search_results = results.clone();
            return druid::Handled::Yes;
        }
        if cmd.is(commands::SHOW_OPEN_PANEL) {
            let dialog = rfd::FileDialog::new();
            if let Some(folder) = dialog.pick_folder() {
                data.root_path = folder.to_string_lossy().to_string();
                data.search_results = Arc::new(Vec::new());
                return druid::Handled::Yes;
            }
            // Removed file selection to force folder-only selection.
        }
        druid::Handled::No
    }
}

fn main() {
    // Create the main window.
    let main_window = WindowDesc::new(build_ui()).title("macOS File Explorer");

    // Initialize the state with the current directory.
    let initial_state = AppState {
        root_path: std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .display()
            .to_string(),
        search_term: "".to_string(),
        search_results: Arc::new(Vec::new()),
    };

    // Launch the application with the delegate to handle background commands.
    AppLauncher::with_window(main_window)
        .configure_env(|env: &mut Env, _| {
            env.set(druid::theme::BACKGROUND_LIGHT, Color::BLACK);
            env.set(druid::theme::TEXT_COLOR, Color::WHITE);
            env.set(druid::theme::PLACEHOLDER_COLOR, Color::grey(0.6));
            // Replace WIDGET_BACKGROUND_COLOR with WINDOW_BACKGROUND_COLOR
            env.set(druid::theme::WINDOW_BACKGROUND_COLOR, Color::rgb8(0x33, 0x33, 0x33));
            env.set(druid::theme::BUTTON_DARK, Color::rgb8(0x44, 0x44, 0x44));
        })
        .delegate(Delegate)
        .launch(initial_state)
        .expect("Failed to launch application");
}
