use std::{
    collections::{HashMap, HashSet},
    fs::File,
    rc::Rc,
};

use serde::de::DeserializeSeed;

use super::{
    cfg_file::ConfigFile,
    error::{Error, Result},
    key::Key,
    parsed_value::{InterpolateKey, ParsedValue, ParsedValueSeed},
};

pub struct Namespace {
    pub key: Rc<Key>,
    pub locales: Vec<Locale>,
}

pub enum LocalesOrNamespaces {
    NameSpaces(Vec<Namespace>),
    Locales(Vec<Locale>),
}

pub struct BuildersKeysInner(pub HashMap<Rc<Key>, LocaleValue>);

pub enum BuildersKeys {
    NameSpaces {
        namespaces: Vec<Namespace>,
        keys: HashMap<Rc<Key>, BuildersKeysInner>,
    },
    Locales {
        locales: Vec<Locale>,
        keys: BuildersKeysInner,
    },
}

impl Namespace {
    pub fn new(locales_dir: &str, key: Rc<Key>, locale_keys: &[Rc<Key>]) -> Result<Self> {
        let mut locales = Vec::with_capacity(locale_keys.len());
        for locale in locale_keys.iter().cloned() {
            let path = format!("{}/{}/{}.json", locales_dir, locale.name, key.name);
            locales.push(Locale::new(path, locale)?);
        }
        Ok(Namespace { key, locales })
    }
}

impl LocalesOrNamespaces {
    pub fn new(cfg_file: &ConfigFile) -> Result<Self> {
        let locale_keys = &cfg_file.locales;
        let locales_dir = cfg_file.locales_dir.as_ref();
        if let Some(namespace_keys) = &cfg_file.name_spaces {
            let mut namespaces = Vec::with_capacity(namespace_keys.len());
            for namespace in namespace_keys {
                namespaces.push(Namespace::new(
                    locales_dir,
                    Rc::clone(namespace),
                    locale_keys,
                )?);
            }
            Ok(LocalesOrNamespaces::NameSpaces(namespaces))
        } else {
            let mut locales = Vec::with_capacity(locale_keys.len());
            for locale in locale_keys.iter().cloned() {
                let path = format!("{}/{}.json", locales_dir, locale.name);
                locales.push(Locale::new(path, locale)?);
            }
            Ok(LocalesOrNamespaces::Locales(locales))
        }
    }
}

pub struct Locale {
    pub name: Rc<Key>,
    pub keys: HashMap<Rc<Key>, ParsedValue>,
}

impl Locale {
    pub fn new(path: String, locale: Rc<Key>) -> Result<Self> {
        let locale_file = match File::open(&path) {
            Ok(file) => file,
            Err(err) => return Err(Error::LocaleFileNotFound { path, err }),
        };

        let mut deserializer = serde_json::Deserializer::from_reader(locale_file);

        LocaleSeed(locale)
            .deserialize(&mut deserializer)
            .map_err(|err| Error::LocaleFileDeser { path, err })
    }

    pub fn get_keys(&self) -> HashSet<Rc<Key>> {
        self.keys.keys().cloned().collect()
    }

    fn key_missmatch(
        locale1: &Self,
        keys1: &HashSet<Rc<Key>>,
        locale2: &Self,
        keys2: &HashSet<Rc<Key>>,
        namespace: Option<&str>,
    ) -> Error {
        let mut locale = locale2;

        let mut diff = keys1
            .difference(keys2)
            .map(|key| key.name.clone())
            .collect::<Vec<_>>();

        if diff.is_empty() {
            locale = locale1;
            diff = keys2
                .difference(keys1)
                .map(|key| key.name.clone())
                .collect();
        }

        Error::MissingKeysInLocale {
            namespace: namespace.map(str::to_string),
            keys: diff,
            locale: locale.name.name.clone(),
        }
    }

