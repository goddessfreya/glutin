#![cfg(any(
    target_os = "windows",
    target_os = "linux",
    target_os = "android",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
))]

mod egl;
pub mod ffi;
mod make_current_guard;

pub use self::egl::Egl;
use self::make_current_guard::MakeCurrentGuard;

use crate::config::{
    Api, ConfigAttribs, ConfigBuilder, ConfigWrapper, ReleaseBehavior, SwapInterval,
    SwapIntervalRange, Version,
};
use crate::context::{ContextBuilderWrapper, Robustness};
use crate::surface::{PBuffer, Pixmap, SurfaceType, SurfaceTypeTrait, Window};

use glutin_interface::inputs::{NativeDisplay, RawDisplay};
use parking_lot::Mutex;
#[cfg(any(
    target_os = "android",
    target_os = "windows",
    target_os = "linux",
    target_os = "dragonfly",
    target_os = "freebsd",
    target_os = "netbsd",
    target_os = "openbsd",
))]
use winit_types::dpi;
use winit_types::error::{Error, ErrorType};
use winit_types::platform::OsError;

use std::convert::TryInto;
use std::ffi::{CStr, CString};
use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};
use std::os::raw;
use std::sync::Arc;

lazy_static! {
    pub static ref EGL: Result<Egl, Error> = Egl::new();
}

type EglVersion = (ffi::egl::types::EGLint, ffi::egl::types::EGLint);

#[derive(Debug)]
pub struct Display {
    display: ffi::egl::types::EGLDisplay,
    egl_version: EglVersion,
    extensions: Vec<String>,
    client_extensions: Vec<String>,
}

impl Display {
    fn has_extension(&self, e: &str) -> bool {
        self.extensions.iter().find(|s| s == &e).is_some()
    }

    fn get_native_display(
        client_extensions: &[String],
        ndisp: &RawDisplay,
    ) -> Result<*const raw::c_void, Error> {
        let egl = EGL.as_ref().unwrap();

        let has_client_extension = |e: &str| client_extensions.iter().find(|s| s == &e).is_some();

        let disp = match *ndisp {
            // Note: Some EGL implementations are missing the
            // `eglGetPlatformDisplay(EXT)` symbol despite reporting
            // `EGL_EXT_platform_base`. I'm pretty sure this is a bug. Therefore we
            // detect whether the symbol is loaded in addition to checking for
            // extensions.
            RawDisplay::Xlib {
                display, screen, ..
            } if has_client_extension("EGL_KHR_platform_x11")
                && egl.GetPlatformDisplay.is_loaded() =>
            {
                let attrib_list = screen.map(|screen| {
                    [
                        ffi::egl::PLATFORM_X11_SCREEN_KHR as ffi::egl::types::EGLAttrib,
                        screen as ffi::egl::types::EGLAttrib,
                        ffi::egl::NONE as ffi::egl::types::EGLAttrib,
                    ]
                });
                unsafe {
                    egl.GetPlatformDisplay(
                        ffi::egl::PLATFORM_X11_KHR,
                        display as *mut _,
                        attrib_list
                            .as_ref()
                            .map(|list| list.as_ptr())
                            .unwrap_or(std::ptr::null()),
                    )
                }
            }

            RawDisplay::Xlib {
                display, screen, ..
            } if has_client_extension("EGL_EXT_platform_x11")
                && egl.GetPlatformDisplayEXT.is_loaded() =>
            {
                let attrib_list = screen.map(|screen| {
                    [
                        ffi::egl::PLATFORM_X11_SCREEN_EXT as ffi::egl::types::EGLint,
                        screen as ffi::egl::types::EGLint,
                        ffi::egl::NONE as ffi::egl::types::EGLint,
                    ]
                });
                unsafe {
                    egl.GetPlatformDisplayEXT(
                        ffi::egl::PLATFORM_X11_EXT,
                        display as *mut _,
                        attrib_list
                            .as_ref()
                            .map(|list| list.as_ptr())
                            .unwrap_or(std::ptr::null()),
                    )
                }
            }

            RawDisplay::Gbm { gbm_device, .. }
                if has_client_extension("EGL_KHR_platform_gbm")
                    && egl.GetPlatformDisplay.is_loaded() =>
            unsafe {
                egl.GetPlatformDisplay(
                    ffi::egl::PLATFORM_GBM_KHR,
                    gbm_device as *mut _,
                    std::ptr::null(),
                )
            }

            RawDisplay::Gbm { gbm_device, .. }
                if has_client_extension("EGL_MESA_platform_gbm")
                    && egl.GetPlatformDisplayEXT.is_loaded() =>
            unsafe {
                egl.GetPlatformDisplayEXT(
                    ffi::egl::PLATFORM_GBM_KHR,
                    gbm_device as *mut _,
                    std::ptr::null(),
                )
            }

            RawDisplay::Wayland { wl_display, .. }
                if has_client_extension("EGL_KHR_platform_wayland")
                    && egl.GetPlatformDisplay.is_loaded() =>
            unsafe {
                egl.GetPlatformDisplay(
                    ffi::egl::PLATFORM_WAYLAND_KHR,
                    wl_display as *mut _,
                    std::ptr::null(),
                )
            }

            RawDisplay::Wayland { wl_display, .. }
                if has_client_extension("EGL_EXT_platform_wayland")
                    && egl.GetPlatformDisplayEXT.is_loaded() =>
            unsafe {
                egl.GetPlatformDisplayEXT(
                    ffi::egl::PLATFORM_WAYLAND_EXT,
                    wl_display as *mut _,
                    std::ptr::null(),
                )
            }

            // TODO: This will never be reached right now, as the android egl
            // bindings use the static generator, so can't rely on
            // GetPlatformDisplay(EXT).
            RawDisplay::Android { .. }
                if has_client_extension("EGL_KHR_platform_android")
                    && egl.GetPlatformDisplay.is_loaded() =>
            unsafe {
                egl.GetPlatformDisplay(
                    ffi::egl::PLATFORM_ANDROID_KHR,
                    ffi::egl::DEFAULT_DISPLAY as *mut _,
                    std::ptr::null(),
                )
            }

            RawDisplay::EGLExtDevice { egl_device_ext, .. }
                if has_client_extension("EGL_EXT_platform_device")
                    && egl.GetPlatformDisplay.is_loaded() =>
            unsafe {
                egl.GetPlatformDisplay(
                    ffi::egl::PLATFORM_DEVICE_EXT,
                    egl_device_ext as *mut _,
                    std::ptr::null(),
                )
            }

            RawDisplay::Xlib {
                display,
                screen: None,
                ..
            }
            | RawDisplay::Gbm {
                gbm_device: display,
                ..
            }
            | RawDisplay::Wayland {
                wl_display: display,
                ..
            }
            | RawDisplay::EGLExtDevice {
                egl_device_ext: display,
                ..
            }
            | RawDisplay::Windows {
                hwnd: Some(display),
                ..
            } => unsafe { egl.GetDisplay(display as *mut _) },

            RawDisplay::Android { .. } | RawDisplay::Windows { hwnd: None, .. } => unsafe {
                egl.GetDisplay(ffi::egl::DEFAULT_DISPLAY as *mut _)
            },

            _ => {
                return Err(make_error!(ErrorType::NotSupported(
                    "Display type unsupported by glutin.".to_string(),
                )));
            }
        };

        match disp {
            ffi::egl::NO_DISPLAY => Err(make_oserror!(OsError::Misc(format!(
                "Creating EGL display failed with 0x{:x}",
                unsafe { egl.GetError() },
            )))),
            disp => Ok(disp),
        }
    }

