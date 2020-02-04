use super::{FromMessage, IntoMessage};
use std::any::TypeId;
use std::sync::atomic::AtomicPtr;

pub struct Message {
    ptr: AtomicPtr<u8>,
    type_id: TypeId,
    destroy: Box<dyn Fn(&mut *mut u8) -> () + 'static + Send + Sync>,
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

    pub fn borrow<'a, T: 'static>(&'a mut self) -> Option<&'a T> {
        let ptr = self.ptr.get_mut();
        match ptr.is_null() {
            true => None, // When ptr is null return None
            false => match TypeId::of::<T>() == self.type_id {
                true => {
                    let ptr: *mut T = ptr.cast();
                    unsafe { ptr.as_ref() }
                }
                false => None, // When types do not match return None
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

pub use json::{JSONMessage, Typed};
mod json {
    use super::super::*;

    use serde::{Deserialize, Serialize};
    use serde_json::Value;
    use std::{any, ops};

    pub static ID_FIELD: &str = "type_id";

    #[derive(Deserialize, Serialize)]
    pub struct Typed<T> {
        type_id: String,

        #[serde(flatten)]
        value: T,
    }

    impl<T: Serialize + for<'de> Deserialize<'de> + Key<String>> From<T> for Typed<T> {
        fn from(value: T) -> Typed<T> {
            Self {
                type_id: T::key(),
                value,
            }
        }
    }

    impl<T> ops::Deref for Typed<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.value
        }
    }

    impl<T: Key<String>> Key<String> for Typed<T> {
        fn key() -> String {
            T::key()
        }
    }


    pub struct JSONMessage {
        value: Value,
        id: String,
        item: Option<Option<Message>>,
    }

    impl ops::Deref for JSONMessage {
        type Target = Value;

        fn deref(&self) -> &Self::Target {
            &self.value
        }
    }

    impl JSONMessage {
        pub fn borrow<'a, T: 'static + for<'de> Deserialize<'de>>(&'a mut self) -> Option<&'a T> {
            if self.item.is_none() {
                self.item = Some(
                    serde_json::from_value(self.value.clone())
                        .ok()
                        .and_then(|item| {
                            <T as IntoMessage<any::TypeId, Message>>::into_msg(item).map(|(_, i)| i)
                        }),
                );
            }

            if let Some(Some(item)) = &mut self.item {
                item.borrow()
            } else {
                None
            }
        }

        pub fn bytes(&self) -> Option<Vec<u8>> {
            serde_json::to_vec(&self.value).ok()
        }

        pub fn into_t<'a, T: FromMessage<String, JSONMessage> + Key<String>>(&'a mut self) -> Option<&'a T> {
            T::from_msg(&T::key(), self)
        }
    }

    // Please don't puke
    impl<T: 'static + for<'de> Deserialize<'de>> FromMessage<String, JSONMessage> for T {
        fn from_msg<'a>(key: &String, msg: &'a mut JSONMessage) -> Option<&'a T> {
            if *key != msg.id {
                return None;
            }

            msg.borrow()
        }
    }

    impl<T: 'static + Serialize> IntoMessage<String, JSONMessage> for T {
        fn into_msg(self) -> Option<(String, JSONMessage)> {

            if let Ok(value) = serde_json::to_value(&self) {
                if let Some(id) = value.get(ID_FIELD).and_then(|v| v.as_str()).map(String::from) {
                    return Some((id.clone(), JSONMessage {
                        value,
                        id,
                        item: None,
                    }))
                }
            }

            None
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::generic::IntoMessage;

    struct Val {
        value: i32,
    }

    #[test]
    fn exploration() {
        let (_, mut maybe) = Val::into_msg(Val { value: 333 }).unwrap();
        let result = maybe.take::<Val>().map(|x| x.value);
        assert_eq!(result, Some(333));

        let result = maybe.take::<Val>().map(|x| x.value);
        assert_eq!(result, None);
    }
}
