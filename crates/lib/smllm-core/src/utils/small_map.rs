//! A string-keyed map kept as a vector sorted by key. `BTreeMap` instantiates
//! the whole B-tree per value type in the wasm (rust-rules); the engine's maps
//! hold a handful of entries, where binary search + shifted insert is plenty.

use crate::prelude::*;

/// A map from `String` keys to `V`, iterated in key order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmallMap<V> {
    entries: Vec<(String, V)>,
}

impl<V> Default for SmallMap<V> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> SmallMap<V> {
    /// An empty map.
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    fn find(&self, key: &str) -> Result<usize, usize> {
        self.entries.binary_search_by(|(k, _)| k.as_str().cmp(key))
    }

    /// The value for `key`.
    pub fn get(&self, key: &str) -> Option<&V> {
        self.find(key).ok().map(|i| &self.entries[i].1)
    }

    /// The value for `key`, mutably.
    pub fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        match self.find(key) {
            Ok(i) => Some(&mut self.entries[i].1),
            Err(_) => None,
        }
    }

    /// Insert or replace; returns the previous value.
    pub fn insert(&mut self, key: impl Into<String>, value: V) -> Option<V> {
        let key = key.into();
        match self.find(&key) {
            Ok(i) => Some(core::mem::replace(&mut self.entries[i].1, value)),
            Err(i) => {
                self.entries.insert(i, (key, value));
                None
            }
        }
    }

    /// Remove `key`; returns its value.
    pub fn remove(&mut self, key: &str) -> Option<V> {
        self.find(key).ok().map(|i| self.entries.remove(i).1)
    }

    /// Whether `key` is present.
    pub fn contains_key(&self, key: &str) -> bool {
        self.find(key).is_ok()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the map is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Entries in key order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// Keys in order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }
}

impl<V, K: Into<String>> FromIterator<(K, V)> for SmallMap<V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        let mut map = Self::new();
        for (k, v) in iter {
            map.insert(k, v);
        }
        map
    }
}

#[cfg(feature = "serde")]
mod serde_impl {
    use core::fmt;
    use core::marker::PhantomData;

    use serde::de::{MapAccess, Visitor};
    use serde::ser::SerializeMap;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::SmallMap;
    use crate::prelude::*;

    impl<V: Serialize> Serialize for SmallMap<V> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut map = s.serialize_map(Some(self.len()))?;
            for (k, v) in self.iter() {
                map.serialize_entry(k, v)?;
            }
            map.end()
        }
    }

    impl<'de, V: Deserialize<'de>> Deserialize<'de> for SmallMap<V> {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct MapVisitor<V>(PhantomData<V>);
            impl<'de, V: Deserialize<'de>> Visitor<'de> for MapVisitor<V> {
                type Value = SmallMap<V>;
                fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                    f.write_str("a map")
                }
                fn visit_map<A: MapAccess<'de>>(
                    self,
                    mut access: A,
                ) -> Result<Self::Value, A::Error> {
                    let mut map = SmallMap::new();
                    while let Some((k, v)) = access.next_entry::<String, V>()? {
                        map.insert(k, v);
                    }
                    Ok(map)
                }
            }
            d.deserialize_map(MapVisitor(PhantomData))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove_in_key_order() {
        let mut m = SmallMap::new();
        assert!(m.is_empty());
        assert_eq!(m.insert("b", 2), None);
        assert_eq!(m.insert("a", 1), None);
        assert_eq!(m.insert("b", 3), Some(2));
        assert_eq!(m.len(), 2);
        assert_eq!(m.get("b"), Some(&3));
        assert!(m.contains_key("a"));
        *m.get_mut("a").unwrap() += 10;
        assert!(m.get_mut("zz").is_none());
        assert_eq!(m.keys().collect::<Vec<_>>(), vec!["a", "b"]);
        assert_eq!(m.remove("a"), Some(11));
        assert_eq!(m.remove("a"), None);
        let m2: SmallMap<u8> = [("y", 1), ("x", 2)].into_iter().collect();
        assert_eq!(m2.iter().next(), Some(("x", &2)));
        assert_eq!(SmallMap::<u8>::default(), SmallMap::new());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn serde_as_a_json_object() {
        let m: SmallMap<u32> = [("b", 2), ("a", 1)].into_iter().collect();
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"{"a":1,"b":2}"#);
        let back: SmallMap<u32> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, m);
        assert!(serde_json::from_str::<SmallMap<u32>>("[1]").is_err());
    }
}
