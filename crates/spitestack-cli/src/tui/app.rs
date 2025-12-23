//! Application state for the TUI.
//!
//! SpiteStack Records - Code Angry.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::time::Instant;

use tokio::sync::mpsc;

use crate::tui::audio::AudioPlayer;

/// The main application state.
pub struct App {
    /// Current view mode
    pub mode: AppMode,

    /// Project state
    pub project: ProjectState,

    /// Compiler state
    pub compiler: CompilerState,

    /// Command input state
    pub input: InputState,

    /// Output log buffer
    pub output: OutputBuffer,

    /// Current errors from last compile
    pub errors: Vec<DiagnosticEntry>,

    /// Fix selection context (when in fix mode)
    pub fix_context: Option<FixContext>,

    /// File watcher state
    pub watcher: WatcherState,

    /// Channel for async task results
    pub task_tx: mpsc::Sender<TaskResult>,
    pub task_rx: mpsc::Receiver<TaskResult>,

    /// Should the app quit?
    pub should_quit: bool,

    /// Show splash screen?
    pub show_splash: bool,
    pub splash_start: Option<Instant>,

    // SpiteStack Records state
    /// Vinyl record animation state
    pub vinyl: VinylState,
    /// Music mode state (full-screen)
    pub music_mode: MusicModeState,
    /// VU meter state for compilation progress
    pub vu_meters: VuMeterState,
    /// Audio playback state
    pub audio: AudioState,
}

impl App {
    pub fn new() -> Self {
        let (task_tx, task_rx) = mpsc::channel(100);

        // Initialize audio player (may fail silently on systems without audio)
        let audio_player = AudioPlayer::new();

        Self {
            mode: AppMode::Command,  // Start with command input active
            project: ProjectState::default(),
            compiler: CompilerState::default(),
            input: InputState::default(),
            output: OutputBuffer::new(1000),
            errors: Vec::new(),
            fix_context: None,
            watcher: WatcherState::default(),
            task_tx,
            task_rx,
            should_quit: false,
            show_splash: true,
            splash_start: None,
            // SpiteStack Records
            vinyl: VinylState::default(),
            music_mode: MusicModeState::default(),
            vu_meters: VuMeterState::default(),
            audio: AudioState {
                enabled: audio_player.is_some(),
                player: audio_player,
            },
        }
    }

    /// Push a line to the output buffer.
    pub fn log(&mut self, level: OutputLevel, content: impl Into<String>) {
        self.output.push(OutputLine {
            content: content.into(),
            level,
            timestamp: Instant::now(),
        });
    }

    /// Push an info message.
    pub fn log_info(&mut self, content: impl Into<String>) {
        self.log(OutputLevel::Info, content);
    }

    /// Push a success message.
    pub fn log_success(&mut self, content: impl Into<String>) {
        self.log(OutputLevel::Success, content);
    }

    /// Push an error message.
    pub fn log_error(&mut self, content: impl Into<String>) {
        self.log(OutputLevel::Error, content);
    }
}

/// Application view mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppMode {
    #[default]
    Dashboard,
    /// Typing a command
    Command,
    /// Compilation in progress
    Compiling,
    /// Dev server running
    DevServer,
    /// Selecting a fix to apply
    FixSelection,
    /// Error detail view
    ErrorDetail,
    /// Full-screen music mode (SpiteStack Records)
    MusicMode,
}

/// Project state.
#[derive(Debug, Clone, Default)]
pub struct ProjectState {
    pub root: Option<PathBuf>,
    pub domain_dir: PathBuf,
    pub output_dir: PathBuf,
    pub name: Option<String>,
    pub last_compile: Option<CompileSnapshot>,
}

/// Snapshot of a compilation result.
#[derive(Debug, Clone)]
pub struct CompileSnapshot {
    pub timestamp: Instant,
    pub duration_ms: u128,
    pub aggregates: usize,
    pub orchestrators: usize,
    pub events: usize,
    pub success: bool,
}

/// Compiler state.
#[derive(Debug, Clone, Default)]
pub struct CompilerState {
    pub status: CompilerStatus,
    pub progress: Option<f64>,
    pub current_file: Option<String>,
}

/// Compiler status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompilerStatus {
    #[default]
    Idle,
    Parsing,
    Validating,
    Generating,
    Complete,
    Error,
}

impl CompilerStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Parsing => "parsing",
            Self::Validating => "validating",
            Self::Generating => "generating",
            Self::Complete => "done",
            Self::Error => "error",
        }
    }
}

/// Command input state.
#[derive(Debug, Clone, Default)]
pub struct InputState {
    pub buffer: String,
    pub cursor: usize,
    pub history: VecDeque<String>,
    pub history_index: Option<usize>,
    /// Index of currently selected suggestion in autocomplete
    pub selected_suggestion: usize,
    /// Whether to show the autocomplete dropdown
    pub show_suggestions: bool,
}

