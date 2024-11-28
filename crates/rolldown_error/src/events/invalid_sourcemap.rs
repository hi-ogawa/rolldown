use crate::{event_kind::EventKind, types::diagnostic_options::DiagnosticOptions};

use super::BuildEvent;

#[derive(Debug)]
pub struct InvalidSourcemap {
  pub message: String,
}

impl BuildEvent for InvalidSourcemap {
  fn kind(&self) -> crate::event_kind::EventKind {
    EventKind::InvalidSourcemap
  }

  fn message(&self, _opts: &DiagnosticOptions) -> String {
    self.message.clone()
  }
}