    fn get_egl_version(disp: ffi::egl::types::EGLDisplay) -> Result<EglVersion, Error> {
        unsafe {
            let egl = EGL.as_ref().unwrap();
            let mut major: ffi::egl::types::EGLint = 0;
            let mut minor: ffi::egl::types::EGLint = 0;

            if egl.Initialize(disp, &mut major, &mut minor) == ffi::egl::FALSE {
                return Err(make_oserror!(OsError::Misc(format!(
                    "eglInitialize failed with 0x{:x}",
                    egl.GetError()
                ))));
            }

            Ok((major, minor))
        }
    }

    pub fn new<ND: NativeDisplay>(nd: &ND) -> Result<Arc<Self>, Error> {
        let egl = EGL.as_ref().map_err(|err| err.clone())?;

        // the first step is to query the list of extensions without any display, if
        // supported
        let client_extensions = unsafe {
            let p = egl.QueryString(ffi::egl::NO_DISPLAY, ffi::egl::EXTENSIONS as i32);

            // this possibility is available only with EGL 1.5 or
            // EGL_EXT_platform_base, otherwise `eglQueryString` returns an
            // error
            if p.is_null() {
                vec![]
            } else {
                let p = CStr::from_ptr(p);
                let list = String::from_utf8(p.to_bytes().to_vec()).unwrap_or_else(|_| format!(""));
                list.split(' ').map(|e| e.to_string()).collect::<Vec<_>>()
            }
        };

        // calling `eglGetDisplay` or equivalent
        let disp = Self::get_native_display(&client_extensions, &nd.display())?;

        let egl_version = Self::get_egl_version(disp)?;

        // the list of extensions supported by the client once initialized is
        // different from the list of extensions obtained earlier
        let extensions = if egl_version >= (1, 2) {
            let p = unsafe { egl.QueryString(disp, ffi::egl::EXTENSIONS as i32) };
            if p.is_null() {
                return Err(make_oserror!(OsError::Misc(format!(
                    "Querying for EGL extensions failed with 0x{:x}",
                    unsafe { egl.GetError() },
                ))));
            }

            let p = unsafe { CStr::from_ptr(p) };
            let list = String::from_utf8(p.to_bytes().to_vec()).unwrap_or_else(|_| format!(""));
            list.split(' ').map(|e| e.to_string()).collect::<Vec<_>>()
        } else {
            vec![]
        };

        Ok(Arc::new(Display {
            display: disp,
            extensions,
            client_extensions,
            egl_version,
        }))
    }

    unsafe fn bind_api(api: Api, egl_version: EglVersion) -> Result<(), Error> {
        let egl = EGL.as_ref().unwrap();
        if egl_version >= (1, 2) {
            if match api {
                Api::OpenGl if egl_version >= (1, 4) => egl.BindAPI(ffi::egl::OPENGL_API),
                Api::OpenGl => ffi::egl::FALSE,
                Api::OpenGlEs if egl_version >= (1, 2) => egl.BindAPI(ffi::egl::OPENGL_ES_API),
                Api::OpenGlEs => ffi::egl::TRUE,
                _ => ffi::egl::FALSE,
            } == ffi::egl::FALSE
            {
                return Err(make_error!(ErrorType::OpenGlVersionNotSupported));
            }
        }

        Ok(())
    }
}

impl Deref for Display {
    type Target = ffi::egl::types::EGLDisplay;

    fn deref(&self) -> &Self::Target {
        &self.display
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        // In a reasonable world, we could uncomment the line bellow.
        //
        // This is no such world. Lets talk about something.
        //
        // You see, every call to `get_native_display` returns the exact
        // same display, just look at the docs:
        //
        // "Multiple calls made to eglGetDisplay with the same display_id
        // will return the same EGLDisplay handle."
        //
        // My EGL implementation does not do any ref counting, nor do the
        // EGL docs mention ref counting anywhere. In fact, the docs state
        // that there will be *no effect*, which, in a way, implies no ref
        // counting:
        //
        // "Initializing an already initialized EGL display connection has
        // no effect besides returning the version numbers."
        //
        // So, if we terminate the display, other people who are using it
        // won't be so happy.
        //
        // Well, how did I stumble on this issue you might ask...
        //
        // In this case, the "other people" was us, for it appears my EGL
        // implementation does not follow the docs, or maybe I'm misreading
        // them. You see, according to the egl docs:
        //
        // "To release the current context without assigning a new one, set
        // context to EGL_NO_CONTEXT and set draw and read to
        // EGL_NO_SURFACE.  [...] ******This is the only case where an
        // uninitialized display may be passed to eglMakeCurrent.******"
        // (Emphasis mine).
        //
        // Well, my computer returns EGL_BAD_DISPLAY if the display passed
        // to eglMakeCurrent is uninitialized, which allowed to me to spot
        // this issue.
        //
        // I would have assumed that if EGL was going to provide us with
        // the same EGLDisplay that they'd at least do some ref counting,
        // but they don't.
        //
        // FIXME: Technically we are leaking resources, not much we can do.
        // Yeah, we could have a global static that does ref counting
        // ourselves, but what if some other library is using the display.
        //
        // On unix operating systems, we could preload a little lib that
        // does ref counting on that level, but:
        //      A) What about other platforms?
        //      B) Do you *really* want all glutin programs to preload a
        //      library?
        //      C) Who the hell is going to maintain that?
        //
        // egl.Terminate(**self.display);
    }
}

