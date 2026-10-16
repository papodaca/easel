use gpui::{Context, EventEmitter};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideMode {
    Editor,
    Settings,
}

pub struct AppState {
    pub side_mode: SideMode,
    pub quit_requested: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            side_mode: SideMode::Editor,
            quit_requested: false,
        }
    }

    pub fn toggle_side_mode(&mut self, cx: &mut Context<Self>) {
        self.side_mode = match self.side_mode {
            SideMode::Editor => SideMode::Settings,
            SideMode::Settings => SideMode::Editor,
        };
        cx.notify();
    }

    pub fn request_quit(&mut self, cx: &mut Context<Self>) {
        self.quit_requested = true;
        cx.notify();
    }
}

impl EventEmitter<()> for AppState {}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}
