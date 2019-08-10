use super::*;

use glutin::event_loop::EventLoopWindowTarget;
use glutin::{Context, ContextSupports};

use std::ffi::c_void;
use std::marker::PhantomData;

#[allow(non_snake_case)]
pub mod SupportsPBuffers {
    use std::fmt::Debug;
    pub trait SupportsPBuffersTrait: Debug + Clone + Send + Sync {
        fn supported() -> bool;
    }

    #[derive(Debug, Clone, Copy)]
    pub enum Yes {}
    #[derive(Debug, Clone, Copy)]
    pub enum No {}

    impl SupportsPBuffersTrait for Yes {
        #[inline]
        fn supported() -> bool {
            true
        }
    }
    impl SupportsPBuffersTrait for No {
        #[inline]
        fn supported() -> bool {
            false
        }
    }
}
pub use SupportsPBuffers::SupportsPBuffersTrait;

#[allow(non_snake_case)]
pub mod SupportsWindowSurfaces {
    use std::fmt::Debug;
    pub trait SupportsWindowSurfacesTrait: Debug + Clone + Send + Sync {
        fn supported() -> bool;
    }

    #[derive(Debug, Clone, Copy)]
    pub enum Yes {}
    #[derive(Debug, Clone, Copy)]
    pub enum No {}

    impl SupportsWindowSurfacesTrait for Yes {
        #[inline]
        fn supported() -> bool {
            true
        }
    }
    impl SupportsWindowSurfacesTrait for No {
        #[inline]
        fn supported() -> bool {
            false
        }
    }
}
pub use SupportsWindowSurfaces::SupportsWindowSurfacesTrait;

#[allow(non_snake_case)]
pub mod SupportsSurfaceless {
    use std::fmt::Debug;
    pub trait SupportsSurfacelessTrait: Debug + Clone + Send + Sync {
        fn supported() -> bool;
    }

    #[derive(Debug, Clone, Copy)]
    pub enum Yes {}
    #[derive(Debug, Clone, Copy)]
    pub enum No {}

    impl SupportsSurfacelessTrait for Yes {
        #[inline]
        fn supported() -> bool {
            true
        }
    }
    impl SupportsSurfacelessTrait for No {
        #[inline]
        fn supported() -> bool {
            false
        }
    }
}
pub use SupportsSurfaceless::SupportsSurfacelessTrait;

#[allow(non_snake_case)]
pub mod ContextIsCurrent {
    use std::fmt::Debug;
    use std::marker::PhantomData;

    /// A trait implemented on both [`NotCurrent`] and
    /// [`PossiblyCurrent`].
    ///
    /// [`NotCurrent`]: enum.NotCurrent.html
    /// [`PossiblyCurrent`]: struct.PossiblyCurrent.html
    ///
    /// FIXME: Docs outdated
    pub trait ContextIsCurrentTrait: Debug + Clone + Copy {}
    pub trait ContextIsCurrentYesTrait:
        ContextIsCurrentTrait + Debug + Clone + Copy
    {
    }

    // This is nightly only:
    // impl !Send for Context<PossiblyCurrent> {}
    // impl !Sync for Context<PossiblyCurrent> {}
    //
    // Instead we add a phantom type
    /// A type that [`Context`]s which might possibly be currently current on
    /// some thread take as a generic.
    ///
    /// See [`Context::make_current_surface`] for more details.
    ///
    /// [`Context::make_current_surface`]:
    /// struct.Context.html#method.make_current_surface
    /// [`Context`]: struct.Context.html
    #[derive(Debug, Clone, Copy)]
    pub struct Possibly {
        phantom: PhantomData<*mut ()>,
    }
    #[derive(Debug, Clone, Copy)]
    pub struct PossiblyAndSurfaceBound {
        phantom: PhantomData<*mut ()>,
    }
    /// A type that [`Context`]s which are not currently current on any thread
    /// take as a generic.
    ///
    /// See [`Context::make_current_surface`] for more details.
    ///
    /// [`Context::make_current_surface`]:
    /// struct.Context.html#method.make_current_surface
    /// [`Context`]: struct.Context.html
    #[derive(Debug, Clone, Copy)]
    pub enum No {}