#[derive(Debug)]
pub struct Context {
    display: Arc<Display>,
    context: ffi::egl::types::EGLContext,
    config: ConfigWrapper<Config, ConfigAttribs>,
}

#[derive(Debug, Clone)]
pub struct Config {
    display: Arc<Display>,
    config_id: ffi::egl::types::EGLConfig,
    version: (Api, Version),
}

impl Config {
    #[inline]
    pub fn new<F, NB: NativeDisplay>(
        cb: &ConfigBuilder,
        nb: &NB,
        mut conf_selector: F,
    ) -> Result<Vec<(ConfigAttribs, Config)>, Error>
    where
        F: FnMut(
            Vec<ffi::egl::types::EGLConfig>,
            &Arc<Display>,
        ) -> Vec<Result<ffi::egl::types::EGLConfig, Error>>,
    {
        let egl = EGL.as_ref().unwrap();
        let display = Display::new(nb)?;

        // TODO: Alternatively, allow EGL_MESA_platform_surfaceless.
        // FIXME: Also check for the GL_OES_surfaceless_context *CONTEXT*
        // extension
        if cb.surfaceless_support
            && display
                .extensions
                .iter()
                .find(|s| s == &"EGL_KHR_surfaceless_context")
                .is_none()
        {
            return Err(make_error!(ErrorType::NotSupported(
                "EGL surfaceless not supported".to_string(),
            )));
        }

        // binding the right API and choosing the version
        unsafe { Display::bind_api(cb.version.0, display.egl_version)? };
        let mut errors = make_error!(ErrorType::NoAvailableConfig);

        match cb.desired_swap_interval {
            Some(SwapInterval::AdaptiveWait(_)) => {
                errors.append(make_error!(ErrorType::AdaptiveSwapControlNotSupported));
                return Err(errors);
            }
            _ => (),
        }

        let descriptor = {
            let mut out: Vec<raw::c_int> = Vec::with_capacity(37);

            if display.egl_version >= (1, 2) {
                out.push(ffi::egl::COLOR_BUFFER_TYPE as raw::c_int);
                out.push(ffi::egl::RGB_BUFFER as raw::c_int);
            }

            out.push(ffi::egl::SURFACE_TYPE as raw::c_int);
            let mut surface_type = 0;
            if cb.window_surface_support {
                surface_type = surface_type | ffi::egl::WINDOW_BIT;
            }
            if cb.pbuffer_surface_support {
                surface_type = surface_type | ffi::egl::PBUFFER_BIT;
            }
            if cb.pixmap_surface_support {
                surface_type = surface_type | ffi::egl::PIXMAP_BIT;
            }
            out.push(surface_type as raw::c_int);

            match cb.version {
                (Api::OpenGlEs, Version(3, _)) => {
                    if display.egl_version < (1, 3) {
                        return Err(errors);
                    }
                    out.push(ffi::egl::RENDERABLE_TYPE as raw::c_int);
                    out.push(ffi::egl::OPENGL_ES3_BIT as raw::c_int);
                    out.push(ffi::egl::CONFORMANT as raw::c_int);
                    out.push(ffi::egl::OPENGL_ES3_BIT as raw::c_int);
                }
                (Api::OpenGlEs, Version(2, _)) => {
                    if display.egl_version < (1, 3) {
                        return Err(errors);
                    }
                    out.push(ffi::egl::RENDERABLE_TYPE as raw::c_int);
                    out.push(ffi::egl::OPENGL_ES2_BIT as raw::c_int);
                    out.push(ffi::egl::CONFORMANT as raw::c_int);
                    out.push(ffi::egl::OPENGL_ES2_BIT as raw::c_int);
                }
                (Api::OpenGlEs, Version(1, _)) => {
                    if display.egl_version >= (1, 3) {
                        out.push(ffi::egl::RENDERABLE_TYPE as raw::c_int);
                        out.push(ffi::egl::OPENGL_ES_BIT as raw::c_int);
                        out.push(ffi::egl::CONFORMANT as raw::c_int);
                        out.push(ffi::egl::OPENGL_ES_BIT as raw::c_int);
                    }
                }
                (Api::OpenGlEs, _) => unimplemented!(),
                (Api::OpenGl, _) => {
                    if display.egl_version < (1, 3) {
                        return Err(errors);
                    }
                    out.push(ffi::egl::RENDERABLE_TYPE as raw::c_int);
                    out.push(ffi::egl::OPENGL_BIT as raw::c_int);
                    out.push(ffi::egl::CONFORMANT as raw::c_int);
                    out.push(ffi::egl::OPENGL_BIT as raw::c_int);
                }
                (_, _) => unimplemented!(),
            };

            if let Some(hardware_accelerated) = cb.hardware_accelerated {
                out.push(ffi::egl::CONFIG_CAVEAT as raw::c_int);
                out.push(if hardware_accelerated {
                    ffi::egl::NONE as raw::c_int
                } else {
                    ffi::egl::SLOW_CONFIG as raw::c_int
                });
            }

            if let Some(color) = cb.color_bits {
                out.push(ffi::egl::RED_SIZE as raw::c_int);
                out.push((color / 3) as raw::c_int);
                out.push(ffi::egl::GREEN_SIZE as raw::c_int);
                out.push((color / 3 + if color % 3 != 0 { 1 } else { 0 }) as raw::c_int);
                out.push(ffi::egl::BLUE_SIZE as raw::c_int);
                out.push((color / 3 + if color % 3 == 2 { 1 } else { 0 }) as raw::c_int);
            }

            if let Some(alpha) = cb.alpha_bits {
                out.push(ffi::egl::ALPHA_SIZE as raw::c_int);
                out.push(alpha as raw::c_int);
            }

            if let Some(depth) = cb.depth_bits {
                out.push(ffi::egl::DEPTH_SIZE as raw::c_int);
                out.push(depth as raw::c_int);
            }

            if let Some(stencil) = cb.stencil_bits {
                out.push(ffi::egl::STENCIL_SIZE as raw::c_int);
                out.push(stencil as raw::c_int);
            }

            if let Some(true) = cb.double_buffer {
                return Err(errors);
            }

            if let Some(multisampling) = cb.multisampling {
                out.push(ffi::egl::SAMPLES as raw::c_int);
                out.push(multisampling as raw::c_int);
            }

            if cb.stereoscopy {
                return Err(errors);
            }

            if let Some(xid) = cb.plat_attr.x11_visual_xid {
                out.push(ffi::egl::NATIVE_VISUAL_ID as raw::c_int);
                out.push(xid as raw::c_int);
            }

            match cb.release_behavior {
                ReleaseBehavior::Flush => (),
                ReleaseBehavior::None => {
                    // FIXME: This isn't a client extension, right?
                    if !display.has_extension("EGL_KHR_context_flush_control") {
                        errors.append(make_error!(ErrorType::FlushControlNotSupported));
                        return Err(errors);
                    }
                }
            }

            if let Some(srgb) = cb.srgb {
                if srgb && !display.has_extension("EGL_KHR_gl_colorspace") {
                    return Err(errors);
                }
            }

            out.push(ffi::egl::NONE as raw::c_int);
            out
        };

        // calling `eglChooseConfig`
        let mut num_confs = 0;
        if unsafe {
            egl.ChooseConfig(
                **display,
                descriptor.as_ptr(),
                std::ptr::null_mut(),
                0,
                &mut num_confs,
            )
        } == 0
        {
            errors.append(make_oserror!(OsError::Misc(format!(
                "eglChooseConfig failed with 0x{:x}",
                unsafe { egl.GetError() },
            ))));
            return Err(errors);
        }

        if num_confs == 0 {
            return Err(errors);
        }

        let mut configs = Vec::with_capacity(num_confs as usize);
        configs.resize_with(num_confs as usize, || unsafe { std::mem::zeroed() });
        if unsafe {
            egl.ChooseConfig(
                **display,
                descriptor.as_ptr(),
                configs.as_mut_ptr(),
                num_confs,
                &mut num_confs,
            )
        } == 0
        {
            errors.append(make_oserror!(OsError::Misc(format!(
                "eglChooseConfig failed with 0x{:x}",
                unsafe { egl.GetError() },
            ))));
            return Err(errors);
        }

        // analyzing each config
        macro_rules! attrib {
            ($egl:expr, $display:expr, $conf:expr, $attr:expr $(,)?) => {{
                let mut value = 0;
                let res = unsafe {
                    $egl.GetConfigAttrib(
                        **$display,
                        $conf,
                        $attr as ffi::egl::types::EGLint,
                        &mut value,
                    )
                };
                match res {
                    0 => Err(make_oserror!(OsError::Misc(format!(
                        "eglGetConfigAttrib failed for {:?} with 0x{:x}",
                        $conf,
                        unsafe { egl.GetError() },
                    )))),
                    _ => Ok(value),
                }
            }};
        };

        let configs = if let Some(desired_swap_interval) = cb.desired_swap_interval {
            let desired_swap_interval = match desired_swap_interval {
                SwapInterval::Wait(n) => n,
                SwapInterval::DontWait => 0,
                SwapInterval::AdaptiveWait(_) => unreachable!(),
            };

            configs
                .into_iter()
                .filter_map(|conf_id| {
                    let min_swap_interval =
                        attrib!(egl, display, conf_id, ffi::egl::MIN_SWAP_INTERVAL);
                    match min_swap_interval {
                        Err(err) => {
                            errors.append(err);
                            return None;
                        }
                        Ok(min) if (desired_swap_interval as i32) < min => {
                            errors.append(make_oserror!(OsError::Misc(format!(
                                "Desired swap interval smaller than minimum for {:?}",
                                conf_id
                            ))));
                            return None;
                        }
                        _ => (),
                    }

                    let max_swap_interval =
                        attrib!(egl, display, conf_id, ffi::egl::MAX_SWAP_INTERVAL);
                    match max_swap_interval {
                        Err(err) => {
                            errors.append(err);
                            return None;
                        }
                        Ok(max) if desired_swap_interval as i32 > max => {
                            errors.append(make_oserror!(OsError::Misc(format!(
                                "Desired swap interval bigger than maximum for {:?}",
                                conf_id
                            ))));
                            return None;
                        }
                        _ => (),
                    }

                    Some(conf_id)
                })
                .collect()
        } else {
            configs
        };

        if configs.is_empty() {
            return Err(errors);
        }

        let mut configs: Vec<_> = conf_selector(configs, &display)
            .into_iter()
            .filter_map(|conf_id| match conf_id {
                Err(err) => {
                    errors.append(err);
                    return None;
                }
                Ok(conf_id) => Some(conf_id),
            })
            .map(|conf_id| {
                // Try into to panic if value is negative. Never trust the driver.
                let min_swap_interval: u32 =
                    attrib!(egl, display, conf_id, ffi::egl::MIN_SWAP_INTERVAL)?
                        .try_into()
                        .unwrap();
                let max_swap_interval: u32 =
                    attrib!(egl, display, conf_id, ffi::egl::MAX_SWAP_INTERVAL)?
                        .try_into()
                        .unwrap();

                let swap_interval_ranges = match (min_swap_interval, max_swap_interval) {
                    (0, 0) => vec![SwapIntervalRange::DontWait],
                    (0, _) => vec![
                        SwapIntervalRange::Wait(1..max_swap_interval),
                        SwapIntervalRange::DontWait,
                    ],
                    (_, _) => vec![SwapIntervalRange::Wait(
                        min_swap_interval..max_swap_interval,
                    )],
                };

                let attribs = ConfigAttribs {
                    version: cb.version,
                    window_surface_support: cb.window_surface_support,
                    pbuffer_surface_support: cb.pbuffer_surface_support,
                    pixmap_surface_support: cb.pixmap_surface_support,
                    surfaceless_support: cb.surfaceless_support,
                    hardware_accelerated: attrib!(egl, display, conf_id, ffi::egl::CONFIG_CAVEAT)?
                        != ffi::egl::SLOW_CONFIG as i32,
                    color_bits: attrib!(egl, display, conf_id, ffi::egl::RED_SIZE)? as u8
                        + attrib!(egl, display, conf_id, ffi::egl::BLUE_SIZE)? as u8
                        + attrib!(egl, display, conf_id, ffi::egl::GREEN_SIZE)? as u8,
                    alpha_bits: attrib!(egl, display, conf_id, ffi::egl::ALPHA_SIZE)? as u8,
                    depth_bits: attrib!(egl, display, conf_id, ffi::egl::DEPTH_SIZE)? as u8,
                    stencil_bits: attrib!(egl, display, conf_id, ffi::egl::STENCIL_SIZE)? as u8,
                    stereoscopy: false,
                    double_buffer: true,
                    multisampling: match attrib!(egl, display, conf_id, ffi::egl::SAMPLES)? {
                        0 | 1 => None,
                        a => Some(a as u16),
                    },
                    release_behavior: cb.release_behavior,
                    srgb: cb.srgb.unwrap_or(false),
                    desired_swap_interval: cb.desired_swap_interval.clone().unwrap_or(
                        match &swap_interval_ranges[0] {
                            SwapIntervalRange::DontWait => SwapInterval::DontWait,
                            SwapIntervalRange::Wait(n) => SwapInterval::Wait(n.start),
                            _ => unreachable!(),
                        },
                    ),
                    swap_interval_ranges,
                };

                Ok((attribs, conf_id))
            })
            // FIXME: Pleasing borrowck. Lokathor demands unrolling this loop.
            .collect::<Vec<_>>()
            .into_iter()
            .filter_map(|conf_id| match conf_id {
                Err(err) => {
                    errors.append(err);
                    return None;
                }
                Ok(conf_id) => Some(conf_id),
            })
            .collect();

        if configs.is_empty() {
            return Err(errors);
        }

        if cb.srgb.is_none() {
            let configs_len = configs.len();
            for i in 0..configs_len {
                let mut new_conf = configs[i].clone();
                new_conf.0.srgb = true;
                configs.push(new_conf);
            }
        }

        Ok(configs
            .into_iter()
            .map(|(attribs, config_id)| {
                (
                    attribs,
                    Config {
                        display: Arc::clone(&display),
                        version: cb.version,
                        config_id,
                    },
                )
            })
            .collect())
    }

