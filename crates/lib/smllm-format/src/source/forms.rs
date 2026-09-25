//! XState's polymorphic shapes: ordered maps, one-or-many, string-or-object.
//! Hand-written visitors (not `#[serde(untagged)]`) so a mistake reports the
//! real problem at its YAML location instead of "did not match any variant".

use std::borrow::Cow;
use std::fmt;
use std::marker::PhantomData;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::de::value::{MapAccessDeserializer, SeqAccessDeserializer};
use serde::de::{Deserializer, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};

/// A map that keeps file order (states, events, params).
#[derive(Debug, Clone, PartialEq)]
pub struct OrderedMap<V>(pub Vec<(String, V)>);

impl<V> Default for OrderedMap<V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<V> OrderedMap<V> {
    /// Entries in file order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// The value for `key`.
    pub fn get(&self, key: &str) -> Option<&V> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Whether `key` is present.
    pub fn contains_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for OrderedMap<V> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V_<V>(PhantomData<V>);
        impl<'de, V: Deserialize<'de>> Visitor<'de> for V_<V> {
            type Value = OrderedMap<V>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a map")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut out: Vec<(String, V)> = Vec::new();
                while let Some((k, v)) = a.next_entry::<String, V>()? {
                    if out.iter().any(|(e, _)| *e == k) {
                        return Err(serde::de::Error::custom(format!("duplicate key `{k}`")));
                    }
                    out.push((k, v));
                }
                Ok(OrderedMap(out))
            }
        }
        d.deserialize_map(V_(PhantomData))
    }
}

impl<V: Serialize> Serialize for OrderedMap<V> {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeMap;
        let mut m = s.serialize_map(Some(self.0.len()))?;
        for (k, v) in &self.0 {
            m.serialize_entry(k, v)?;
        }
        m.end()
    }
}

impl<V: JsonSchema> JsonSchema for OrderedMap<V> {
    fn schema_name() -> Cow<'static, str> {
        format!("Map_of_{}", V::schema_name()).into()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        json_schema!({ "type": "object", "additionalProperties": g.subschema_for::<V>() })
    }
}

/// One item or a list of them (`entry`, `actions`, `on.<event>`, `always`).
#[derive(Debug, Clone, PartialEq)]
pub struct OneOrMany<T>(pub Vec<T>);

impl<T> Default for OneOrMany<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for OneOrMany<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V_<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V_<T> {
            type Value = OneOrMany<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("one item or a list")
            }
            fn visit_seq<A: SeqAccess<'de>>(self, a: A) -> Result<Self::Value, A::Error> {
                Vec::<T>::deserialize(SeqAccessDeserializer::new(a)).map(OneOrMany)
            }
            fn visit_map<A: MapAccess<'de>>(self, a: A) -> Result<Self::Value, A::Error> {
                T::deserialize(MapAccessDeserializer::new(a)).map(|t| OneOrMany(vec![t]))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                T::deserialize(serde::de::value::StrDeserializer::<E>::new(v))
                    .map(|t| OneOrMany(vec![t]))
            }
        }
        d.deserialize_any(V_(PhantomData))
    }
}

impl<T: JsonSchema> JsonSchema for OneOrMany<T> {
    fn schema_name() -> Cow<'static, str> {
        format!("OneOrMany_{}", T::schema_name()).into()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        let item = g.subschema_for::<T>();
        json_schema!({ "anyOf": [item, { "type": "array", "items": item }] })
    }
}

/// A bare string (`WORK`, `setRef`) or an object.
#[derive(Debug, Clone, PartialEq)]
pub enum StringOr<T> {
    /// The string form.
    Str(String),
    /// The object form.
    Obj(T),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for StringOr<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct V_<T>(PhantomData<T>);
        impl<'de, T: Deserialize<'de>> Visitor<'de> for V_<T> {
            type Value = StringOr<T>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string or a map")
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(StringOr::Str(v.to_string()))
            }
            fn visit_map<A: MapAccess<'de>>(self, a: A) -> Result<Self::Value, A::Error> {
                T::deserialize(MapAccessDeserializer::new(a)).map(StringOr::Obj)
            }
        }
        d.deserialize_any(V_(PhantomData))
    }
}

impl<T: JsonSchema> JsonSchema for StringOr<T> {
    fn schema_name() -> Cow<'static, str> {
        format!("StringOr_{}", T::schema_name()).into()
    }

    fn json_schema(g: &mut SchemaGenerator) -> Schema {
        json_schema!({ "anyOf": [{ "type": "string" }, g.subschema_for::<T>()] })
    }
}

/// A command: a shell string, or an argv list run without a shell (DEC-4).
#[derive(Debug, Clone, PartialEq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Run {
    /// `sh -c` / `cmd /C`.
    Shell(String),
    /// Exec, no shell (DEC-9).
    Exec(Vec<String>),
}