    impl ContextIsCurrentTrait for Possibly {}
    impl ContextIsCurrentTrait for PossiblyAndSurfaceBound {}
    impl ContextIsCurrentTrait for No {}

    impl ContextIsCurrentYesTrait for Possibly {}
    impl ContextIsCurrentYesTrait for PossiblyAndSurfaceBound {}
}
pub use ContextIsCurrent::{ContextIsCurrentTrait, ContextIsCurrentYesTrait};

#[derive(Debug)]
pub struct SplitContext<
    IC: ContextIsCurrentTrait,
    PBT: SupportsPBuffersTrait,
    WST: SupportsWindowSurfacesTrait,
    ST: SupportsSurfacelessTrait,
> {
    pub(crate) context: Context,
    pub(crate) phantom: PhantomData<(IC, PBT, WST, ST)>,
}

impl<
        IC: ContextIsCurrentTrait,
        PBT: SupportsPBuffersTrait,
        WST: SupportsWindowSurfacesTrait,
        ST: SupportsSurfacelessTrait,
    > SplitContext<IC, PBT, WST, ST>
{
    #[inline]
    pub(crate) fn inner(&self) -> &Context {
        &self.context
    }
}

impl<
        IC: ContextIsCurrentTrait,
        PBT: SupportsPBuffersTrait,
        ST: SupportsSurfacelessTrait,
    > SplitContext<IC, PBT, SupportsWindowSurfaces::Yes, ST>
{
    /// Sets this context as the current context. The previously current context
    /// (if any) is no longer current.
    ///
    /// A failed call to `make_current_*` might make this, or no context
    /// current. It could also keep the previous context current. What happens
    /// varies by platform and error.
    ///
    /// To attempt to recover and get back into a know state, either:
    ///
    ///  * attempt to use [`is_current`] to find the new current context; or
    ///  * call [`make_not_current`] on both the previously
    ///  current context and this context.
    ///
    /// # An higher level overview.
    ///
    /// In OpenGl, only a single context can be current in a thread at a time.
    /// Making a new context current will make the old one not current.
    /// Contexts can only be sent to different threads if they are not current.
    ///
    /// If you call `make_current_*` on some context, you should call
    /// [`treat_as_not_current`] as soon as possible on the previously current
    /// context.
    ///
    /// If you wish to move a currently current context to a different thread,
    /// you should do one of two options:
    ///
    ///  * Call `make_current_*` on an other context, then call
    ///  [`treat_as_not_current`] on this context.
    ///  * Call [`make_not_current`] on this context.
    ///
    /// If you are aware of what context you intend to make current next, it is
    /// preferable for performance reasons to call `make_current_*` on that
    /// context, then [`treat_as_not_current`] on this context.
    ///
    /// If you are not aware of what context you intend to make current next,
    /// consider waiting until you do. If you need this context not current
    /// immediately (e.g. to transfer it to an other thread), then call
    /// [`make_not_current`] on this context.
    ///
    /// Please avoid calling [`make_not_current`] on one context only to call
    /// `make_current_*` on an other context before and/or after. This hurts
    /// performance by requiring glutin to:
    ///
    ///  * Check if this context is current; then
    ///  * If it is, change the current context from this context to none; then
    ///  * Change the current context from none to the new context.
    ///
    /// Instead prefer the method we mentioned above with `make_current_*` and
    /// [`treat_as_not_current`].
    ///
    /// [`make_not_current`]: struct.Context.html#method.make_not_current
    /// [`treat_as_not_current`]:
    /// struct.Context.html#method.treat_as_not_current
    /// [`is_current`]: struct.Context.html#method.is_current
    #[inline]
    pub unsafe fn make_current_surface<W, IU: SurfaceInUseTrait>(
        self,
        surface: LighterWindowSurfaceWrapper<W, IU>,
    ) -> Result<
        (
            SplitContext<
                ContextIsCurrent::PossiblyAndSurfaceBound,
                PBT,
                SupportsWindowSurfaces::Yes,
                ST,
            >,
            LighterWindowSurfaceWrapper<W, SurfaceInUse::Possibly>,
        ),
        (
            SplitContext<
                ContextIsCurrent::PossiblyAndSurfaceBound,
                PBT,
                SupportsWindowSurfaces::Yes,
                ST,
            >,
            LighterWindowSurfaceWrapper<W, SurfaceInUse::Possibly>,
            ContextError,
        ),
    > {
        match self.context.make_current_surface(surface.inner()) {
            Ok(()) => Ok((
                self.treat_as_current(),
                LighterSurface::treat_as_current(surface),
            )),
            Err(err) => Err((
                self.treat_as_current(),
                LighterSurface::treat_as_current(surface),
                err,
            )),
        }
    }

    #[inline]
    pub fn unify_with_window<W, IU: SurfaceInUseTrait>(
        self,
        surface: LighterWindowSurfaceWrapper<W, IU>,
    ) -> UnifiedContext<
        IC,
        PBT,
        SupportsWindowSurfaces::Yes,
        ST,
        LighterWindowSurfaceWrapper<W, IU>,
    > {
        UnifiedContext {
            context: self,
            surface,
        }
    }
}

