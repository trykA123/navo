use std::time::Instant;

use serde::{Serialize, de::DeserializeOwned};

use super::{Cancellation, PipeError};

#[derive(Default)]
pub(super) struct CancelEvent;

impl CancelEvent {
    pub(super) fn signal(&self) {}
}

pub(super) struct Pipe;

impl Pipe {
    pub(super) fn connect(_name: &str) -> Result<Self, PipeError> {
        Err(PipeError::NotAvailable)
    }

    pub(super) fn bind_instances(_name: &str, _sid: &str) -> Result<Vec<Self>, PipeError> {
        Err(PipeError::NotAvailable)
    }

    pub(super) fn accept(&self, cancel: &Cancellation<'_>) -> Result<(), PipeError> {
        if cancel.is_cancelled() {
            return Err(PipeError::Cancelled);
        }
        Err(PipeError::NotAvailable)
    }

    pub(super) fn disconnect(&self) -> Result<(), PipeError> {
        Err(PipeError::NotAvailable)
    }

    pub(super) fn read<T: DeserializeOwned>(
        &self,
        _deadline: Option<Instant>,
        _cancel: &Cancellation<'_>,
    ) -> Result<T, PipeError> {
        Err(PipeError::NotAvailable)
    }

    pub(super) fn write(
        &self,
        _value: &impl Serialize,
        _deadline: Option<Instant>,
    ) -> Result<(), PipeError> {
        Err(PipeError::NotAvailable)
    }
}