impl InputState {
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor = 0;
        self.history_index = None;
        self.selected_suggestion = 0;
        self.show_suggestions = false;
    }

    pub fn insert(&mut self, c: char) {
        self.buffer.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.buffer.remove(self.cursor);
        }
    }

    pub fn delete(&mut self) {
        if self.cursor < self.buffer.len() {
            self.buffer.remove(self.cursor);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.buffer.len() {
            self.cursor += 1;
        }
    }

    pub fn move_start(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.buffer.len();
    }

    pub fn push_history(&mut self) {
        if !self.buffer.is_empty() {
            // Don't duplicate consecutive entries
            if self.history.front() != Some(&self.buffer) {
                self.history.push_front(self.buffer.clone());
                if self.history.len() > 100 {
                    self.history.pop_back();
                }
            }
        }
    }

    pub fn history_up(&mut self) {
        if self.history.is_empty() {
            return;
        }
        let new_index = match self.history_index {
            None => 0,
            Some(i) => (i + 1).min(self.history.len() - 1),
        };
        self.history_index = Some(new_index);
        if let Some(entry) = self.history.get(new_index) {
            self.buffer = entry.clone();
            self.cursor = self.buffer.len();
        }
    }

    pub fn history_down(&mut self) {
        match self.history_index {
            None => {}
            Some(0) => {
                self.history_index = None;
                self.buffer.clear();
                self.cursor = 0;
            }
            Some(i) => {
                let new_index = i - 1;
                self.history_index = Some(new_index);
                if let Some(entry) = self.history.get(new_index) {
                    self.buffer = entry.clone();
                    self.cursor = self.buffer.len();
                }
            }
        }
    }
}

/// Output buffer for log messages.
#[derive(Debug, Clone)]
pub struct OutputBuffer {
    pub lines: VecDeque<OutputLine>,
    pub max_lines: usize,
    pub scroll_offset: usize,
    pub follow_tail: bool,
}

impl OutputBuffer {
    pub fn new(max_lines: usize) -> Self {
        Self {
            lines: VecDeque::new(),
            max_lines,
            scroll_offset: 0,
            follow_tail: true,
        }
    }

    pub fn push(&mut self, line: OutputLine) {
        self.lines.push_back(line);
        if self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
        if self.follow_tail {
            self.scroll_offset = 0;
        }
    }

    pub fn clear(&mut self) {
        self.lines.clear();
        self.scroll_offset = 0;
    }
}

/// A line in the output buffer.
#[derive(Debug, Clone)]
pub struct OutputLine {
    pub content: String,
    pub level: OutputLevel,
    pub timestamp: Instant,
}

/// Output level for styling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputLevel {
    #[default]
    Info,
    Success,
    Warning,
    Error,
    Debug,
}

/// A diagnostic entry (error from compilation).
#[derive(Debug, Clone)]
pub struct DiagnosticEntry {
    pub message: String,
    pub code: Option<String>,
    pub file: Option<PathBuf>,
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub snippet: Option<String>,
    pub help: Option<String>,
    pub fix: Option<FixSuggestion>,
}

/// A fix suggestion for an error.
#[derive(Debug, Clone)]
pub struct FixSuggestion {
    pub description: String,
    pub action: FixAction,
}

/// The action to take for a fix.
#[derive(Debug, Clone)]
pub enum FixAction {
    DeleteLine { file: PathBuf, line: usize },
    ReplaceLine { file: PathBuf, line: usize, content: String },
    InsertAfter { file: PathBuf, line: usize, content: String },
    ShowHint { message: String },
}

/// Context for fix selection mode.
#[derive(Debug, Clone)]
pub struct FixContext {
    pub errors: Vec<DiagnosticEntry>,
    pub selected_index: usize,
}

/// File watcher state.
#[derive(Debug, Clone, Default)]
pub struct WatcherState {
    pub active: bool,
    pub watched_paths: Vec<PathBuf>,
    pub pending_changes: usize,
    pub last_event: Option<Instant>,
}

/// Results from async tasks.
#[derive(Debug)]
pub enum TaskResult {
    CompileComplete {
        success: bool,
        duration_ms: u128,
        aggregates: usize,
        events: usize,
        errors: Vec<DiagnosticEntry>,
    },
    FileChanged(Vec<PathBuf>),
    DevServerStarted { port: u16 },
    DevServerStopped,
    FixApplied { file: PathBuf, success: bool },
}

// ============================================================================
// SpiteStack Records - "Code Angry"
// ============================================================================

/// Vinyl record animation state.
#[derive(Debug, Clone)]
pub struct VinylState {
    /// Current animation frame (continuous counter for smooth Braille rotation)
    pub frame: usize,
    /// Is the record spinning?
    pub spinning: bool,
    /// Is the record scratching (on error)?
    pub scratching: bool,
    /// Countdown frames for scratch animation
    pub scratch_frames: usize,
    /// RPM (slows down on pause, speeds up on compile)
    pub rpm: f32,
}

