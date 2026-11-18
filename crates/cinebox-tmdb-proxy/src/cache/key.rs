use std::path::{Path, PathBuf};

const API_KEY_PARAM: &str = "api_key";

/// Which upstream a cached response came from, so that equal paths on the two hosts never share a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Route {
    Api,
    Image,
}

/// Cache key: blake3 of the route, the path and the query pairs sorted, with `api_key` left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct Key([u8; blake3::OUT_LEN]);

impl Route {
    const fn tag(self) -> u8 {
        match self {
            Self::Api => 1,
            Self::Image => 2,
        }
    }
}

impl Key {
    /// The order of `query` does not matter; repeated parameters are kept, so `a=1&a=2` differs from `a=1`.
    #[must_use]
    pub(crate) fn new<'a>(route: Route, path: &str, query: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let kept = query.into_iter().filter(|(name, _)| *name != API_KEY_PARAM);
        let mut pairs: Vec<(&str, &str)> = kept.collect();
        pairs.sort_unstable();

        let mut hasher = blake3::Hasher::new();
        hasher.update(&[route.tag()]);
        update_framed(&mut hasher, path);

        for (name, value) in pairs {
            update_framed(&mut hasher, name);
            update_framed(&mut hasher, value);
        }

        Self(*hasher.finalize().as_bytes())
    }

    /// Parses the file name the disk tier gives an entry; `None` for anything else.
    #[must_use]
    pub(super) fn from_file_name(name: &str) -> Option<Self> {
        let hash = blake3::Hash::from_hex(name).ok()?;

        Some(Self(*hash.as_bytes()))
    }

    /// `root/ab/<hex>`, where `ab` are the first two hex digits.
    #[must_use]
    pub(super) fn path_in(&self, root: &Path) -> PathBuf {
        let hex = blake3::Hash::from_bytes(self.0).to_hex();
        let (shard, _) = hex.split_at(2);

        root.join(shard).join(hex.as_str())
    }

    #[must_use]
    pub(super) fn file_name(&self) -> String {
        blake3::Hash::from_bytes(self.0).to_hex().to_string()
    }
}

// Each part is prefixed with its length, so that `("ab", "c")` and `("a", "bc")` hash differently.
fn update_framed(hasher: &mut blake3::Hasher, part: &str) {
    let length = part.len() as u64;
    hasher.update(&length.to_le_bytes());
    hasher.update(part.as_bytes());
}
