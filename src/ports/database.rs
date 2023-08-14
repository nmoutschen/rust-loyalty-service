use uuid::Uuid;

use crate::domain::{Loyalty, LoyaltyEvent};

#[mockall::automock]
#[async_trait::async_trait]
pub trait DatabasePort {
    async fn get_loyalty_points(&self, member_id: Uuid) -> Result<Loyalty, Error>;
    async fn register_loyalty_event(
        &self,
        member_id: Uuid,
        loyalty_event: LoyaltyEvent,
    ) -> Result<Loyalty, Error>;
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Trying to remove too many loyalty points
    ///
    /// This would result in a negative number of loyalty points, which is not supported.
    #[error("trying to subtract too many points: {delta_points} from {current_points}")]
    NegativePointsTotal {
        current_points: u32,
        delta_points: i32,
    },

    /// Concrete adapter errors
    ///
    /// This could represent any errors from a concrete adapter that is not part of the domain
    /// model, such as connectivity, configuration, or permission errors.
    #[error("adapter error: {0:?}")]
    Adapter(Box<dyn std::error::Error + Send + Sync>),
}