    #[inline]
    pub fn get_native_visual_id(&self) -> Result<ffi::egl::types::EGLint, Error> {
        get_native_visual_id(**self.display, self.config_id)
    }
}

impl Context {
    /// Start building an EGL context.
    ///
    /// This function initializes some things and chooses the pixel format.
    ///
    /// To finish the process, you must call `.finish(window)` on the
    /// `ContextPrototype`.
    ///
    pub(crate) fn new(
        cb: ContextBuilderWrapper<&Context>,
        conf: ConfigWrapper<&Config, &ConfigAttribs>,
    ) -> Result<Context, Error> {
        let display = Arc::clone(&conf.config.display);
        let egl = EGL.as_ref().unwrap();
        let (api, version) = conf.attribs.version;

        unsafe {
            // FIXME: Support mixing apis
            Display::bind_api(api, display.egl_version)?;
        }

        let share = match cb.sharing {
            Some(ctx) => ctx.context,
            None => std::ptr::null(),
        };

        let mut context_attributes = Vec::with_capacity(10);
        let mut flags = 0;

        if display.egl_version >= (1, 5)
            || display
                .extensions
                .iter()
                .find(|s| s == &"EGL_KHR_create_context")
                .is_some()
        {
            context_attributes.push(ffi::egl::CONTEXT_MAJOR_VERSION as raw::c_int);
            context_attributes.push(version.0 as raw::c_int);
            context_attributes.push(ffi::egl::CONTEXT_MINOR_VERSION as raw::c_int);
            context_attributes.push(version.1 as raw::c_int);

            // handling robustness
            let supports_robustness = display.egl_version >= (1, 5)
                || display
                    .extensions
                    .iter()
                    .find(|s| s == &"EGL_EXT_create_context_robustness")
                    .is_some();

            match cb.robustness {
                Robustness::NotRobust => (),

                Robustness::NoError => {
                    if display
                        .extensions
                        .iter()
                        .find(|s| s == &"EGL_KHR_create_context_no_error")
                        .is_some()
                    {
                        context_attributes
                            .push(ffi::egl::CONTEXT_OPENGL_NO_ERROR_KHR as raw::c_int);
                        context_attributes.push(1);
                    }
                }

                Robustness::RobustNoResetNotification => {
                    if supports_robustness {
                        context_attributes.push(
                            ffi::egl::CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY as raw::c_int,
                        );
                        context_attributes.push(ffi::egl::NO_RESET_NOTIFICATION as raw::c_int);
                        flags = flags | ffi::egl::CONTEXT_OPENGL_ROBUST_ACCESS as raw::c_int;
                    } else {
                        return Err(make_error!(ErrorType::RobustnessNotSupported));
                    }
                }

                Robustness::TryRobustNoResetNotification => {
                    if supports_robustness {
                        context_attributes.push(
                            ffi::egl::CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY as raw::c_int,
                        );
                        context_attributes.push(ffi::egl::NO_RESET_NOTIFICATION as raw::c_int);
                        flags = flags | ffi::egl::CONTEXT_OPENGL_ROBUST_ACCESS as raw::c_int;
                    }
                }

                Robustness::RobustLoseContextOnReset => {
                    if supports_robustness {
                        context_attributes.push(
                            ffi::egl::CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY as raw::c_int,
                        );
                        context_attributes.push(ffi::egl::LOSE_CONTEXT_ON_RESET as raw::c_int);
                        flags = flags | ffi::egl::CONTEXT_OPENGL_ROBUST_ACCESS as raw::c_int;
                    } else {
                        return Err(make_error!(ErrorType::RobustnessNotSupported));
                    }
                }

                Robustness::TryRobustLoseContextOnReset => {
                    if supports_robustness {
                        context_attributes.push(
                            ffi::egl::CONTEXT_OPENGL_RESET_NOTIFICATION_STRATEGY as raw::c_int,
                        );
                        context_attributes.push(ffi::egl::LOSE_CONTEXT_ON_RESET as raw::c_int);
                        flags = flags | ffi::egl::CONTEXT_OPENGL_ROBUST_ACCESS as raw::c_int;
                    }
                }
            }

            if cb.debug {
                if display.egl_version >= (1, 5) {
                    context_attributes.push(ffi::egl::CONTEXT_OPENGL_DEBUG as raw::c_int);
                    context_attributes.push(ffi::egl::TRUE as raw::c_int);
                }

                // TODO: using this flag sometimes generates an error there was a
                // change in the specs that added this flag, so it may not be
                // supported everywhere; however it is not possible to know whether
                // it is supported or not
                //
                // flags = flags | ffi::egl::CONTEXT_OPENGL_DEBUG_BIT_KHR as raw::c_int;
            }

            // In at least some configurations, the Android emulator’s GL
            // implementation advertises support for the
            // EGL_KHR_create_context extension but returns BAD_ATTRIBUTE
            // when CONTEXT_FLAGS_KHR is used.
            if flags != 0 {
                context_attributes.push(ffi::egl::CONTEXT_FLAGS_KHR as raw::c_int);
                context_attributes.push(flags);
            }
        } else if display.egl_version >= (1, 3) && api == Api::OpenGlEs {
            // robustness is not supported
            match cb.robustness {
                Robustness::RobustNoResetNotification | Robustness::RobustLoseContextOnReset => {
                    return Err(make_error!(ErrorType::RobustnessNotSupported));
                }
                _ => (),
            }

            context_attributes.push(ffi::egl::CONTEXT_CLIENT_VERSION as raw::c_int);
            context_attributes.push(version.0 as raw::c_int);
        }

        match conf.attribs.release_behavior {
            ReleaseBehavior::Flush => {
                // FIXME: This isn't a client extension, right?
                if display.has_extension("EGL_KHR_context_flush_control") {
                    // With how shitty drivers are, never hurts to be explicit
                    context_attributes.push(ffi::egl::CONTEXT_RELEASE_BEHAVIOR_KHR as raw::c_int);
                    context_attributes
                        .push(ffi::egl::CONTEXT_RELEASE_BEHAVIOR_FLUSH_KHR as raw::c_int);
                }
            }
            ReleaseBehavior::None => {
                context_attributes.push(ffi::egl::CONTEXT_RELEASE_BEHAVIOR_KHR as raw::c_int);
                context_attributes.push(ffi::egl::CONTEXT_RELEASE_BEHAVIOR_NONE_KHR as raw::c_int);
            }
        }

        context_attributes.push(ffi::egl::NONE as raw::c_int);

        let context = unsafe {
            egl.CreateContext(
                **display,
                conf.config.config_id,
                share,
                context_attributes.as_ptr(),
            )
        };

        if context.is_null() {
            match unsafe { egl.GetError() } as u32 {
                ffi::egl::BAD_MATCH | ffi::egl::BAD_ATTRIBUTE => {
                    return Err(make_error!(ErrorType::OpenGlVersionNotSupported));
                }
                err => {
                    return Err(make_oserror!(OsError::Misc(format!(
                        "eglCreateContext failed with 0x{:x}",
                        err,
                    ))));
                }
            }
        }

        Ok(Context {
            display,
            context,
            config: conf.clone_inner(),
        })
    }

