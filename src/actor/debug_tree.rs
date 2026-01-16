use objc2::MainThreadMarker;
use tracing::instrument;

use crate::actor::{self, reactor};
use crate::common::config::Config;
use crate::ui::debug_tree::DebugTreeWindow;

#[derive(Debug)]
pub enum Event {
    Show {
        tree_text: String,
    },
    Hide,
    Toggle {
        tree_text: String,
    },
    ConfigUpdated(Config),
}

pub struct DebugTree {
    config: Config,
    rx: Receiver,
    #[allow(dead_code)]
    mtm: MainThreadMarker,
    window: Option<DebugTreeWindow>,
    #[allow(dead_code)]
    reactor_tx: reactor::Sender,
}

pub type Sender = actor::Sender<Event>;
pub type Receiver = actor::Receiver<Event>;

impl DebugTree {
    pub fn new(
        config: Config,
        rx: Receiver,
        mtm: MainThreadMarker,
        reactor_tx: reactor::Sender,
    ) -> Self {
        Self {
            config,
            rx,
            mtm,
            window: None,
            reactor_tx,
        }
    }

    pub async fn run(mut self) {
        if !self.is_enabled() {
            tracing::debug!("debug tree disabled at start; will listen for config changes");
        }

        while let Some((span, event)) = self.rx.recv().await {
            let _guard = span.enter();
            self.handle_event(event);
        }
    }

    fn is_enabled(&self) -> bool {
        // Debug tree is always enabled (can be toggled on/off via commands)
        self.config.settings.ui.debug_tree_enabled.unwrap_or(true)
    }

    #[instrument(name = "debug_tree::handle_event", skip(self))]
    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Show { tree_text } => {
                self.handle_show(tree_text);
            }
            Event::Hide => {
                self.handle_hide();
            }
            Event::Toggle { tree_text } => {
                self.handle_toggle(tree_text);
            }
            Event::ConfigUpdated(config) => {
                self.handle_config_updated(config);
            }
        }
    }

    fn handle_show(&mut self, tree_text: String) {
        if !self.is_enabled() {
            return;
        }

        self.ensure_window();
        if let Some(window) = &self.window {
            // Only update if the window is currently visible
            if window.is_visible() {
                if let Err(err) = window.update(tree_text) {
                    tracing::warn!(?err, "failed to update debug tree window");
                }
            }
        }
    }

    fn handle_hide(&mut self) {
        if let Some(window) = &self.window {
            if let Err(err) = window.hide() {
                tracing::warn!(?err, "failed to hide debug tree window");
            }
        }
    }

    fn handle_toggle(&mut self, tree_text: String) {
        if !self.is_enabled() {
            tracing::debug!("debug tree toggle ignored: feature disabled");
            return;
        }

        self.ensure_window();
        
        if let Some(window) = &self.window {
            if window.is_visible() {
                // Window is visible, hide it
                if let Err(err) = window.hide() {
                    tracing::warn!(?err, "failed to hide debug tree window");
                }
                tracing::debug!("Debug tree window hidden");
            } else {
                // Window is hidden, show it with new content
                if let Err(err) = window.update(tree_text) {
                    tracing::warn!(?err, "failed to update debug tree window");
                } else {
                    tracing::debug!("Debug tree window shown");
                }
            }
        }
    }

    fn handle_config_updated(&mut self, config: Config) {
        let old_enabled = self.is_enabled();
        self.config = config;
        let new_enabled = self.is_enabled();

        if old_enabled && !new_enabled {
            // Disable the debug tree window
            if let Some(window) = &self.window {
                if let Err(err) = window.hide() {
                    tracing::warn!(?err, "failed to hide debug tree during config update");
                }
            }
            self.window = None;
        } else if !old_enabled && new_enabled {
            // Re-enable - window will be created on next show/toggle
            tracing::debug!("debug tree re-enabled via config update");
        }

        tracing::debug!("Updated debug tree configuration");
    }

    fn ensure_window(&mut self) {
        if self.window.is_none() {
            match DebugTreeWindow::new() {
                Ok(window) => {
                    self.window = Some(window);
                }
                Err(err) => {
                    tracing::warn!(?err, "failed to create debug tree window");
                }
            }
        }
    }
}
