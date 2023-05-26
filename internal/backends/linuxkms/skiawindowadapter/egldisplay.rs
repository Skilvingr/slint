// Copyright © SixtyFPS GmbH <info@slint-ui.com>
// SPDX-License-Identifier: GPL-3.0-only OR LicenseRef-Slint-commercial

use std::cell::{Cell, RefCell};
use std::fs::File;
use std::os::fd::{AsRawFd, RawFd};
use std::sync::Arc;

use i_slint_core::api::PhysicalSize as PhysicalWindowSize;
use i_slint_core::platform::PlatformError;
use i_slint_renderer_skia::opengl_surface::OpenGLContextWrapper;
use i_slint_renderer_skia::SkiaRenderer;
use smithay::backend::allocator::Format;
use smithay::backend::drm::{DrmDevice, GbmBufferedSurface};
use smithay::backend::renderer::Bind;
use smithay::reexports::drm::control::Device;

#[derive(Clone)]
pub struct SharedFd(Arc<File>);
impl AsRawFd for SharedFd {
    fn as_raw_fd(&self) -> RawFd {
        self.0.as_raw_fd()
    }
}

struct GbmOpenGLContextWrapper {
    drm_device: DrmDevice<SharedFd>,
    //egl_context: smithay::backend::egl::EGLContext,
    buffered_surface: RefCell<GbmBufferedSurface<SharedFd>>,
    // This is a hack to avoid having to do the binding of the dmabuf to the current framebuffer
    // manually, which we should do eventually though.
    renderer: RefCell<smithay::backend::renderer::gles2::Gles2Renderer>,
    rendering: Cell<bool>,
    //pixel_format: smithay::backend::egl::display::PixelFormat,
}

unsafe impl i_slint_renderer_skia::opengl_surface::OpenGLContextWrapper
    for GbmOpenGLContextWrapper
{
    fn ensure_current(&self) -> Result<(), PlatformError> {
        if self.rendering.replace(true) {
            return Ok(());
        }

        let mut gbm_surface = self.buffered_surface.borrow_mut();

        gbm_surface
            .frame_submitted()
            .map_err(|e| format!("Error marking frame as submitted {e}"))?;

        let mut renderer = self.renderer.borrow_mut();
        renderer
            .with_context(|_, _| {})
            .map_err(|e| format!("Error making EGL context current {e}"))?;

        let next_dmabuf = gbm_surface
            .next_buffer()
            .map_err(|e| format!("Error retrieving next buffer from swapchain: {e}"))?;

        renderer
            .bind(next_dmabuf)
            .map_err(|e| format!("Error binding dmabuf to framebuffer {e}"))?;

        Ok(())
    }

    fn swap_buffers(&self) -> Result<(), PlatformError> {
        if !self.rendering.replace(false) {
            return Ok(());
        }

        self.renderer
            .borrow_mut()
            .with_context(|_, gles| unsafe {
                gles.Flush();
                gles.Finish();
            })
            .map_err(|e| {
                format!("internal error making context current for flush and finish {e}")
            })?;

        self.buffered_surface
            .borrow_mut()
            .queue_buffer()
            .map_err(|e| format!("Error queuing {e}"))?;

        'out: loop {
            if let Ok(events) = self.drm_device.receive_events() {
                for event in events {
                    match event {
                        smithay::reexports::drm::control::Event::Vblank(_) => continue,
                        smithay::reexports::drm::control::Event::PageFlip(_) => break 'out,
                        smithay::reexports::drm::control::Event::Unknown(_) => continue,
                    }
                }
            }
        }

        Ok(())
    }

    fn resize(&self, size: PhysicalWindowSize) -> Result<(), PlatformError> {
        todo!()
    }

    fn get_proc_address(&self, name: &std::ffi::CStr) -> *const std::ffi::c_void {
        unsafe { smithay::backend::egl::get_proc_address(name.to_str().unwrap()) }
    }

    fn num_samples(&self) -> u8 {
        1
        //self.pixel_format.multisampling.map(|n| n as u8).unwrap_or(1)
    }

    fn stencil_size(&self) -> u8 {
        8
        //self.pixel_format.stencil_bits
    }

    fn bits_per_pixel(&self) -> Result<u8, PlatformError> {
        Ok(32)
        //Ok(self.pixel_format.alpha_bits + self.pixel_format.color_bits)
    }
}