    pub(crate) unsafe fn make_current<T: SurfaceTypeTrait>(
        &self,
        surf: &Surface<T>,
    ) -> Result<(), Error> {
        let egl = EGL.as_ref().unwrap();

        let ret = egl.MakeCurrent(**self.display, surf.surface, surf.surface, self.context);
        Self::check_make_current(Some(ret))?;

        // Swap interval defaults to 1
        let mut has_been_current = surf.has_been_current.lock();
        if !*has_been_current && T::surface_type() == SurfaceType::Window {
            let n = match surf.config.attribs.desired_swap_interval {
                SwapInterval::Wait(n) => n,
                SwapInterval::DontWait => 0,
                SwapInterval::AdaptiveWait(_) => unreachable!(),
            };
            if egl.SwapInterval(**surf.display, n as i32) == ffi::egl::FALSE {
                return Err(make_oserror!(OsError::Misc(format!(
                    "eglSwapInterval failed with 0x{:x}",
                    egl.GetError()
                ))));
            }
            *has_been_current = true;
        }

        Ok(())
    }

    #[inline]
    pub unsafe fn make_current_surfaceless(&self) -> Result<(), Error> {
        let egl = EGL.as_ref().unwrap();
        let ret = egl.MakeCurrent(
            **self.display,
            ffi::egl::NO_SURFACE,
            ffi::egl::NO_SURFACE,
            self.context,
        );

        Self::check_make_current(Some(ret))
    }

