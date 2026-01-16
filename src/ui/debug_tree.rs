use std::cell::RefCell;
use std::ptr;

use objc2::rc::Retained;
use objc2_app_kit::NSStatusWindowLevel;
use objc2_core_foundation::{CFType, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGContext;
use objc2_quartz_core::{CALayer, CATextLayer, CATransaction, kCAAlignmentLeft};
use tracing::warn;

use crate::sys::cgs_window::{CgsWindow, CgsWindowError};
use crate::sys::skylight::{CFRelease, G_CONNECTION, SLSFlushWindowContentRegion, SLWindowContextCreate};
use crate::ui::stack_line::Color;

unsafe extern "C" {
    fn CGContextFlush(ctx: *mut CGContext);
    fn CGContextSaveGState(ctx: *mut CGContext);
    fn CGContextRestoreGState(ctx: *mut CGContext);
}

/// Maximum window dimensions
const MAX_WIDTH: f64 = 800.0;
const MAX_HEIGHT: f64 = 600.0;
/// Padding from screen edges
const SCREEN_PADDING: f64 = 20.0;
/// Font size for debug tree text
const FONT_SIZE: f64 = 11.0;
/// Padding inside the window
const CONTENT_PADDING: f64 = 10.0;

pub struct DebugTreeWindow {
    cgs_window: CgsWindow,
    root_layer: Retained<CALayer>,
    current_frame: RefCell<Option<CGRect>>,
}

impl DebugTreeWindow {
    pub fn new() -> Result<Self, CgsWindowError> {
        // Create initial window with minimal size
        let initial_frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1.0, 1.0));
        let cgs_window = CgsWindow::new(initial_frame)?;
        
        // Configure window properties
        if let Err(err) = cgs_window.set_opacity(false) {
            warn!(error=?err, "failed to set debug tree window opacity");
        }
        if let Err(err) = cgs_window.set_alpha(0.9) {
            warn!(error=?err, "failed to set debug tree window alpha");
        }
        if let Err(err) = cgs_window.set_level(NSStatusWindowLevel as i32) {
            warn!(error=?err, "failed to set debug tree window level");
        }

        let root_layer = CALayer::layer();
        root_layer.setFrame(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1.0, 1.0)));
        // Set scale for retina displays to avoid blurriness
        root_layer.setContentsScale(2.0);

        Ok(Self {
            cgs_window,
            root_layer,
            current_frame: RefCell::new(None),
        })
    }

    pub fn update(&self, tree_text: String) -> Result<(), CgsWindowError> {
        // Calculate window size based on content
        let (window_width, window_height) = self.calculate_window_size(&tree_text);
        
        // Position at top-left corner with padding
        let window_frame = CGRect::new(
            CGPoint::new(SCREEN_PADDING, SCREEN_PADDING),
            CGSize::new(window_width, window_height),
        );
        
        *self.current_frame.borrow_mut() = Some(window_frame);
        
        // Update window shape
        self.cgs_window.set_shape(window_frame)?;
        
        // Set resolution for retina displays
        if let Err(err) = self.cgs_window.set_resolution(2.0) {
            warn!(error=?err, "failed to set debug tree window resolution");
        }
        
        // Update root layer
        self.root_layer.setFrame(CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(window_width, window_height),
        ));
        self.root_layer.setContentsScale(2.0);

        // Clear existing layers
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        
        unsafe { 
            self.root_layer.setSublayers(None);
        }

        // Create background layer
        let background_layer = CALayer::layer();
        background_layer.setFrame(CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(window_width, window_height),
        ));
        let bg_color = Color::new(0.1, 0.1, 0.1, 0.85);
        background_layer.setBackgroundColor(Some(&bg_color.to_nscolor().CGColor()));
        background_layer.setCornerRadius(8.0);
        background_layer.setContentsScale(2.0);
        self.root_layer.addSublayer(&background_layer);

        // Create text layer
        let text_layer = CATextLayer::new();
        let text_frame = CGRect::new(
            CGPoint::new(CONTENT_PADDING, CONTENT_PADDING),
            CGSize::new(window_width - 2.0 * CONTENT_PADDING, window_height - 2.0 * CONTENT_PADDING),
        );
        text_layer.setFrame(text_frame);
        
        unsafe {
            text_layer.setString(Some(&objc2_foundation::NSString::from_str(&tree_text)));
            text_layer.setAlignmentMode(kCAAlignmentLeft);
        }
        
        text_layer.setFontSize(FONT_SIZE);
        text_layer.setForegroundColor(Some(&Color::new(1.0, 1.0, 1.0, 1.0).to_nscolor().CGColor()));
        text_layer.setContentsScale(2.0);
        text_layer.setWrapped(true);
        
        self.root_layer.addSublayer(&text_layer);

        CATransaction::commit();

        // Force the window to update
        let _ = self.cgs_window.order_out();
        
        // Present the window with new content
        self.present();
        
        self.cgs_window.order_above(None)
    }

    pub fn hide(&self) -> Result<(), CgsWindowError> {
        *self.current_frame.borrow_mut() = None;
        self.cgs_window.order_out()
    }

    pub fn show(&self) -> Result<(), CgsWindowError> {
        if self.current_frame.borrow().is_some() {
            self.cgs_window.order_above(None)
        } else {
            Ok(())
        }
    }

    pub fn is_visible(&self) -> bool {
        self.current_frame.borrow().is_some()
    }

    fn calculate_window_size(&self, text: &str) -> (f64, f64) {
        // Count lines and estimate maximum line width
        let lines: Vec<&str> = text.lines().collect();
        let line_count = lines.len();
        let max_line_length = lines.iter().map(|l| l.len()).max().unwrap_or(0);
        
        // Estimate character width for monospace font (approximately)
        let char_width = FONT_SIZE * 0.6;
        let line_height = FONT_SIZE * 1.4;
        
        // Calculate dimensions with padding
        let content_width = (max_line_length as f64 * char_width).min(MAX_WIDTH - 2.0 * CONTENT_PADDING);
        let content_height = (line_count as f64 * line_height).min(MAX_HEIGHT - 2.0 * CONTENT_PADDING);
        
        let window_width = (content_width + 2.0 * CONTENT_PADDING).min(MAX_WIDTH);
        let window_height = (content_height + 2.0 * CONTENT_PADDING).min(MAX_HEIGHT);
        
        (window_width, window_height)
    }

    fn present(&self) {
        let Some(_frame) = *self.current_frame.borrow() else {
            return;
        };
        
        let ctx: *mut CGContext = unsafe {
            SLWindowContextCreate(
                *G_CONNECTION,
                self.cgs_window.id(),
                ptr::null_mut(),
            )
        };
        if ctx.is_null() {
            warn!("Failed to create window context for debug tree");
            return;
        }

        unsafe {
            CGContextSaveGState(ctx);
            self.root_layer.renderInContext(&*ctx);
            CGContextRestoreGState(ctx);
            CGContextFlush(ctx);
            SLSFlushWindowContentRegion(*G_CONNECTION, self.cgs_window.id(), ptr::null_mut());
            CFRelease(ctx as *mut CFType);
        }
    }
}

impl Drop for DebugTreeWindow {
    fn drop(&mut self) {
        let _ = self.hide();
    }
}
