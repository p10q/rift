use std::cell::RefCell;
use std::ptr;

use objc2::rc::Retained;
use objc2_app_kit::NSStatusWindowLevel;
use objc2_core_foundation::{CFType, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGContext;
use objc2_quartz_core::{CALayer, CATransaction};
use tracing::warn;

use crate::sys::cgs_window::{CgsWindow, CgsWindowError};
use crate::sys::skylight::{CFRelease, G_CONNECTION, SLSFlushWindowContentRegion, SLWindowContextCreate};
use crate::ui::stack_line::Color;

unsafe extern "C" {
    fn CGContextFlush(ctx: *mut CGContext);
    fn CGContextSaveGState(ctx: *mut CGContext);
    fn CGContextRestoreGState(ctx: *mut CGContext);
}

/// Size of each corner dot in points
const DOT_SIZE: f64 = 10.0;
/// How far inset from the actual corner
const CORNER_INSET: f64 = 2.0;

pub struct CornerIndicatorWindow {
    cgs_window: CgsWindow,
    root_layer: Retained<CALayer>,
    current_frame: RefCell<Option<CGRect>>,
}

impl CornerIndicatorWindow {
    pub fn new() -> Result<Self, CgsWindowError> {
        // Create a single window that will cover the entire screen
        let initial_frame = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1.0, 1.0));
        let cgs_window = CgsWindow::new(initial_frame)?;
        
        if let Err(err) = cgs_window.set_opacity(false) {
            warn!(error=?err, "failed to set corner indicator opacity");
        }
        if let Err(err) = cgs_window.set_alpha(1.0) {
            warn!(error=?err, "failed to set corner indicator alpha");
        }
        if let Err(err) = cgs_window.set_level(NSStatusWindowLevel as i32) {
            warn!(error=?err, "failed to set corner indicator level");
        }

        let root_layer = CALayer::layer();
        root_layer.setFrame(CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(1.0, 1.0)));

        Ok(Self {
            cgs_window,
            root_layer,
            current_frame: RefCell::new(None),
        })
    }

    pub fn update(&self, container_frame: CGRect) -> Result<(), CgsWindowError> {
        *self.current_frame.borrow_mut() = Some(container_frame);

        // Update the window to cover the container area
        self.cgs_window.set_shape(container_frame)?;
        self.root_layer.setFrame(CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(container_frame.size.width, container_frame.size.height),
        ));

        // Clear existing layers
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        
        unsafe { self.root_layer.setSublayers(None) };

        // Create 4 corner dot layers (in local coordinates)
        let positions = [
            // Top-left
            CGPoint::new(CORNER_INSET, container_frame.size.height - CORNER_INSET - DOT_SIZE),
            // Top-right  
            CGPoint::new(container_frame.size.width - CORNER_INSET - DOT_SIZE, container_frame.size.height - CORNER_INSET - DOT_SIZE),
            // Bottom-left
            CGPoint::new(CORNER_INSET, CORNER_INSET),
            // Bottom-right
            CGPoint::new(container_frame.size.width - CORNER_INSET - DOT_SIZE, CORNER_INSET),
        ];

        let color = Color::new(0.0, 0.5, 1.0, 0.95); // Blue color

        for pos in &positions {
            let layer = CALayer::layer();
            layer.setFrame(CGRect::new(*pos, CGSize::new(DOT_SIZE, DOT_SIZE)));
            layer.setCornerRadius(DOT_SIZE / 2.0); // Make it circular
            layer.setBackgroundColor(Some(&color.to_nscolor().CGColor()));
            
            // Add border for better visibility
            let border_color = Color::new(1.0, 1.0, 1.0, 0.8); // White border
            layer.setBorderColor(Some(&border_color.to_nscolor().CGColor()));
            layer.setBorderWidth(1.5);
            
            self.root_layer.addSublayer(&layer);
        }

        CATransaction::commit();

        // Present the window
        self.present();
        
        self.cgs_window.order_above(None)
    }

    pub fn hide(&self) -> Result<(), CgsWindowError> {
        *self.current_frame.borrow_mut() = None;
        self.cgs_window.order_out()
    }

    pub fn is_visible(&self) -> bool {
        self.current_frame.borrow().is_some()
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
            warn!("Failed to create window context for corner indicator");
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

impl Drop for CornerIndicatorWindow {
    fn drop(&mut self) {
        let _ = self.hide();
    }
}
