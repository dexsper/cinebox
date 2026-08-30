//! Stale-while-revalidate over `Bind` plus an optional disk snapshot.

use std::future::Future;

use egui_async::Bind;

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
    bind: &mut Bind<T, String>,
    has_disk: bool,
    skip_network: bool,
    request: F,
) -> Outcome
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, String>> + Send + 'static,
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
