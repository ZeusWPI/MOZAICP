
use std::any::TypeId;
use std::mem;

pub struct Message {
    ptr: *mut u8,
    type_id: TypeId,
    destroy: Box<dyn Fn(*mut u8) -> ()>
}

impl Message {
    fn new<T: 'static>(value: T) -> Message {
        let boxed = Box::new(value);

        Message {
            ptr: Box::into_raw(boxed).cast(),
            type_id: TypeId::of::<T>(),
            destroy: Box::new(|ptr| {
                unsafe { mem::transmute::<*mut u8, Box<T>>(ptr) };
            })
        }
    }
}

impl Message {
    fn take<T: 'static>(&mut self) -> Option<T> {
        match self.ptr.is_null() {
            true => None, // When ptr is null return None
            false => match TypeId::of::<T>() == self.type_id {
                true => { // When types match

                    // Transmute into returned value and set internal pointer to
                    // null, so we avoid owning same value in several places.

                    let result: Box<T> = unsafe { Box::from_raw(self.ptr.cast()) };
                    self.ptr = std::ptr::null_mut();

                    Some(*result) // Unbox and return Some
                },
                false => None, // When types do not match return None
            },
        }
    }
}

impl Drop for Message {
    fn drop(&mut self) {
        match self.ptr.is_null() {
            true => (),
            false => (self.destroy)(self.ptr),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Message;

    struct Val {
        value: i32,
    }

    #[test]
    fn exploration() {
        let mut maybe = Message::new(Val { value: 333 });
        let result = maybe.take::<Val>().map(|x| x.value );
        assert_eq!(result, Some(333));

        let result = maybe.take::<Val>().map(|x| x.value );
        assert_eq!(result, None);
    }
}
