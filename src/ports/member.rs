use chrono::{DateTime, Utc};
use uuid::Uuid;

#[mockall::automock]
#[async_trait::async_trait]
pub trait MemberPort {
    async fn get_member(&self, member_id: Uuid) -> Result<Member, Error>;
}

pub struct Member {
    pub member_id: Uuid,
    pub active_member: bool,
    pub membership_since: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Domain-level error when a member does not exist
    #[error("member {0} does not exist")]
    MemberDoesNotExist(Uuid),

    /// Concrete adapter errors
    ///
    /// This could represent any errors from a concrete adapter that is not part of the domain
    /// model, such as connectivity, configuration, or permission errors.
    #[error("adapter error: {0:?}")]
    Adapter(Box<dyn std::error::Error + Send + Sync>),
}