    #[inline]
    pub unsafe fn make_not_current(&self) -> Result<(), Error> {
        let egl = EGL.as_ref().unwrap();

        if egl.GetCurrentContext() == self.context {
            let ret = egl.MakeCurrent(
                **self.display,
                ffi::egl::NO_SURFACE,
                ffi::egl::NO_SURFACE,
                ffi::egl::NO_CONTEXT,
            );

            Self::check_make_current(Some(ret))
        } else {
            Self::check_make_current(None)
        }
    }

    #[inline]
    pub fn is_current(&self) -> bool {
        let egl = EGL.as_ref().unwrap();
        unsafe { egl.GetCurrentContext() == self.context }
    }

    #[inline]
    pub fn get_config(&self) -> ConfigWrapper<Config, ConfigAttribs> {
        self.config.clone()
    }

    #[inline]
    pub fn get_api(&self) -> Api {
        self.config.config.version.0
    }

    // FIXME: Needed for android support.
    // winit doesn't have it, I'll add this back in when it does.
    //
    // // Handle Android Life Cycle.
    // // Android has started the activity or sent it to foreground.
    // // Create a new surface and attach it to the recreated ANativeWindow.
    // // Restore the EGLContext.
    // #[cfg(target_os = "android")]
    // pub unsafe fn on_surface_created(&self, nwin: ffi::EGLNativeWindowType) {
    //     let egl = EGL.as_ref().unwrap();
    //     let mut surface = self.surface.as_ref().unwrap().lock();
    //     if *surface != ffi::egl::NO_SURFACE {
    //         return;
    //     }
    //     *surface = egl.CreateWindowSurface(
    //         **self.display,
    //         self.config_id,
    //         nwin,
    //         std::ptr::null(),
    //     );
    //     if surface.is_null() {
    //         panic!(
    //             "[glutin] on_surface_created: eglCreateWindowSurface failed with
    // 0x{:x}",             egl.GetError()
    //         )
    //     }
    //     let ret =
    //         egl.MakeCurrent(**self.display, *surface, *surface,
    // self.context);     if ret == 0 {
    //         panic!(
    //             "[glutin] on_surface_created: eglMakeCurrent failed with 0x{:x}",
    //             egl.GetError()
    //         )
    //     }
    // }
    //
    // // Handle Android Life Cycle.
    // // Android has stopped the activity or sent it to background.
    // // Release the surface attached to the destroyed ANativeWindow.
    // // The EGLContext is not destroyed so it can be restored later.
    // #[cfg(target_os = "android")]
    // pub unsafe fn on_surface_destroyed(&self) {
    //     let egl = EGL.as_ref().unwrap();
    //     let mut surface = self.surface.as_ref().unwrap().lock();
    //     if *surface == ffi::egl::NO_SURFACE {
    //         return;
    //     }
    //     let ret = egl.MakeCurrent(
    //         **self.display,
    //         ffi::egl::NO_SURFACE,
    //         ffi::egl::NO_SURFACE,
    //         ffi::egl::NO_CONTEXT,
    //     );
    //     if ret == 0 {
    //         panic!(
    //             "[glutin] on_surface_destroyed: eglMakeCurrent failed with 0x{:x}",
    //             egl.GetError()
    //         )
    //     }
    //
    //     egl.DestroySurface(**self.display, *surface);
    //     *surface = ffi::egl::NO_SURFACE;
    // }

