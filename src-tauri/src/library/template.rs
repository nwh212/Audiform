// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (c) The GeekCast Radio Network, LLC.
// This file is part of Audiform, based on Altered Audio Editor.

//! Multitrack session templates, saved as `.altemplate`.
//!
//! A template holds a full session: tracks, buses, routing, effects,
//! markers, and optionally clips such as a standing intro. The goal is
//! that every episode starts clean instead of reusing an old episode
//! file. Creating a session from a template (a copy with fresh ids) is
//! not built yet; this is the stored format only.

use chrono::{DateTime, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::session::document::{self, Document, DocumentError};
use crate::session::model::{new_id, Id, Session};

/// A reusable starting point for new sessions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Template {
    /// Always `altaudio.template`.
    #[schemars(extend("const" = "altaudio.template"))]
    pub format: String,
    pub schema_version: u32,

    pub id: Id,
    pub name: String,
    pub description: String,
    pub created: DateTime<Utc>,
    /// The session this template produces. It carries its own
    /// `schema_version` and is upgraded with the session migrations.
    pub session: Session,
}

impl Document for Template {
    const FORMAT: &'static str = "altaudio.template";
    const CURRENT_VERSION: u32 = 1;
    const SCHEMA_FILE: &'static str = "template.schema.json";

    fn upgrade_nested(value: &mut Value) -> Result<(), DocumentError> {
        if let Some(inner) = value.get_mut("session") {
            let upgraded = document::upgrade::<Session>(inner.take())?;
            *inner = upgraded;
        }
        Ok(())
    }
}

impl Template {
    /// Wraps a session as a template.
    pub fn from_session(name: impl Into<String>, session: Session) -> Self {
        Self {
            format: Self::FORMAT.into(),
            schema_version: Self::CURRENT_VERSION,
            id: new_id(),
            name: name.into(),
            description: String::new(),
            created: Utc::now(),
            session,
        }
    }
}
