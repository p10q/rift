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
    pub child_count: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct RangeSelection {
    pub space_id: SpaceId,
    pub frames: Vec<CGRect>,
}

#[derive(Debug)]
pub enum Event {
    SelectionUpdated {
        space_id: SpaceId,
        container: Option<ContainerSelection>,
        range: Option<RangeSelection>,
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
    range_indicators: Vec<CornerIndicatorWindow>,
    current_selection: Option<ContainerSelection>,
    current_range: Option<RangeSelection>,
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
            range_indicators: Vec::new(),
            current_selection: None,
            current_range: None,
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
            Event::SelectionUpdated { space_id, container, range } => {
                self.handle_selection_updated(space_id, container, range);
            }
            Event::ScreenParametersChanged(converter) => {
                self.handle_screen_parameters_changed(converter);
            }
            Event::ConfigUpdated(config) => {
                self.handle_config_updated(config);
            }
        }
    }

    fn handle_selection_updated(&mut self, _space_id: SpaceId, container: Option<ContainerSelection>, range: Option<RangeSelection>) {
        tracing::debug!("handle_selection_updated: container={:?}, range frames={}", 
            container.as_ref().map(|c| format!("frame={:?}", c.frame)), 
            range.as_ref().map(|r| r.frames.len()).unwrap_or(0));
        // Check if anything actually changed
        let needs_update = match (&self.current_selection, &container) {
            (None, None) => false,
            (Some(_), None) | (None, Some(_)) => true,
            (Some(old), Some(new)) => {
                // Check if frame or child_count changed
                let frame_changed = (old.frame.origin.x - new.frame.origin.x).abs() > 0.5
                    || (old.frame.origin.y - new.frame.origin.y).abs() > 0.5
                    || (old.frame.size.width - new.frame.size.width).abs() > 0.5
                    || (old.frame.size.height - new.frame.size.height).abs() > 0.5;
                let child_count_changed = old.child_count != new.child_count;
                
                frame_changed || child_count_changed
            }
        };

        let range_changed = match (&self.current_range, &range) {
            (None, None) => false,
            (Some(_), None) | (None, Some(_)) => true,
            (Some(old), Some(new)) => old.frames.len() != new.frames.len() || 
                old.frames.iter().zip(&new.frames).any(|(a, b)| {
                    (a.origin.x - b.origin.x).abs() > 0.5 ||
                    (a.origin.y - b.origin.y).abs() > 0.5 ||
                    (a.size.width - b.size.width).abs() > 0.5 ||
                    (a.size.height - b.size.height).abs() > 0.5
                }),
        };

        if !needs_update && !range_changed {
            return;
        }

        self.current_selection = container.clone();
        self.current_range = range.clone();

        // Clear all existing indicators
        if let Some(indicator) = &self.indicator {
            let _ = indicator.hide();
        }
        self.indicator = None;
        
        for indicator in &self.range_indicators {
            let _ = indicator.hide();
        }
        self.range_indicators.clear();

        // If there's a range, show yellow dots on ALL nodes in the range
        if let Some(range_sel) = &range {
            tracing::debug!("Creating {} yellow indicators for range", range_sel.frames.len());
            for (i, frame) in range_sel.frames.iter().enumerate() {
                match CornerIndicatorWindow::new_with_color(*frame, None, (1.0, 0.8, 0.0)) {
                    Ok(indicator) => {
                        tracing::debug!("Created yellow indicator {} at {:?}", i, frame);
                        self.range_indicators.push(indicator);
                    }
                    Err(err) => {
                        tracing::warn!(?err, "failed to create range indicator window");
                    }
                }
            }
        } else if let Some(selection) = container {
            // No range - show blue dots on the selected node only
            tracing::debug!("Creating blue indicator at {:?}", selection.frame);
            match CornerIndicatorWindow::new() {
                Ok(indicator) => {
                    if let Err(err) = indicator.update(selection.frame, selection.child_count) {
                        tracing::warn!(?err, "failed to update corner indicator");
                    } else {
                        tracing::debug!("Successfully updated blue indicator");
                    }
                    self.indicator = Some(indicator);
                }
                Err(err) => {
                    tracing::warn!(?err, "failed to recreate corner indicator window");
                }
            }
        } else {
            tracing::debug!("No container or range to show");
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
                    if let Err(err) = indicator.update(selection.frame, selection.child_count) {
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