impl Default for VinylState {
    fn default() -> Self {
        Self {
            frame: 0,
            spinning: true,
            scratching: false,
            scratch_frames: 0,
            rpm: 33.3,
        }
    }
}

impl VinylState {
    /// Advance the vinyl animation by one frame.
    pub fn tick(&mut self) {
        if self.scratching {
            if self.scratch_frames > 0 {
                self.scratch_frames -= 1;
            } else {
                self.scratching = false;
            }
        } else if self.spinning {
            // Continuous increment for smooth Braille rotation
            self.frame = self.frame.wrapping_add(1);
        }
    }

    /// Get frame index for ASCII/Unicode tiers (0-3).
    /// These tiers have 4 discrete animation frames.
    pub fn ascii_frame(&self) -> usize {
        self.frame % 4
    }

    /// Trigger the scratch animation (on compile error).
    pub fn trigger_scratch(&mut self) {
        self.scratching = true;
        self.scratch_frames = 6; // About 300ms at 50ms tick rate
    }
}

/// State for the full-screen music mode.
#[derive(Debug, Clone, Default)]
pub struct MusicModeState {
    /// Currently selected link (0 = Spotify, 1 = Apple Music)
    pub selected_link: usize,
    /// Playlist URLs
    pub playlist_urls: PlaylistLinks,
}

/// Links to curated playlists.
#[derive(Debug, Clone)]
pub struct PlaylistLinks {
    pub spotify: String,
    pub apple_music: String,
}

impl Default for PlaylistLinks {
    fn default() -> Self {
        Self {
            // TODO: Replace with actual SpiteStack Records playlist
            spotify: "https://open.spotify.com/album/3dZBkDuB0QAO7hj1ACqgR9".to_string(), // Glassjaw - Worship and Tribute
            apple_music: "https://music.apple.com/album/worship-and-tribute/1440834623".to_string(),
        }
    }
}

/// VU meter state for compilation progress.
#[derive(Debug, Clone)]
pub struct VuMeterState {
    /// Progress for each compilation track
    pub tracks: [TrackProgress; 4],
    /// Currently active track index
    pub active_track: usize,
}

impl Default for VuMeterState {
    fn default() -> Self {
        Self {
            tracks: [
                TrackProgress::new("Parsing"),
                TrackProgress::new("IR Conv"),
                TrackProgress::new("Validate"),
                TrackProgress::new("CodeGen"),
            ],
            active_track: 0,
        }
    }
}

impl VuMeterState {
    /// Reset all tracks to zero.
    pub fn reset(&mut self) {
        for track in &mut self.tracks {
            track.progress = 0.0;
            track.peak = 0.0;
            track.complete = false;
        }
        self.active_track = 0;
    }

    /// Update VU meters based on compiler status.
    pub fn update_for_status(&mut self, status: &CompilerStatus) {
        match status {
            CompilerStatus::Idle => self.reset(),
            CompilerStatus::Parsing => {
                self.active_track = 0;
                self.tracks[0].progress = 0.5;
            }
            CompilerStatus::Validating => {
                self.tracks[0].progress = 1.0;
                self.tracks[0].complete = true;
                self.tracks[1].progress = 1.0;
                self.tracks[1].complete = true;
                self.active_track = 2;
                self.tracks[2].progress = 0.5;
            }
            CompilerStatus::Generating => {
                for track in &mut self.tracks[0..3] {
                    track.progress = 1.0;
                    track.complete = true;
                }
                self.active_track = 3;
                self.tracks[3].progress = 0.5;
            }
            CompilerStatus::Complete => {
                for track in &mut self.tracks {
                    track.progress = 1.0;
                    track.complete = true;
                }
            }
            CompilerStatus::Error => {
                // Keep current state
            }
        }
    }
}

/// Progress for a single compilation track.
#[derive(Debug, Clone)]
pub struct TrackProgress {
    /// Track name
    pub name: &'static str,
    /// Progress (0.0 to 1.0)
    pub progress: f32,
    /// Peak value for VU meter decay effect
    pub peak: f32,
    /// Is this track complete?
    pub complete: bool,
}

impl TrackProgress {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            progress: 0.0,
            peak: 0.0,
            complete: false,
        }
    }
}

/// Audio playback state.
#[derive(Debug)]
pub struct AudioState {
    /// Is audio enabled?
    pub enabled: bool,
    /// Audio player handle
    pub player: Option<AudioPlayer>,
}

impl Clone for AudioState {
    fn clone(&self) -> Self {
        // AudioPlayer can't be cloned, so we just disable audio in the clone
        Self {
            enabled: false,
            player: None,
        }
    }
}
