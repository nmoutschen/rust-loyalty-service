use crate::{
    domain::{Loyalty, LoyaltyEvent},
    ports::database::{DatabasePort, Error},
};
use std::{
    collections::{hash_map::Entry, HashMap},
    sync::{Arc, Mutex, PoisonError},
};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct MemoryDatabase {
    loyalties: Arc<Mutex<HashMap<Uuid, Loyalty>>>,
}

#[async_trait::async_trait]
impl DatabasePort for MemoryDatabase {
    async fn get_loyalty_points(&self, member_id: Uuid) -> Result<Loyalty, Error> {
        let loyalty = self
            .loyalties
            .lock()?
            .get(&member_id)
            .cloned()
            .unwrap_or_else(|| Loyalty::new(member_id));

        Ok(loyalty)
    }
    async fn register_loyalty_event(
        &self,
        member_id: Uuid,
        event: LoyaltyEvent,
    ) -> Result<Loyalty, Error> {
        let loyalty = match self.loyalties.lock()?.entry(member_id) {
            // Loyalty already exists
            Entry::Occupied(mut entry) => {
                let loyalty = entry.get_mut();
                let new_points = loyalty.points as i32 + event.delta_points;
                // Return an error if this would make the number of loyalty points negative
                if new_points < 0 {
                    return Err(Error::NegativePointsTotal {
                        current_points: loyalty.points,
                        delta_points: event.delta_points,
                    });
                }

                loyalty.points = new_points as u32;
                loyalty.events.push(event);
                loyalty.clone()
            }
            // Loyalty does not exist
            Entry::Vacant(entry) => {
                let mut loyalty = Loyalty::new(member_id);
                // Return an error if this would make the number of loyalty points negative
                if event.delta_points < 0 {
                    return Err(Error::NegativePointsTotal {
                        current_points: loyalty.points,
                        delta_points: event.delta_points,
                    });
                }
                loyalty.points = event.delta_points as u32;
                loyalty.events.push(event);
                entry.insert(loyalty.clone());
                loyalty
            }
        };

        Ok(loyalty)
    }
}

impl Default for MemoryDatabase {
    fn default() -> Self {
        Self {
            loyalties: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// Erased [`PoisonError`]
///
/// `PoisonError` keeps the `MutexGuard` internally, which is not send. Thus we erase the error
/// and only keep the string representation instead.
#[derive(Debug, thiserror::Error)]
#[error("poison error: {0}")]
pub struct ErasedPoisonError(String);

/// We need to create a custom `From` implementation here for an error that's specific to this
/// adapter.
impl<T> From<PoisonError<T>> for Error {
    fn from(err: PoisonError<T>) -> Self {
        Self::Adapter(Box::new(ErasedPoisonError(err.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use speculoos::prelude::*;

    #[tokio::test]
    async fn test_register_retrieve() {
        let database = MemoryDatabase::default();
        let loyalty = Loyalty::new(Uuid::new_v4());
        // Create the loyalty in the database
        let res = database
            .register_loyalty_event(
                loyalty.member_id,
                LoyaltyEvent {
                    event_id: Uuid::new_v4(),
                    delta_points: 5,
                    reason: "".to_string(),
                },
            )
            .await;
        assert_that!(res).is_ok().matches(|stored_loyalty| {
            stored_loyalty.member_id == loyalty.member_id && stored_loyalty.points == 5
        });
        // Retrieving the loyalty should return the updated total
        let res = database.get_loyalty_points(loyalty.member_id).await;
        assert_that!(res).is_ok().matches(|stored_loyalty| {
            stored_loyalty.member_id == loyalty.member_id && stored_loyalty.points == 5
        });
    }

    #[tokio::test]
    async fn test_negative_points_empty() {
        let database = MemoryDatabase::default();
        let res = database
            .register_loyalty_event(
                Uuid::new_v4(),
                LoyaltyEvent {
                    event_id: Uuid::new_v4(),
                    delta_points: -5,
                    reason: "".to_string(),
                },
            )
            .await;
        assert_that!(res)
            .is_err()
            .matches(|err| matches!(err, Error::NegativePointsTotal { .. }));
    }

    #[tokio::test]
    async fn test_negative_points_exists() {
        let database = MemoryDatabase::default();
        let loyalty = Loyalty::new(Uuid::new_v4());
        // Create the loyalty in the database
        let res = database
            .register_loyalty_event(
                loyalty.member_id,
                LoyaltyEvent {
                    event_id: Uuid::new_v4(),
                    delta_points: 5,
                    reason: "".to_string(),
                },
            )
            .await;
        assert_that!(res).is_ok();
        // Removing the current number of points is OK
        let res = database
            .register_loyalty_event(
                loyalty.member_id,
                LoyaltyEvent {
                    event_id: Uuid::new_v4(),
                    delta_points: -5,
                    reason: "".to_string(),
                },
            )
            .await;
        assert_that!(res).is_ok();
        // This would cause the number of points to go to -1
        let res = database
            .register_loyalty_event(
                loyalty.member_id,
                LoyaltyEvent {
                    event_id: Uuid::new_v4(),
                    delta_points: -1,
                    reason: "".to_string(),
                },
            )
            .await;
        assert_that!(res)
            .is_err()
            .matches(|err| matches!(err, Error::NegativePointsTotal { .. }));
    }
}
