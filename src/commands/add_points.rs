use std::{
    borrow::Cow,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};

use crate::{
    domain::{LoyaltyEvent, Member, Tier},
    ports::{database::DatabasePort, member::MemberPort},
};
use chrono::{DateTime, Datelike, Months, Utc};
use tower::Service;
use uuid::Uuid;

use super::{DomainLogic, Error};

pub struct AddPointsRequest {
    member_id: Uuid,
    event: AddPointsEvent,
}

pub enum AddPointsEvent {
    /// The member continues their membership for another monthg
    MembershipRenewed,
    /// The member makes a purchase in a physical store
    InStorePurchase { purchase_amount: f64 },
    /// The member makes a purchase online
    OnlinePurchase { purchase_amount: f64 },
    /// Manually adding points, e.g. for support
    Manual {
        loyalty_points: u32,
        reason: Option<String>,
    },
}

impl AddPointsEvent {
    pub fn reason(&self) -> Cow<'static, str> {
        match self {
            AddPointsEvent::MembershipRenewed => "Membership renewed".into(),
            AddPointsEvent::InStorePurchase { .. } => "In-store purchase".into(),
            AddPointsEvent::OnlinePurchase { .. } => "Online purchase".into(),
            AddPointsEvent::Manual { reason, .. } => reason
                .as_ref()
                .cloned()
                .map(Into::into)
                .unwrap_or("Manual addition".into()),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct AddPointsResponse {
    pub member_id: Uuid,
    pub tier: Tier,
    /// Previous number of loyalty points
    pub old_loyalty_points: u32,
    /// New number of loyalty points
    pub new_loyalty_points: u32,
}

impl<D, M> Service<AddPointsRequest> for DomainLogic<D, M>
where
    D: DatabasePort + 'static,
    M: MemberPort + 'static,
{
    type Response = AddPointsResponse;
    type Error = Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>>>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: AddPointsRequest) -> Self::Future {
        let member = self.member.clone();
        let database = self.database.clone();
        Box::pin(async move {
            // Fetch necessary data
            let db_member = member.get_member(req.member_id).await?;
            let loyalty = database.get_loyalty_points(db_member.member_id).await?;

            // Create a Member object
            let membership_months = if db_member.active_member {
                Some(months_since(db_member.membership_since)?)
            } else {
                None
            };
            let member = Member::new(db_member.member_id, membership_months, loyalty.points);

            // Create and store the new loyalty event
            let event = create_event(&member.tier(), &req.event);
            let updated_loyalty = database
                .register_loyalty_event(member.member_id, event)
                .await?;

            // Return the response
            Ok(AddPointsResponse {
                member_id: member.member_id,
                tier: member.tier(),
                old_loyalty_points: loyalty.points,
                new_loyalty_points: updated_loyalty.points,
            })
        })
    }
}

/// Months since the provided date
fn months_since(date: DateTime<Utc>) -> Result<u32, Error> {
    let now = Utc::now();

    let months = (now.year() - date.year()) * 12 + date.month() as i32 - now.month() as i32;

    if months < 0 {
        return Err(Error::InvalidState(
            format!("start date is {} month(s) in the past", -months).into(),
        ));
    }

    Ok(months as u32)
}

fn create_event(tier: &Tier, input: &AddPointsEvent) -> LoyaltyEvent {
    const MEMBERSHIP_RENEWED_POINTS: i32 = 290;

    let delta_points = match input {
        AddPointsEvent::MembershipRenewed => MEMBERSHIP_RENEWED_POINTS,
        AddPointsEvent::InStorePurchase { purchase_amount }
        | AddPointsEvent::OnlinePurchase { purchase_amount } => {
            *purchase_amount as i32 * tier.ratio()
        }
        AddPointsEvent::Manual { loyalty_points, .. } => *loyalty_points as i32,
    };

    LoyaltyEvent {
        event_id: Uuid::new_v4(),
        delta_points,
        reason: input.reason().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{adapters::database::memory::MemoryDatabase, ports::member::MockMemberPort};
    use chrono::Duration;
    use mockall::predicate::*;
    use rstest::*;
    use speculoos::prelude::*;
    use std::sync::Arc;
    use tower::{BoxError, ServiceExt};

    /// Test all cases that generate a static number of point irregardless of tier
    #[rstest]
    #[case(AddPointsEvent::MembershipRenewed, 290)]
    #[case(AddPointsEvent::Manual { loyalty_points: 200, reason: None }, 200)]
    fn test_create_event_static(
        #[values(Tier::None, Tier::Basic, Tier::Silver, Tier::Gold, Tier::Platinum)] tier: Tier,
        #[case] input: AddPointsEvent,
        #[case] expected: i32,
    ) {
        // GIVEN a Tier and AddPointsEvent

        // WHEN calling `create_event`
        let res = create_event(&tier, &input);

        // THEN it should match the expected points amount
        assert_that!(res.delta_points).is_equal_to(expected);
    }

    /// Test all cases that generate a different number of points based on tier
    #[rstest]
    #[case(Tier::None, 0)]
    #[case(Tier::Basic, 10)]
    #[case(Tier::Silver, 12)]
    #[case(Tier::Gold, 15)]
    #[case(Tier::Platinum, 20)]
    fn test_create_event_variable(
        #[case] tier: Tier,
        #[values(AddPointsEvent::InStorePurchase { purchase_amount: 1.5 }, AddPointsEvent::OnlinePurchase { purchase_amount: 1.5 })]
        input: AddPointsEvent,
        #[case] expected: i32,
    ) {
        // GIVEN a Tier and AddPointsEvent

        // WHEN calling `create_event`
        let res = create_event(&tier, &input);

        // THEN it should match the expected points amount
        assert_that!(res.delta_points).is_equal_to(expected);
    }

    #[fixture]
    fn member_id() -> Uuid {
        Uuid::new_v4()
    }

    #[rstest]
    #[tokio::test]
    async fn test_call(member_id: Uuid) -> Result<(), BoxError> {
        // GIVEN
        // * a member port that returns information
        // * a database with existing loyalty data
        let mut member = MockMemberPort::new();
        member
            .expect_get_member()
            .times(1)
            .with(eq(member_id))
            .returning(move |_| {
                Ok(crate::ports::member::Member {
                    active_member: true,
                    member_id,
                    membership_since: Utc::now() - Duration::days(700),
                })
            });
        let database = MemoryDatabase::default();
        database
            .register_loyalty_event(
                member_id,
                LoyaltyEvent {
                    event_id: Uuid::new_v4(),
                    delta_points: 305,
                    reason: "SOME REASON".to_string(),
                },
            )
            .await?;

        let mut domain = DomainLogic {
            member: Arc::new(member),
            database: Arc::new(database.clone()),
        };

        // WHEN calling the service
        let req = AddPointsRequest {
            event: AddPointsEvent::InStorePurchase {
                purchase_amount: 3.65,
            },
            member_id,
        };
        let res = domain.ready().await?.call(req).await;

        // THEN
        // * It returns a valid response
        // * All ports are called
        assert_that!(res).is_ok().is_equal_to(AddPointsResponse {
            member_id,
            tier: Tier::Gold,
            old_loyalty_points: 305,
            new_loyalty_points: 350,
        });
        Arc::into_inner(domain.member).unwrap().checkpoint();

        Ok(())
    }
}