impl<
        IC: ContextIsCurrentTrait,
        WST: SupportsWindowSurfacesTrait,
        ST: SupportsSurfacelessTrait,
    > SplitContext<IC, SupportsPBuffers::Yes, WST, ST>
{
    #[inline]
    pub unsafe fn make_current_pbuffer<IU: SurfaceInUseTrait>(
        self,
        pbuffer: LighterPBuffer<IU>,
    ) -> Result<
        (
            SplitContext<
                ContextIsCurrent::Possibly,
                SupportsPBuffers::Yes,
                WST,
                ST,
            >,
            LighterPBuffer<SurfaceInUse::Possibly>,
        ),
        (
            SplitContext<
                ContextIsCurrent::PossiblyAndSurfaceBound,
                SupportsPBuffers::Yes,
                WST,
                ST,
            >,
            LighterPBuffer<SurfaceInUse::Possibly>,
            ContextError,
        ),
    > {
        match self.context.make_current_pbuffer(pbuffer.inner()) {
            Ok(()) => Ok((
                self.treat_as_current(),
                LighterSurface::treat_as_current(pbuffer),
            )),
            Err(err) => Err((
                self.treat_as_current(),
                LighterSurface::treat_as_current(pbuffer),
                err,
            )),
        }
    }

    #[inline]
    pub fn unify_with_pbuffer<IU: SurfaceInUseTrait>(
        self,
        pbuffer: LighterPBuffer<IU>,
    ) -> UnifiedContext<IC, SupportsPBuffers::Yes, WST, ST, LighterPBuffer<IU>>
    {
        UnifiedContext {
            context: self,
            surface: pbuffer,
        }
    }
}

impl<
        IC: ContextIsCurrentTrait,
        PBT: SupportsPBuffersTrait,
        WST: SupportsWindowSurfacesTrait,
    > SplitContext<IC, PBT, WST, SupportsSurfaceless::Yes>
{
    #[inline]
    pub unsafe fn make_current_surfaceless(
        self,
    ) -> Result<
        SplitContext<
            ContextIsCurrent::Possibly,
            PBT,
            WST,
            SupportsSurfaceless::Yes,
        >,
        (
            SplitContext<
                ContextIsCurrent::PossiblyAndSurfaceBound,
                PBT,
                WST,
                SupportsSurfaceless::Yes,
            >,
            ContextError,
        ),
    > {
        match self.context.make_current_surfaceless() {
            Ok(()) => Ok(self.treat_as_current()),
            Err(err) => Err((self.treat_as_current(), err)),
        }
    }

    #[inline]
    pub fn unify<IU: SurfaceInUseTrait>(
        self,
    ) -> UnifiedContext<IC, PBT, WST, SupportsSurfaceless::Yes, ()> {
        UnifiedContext {
            context: self,
            surface: (),
        }
    }
}

