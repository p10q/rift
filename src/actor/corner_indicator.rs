use objc2::MainThreadMarker;
use objc2_core_foundation::CGRect;
use tracing::instrument;

use crate::actor::{self, reactor};
use crate::common::config::Config;
use crate::sys::screen::{CoordinateConverter, SpaceId};
use crate::ui::corner_indicator::CornerIndicatorWindow;

#[derive(Debug, Clone)]
pub struct ContainerSelection {
    pub space_id: SpaceId,
    pub frame: CGRect,
}

#[derive(Debug)]
pub enum Event {
    SelectionUpdated {
        space_id: SpaceId,
        container: Option<ContainerSelection>,
    },
    ScreenParametersChanged(CoordinateConverter),
    ConfigUpdated(Config),
}

pub struct CornerIndicator {
    config: Config,
    rx: Receiver,
    #[allow(dead_code)]
    mtm: MainThreadMarker,
    indicator: Option<CornerIndicatorWindow>,
    current_selection: Option<ContainerSelection>,
    #[allow(dead_code)]
    reactor_tx: reactor::Sender,
    #[allow(dead_code)]
    coordinate_converter: CoordinateConverter,
}

pub type Sender = actor::Sender<Event>;
pub type Receiver = actor::Receiver<Event>;

impl CornerIndicator {
    pub fn new(
        config: Config,
        rx: Receiver,
        mtm: MainThreadMarker,
        reactor_tx: reactor::Sender,
        coordinate_converter: CoordinateConverter,
    ) -> Self {
        Self {
            config,
            rx,
            mtm,
            indicator: None,
            current_selection: None,
            reactor_tx,
            coordinate_converter,
        }
    }

    pub async fn run(mut self) {
        if !self.is_enabled() {
            tracing::debug!("corner indicator disabled at start; will listen for config changes");
        }

        while let Some((span, event)) = self.rx.recv().await {
            let _guard = span.enter();
            self.handle_event(event);
        }
    }

    fn is_enabled(&self) -> bool {
        self.config.settings.ui.corner_indicator_enabled.unwrap_or(true)
    }

    #[instrument(name = "corner_indicator::handle_event", skip(self))]
    fn handle_event(&mut self, event: Event) {
        if !self.is_enabled()
            && !matches!(
                event,
                Event::ConfigUpdated(_) | Event::ScreenParametersChanged(_)
            )
        {
            return;
        }
        match event {
            Event::SelectionUpdated { space_id, container } => {
                self.handle_selection_updated(space_id, container);
            }
            Event::ScreenParametersChanged(converter) => {
                self.handle_screen_parameters_changed(converter);
            }
            Event::ConfigUpdated(config) => {
                self.handle_config_updated(config);
            }
        }
    }

    fn handle_selection_updated(&mut self, _space_id: SpaceId, container: Option<ContainerSelection>) {
        self.current_selection = container.clone();

        if let Some(selection) = container {
            // Show indicator at the container's frame
            self.ensure_indicator();
            if let Some(indicator) = &self.indicator {
                if let Err(err) = indicator.update(selection.frame) {
                    tracing::warn!(?err, "failed to update corner indicator");
                }
            }
        } else {
            // Hide indicator
            if let Some(indicator) = &self.indicator {
                if let Err(err) = indicator.hide() {
                    tracing::warn!(?err, "failed to hide corner indicator");
                }
            }
        }
    }

    fn handle_screen_parameters_changed(&mut self, converter: CoordinateConverter) {
        self.coordinate_converter = converter;
        tracing::debug!("Updated coordinate converter for corner indicator");
    }

    fn handle_config_updated(&mut self, config: Config) {
        let old_enabled = self.is_enabled();
        self.config = config;
        let new_enabled = self.is_enabled();

        if old_enabled && !new_enabled {
            // Disable the indicator
            if let Some(indicator) = &self.indicator {
                if let Err(err) = indicator.hide() {
                    tracing::warn!(?err, "failed to hide corner indicator during config update");
                }
            }
            self.indicator = None;
            self.current_selection = None;
        } else if !old_enabled && new_enabled {
            // Re-enable if we had a selection
            if let Some(selection) = self.current_selection.clone() {
                self.ensure_indicator();
                if let Some(indicator) = &self.indicator {
                    if let Err(err) = indicator.update(selection.frame) {
                        tracing::warn!(?err, "failed to re-enable corner indicator");
                    }
                }
            }
        }

        tracing::debug!("Updated corner indicator configuration");
    }

    fn ensure_indicator(&mut self) {
        if self.indicator.is_none() {
            match CornerIndicatorWindow::new() {
                Ok(indicator) => {
                    self.indicator = Some(indicator);
                }
                Err(err) => {
                    tracing::warn!(?err, "failed to create corner indicator window");
                }
            }
        }
    }
}
