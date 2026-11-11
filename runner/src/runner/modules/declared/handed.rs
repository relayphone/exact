//! A declared list the runner keeps where an implementation is bound (LLP
//! 1133.033 §13, Charlie's 2026-10-27 ruling "Runner hands it over"). An
//! interface whose `substitute` is `"runner"` keeps its list forms (`list`,
//! `update`, `unlist`, `listed`) here even when the host binds its
//! implementation; its other names go to the implementation as any bound
//! name does (`admit.rs`'s `route`).
//!
//! - **Handed over:** each time the list changes, and once when the
//!   extension is bound, the implementation is sent `{"extension", "send":
//!   "exact:listed", "args": {"state", "entries"}}`, each entry its fields
//!   and its `ref`. The platform's side of the list (CallKit's and
//!   Core-Telecom's calls) follows it.
//! - **Chosen:** the platform's report of a person's act on its own screen
//!   (an answer on the lock screen, a hang-up from a headset) is a topic the
//!   implementation's `changed` says, `exact-choice:` then `{"extension",
//!   "state", "key", "choice"}`: the entry whose list key is `key` takes the
//!   declared choice as `tap @ref <choice>` does, its event raised with a
//!   person's origin. Only an interface the runner keeps, bound here, may
//!   say one, and only of its declared choices.
//! - **Reported:** a call the platform started, such as one a VoIP push rang
//!   before the app ran, is a topic the implementation's `changed` says,
//!   `exact-report:` then `{"extension", "send", "args"}`: the send's declared
//!   list form applies, and the send's origin and foreground rules do not,
//!   since no app code asked. Only an interface the runner keeps, bound here,
//!   may say one, and only of a send that declares a list.

use super::super::super::{DataSource, Runner};
use super::super::{Json, ModulesRow};
use crate::request::Reply;
use serde_json::json;

/// The prefix of a `changed` topic that is a bound implementation's choice
/// (module doc).
pub(in crate::runner) const CHOSEN: &str = "exact-choice:";

/// The prefix of a `changed` topic that is a bound implementation's report
/// of an entry the platform started (module doc).
pub(in crate::runner) const REPORTED: &str = "exact-report:";

impl<D: DataSource> Runner<D> {
    /// Whether the runner answers `row` though its extension is bound: a
    /// list form of an interface the runner keeps (module doc).
    pub(in crate::runner) fn kept_by_runner(&self, row: &ModulesRow) -> bool {
        let s = self.substitute(row);
        s["runner"] == true
            && ["list", "update", "unlist", "listed"]
                .iter()
                .any(|k| !s[*k].is_null())
    }

    /// The lists, by extension and state, of each bound interface the runner
    /// keeps.
    fn handed_lists(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = Vec::new();
        for row in &self.plan.modules {
            let extension = self.plan.str(row.extension);
            let s = self.substitute(row);
            let Some(state) = s["list"]["state"].as_str() else {
                continue;
            };
            let at = (extension.to_owned(), state.to_owned());
            if s["runner"] == true
                && self.extensions.bound.contains(extension)
                && !out.contains(&at)
            {
                out.push(at);
            }
        }
        out
    }

    /// Tell the bound implementation that owns list `state` its entries.
    pub(super) fn hand_over(&self, state: &str) {
        let Some(handler) = self.native_slot().handler() else {
            return;
        };
        for (extension, _) in self.handed_lists().into_iter().filter(|(_, s)| s == state) {
            let entries: Vec<Json> = (self.entries(&extension, state).into_iter())
                .map(|e| {
                    let mut fields = e.fields;
                    fields.insert("ref".into(), json!(e.reference));
                    Json::Object(fields)
                })
                .collect();
            let body = json!({ "extension": extension, "send": "exact:listed",
                "args": { "state": state, "entries": entries } });
            handler(body.to_string().into_bytes(), Reply::new(|_| {}));
        }
    }

    /// Hand every kept list to its implementation, as it is bound.
    pub(in crate::runner) fn hand_over_all(&self) {
        for (_, state) in self.handed_lists() {
            self.hand_over(&state);
        }
    }

    /// A bound implementation's choice (module doc), applied as the
    /// person's; journaled when refused.
    pub(in crate::runner) fn listed_choice(&mut self, body: &str) {
        let v: Json = serde_json::from_str(body).unwrap_or_default();
        let (Some(extension), Some(state), Some(choice)) = (
            v["extension"].as_str(),
            v["state"].as_str(),
            v["choice"].as_str(),
        ) else {
            self.log(format!("{CHOSEN} not a choice: {body}"));
            return;
        };
        let at = (extension.to_owned(), state.to_owned());
        if !self.handed_lists().contains(&at) {
            self.log(format!(
                "{extension}.{state}: a choice from an implementation whose list the runner does not keep here, refused"
            ));
            return;
        }
        let key = self.list_decl(extension, state)["key"]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let reference = (self.entries(extension, state).into_iter())
            .find(|e| e.fields.get(&key) == Some(&v["key"]))
            .map(|e| e.reference);
        let Some(reference) = reference else {
            self.log(format!(
                "{extension}.{state}: `{choice}` for {}: no such entry",
                v["key"]
            ));
            return;
        };
        if let Err(why) = self.tap_choice(extension, state, reference, Some(choice)) {
            self.log(format!("{extension}.{state}: `{choice}` refused: {why}"));
        }
    }

    /// A bound implementation's report of an entry the platform started
    /// (module doc); journaled when refused.
    pub(in crate::runner) fn listed_report(&mut self, body: &str) {
        let v: Json = serde_json::from_str(body).unwrap_or_default();
        let (Some(extension), Some(send)) = (v["extension"].as_str(), v["send"].as_str()) else {
            self.log(format!("{REPORTED} not a report: {body}"));
            return;
        };
        let row = (self.plan.modules.iter())
            .find(|row| {
                self.plan.str(row.extension) == extension && self.plan.str(row.declared) == send
            })
            .cloned();
        let state = (row.as_ref()).and_then(|row| {
            self.substitute(row)["list"]["state"]
                .as_str()
                .map(str::to_owned)
        });
        let (Some(row), Some(state)) = (row, state) else {
            self.log(format!(
                "{extension}.{send}: a report of no declared list, refused"
            ));
            return;
        };
        if !self.handed_lists().contains(&(extension.to_owned(), state)) {
            self.log(format!(
                "{extension}.{send}: a report from an implementation whose list the runner does not keep here, refused"
            ));
            return;
        }
        if let Err(why) = self.apply_effect(&row, &v["args"]) {
            self.log(format!("{extension}.{send}: report refused: {why}"));
        }
    }
}
