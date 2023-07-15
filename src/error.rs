use thiserror::Error;

pub type ReconcileResult<T, E = ReconcileError> = std::result::Result<T, E>;

#[derive(Error, Debug)]
pub enum ReconcileError {
    #[error("create-binding-failed")]
    CreateBindingFailed,

    #[error("create-binding-object-failed")]
    CreateBindingObjectFailed,

    #[error("no-node-found")]
    NoNodeFound,
}
