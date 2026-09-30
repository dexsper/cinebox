//! Job failures worded for the viewer: what happened, what to try, where to fix it.

use rust_i18n::t;

use crate::jobs::JobError;
use crate::nav::SettingsPage;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserError {
    pub title: String,
    pub hint: Option<String>,
    /// Settings page that can fix the problem.
    pub fix: Option<SettingsPage>,
    /// Technical text (error chain) for the small print and logs.
    pub detail: Option<String>,
}

impl UserError {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            hint: None,
            fix: None,
            detail: None,
        }
    }

    #[must_use]
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    #[must_use]
    pub fn fix(mut self, page: SettingsPage) -> Self {
        self.fix = Some(page);
        self
    }

    /// The failure a job binding settled with; a generic one if it did not fail.
    pub fn from_read<T>(read: &Option<Result<T, JobError>>) -> Self {
        match read {
            Some(Err(error)) => Self::from(error),
            _ => Self::new(t!("common.failed")),
        }
    }

    /// Title and hint on one line, for toasts and probe results.
    #[must_use]
    pub fn summary(&self) -> String {
        match &self.hint {
            Some(hint) => format!("{}. {hint}", self.title),
            None => self.title.clone(),
        }
    }
}

impl From<&JobError> for UserError {
    fn from(error: &JobError) -> Self {
        let mut user = match error {
            JobError::Tmdb(error) => tmdb(error),
            JobError::Indexer(error) => indexer(error),
            JobError::TorrServer(error) => torrserver(error),
            JobError::Youtube(error) => youtube(error),
            JobError::Skip(_) => UserError::new(t!("error.skip")),
            JobError::Store(_) => UserError::new(t!("error.store")),
        };

        user.detail = Some(error_chain(error));
        user
    }
}

fn tmdb(error: &cinebox_tmdb::Error) -> UserError {
    use cinebox_tmdb::Error;

    match error {
        Error::EmptyKey => brief("tmdb_no_key").fix(SettingsPage::Tmdb),
        Error::AccessToken => advised("tmdb_access_token").fix(SettingsPage::Tmdb),
        Error::Unauthorized => advised("tmdb_rejected").fix(SettingsPage::Tmdb),
        Error::Request(_) => advised("tmdb_offline").fix(SettingsPage::General),
        Error::Http(code) => http_status("TMDB", *code),
        Error::UnsupportedKind | Error::IncompletePayload => unexpected("TMDB"),
        Error::Json(_) => unexpected("TMDB"),
    }
}

fn indexer(error: &cinebox_indexer::Error) -> UserError {
    use cinebox_indexer::Error;

    let parser = t!("error.parser_name");
    let user = match error {
        Error::EmptyUrl => brief("parser_no_url"),
        Error::Request(_) => advised("parser_offline"),
        Error::Http(401 | 403) => brief("parser_rejected"),
        Error::Http(code) => http_status(&parser, *code),
        Error::BadJson(_) => unexpected(&parser).hint(t!("error.parser_kind_hint")),
    };

    user.fix(SettingsPage::Parser)
}

fn torrserver(error: &cinebox_torrserver::Error) -> UserError {
    use cinebox_torrserver::Error;

    let setup = SettingsPage::TorrServer;
    let few_seeds = t!("error.few_seeds_hint");
    match error {
        Error::EmptyUrl => brief("torr_no_url").fix(setup),
        Error::Client(_) | Error::Request(_) => advised("torr_offline").fix(setup),
        Error::Http(401 | 403) => brief("torr_rejected").fix(setup),
        Error::Http(code) => http_status("TorrServer", *code).fix(setup),
        Error::EmptyEcho | Error::BadJson(_) => unexpected("TorrServer").fix(setup),
        Error::EmptyLink | Error::EmptyHash => brief("torr_no_link"),
        Error::NotFound => brief("torr_not_found"),
        Error::NoData => brief("torr_no_data").hint(few_seeds),
        Error::FilesTimeout => brief("torr_files_timeout").hint(few_seeds),
        Error::PreloadTimeout => brief("torr_preload_timeout").hint(few_seeds),
    }
}

fn youtube(error: &cinebox_youtube::Error) -> UserError {
    use cinebox_youtube::Error;

    match error {
        Error::Request(_) | Error::Http(_) => brief("youtube_offline"),
        Error::Unplayable | Error::NoFormats => brief("youtube_unplayable"),
        _ => brief("youtube_failed"),
    }
}

/// Title from `error.<id>`.
fn brief(id: &str) -> UserError {
    UserError::new(crate::i18n::tr(&format!("error.{id}")))
}

/// Title from `error.<id>`, hint from `error.<id>_hint`.
fn advised(id: &str) -> UserError {
    let hint = crate::i18n::tr(&format!("error.{id}_hint")).into_owned();
    brief(id).hint(hint)
}

fn unexpected(service: &str) -> UserError {
    UserError::new(t!("error.unexpected_reply", service = service))
}

fn http_status(service: &str, code: u16) -> UserError {
    let user = UserError::new(t!("error.http_status", service = service, code = code));
    if code == 429 {
        return user.hint(t!("error.rate_limited_hint"));
    }

    if code >= 500 {
        return user.hint(t!("error.server_down_hint"));
    }

    user
}

/// `Display` of the error and every `source`, skipping parts already printed
/// (some errors fold their cause into their own message).
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut out = error.to_string();
    let mut cause = error.source();

    while let Some(error) = cause {
        let text = error.to_string();
        if !out.contains(&text) {
            out.push_str(": ");
            out.push_str(&text);
        }
        cause = error.source();
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejected_tmdb_key_points_to_tmdb_settings() {
        let error = JobError::Tmdb(cinebox_tmdb::Error::Unauthorized);
        let user = UserError::from(&error);

        assert_eq!(user.fix, Some(SettingsPage::Tmdb));
        assert!(user.hint.is_some());
        assert_eq!(user.detail.as_deref(), Some("tmdb api key was rejected"));
    }

    #[test]
    fn parser_errors_always_point_to_parser_settings() {
        let error = JobError::Indexer(cinebox_indexer::Error::Http(500));
        let user = UserError::from(&error);

        assert_eq!(user.fix, Some(SettingsPage::Parser));
        assert!(user.title.contains("500"));
    }

    #[test]
    fn torrent_timeout_is_not_a_settings_problem() {
        let error = JobError::TorrServer(cinebox_torrserver::Error::FilesTimeout);
        let user = UserError::from(&error);

        assert_eq!(user.fix, None);
        assert!(user.hint.is_some());
    }

    #[test]
    fn summary_joins_title_and_hint() {
        let user = UserError::new("Down").hint("Try later");

        assert_eq!(user.summary(), "Down. Try later");
    }
}