impl<
        IC: ContextIsCurrentYesTrait,
        PBT: SupportsPBuffersTrait,
        WST: SupportsWindowSurfacesTrait,
        ST: SupportsSurfacelessTrait,
    > SplitContext<IC, PBT, WST, ST>
{
    /// Returns the address of an OpenGL function.
    #[inline]
    pub fn get_proc_address(&self, addr: &str) -> *const c_void {
        self.context.get_proc_address(addr)
    }
}

impl<
        IC: ContextIsCurrentTrait,
        PBT: SupportsPBuffersTrait,
        WST: SupportsWindowSurfacesTrait,
        ST: SupportsSurfacelessTrait,
    > SplitContext<IC, PBT, WST, ST>
{
    /// Returns true if this context is the current one in this thread.
    #[inline]
    pub fn is_current(&self) -> bool {
        self.context.is_current()
    }

    #[inline]
    pub fn get_pixel_format(&self) -> PixelFormat {
        self.context.get_pixel_format()
    }

    /// Returns the OpenGL API being used.
    #[inline]
    pub fn get_api(&self) -> Api {
        self.context.get_api()
    }

    /// If this context is current, makes this context not current. If this
    /// context is not current however, this function does nothing.
    ///
    /// Please see [`make_current_surface`].
    ///
    /// [`make_current_surface`]:
    /// struct.Context.html#method.make_current_surface
    #[inline]
    pub unsafe fn make_not_current(
        self,
    ) -> Result<
        SplitContext<ContextIsCurrent::No, PBT, WST, ST>,
        (
            SplitContext<
                ContextIsCurrent::PossiblyAndSurfaceBound,
                PBT,
                WST,
                ST,
            >,
            ContextError,
        ),
    > {
        match self.context.make_not_current() {
            Ok(()) => Ok(SplitContext {
                context: self.context,
                phantom: PhantomData,
            }),
            Err(err) => Err((self.treat_as_current(), err)),
        }
    }

    /// Treats this context as not current, even if it is current. We do no
    /// checks to confirm that this is actually case.
    ///
    /// If unsure whether or not this context is current, please use
    /// [`make_not_current`] which will do nothing if this context is not
    /// current.
    ///
    /// Please see [`make_current_surface`].
    ///
    /// [`make_not_current`]: struct.Context.html#method.make_not_current
    /// [`make_current_surface`]:
    /// struct.Context.html#method.make_current_surface
    #[inline]
    pub unsafe fn treat_as_not_current(
        self,
    ) -> SplitContext<ContextIsCurrent::No, PBT, WST, ST> {
        SplitContext {
            context: self.context,
            phantom: PhantomData,
        }
    }

    /// Treats this context as current, even if it is not current. We do no
    /// checks to confirm that this is actually case.
    ///
    /// This function should only be used if you intend to track context
    /// currency without the limited aid of glutin, and you wish to store
    /// all the [`Context`]s as [`NotCurrent`].
    ///
    /// Please see [`make_current_surface`] for the prefered method of handling
    /// context currency.
    ///
    /// [`make_current_surface`]:
    /// struct.Context.html#method.make_current_surface [`NotCurrent`]:
    /// enum.NotCurrent.html [`Context`]: struct.Context.html
    ///
    /// FIXME: docs
    #[inline]
    pub unsafe fn treat_as_current<IC2: ContextIsCurrentYesTrait>(
        self,
    ) -> SplitContext<IC2, PBT, WST, ST> {
        SplitContext {
            context: self.context,
            phantom: PhantomData,
        }
    }
}

impl<PBT: SupportsPBuffersTrait, ST: SupportsSurfacelessTrait>
    SplitContext<
        ContextIsCurrent::PossiblyAndSurfaceBound,
        PBT,
        SupportsWindowSurfaces::Yes,
        ST,
    >
{
    /// Update the context after the underlying surface resizes.
    ///
    /// Macos requires updating the context when the underlying surface resizes.
    ///
    /// The easiest way of doing this is to take every [`Resized`] window event
    /// that is received and call this function.
    ///
    /// Note: You still have to call the [`WindowSurface`]'s
    /// [`update_after_resize`] function for Wayland.
    ///
    /// [`Resized`]: event/enum.WindowEvent.html#variant.Resized
    /// FIXME: Links
    #[inline]
    pub fn update_after_resize(&self) {
        self.context.update_after_resize()
    }
}