    #[inline]
    pub fn get_proc_address(&self, addr: &str) -> *const raw::c_void {
        let egl = EGL.as_ref().unwrap();
        let addr = CString::new(addr.as_bytes()).unwrap();
        let addr = addr.as_ptr();
        unsafe { egl.GetProcAddress(addr) as *const _ }
    }

    #[inline]
    pub fn get_native_visual_id(&self) -> Result<ffi::egl::types::EGLint, Error> {
        get_native_visual_id(**self.display, self.config.config.config_id)
    }

    unsafe fn check_make_current(ret: Option<u32>) -> Result<(), Error> {
        let egl = EGL.as_ref().unwrap();
        if ret == Some(0) {
            match egl.GetError() as u32 {
                ffi::egl::CONTEXT_LOST => Err(make_error!(ErrorType::ContextLost)),
                err => Err(make_oserror!(OsError::Misc(format!(
                    "eglMakeCurrent failed (eglGetError returned 0x{:x})",
                    err,
                )))),
            }
        } else {
            Ok(())
        }
    }
}

unsafe impl Send for Context {}
unsafe impl Sync for Context {}

unsafe impl<T: SurfaceTypeTrait> Send for Surface<T> {}
unsafe impl<T: SurfaceTypeTrait> Sync for Surface<T> {}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            let egl = EGL.as_ref().unwrap();

            let mut guard = MakeCurrentGuard::new_keep(**self.display);
            guard.if_any_same_then_invalidate(
                ffi::egl::NO_SURFACE,
                ffi::egl::NO_SURFACE,
                self.context,
            );
            std::mem::drop(guard);

            egl.DestroyContext(**self.display, self.context);
            self.context = ffi::egl::NO_CONTEXT;
        }
    }
}

#[inline]
pub fn get_native_visual_id(
    disp: ffi::egl::types::EGLDisplay,
    conf_id: ffi::egl::types::EGLConfig,
) -> Result<ffi::egl::types::EGLint, Error> {
    let egl = EGL.as_ref().unwrap();
    let mut value = 0;
    let ret = unsafe {
        egl.GetConfigAttrib(
            disp,
            conf_id,
            ffi::egl::NATIVE_VISUAL_ID as ffi::egl::types::EGLint,
            &mut value,
        )
    };
    if ret == 0 {
        return Err(make_oserror!(OsError::Misc(format!(
            "eglGetConfigAttrib failed with 0x{:x}",
            unsafe { egl.GetError() },
        ))));
    };
    Ok(value)
}

#[derive(Debug)]
pub struct Surface<T: SurfaceTypeTrait> {
    display: Arc<Display>,
    surface: ffi::egl::types::EGLSurface,
    config: ConfigWrapper<Config, ConfigAttribs>,
    phantom: PhantomData<T>,
    has_been_current: Mutex<bool>,
}

impl<T: SurfaceTypeTrait> Surface<T> {
    #[inline]
    fn assemble_desc(
        conf: ConfigWrapper<&Config, &ConfigAttribs>,
        size: Option<dpi::PhysicalSize>,
    ) -> Vec<raw::c_int> {
        let mut out = Vec::new();
        match conf.attribs.srgb {
            false => {
                if conf.config.display.has_extension("EGL_KHR_gl_colorspace") {
                    // With how shitty drivers are, never hurts to be explicit
                    out.push(ffi::egl::GL_COLORSPACE_KHR as raw::c_int);
                    out.push(ffi::egl::GL_COLORSPACE_LINEAR_KHR as raw::c_int);
                }
            }
            true => {
                out.push(ffi::egl::GL_COLORSPACE_KHR as raw::c_int);
                out.push(ffi::egl::GL_COLORSPACE_SRGB_KHR as raw::c_int);
            }
        }

        if let Some(size) = size {
            let size: (u32, u32) = size.into();
            out.push(ffi::egl::TEXTURE_FORMAT as raw::c_int);
            out.push(match size {
                (0, _) | (_, 0) => ffi::egl::NO_TEXTURE,
                _ if conf.attribs.alpha_bits > 0 => ffi::egl::TEXTURE_RGBA,
                _ => ffi::egl::TEXTURE_RGB,
            } as raw::c_int);

            out.push(ffi::egl::WIDTH as raw::c_int);
            out.push(size.0 as raw::c_int);
            out.push(ffi::egl::HEIGHT as raw::c_int);
            out.push(size.1 as raw::c_int);
        }

        out.push(ffi::egl::NONE as raw::c_int);
        out
    }