pub fn create_skia_renderer_with_egl() -> Result<(SkiaRenderer, PhysicalWindowSize), PlatformError>
{
    let dri_fd = SharedFd(Arc::new(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/dri/card0")
            .map_err(|err| format!("Error opening /dev/dri/card0: {}", err))?,
    ));

    let drm_device = smithay::backend::drm::DrmDevice::new(dri_fd.clone(), false, None)
        .map_err(|e| format!("Error opening DRM device: {}", e))?;

    let resources = drm_device
        .resource_handles()
        .map_err(|e| format!("Error reading DRM resource handles: {e}"))?;

    let first_connector_handle =
        resources.connectors().first().ok_or_else(|| format!("Could not locate any connectors"))?;

    let connector = drm_device
        .get_connector(*first_connector_handle)
        .map_err(|e| format!("Could not retrieve connector details: {e}"))?;

    let encoder_handle =
        connector.encoders().first().ok_or_else(|| format!("No encoder found"))?.unwrap();
    let encoder = drm_device
        .get_encoder(encoder_handle)
        .map_err(|e| format!("Error retrieving encoder info: {e}"))?;

    let crtc = encoder.crtc().ok_or_else(|| format!("no crtc for encoder"))?;

    let mode = connector.modes().first().ok_or_else(|| format!("no modes found"))?;

    let (width, height) = mode.size();

    eprintln!("mode {}/{}", width, height);

    let drm_surface = drm_device
        .create_surface(crtc, *mode, &[connector.handle()])
        .map_err(|e| format!("Error creating drm surface {e}"))?;

    let gbm_device = smithay::backend::allocator::gbm::GbmDevice::new(dri_fd)
        .map_err(|e| format!("Error creating gbm allocator: {e}"))?;

    let buffered_surface = GbmBufferedSurface::new(
        drm_surface,
        gbm_device.clone(),
        [Format {
            code: smithay::backend::allocator::Fourcc::Argb8888,
            modifier: smithay::backend::allocator::Modifier::Invalid,
        }]
        .into_iter()
        .collect(),
        None,
    )
    .map_err(|e| format!("Error creating buffered gbm surface: {e}"))?;

    let egl_display = smithay::backend::egl::EGLDisplay::new(&gbm_device, None)
        .map_err(|e| format!("Error creating EGL display: {e}"))?;

    let egl_context = smithay::backend::egl::EGLContext::new(&egl_display, None)
        .map_err(|e| format!("Error creating EGL context: {e}"))?;

    /*
    let pixel_format = egl_context
        .pixel_format()
        .ok_or_else(|| format!("Error reading egl pixel format config"))?;
    */

    let size = PhysicalWindowSize::new(width as _, height as _);

    let renderer = unsafe {
        smithay::backend::renderer::gles2::Gles2Renderer::new(egl_context, None)
            .map_err(|e| format!("error creating GLES2 renderer hack {e}"))?
    };

    let context_wrapper = GbmOpenGLContextWrapper {
        drm_device,
        buffered_surface: RefCell::new(buffered_surface),
        renderer: RefCell::new(renderer),
        rendering: Cell::new(false),
        //pixel_format,
    };

    context_wrapper.ensure_current()?;

    let surface = i_slint_renderer_skia::opengl_surface::OpenGLSurface::new_with_context(
        context_wrapper,
        size,
    )?;

    Ok((i_slint_renderer_skia::SkiaRenderer::new_with_surface(surface), size))
}
