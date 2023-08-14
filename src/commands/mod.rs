use std::{borrow::Cow, sync::Arc};

pub mod add_points;

pub struct DomainLogic<D, M> {
    database: Arc<D>,
    member: Arc<M>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("database port error: {0:?}")]
    Database(#[from] crate::ports::database::Error),
    #[error("member port error: {0:?}")]
    Member(#[from] crate::ports::member::Error),

    #[error("invalid state")]
    InvalidState(Cow<'static, str>),
}
