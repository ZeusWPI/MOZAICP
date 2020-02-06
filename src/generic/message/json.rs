use crate::generic::*;

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

    pub fn value<'a>(&'a self) -> &'a Value {
        &self.value
    }

    pub fn bytes(&self) -> Option<Vec<u8>> {
        serde_json::to_vec(&self.value).ok()
    }

    pub fn into_t<'a, T: FromMessage<String, JSONMessage> + Key<String>>(
        &'a mut self,
    ) -> Option<&'a T> {
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
        match serde_json::to_value(&self) {
            Ok(value) => {
                if let Some(id) = value
                    .get(ID_FIELD)
                    .and_then(|v| v.as_str())
                    .map(String::from)
                {
                    return Some((
                        id.clone(),
                        JSONMessage {
                            value,
                            id,
                            item: None,
                        },
                    ));
                } else {
                    println!("No id field found");
                }
            }
            Err(e) => println!("To value failed {:?}", e),
        }

        None
    }
}
