use super::{EventedRead, EventedWrite, Pty, PtyImpl};
use crate::event::OnResize;
use crate::term::SizeInfo;
use std::marker::PhantomData;

// This trait is used to convert statically-dispatched Ptys into type-erased,
// dynamically-dispatched ones.
// This has to be done in two steps:
//   1) The DynPtyImpl struct converts all the PtyImpl output types into
//      dynamically-dispatched forms (e.g. Conout -> dyn EventedRead + 'a)
//   2) The DynPty struct wraps a type-erased Box<DynPtyImpl>, thus enabling
//      the type of the original PtyImpl to be erased.
pub trait IntoDynamicPty<'a> {
    fn into_dynamic_pty(self) -> Pty<DynPty<'a>>;
}

impl<'a, T> IntoDynamicPty<'a> for Pty<T>
where
    T: PtyImpl + Send + 'a,
    <T as PtyImpl>::ResizeHandle: 'a,
    <T as PtyImpl>::Conout: Sized + 'a,
    <T as PtyImpl>::Conin: Sized + 'a,
{
    fn into_dynamic_pty(self) -> Pty<DynPty<'a>> {
        Pty {
            inner: DynPty(Box::new(DynPtyImpl(self.inner, PhantomData))),
            read_token: self.read_token,
            write_token: self.write_token,
            child_event_token: self.child_event_token,
            child_watcher: self.child_watcher,
        }
    }
}

pub struct DynPty<'a>(
    Box<
        dyn PtyImpl<
                ResizeHandle = DynResizeHandle<'a>,
                Conout = dyn EventedRead + 'a,
                Conin = dyn EventedWrite + 'a,
            > + Send
            + 'a,
    >,
);

pub struct DynResizeHandle<'a>(Box<dyn OnResize + 'a>);

struct DynPtyImpl<'a, T>(T, PhantomData<Box<DynPty<'a>>>);

impl<'a> OnResize for DynResizeHandle<'a> {
    fn on_resize(&mut self, size: &SizeInfo) {
        (*self.0).on_resize(size)
    }
}

impl<'a, T, R, CO, CI> PtyImpl for DynPtyImpl<'a, T>
where
    T: PtyImpl<ResizeHandle = R, Conout = CO, Conin = CI> + 'a,
    R: OnResize + 'a,
    CO: EventedRead + Sized + 'a,
    CI: EventedWrite + Sized + 'a,
{
    type ResizeHandle = DynResizeHandle<'a>;
    type Conout = dyn EventedRead + 'a;
    type Conin = dyn EventedWrite + 'a;

    fn resize_handle(&self) -> DynResizeHandle<'a> {
        DynResizeHandle(Box::new(self.0.resize_handle()))
    }

    fn conout(&self) -> &(dyn EventedRead + 'a) {
        self.0.conout()
    }

    fn conout_mut(&mut self) -> &mut (dyn EventedRead + 'a) {
        self.0.conout_mut()
    }
    fn conin(&self) -> &(dyn EventedWrite + 'a) {
        self.0.conin()
    }
    fn conin_mut(&mut self) -> &mut (dyn EventedWrite + 'a) {
        self.0.conin_mut()
    }
}

impl<'a> PtyImpl for DynPty<'a> {
    type ResizeHandle = DynResizeHandle<'a>;
    type Conout = dyn EventedRead + 'a;
    type Conin = dyn EventedWrite + 'a;

    fn resize_handle(&self) -> Self::ResizeHandle {
        self.0.resize_handle()
    }

    fn conout(&self) -> &Self::Conout {
        self.0.conout()
    }

    fn conout_mut(&mut self) -> &mut Self::Conout {
        self.0.conout_mut()
    }

    fn conin(&self) -> &Self::Conin {
        self.0.conin()
    }

    fn conin_mut(&mut self) -> &mut Self::Conin {
        self.0.conin_mut()
    }
}
