//! Stale-while-revalidate over `Bind` plus an optional disk snapshot.

use std::convert::Infallible;
use std::future::Future;

use cinebox_core::UiLanguage;
use egui_async::Bind;

use crate::jobs::JobError;

/// Screen-side SWR state: live bind, async disk snapshot, language tracking.
///
/// The disk snapshot is loaded off the UI thread through its own `Bind`;
/// `hydrate` returns `false` while that read is still in flight, so screens
/// can paint a pending frame instead of blocking on SQLite.
pub struct Cached<T, D> {
    pub bind: Bind<T, JobError>,
    pub disk: Option<D>,
    disk_bind: Bind<Option<D>, Infallible>,
    disk_checked: bool,
    lang: Option<UiLanguage>,
    force_refresh: bool,
}

impl<T: Send + 'static, D: Send + 'static> Default for Cached<T, D> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send + 'static, D: Send + 'static> Cached<T, D> {
    #[must_use]
    pub fn new() -> Self {
        Self {
            bind: Bind::new(true),
            disk: None,
            disk_bind: Bind::new(true),
            disk_checked: false,
            lang: None,
            force_refresh: false,
        }
    }

    /// New subject: drop live and disk state without forcing a network reload.
    pub fn reset(&mut self) {
        self.bind = Bind::new(true);
        self.disk_bind.clear();
        self.disk = None;
        self.disk_checked = false;
        self.force_refresh = false;
    }

    /// Drop everything and force a network reload on the next `resolve`.
    pub fn invalidate(&mut self) {
        self.reset();
        self.force_refresh = true;
    }

    /// Like `invalidate`, but also forgets the tracked language so the next
    /// `sync_lang` call is treated as the first one.
    pub fn forget_live(&mut self) {
        self.lang = None;
        self.invalidate();
    }

    /// Track the UI language; a switch drops caches and forces a refresh.
    pub fn sync_lang(&mut self, lang: UiLanguage) {
        if self.lang == Some(lang) {
            return;
        }

        let switched = self.lang.is_some();
        self.lang = Some(lang);

        if switched {
            self.invalidate();
        }
    }

    /// Drive the async disk read. Returns `false` while it is in flight.
    pub fn hydrate<Fut>(&mut self, load: Fut) -> bool
    where
        Fut: Future<Output = Option<D>> + Send + 'static,
    {
        if self.disk_checked {
            return true;
        }

        let _ = self
            .disk_bind
            .read_or_request(|| async move { Ok::<_, Infallible>(load.await) });

        let Some(result) = self.disk_bind.take() else {
            return false;
        };

        self.disk = result.unwrap_or_default();
        self.disk_checked = true;

        true
    }

    /// Resolve live vs disk vs network. `fresh` says the disk snapshot is
    /// still valid, so the network can be skipped.
    pub fn resolve<F, Fut>(&mut self, fresh: bool, request: F) -> Outcome
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T, JobError>> + Send + 'static,
    {
        let skip_network = !self.force_refresh && fresh;
        let outcome = resolve(&mut self.bind, self.disk.is_some(), skip_network, request);

        if outcome.from_network {
            self.force_refresh = false;
        }

        outcome
    }

    /// Failed-view retry: clear the live bind and force a network reload.
    pub fn retry(&mut self) {
        self.bind.clear();
        self.force_refresh = true;
    }
}

/// Where the value to paint should come from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Swr {
    Live,
    Disk,
    Failed,
    Pending,
}

/// Disk vs network choice for one bind.
pub struct Outcome {
    pub view: Swr,
    pub from_network: bool,
    pub in_flight: bool,
}

/// Prefer a live bind, else a fresh disk hit, else start `request`.
pub fn resolve<T, F, Fut>(
    bind: &mut Bind<T, JobError>,
    has_disk: bool,
    skip_network: bool,
    request: F,
) -> Outcome
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, JobError>> + Send + 'static,
    T: Send + 'static,
{
    if matches!(bind.read(), Some(Ok(_))) {
        return Outcome {
            view: Swr::Live,
            from_network: true,
            in_flight: false,
        };
    }

    if matches!(bind.read(), Some(Err(_))) {
        if has_disk {
            return Outcome {
                view: Swr::Disk,
                from_network: false,
                in_flight: false,
            };
        }

        return Outcome {
            view: Swr::Failed,
            from_network: false,
            in_flight: false,
        };
    }

    if skip_network && has_disk {
        return Outcome {
            view: Swr::Disk,
            from_network: false,
            in_flight: false,
        };
    }

    let arrived = bind.read_or_request(request).is_some();
    if arrived {
        if matches!(bind.read(), Some(Ok(_))) {
            return Outcome {
                view: Swr::Live,
                from_network: true,
                in_flight: false,
            };
        }

        if has_disk {
            return Outcome {
                view: Swr::Disk,
                from_network: false,
                in_flight: false,
            };
        }

        return Outcome {
            view: Swr::Failed,
            from_network: false,
            in_flight: false,
        };
    }

    if has_disk {
        return Outcome {
            view: Swr::Disk,
            from_network: false,
            in_flight: true,
        };
    }

    Outcome {
        view: Swr::Pending,
        from_network: false,
        in_flight: true,
    }
}
