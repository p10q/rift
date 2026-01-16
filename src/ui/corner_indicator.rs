use std::cell::RefCell;
use std::ptr;

use objc2::rc::Retained;
use objc2_app_kit::NSStatusWindowLevel;
use objc2_core_foundation::{CFType, CGPoint, CGRect, CGSize};
use objc2_core_graphics::CGContext;
use objc2_foundation::NSString;
use objc2_quartz_core::{CALayer, CATextLayer, CATransaction, kCAAlignmentCenter};
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
const DOT_SIZE: f64 = 12.0;
/// How far inset from the actual corner
const CORNER_INSET: f64 = 4.0;
/// Size of the count indicator box
const COUNT_BOX_SIZE: f64 = 24.0;

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
        // Set scale for retina displays to avoid blurriness
        root_layer.setContentsScale(2.0);

        Ok(Self {
            cgs_window,
            root_layer,
            current_frame: RefCell::new(None),
        })
    }

    pub fn new_with_color(container_frame: CGRect, child_count: Option<usize>, color_rgb: (f64, f64, f64)) -> Result<Self, CgsWindowError> {
        let window = Self::new()?;
        window.update_with_color(container_frame, child_count, color_rgb)?;
        Ok(window)
    }

    fn update_with_color(&self, container_frame: CGRect, _child_count: Option<usize>, color_rgb: (f64, f64, f64)) -> Result<(), CgsWindowError> {
        *self.current_frame.borrow_mut() = Some(container_frame);

        // Update the window to cover the container area
        self.cgs_window.set_shape(container_frame)?;
        
        // Set resolution for retina displays
        if let Err(err) = self.cgs_window.set_resolution(2.0) {
            warn!(error=?err, "failed to set corner indicator resolution");
        }
        
        self.root_layer.setFrame(CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(container_frame.size.width, container_frame.size.height),
        ));
        self.root_layer.setContentsScale(2.0);

        // Clear existing layers and force a complete redraw
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        
        // Completely clear the root layer
        unsafe { 
            self.root_layer.setSublayers(None);
            // Force the root layer to redraw by updating its frame
            let current_frame = self.root_layer.frame();
            self.root_layer.setFrame(current_frame);
        }

        let color = Color::new(color_rgb.0, color_rgb.1, color_rgb.2, 0.95);
        let border_color = Color::new(1.0, 1.0, 1.0, 0.9); // White border

        // Show dots in all 4 corners for range indicators
        let dot_positions = [
            // Top-left
            CGPoint::new(CORNER_INSET, container_frame.size.height - CORNER_INSET - DOT_SIZE),
            // Top-right  
            CGPoint::new(container_frame.size.width - CORNER_INSET - DOT_SIZE, container_frame.size.height - CORNER_INSET - DOT_SIZE),
            // Bottom-left
            CGPoint::new(CORNER_INSET, CORNER_INSET),
            // Bottom-right
            CGPoint::new(container_frame.size.width - CORNER_INSET - DOT_SIZE, CORNER_INSET),
        ];

        for pos in &dot_positions {
            let layer = CALayer::layer();
            layer.setFrame(CGRect::new(*pos, CGSize::new(DOT_SIZE, DOT_SIZE)));
            layer.setCornerRadius(DOT_SIZE / 2.0);
            layer.setBackgroundColor(Some(&color.to_nscolor().CGColor()));
            layer.setBorderColor(Some(&border_color.to_nscolor().CGColor()));
            layer.setBorderWidth(2.0);
            layer.setContentsScale(2.0);
            self.root_layer.addSublayer(&layer);
        }

        CATransaction::commit();

        // Force the window to update by briefly ordering it out and back in
        let _ = self.cgs_window.order_out();
        
        // Present the window with new content
        self.present();
        
        self.cgs_window.order_above(None)
    }

    pub fn update(&self, container_frame: CGRect, child_count: Option<usize>) -> Result<(), CgsWindowError> {
        *self.current_frame.borrow_mut() = Some(container_frame);

        // Update the window to cover the container area
        self.cgs_window.set_shape(container_frame)?;
        
        // Set resolution for retina displays
        if let Err(err) = self.cgs_window.set_resolution(2.0) {
            warn!(error=?err, "failed to set corner indicator resolution");
        }
        
        self.root_layer.setFrame(CGRect::new(
            CGPoint::new(0.0, 0.0),
            CGSize::new(container_frame.size.width, container_frame.size.height),
        ));
        self.root_layer.setContentsScale(2.0);

        // Clear existing layers and force a complete redraw
        CATransaction::begin();
        CATransaction::setDisableActions(true);
        
        // Completely clear the root layer
        unsafe { 
            self.root_layer.setSublayers(None);
            // Force the root layer to redraw by updating its frame
            let current_frame = self.root_layer.frame();
            self.root_layer.setFrame(current_frame);
        }

        let color = Color::new(0.0, 0.5, 1.0, 0.95); // Blue color
        let border_color = Color::new(1.0, 1.0, 1.0, 0.9); // White border

        // If we have a child count (container), show count in top-left, dots in other 3 corners
        // If no child count (window), show dots in all 4 corners
        if let Some(count) = child_count {
            // Top-left: Show count badge
            let count_text = if count <= 9 {
                format!("{}", count)
            } else {
                "+".to_string()
            };
            
            let text_layer = CATextLayer::new();
            let top_left_pos = CGPoint::new(
                CORNER_INSET,
                container_frame.size.height - CORNER_INSET - COUNT_BOX_SIZE
            );
            text_layer.setFrame(CGRect::new(top_left_pos, CGSize::new(COUNT_BOX_SIZE, COUNT_BOX_SIZE)));
            unsafe {
                text_layer.setString(Some(&NSString::from_str(&count_text)));
                text_layer.setAlignmentMode(kCAAlignmentCenter);
            }
            text_layer.setFontSize(16.0);
            text_layer.setForegroundColor(Some(&Color::new(1.0, 1.0, 1.0, 1.0).to_nscolor().CGColor()));
            text_layer.setBackgroundColor(Some(&color.to_nscolor().CGColor()));
            text_layer.setCornerRadius(COUNT_BOX_SIZE / 2.0);
            text_layer.setBorderColor(Some(&border_color.to_nscolor().CGColor()));
            text_layer.setBorderWidth(2.0);
            text_layer.setContentsScale(2.0);
            self.root_layer.addSublayer(&text_layer);

            // Other 3 corners: Show dots
            let dot_positions = [
                // Top-right  
                CGPoint::new(container_frame.size.width - CORNER_INSET - DOT_SIZE, container_frame.size.height - CORNER_INSET - DOT_SIZE),
                // Bottom-left
                CGPoint::new(CORNER_INSET, CORNER_INSET),
                // Bottom-right
                CGPoint::new(container_frame.size.width - CORNER_INSET - DOT_SIZE, CORNER_INSET),
            ];

            for pos in &dot_positions {
                let layer = CALayer::layer();
                layer.setFrame(CGRect::new(*pos, CGSize::new(DOT_SIZE, DOT_SIZE)));
                layer.setCornerRadius(DOT_SIZE / 2.0);
                layer.setBackgroundColor(Some(&color.to_nscolor().CGColor()));
                layer.setBorderColor(Some(&border_color.to_nscolor().CGColor()));
                layer.setBorderWidth(2.0);
                layer.setContentsScale(2.0);
                self.root_layer.addSublayer(&layer);
            }
        } else {
            // No child count (window): Show dots in all 4 corners
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

            for pos in &positions {
                let layer = CALayer::layer();
                layer.setFrame(CGRect::new(*pos, CGSize::new(DOT_SIZE, DOT_SIZE)));
                layer.setCornerRadius(DOT_SIZE / 2.0);
                layer.setBackgroundColor(Some(&color.to_nscolor().CGColor()));
                layer.setBorderColor(Some(&border_color.to_nscolor().CGColor()));
                layer.setBorderWidth(2.0);
                layer.setContentsScale(2.0);
                self.root_layer.addSublayer(&layer);
            }
        }

        CATransaction::commit();

        // Force the window to update by briefly ordering it out and back in
        let _ = self.cgs_window.order_out();
        
        // Present the window with new content
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
