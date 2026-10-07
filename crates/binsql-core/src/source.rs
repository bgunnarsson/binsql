//! Turning what someone typed about a data source into what gets saved.
//!
//! The TUI's form and the command line both describe a data source the same
//! way — a folder, a name, a connection string, a driver and a few switches —
//! so the rules for what may be saved live here once, and the messages they
//! give are the ones both show.

use crate::backend::Backend;
use crate::config::{DataSource, SEPARATOR, qualify};
use crate::secrets::keychain;
use crate::workspace::Scope;

/// What a description produced: where to save it, and — when the connection
/// string is to live in the credential store rather than the config — the
/// string to file there first.
pub struct Saved {
    pub id: String,
    pub source: DataSource,
    /// The connection string to write to the credential store under `id`.
    /// `None` leaves the store alone, either because the string is in the
    /// config or because an edit did not retype it.
    pub secret: Option<String>,
    /// The name this was saved under before, so a rename can take the old entry
    /// with it.
    pub previous: Option<String>,
    /// The config file to write it to.
    pub scope: Scope,
}

/// A data source as described, before anything checks it.
pub struct Draft<'a> {
    /// Groups the connection. Empty leaves it at the top level.
    pub folder: &'a str,
    pub name: &'a str,
    pub dsn: &'a str,
    /// The driver chosen, or `None` to tell it from the connection string.
    pub backend: Option<Backend>,
    /// Keep the connection string in the OS credential store, leaving only a
    /// `keychain://` reference in the config.
    pub keychain: bool,
    pub read_only: bool,
    pub open_on_start: bool,
    /// The name being edited, or `None` for a new data source.
    pub previous: Option<String>,
    pub scope: Scope,
}

impl Draft<'_> {
    /// Validates and returns what to save.
    pub fn build(&self) -> Result<Saved, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err("A data source needs a name".into());
        }
        let folder = self.folder.trim();
        if name.contains(SEPARATOR) || folder.contains(SEPARATOR) {
            return Err(format!(
                "'{SEPARATOR}' separates the folder from the name, so neither may contain it"
            ));
        }
        let dsn = self.dsn.trim();
        if dsn.is_empty() {
            return Err("A data source needs a connection string".into());
        }
        let backend = self
            .backend
            .or_else(|| Backend::infer(self.dsn))
            .ok_or_else(|| {
                "Could not tell the driver from that connection string — pick one".to_string()
            })?;

        let id = qualify(Some(folder).filter(|f| !f.is_empty()), name);

        // Three cases. An untouched reference is re-keyed to the name being
        // saved under and the store is moved, not rewritten. A typed string
        // with the toggle on becomes a reference and the string is filed. With
        // the toggle off it stays in the config as it always did.
        let (stored_dsn, secret) = if keychain::is_reference(dsn) {
            if !self.keychain {
                // Moving it back into the config would mean reading the store
                // to find what to write, which is not something a form should
                // do behind a toggle. Retyping the string says it deliberately.
                return Err(format!(
                    "To move this out of the {}, clear the connection string and type it again",
                    keychain::STORE_NAME
                ));
            }
            (keychain::reference(&id), None)
        } else if self.keychain {
            (keychain::reference(&id), Some(dsn.to_string()))
        } else {
            (dsn.to_string(), None)
        };

        Ok(Saved {
            source: DataSource {
                backend,
                dsn: stored_dsn,
                description: String::new(),
                read_only: self.read_only,
                open_on_start: self.open_on_start,
            },
            id,
            secret,
            previous: self.previous.clone(),
            scope: self.scope,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft<'a>(folder: &'a str, name: &'a str, dsn: &'a str) -> Draft<'a> {
        Draft {
            folder,
            name,
            dsn,
            backend: None,
            keychain: false,
            read_only: false,
            open_on_start: false,
            previous: None,
            scope: Scope::User,
        }
    }

    fn refused(draft: Draft) -> String {
        draft.build().err().expect("the draft should be refused")
    }

    #[test]
    fn an_empty_name_is_refused() {
        assert_eq!(
            refused(draft("", "  ", "postgres://h/db")),
            "A data source needs a name"
        );
    }

    #[test]
    fn the_separator_is_refused_in_the_name_and_the_folder() {
        let message = "'/' separates the folder from the name, so neither may contain it";
        assert_eq!(refused(draft("", "a/b", "postgres://h/db")), message);
        assert_eq!(refused(draft("a/b", "c", "postgres://h/db")), message);
    }

    #[test]
    fn an_empty_connection_string_is_refused() {
        assert_eq!(
            refused(draft("", "prod", " ")),
            "A data source needs a connection string"
        );
    }

    #[test]
    fn an_undetectable_driver_is_refused() {
        assert_eq!(
            refused(draft("", "prod", "not a dsn")),
            "Could not tell the driver from that connection string — pick one"
        );
    }

    #[test]
    fn a_reference_with_the_keychain_off_is_refused() {
        let mut draft = draft("", "prod", "keychain://prod");
        draft.backend = Some(Backend::Postgres);
        assert_eq!(
            refused(draft),
            format!(
                "To move this out of the {}, clear the connection string and type it again",
                keychain::STORE_NAME
            )
        );
    }

    #[test]
    fn a_folder_qualifies_the_name() {
        let saved = draft(" eimskip ", " prod ", "postgres://h/db").build().unwrap();
        assert_eq!(saved.id, "eimskip/prod");
        assert_eq!(saved.source.backend, Backend::Postgres);
    }

    #[test]
    fn a_typed_string_with_the_keychain_on_is_filed_and_referenced() {
        let mut draft = draft("eimskip", "prod", "postgres://h/db");
        draft.keychain = true;
        let saved = draft.build().unwrap();
        assert_eq!(saved.source.dsn, "keychain://eimskip/prod");
        assert_eq!(saved.secret.as_deref(), Some("postgres://h/db"));
    }

    #[test]
    fn a_typed_string_with_the_keychain_off_stays_in_the_config() {
        let saved = draft("", "prod", "postgres://h/db").build().unwrap();
        assert_eq!(saved.source.dsn, "postgres://h/db");
        assert!(saved.secret.is_none());
    }

    #[test]
    fn an_untouched_reference_is_rekeyed_and_files_nothing() {
        let mut draft = draft("", "staging", "keychain://prod");
        draft.backend = Some(Backend::Postgres);
        draft.keychain = true;
        draft.previous = Some("prod".into());
        let saved = draft.build().unwrap();
        assert_eq!(saved.source.dsn, "keychain://staging");
        assert!(saved.secret.is_none());
        assert_eq!(saved.previous.as_deref(), Some("prod"));
    }
}