    #[inline]
    pub fn is_current(&self) -> bool {
        let egl = EGL.as_ref().unwrap();
        unsafe {
            egl.GetCurrentSurface(ffi::egl::DRAW as i32) == self.surface
                || egl.GetCurrentSurface(ffi::egl::READ as i32) == self.surface
        }
    }

    #[inline]
    pub fn get_config(&self) -> ConfigWrapper<Config, ConfigAttribs> {
        self.config.clone()
    }

    #[inline]
    pub unsafe fn make_not_current(&self) -> Result<(), Error> {
        let egl = EGL.as_ref().unwrap();

        if egl.GetCurrentSurface(ffi::egl::DRAW as i32) == self.surface
            || egl.GetCurrentSurface(ffi::egl::READ as i32) == self.surface
        {
            let ret = egl.MakeCurrent(
                **self.display,
                ffi::egl::NO_SURFACE,
                ffi::egl::NO_SURFACE,
                ffi::egl::NO_CONTEXT,
            );

            Context::check_make_current(Some(ret))
        } else {
            Context::check_make_current(None)
        }
    }
}

impl Surface<Window> {
    #[inline]
    pub fn new(
        conf: ConfigWrapper<&Config, &ConfigAttribs>,
        nwin: ffi::EGLNativeWindowType,
    ) -> Result<Self, Error> {
        let display = Arc::clone(&conf.config.display);
        let egl = EGL.as_ref().unwrap();
        let desc = Self::assemble_desc(conf.clone(), None);
        let surf = unsafe {
            let surf =
                egl.CreateWindowSurface(**display, conf.config.config_id, nwin, desc.as_ptr());
            if surf.is_null() {
                return Err(make_oserror!(OsError::Misc(format!(
                    "eglCreateWindowSurface failed with 0x{:x}",
                    egl.GetError()
                ))));
            }
            surf
        };

        Ok(Surface {
            display,
            config: conf.clone_inner(),
            surface: surf,
            phantom: PhantomData,
            has_been_current: Mutex::new(false),
        })
    }

    #[inline]
    pub fn swap_buffers(&self) -> Result<(), Error> {
        let egl = EGL.as_ref().unwrap();
        if self.surface == ffi::egl::NO_SURFACE {
            return Err(make_error!(ErrorType::ContextLost));
        }

        let ret = unsafe { egl.SwapBuffers(**self.display, self.surface) };

        if ret == 0 {
            match unsafe { egl.GetError() } as u32 {
                ffi::egl::CONTEXT_LOST => {
                    return Err(make_error!(ErrorType::ContextLost));
                }
                err => Err(make_oserror!(OsError::Misc(format!(
                    "eglSwapBuffers failed with 0x{:x}",
                    err,
                )))),
            }
        } else {
            Ok(())
        }
    }

    #[inline]
    pub fn swap_buffers_with_damage(&self, rects: &[dpi::Rect]) -> Result<(), Error> {
        let egl = EGL.as_ref().unwrap();

        if !egl.SwapBuffersWithDamageKHR.is_loaded() {
            return Err(make_error!(ErrorType::NotSupported(
                "buffer damage not suported".to_string(),
            )));
        }

        if self.surface == ffi::egl::NO_SURFACE {
            return Err(make_error!(ErrorType::ContextLost));
        }

        let mut ffirects: Vec<ffi::egl::types::EGLint> = Vec::with_capacity(rects.len() * 4);

        for rect in rects {
            ffirects.push(rect.x as ffi::egl::types::EGLint);
            ffirects.push(rect.y as ffi::egl::types::EGLint);
            ffirects.push(rect.width as ffi::egl::types::EGLint);
            ffirects.push(rect.height as ffi::egl::types::EGLint);
        }

        let ret = unsafe {
            egl.SwapBuffersWithDamageKHR(
                **self.display,
                self.surface,
                ffirects.as_mut_ptr(),
                rects.len() as ffi::egl::types::EGLint,
            )
        };

        if ret == ffi::egl::FALSE {
            match unsafe { egl.GetError() } as u32 {
                ffi::egl::CONTEXT_LOST => {
                    return Err(make_error!(ErrorType::ContextLost));
                }
                err => Err(make_oserror!(OsError::Misc(format!(
                    "eglSwapBuffersWithDamageKHR failed with 0x{:x}",
                    err,
                )))),
            }
        } else {
            Ok(())
        }
    }
}

impl Surface<PBuffer> {
    #[inline]
    pub fn new(
        conf: ConfigWrapper<&Config, &ConfigAttribs>,
        size: dpi::PhysicalSize,
    ) -> Result<Self, Error> {
        let display = Arc::clone(&conf.config.display);
        let egl = EGL.as_ref().unwrap();

        let desc = Self::assemble_desc(conf.clone(), Some(size));
        let surf = unsafe {
            let pbuffer = egl.CreatePbufferSurface(**display, conf.config.config_id, desc.as_ptr());
            if pbuffer.is_null() || pbuffer == ffi::egl::NO_SURFACE {
                return Err(make_oserror!(OsError::Misc(format!(
                    "eglCreatePbufferSurface failed with 0x{:x}",
                    egl.GetError(),
                ))));
            }
            pbuffer
        };

        Ok(Surface {
            display,
            config: conf.clone_inner(),
            surface: surf,
            phantom: PhantomData,
            has_been_current: Mutex::new(false),
        })
    }
}

impl<T: SurfaceTypeTrait> Drop for Surface<T> {
    fn drop(&mut self) {
        unsafe {
            let egl = EGL.as_ref().unwrap();

            let mut guard = MakeCurrentGuard::new_keep(**self.display);
            guard.if_any_same_then_invalidate(self.surface, self.surface, ffi::egl::NO_CONTEXT);
            std::mem::drop(guard);

            egl.DestroySurface(**self.display, self.surface);
            self.surface = ffi::egl::NO_SURFACE;
        }
    }
}
