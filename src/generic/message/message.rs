use crate::generic::{FromMessage, IntoMessage};
use std::any::TypeId;
use std::sync::atomic::AtomicPtr;
use std::sync::Arc;
use std::sync::Mutex;

// TODO: Mutex shouldn't be needed :/
#[derive(Clone)]
pub struct Message {
    ptr: Arc<Mutex<AtomicPtr<u8>>>,
    type_id: TypeId,
    destroy: Arc<Box<dyn Fn(&mut *mut u8) -> () + 'static + Send + Sync>>,
}

impl Message {
    pub fn borrow<'a, T: 'static>(&'a mut self) -> Option<&'a T> {
        let mut guard = self.ptr.lock().ok()?;
        let ptr = guard.get_mut();

        match ptr.is_null() {
            true => None, // When ptr is null return None
            false => match TypeId::of::<T>() == self.type_id {
                true => {
                    let ptr: *mut T = ptr.cast();
                    unsafe { ptr.as_ref() }
                }
                false => {
                    trace!("Trying to deref message with wrong type");
                    None
                }
            },
        }
    }
}

impl<T: 'static> FromMessage<TypeId, Message> for T {
    fn from_msg<'a>(_: &TypeId, msg: &'a mut Message) -> Option<&'a T> {
        msg.borrow()
    }
}

impl<T: 'static> IntoMessage<TypeId, Message> for T {
    fn into_msg(self) -> Option<(TypeId, Message)> {
        let boxed = Box::new(self);
        let type_id = TypeId::of::<T>();

        Some((
            type_id,
            Message {
                ptr: Arc::new(Mutex::new(AtomicPtr::new(Box::into_raw(boxed).cast()))),
                type_id,

                destroy: Arc::new(Box::new(|ptr| {
                    if !ptr.is_null() {
                        unsafe { Box::from_raw(ptr.cast::<T>()) };
                    }
                })),
            },
        ))
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        if Arc::strong_count(&self.destroy) == 1 {
            (self.destroy)(&mut self.ptr.lock().unwrap().get_mut());
        }
    }
}
