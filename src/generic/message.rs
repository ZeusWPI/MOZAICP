use super::{Borrowable, Transmutable};
use std::any::TypeId;
use std::sync::atomic::AtomicPtr;

pub struct Message {
    ptr: AtomicPtr<u8>,
    type_id: TypeId,
    destroy: Box<dyn Fn(&mut *mut u8) -> () + 'static + Send>,
}

impl Message {
    pub fn take<T: 'static>(&mut self) -> Option<T> {
        let ptr = self.ptr.get_mut();

        match ptr.is_null() {
            true => None, // When ptr is null return None
            false => match TypeId::of::<T>() == self.type_id {
                true => {
                    // When types match

                    // Transmute into returned value and set internal pointer to
                    // null, so we avoid owning same value in several places.

                    let result: Box<T> = unsafe { Box::from_raw(ptr.cast()) };
                    self.ptr = AtomicPtr::new(std::ptr::null_mut());

                    Some(*result) // Unbox and return Some
                }
                false => None, // When types do not match return None
            },
        }
    }
}

impl Borrowable for Message {
    fn borrow<'a, T: 'static>(&'a mut self) -> Option<&'a T> {
        let ptr = self.ptr.get_mut();
        match ptr.is_null() {
            true => None, // When ptr is null return None
            false => match TypeId::of::<T>() == self.type_id {
                true => {
                    // When types match
                    let ptr: *const T = ptr.cast();
                    unsafe { ptr.as_ref() }
                }
                false => None, // When types do not match return None
            },
        }
    }
}

impl Transmutable<TypeId> for Message {
    fn transmute<T: 'static>(value: T) -> Option<(TypeId, Self)> {
        let boxed = Box::new(value);
        let type_id = TypeId::of::<T>();

        Some((
            type_id,
            Message {
                ptr: AtomicPtr::new(Box::into_raw(boxed).cast()),
                type_id,

                destroy: Box::new(|ptr| {
                    if !ptr.is_null() {
                        unsafe { Box::from_raw(ptr.cast::<T>()) };
                    }
                }),
            },
        ))
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        (self.destroy)(self.ptr.get_mut());
    }
}

#[cfg(test)]
mod tests {
    use super::Message;
    use generic::Transmutable;

    struct Val {
        value: i32,
    }

    #[test]
    fn exploration() {
        let (_, mut maybe) = Message::transmute(Val { value: 333 }).unwrap();
        let result = maybe.take::<Val>().map(|x| x.value);
        assert_eq!(result, Some(333));

        let result = maybe.take::<Val>().map(|x| x.value);
        assert_eq!(result, None);
    }
}