pub trait LighterContextBuilderTrait {
    /// FIXME UPDATE DOIC:
    ///
    /// Errors can occur in two scenarios:
    ///  - If the window could not be created (via permission denied,
    ///  incompatible system, out of memory, etc.). This should be very rare.
    ///  - If the OpenGL [`Context`] could not be created. This generally
    ///    happens
    ///  because the underlying platform doesn't support a requested feature.
    ///
    /// [`WindowedContext<T>`]: type.WindowedContext.html
    /// [`Context`]: struct.Context.html
    ///
    /// Builds the given GL context.
    ///
    /// When on a unix operating system, prefer [`build_surfaceless`]. If both
    /// [`build_surfaceless`] and `build_headless` fail, try using a hidden
    /// window, or [`build_osmesa`]. Please note that if you choose to use a
    /// hidden window, you must still handle the events it generates on the
    /// events loop.
    ///
    /// Errors can occur in two scenarios:
    ///  - If the window could not be created (via permission denied,
    ///  incompatible system, out of memory, etc.). This should be very rare.
    ///  - If the OpenGL [`Context`] could not be created. This generally
    ///    happens
    ///  because the underlying platform doesn't support a requested feature.
    ///
    /// [`Context`]: struct.Context.html
    #[cfg_attr(
        not(any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
        )),
        doc = "\
    [`build_surfaceless`]: os/index.html
    [`build_osmesa`]: os/index.html
    "
    )]
    #[cfg_attr(
        any(
            target_os = "linux",
            target_os = "dragonfly",
            target_os = "freebsd",
            target_os = "netbsd",
            target_os = "openbsd",
        ),
        doc = "\
    [`build_surfaceless`]: os/unix/trait.HeadlessContextExt.html#tymethod.build_surfaceless
    [`build_osmesa`]: os/unix/trait.HeadlessContextExt.html#tymethod.build_osmesa
    "
    )]
    #[inline]
    fn build_lighter<
        TE,
        PBT2: SupportsPBuffersTrait,
        WST2: SupportsWindowSurfacesTrait,
        ST2: SupportsSurfacelessTrait,
    >(
        self,
        el: &EventLoopWindowTarget<TE>,
        _pbuffer_support: PBT2,
        _window_surface_support: WST2,
        _surfaceless_support: ST2,
    ) -> Result<
        SplitContext<ContextIsCurrent::No, PBT2, WST2, ST2>,
        CreationError,
    >;
}

impl<
        'a,
        IC: ContextIsCurrentTrait,
        PBT: SupportsPBuffersTrait,
        WST: SupportsWindowSurfacesTrait,
        ST: SupportsSurfacelessTrait,
    > LighterContextBuilderTrait
    for LighterContextBuilder<'a, IC, PBT, WST, ST>
{
    #[inline]
    fn build_lighter<
        TE,
        PBT2: SupportsPBuffersTrait,
        WST2: SupportsWindowSurfacesTrait,
        ST2: SupportsSurfacelessTrait,
    >(
        self,
        el: &EventLoopWindowTarget<TE>,
        _pbuffer_support: PBT2,
        _window_surface_support: WST2,
        _surfaceless_support: ST2,
    ) -> Result<
        SplitContext<ContextIsCurrent::No, PBT2, WST2, ST2>,
        CreationError,
    > {
        let mut ctx_supports: ContextSupports = Default::default();
        if PBT2::supported() {
            ctx_supports = ctx_supports | ContextSupports::PBUFFERS
        }
        if WST2::supported() {
            ctx_supports = ctx_supports | ContextSupports::WINDOW_SURFACES
        }
        if ST2::supported() {
            ctx_supports = ctx_supports | ContextSupports::SURFACELESS
        }

        self.map_sharing(|ctx| &ctx.context)
            .build(el, ctx_supports)
            .map(|context| SplitContext {
                context,
                phantom: PhantomData,
            })
    }
}