    pub fn check_locales_inner(
        locales: &[Locale],
        namespace: Option<&str>,
    ) -> Result<BuildersKeysInner> {
        let mut locales = locales.iter();
        let first_locale = locales.next().unwrap();

        let first_locale_keys = first_locale.get_keys();

        let mut mapped_keys: HashMap<_, _> = first_locale
            .keys
            .iter()
            .map(|(key, value)| (key, value.get_keys()))
            .collect();

        for locale in locales {
            let keys = locale.get_keys();
            if first_locale_keys != keys {
                return Err(Self::key_missmatch(
                    first_locale,
                    &first_locale_keys,
                    locale,
                    &keys,
                    namespace,
                ));
            }

            for (key, key_kind) in &mut mapped_keys {
                if let Some(value) = locale.keys.get(*key) {
                    value.get_keys_inner(key_kind)
                }
            }
        }

        let iter = mapped_keys
            .iter_mut()
            .filter_map(|(locale_key, value)| Some((locale_key, value.as_mut()?)));

        for (locale_key, keys) in iter {
            let count_type = keys.iter().find_map(|key| match key {
                InterpolateKey::Count(plural_type) => Some(plural_type),
                _ => None,
            });
            if let Some(count_type) = count_type {
                let plural_type_mismatch = keys.iter().any(|key| matches!(key, InterpolateKey::Count(plural_type) if plural_type != count_type));

                if plural_type_mismatch {
                    return Err(Error::PluralTypeMissmatch {
                        locale_key: locale_key.name.to_string(),
                        namespace: namespace.map(str::to_string),
                    });
                }

                // if the set contains InterpolateKey::Count, remove variable keys with name "count"
                // ("var_count" with the rename)
                keys.retain(
                    |key| !matches!(key, InterpolateKey::Variable(key) if key.name == "var_count"),
                );
            }
        }

        Ok(BuildersKeysInner(
            mapped_keys
                .into_iter()
                .map(|(key, value)| (Rc::clone(key), LocaleValue::new(value)))
                .collect(),
        ))
    }

    pub fn check_locales(locales: LocalesOrNamespaces) -> Result<BuildersKeys> {
        match locales {
            LocalesOrNamespaces::NameSpaces(namespaces) => {
                let mut keys = HashMap::with_capacity(namespaces.len());
                for namespace in &namespaces {
                    let k =
                        Self::check_locales_inner(&namespace.locales, Some(&namespace.key.name))?;
                    keys.insert(Rc::clone(&namespace.key), k);
                }
                Ok(BuildersKeys::NameSpaces { namespaces, keys })
            }
            LocalesOrNamespaces::Locales(locales) => {
                let keys = Self::check_locales_inner(&locales, None)?;
                Ok(BuildersKeys::Locales { locales, keys })
            }
        }
    }
}

#[derive(PartialEq, Eq)]
pub enum LocaleValue {
    String,
    Builder(HashSet<InterpolateKey>),
}

impl LocaleValue {
    fn new(value: Option<HashSet<InterpolateKey>>) -> Self {
        match value {
            Some(keys) => Self::Builder(keys),
            None => Self::String,
        }
    }
}

#[derive(Debug, Clone)]
struct LocaleSeed(Rc<Key>);

impl<'de> serde::de::Visitor<'de> for LocaleSeed {
    type Value = HashMap<Rc<Key>, ParsedValue>;

    fn visit_map<A>(self, mut map: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        let mut keys = HashMap::new();

        while let Some(locale_key) = map.next_key()? {
            let value = map.next_value_seed(ParsedValueSeed(false))?;
            keys.insert(locale_key, value);
        }

        Ok(keys)
    }

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            formatter,
            "a map of string keys and value either string or map"
        )
    }
}

impl<'a: 'de, 'de> serde::de::DeserializeSeed<'de> for LocaleSeed {
    type Value = Locale;

    fn deserialize<D>(self, deserializer: D) -> std::result::Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let keys = deserializer.deserialize_map(self.clone())?;
        Ok(Locale { name: self.0, keys })
    }
}
