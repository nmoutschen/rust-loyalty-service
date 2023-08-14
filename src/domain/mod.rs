use uuid::Uuid;

pub struct Member {
    /// Unique identifier for the `Member`
    ///
    /// This is also used by other services.
    pub member_id: Uuid,
    /// Number of continuous months of membership
    ///
    /// This is set to `None` if the person is not an active member anymore.
    membership_months: Option<u32>,
    /// Number of accrued loyalty points
    loyalty_points: u32,
}

impl Member {
    pub fn new(member_id: Uuid, membership_months: Option<u32>, loyalty_points: u32) -> Self {
        Self {
            member_id,
            membership_months,
            loyalty_points,
        }
    }

    pub fn tier(&self) -> Tier {
        match self.membership_months {
            // Non-members
            None => Tier::None,
            // First year of continuous membership
            Some(0..=11) => Tier::Basic,
            // Second year of continuous membership
            Some(12..=23) => Tier::Silver,
            // Third year of continuous membership
            Some(24..=35) => Tier::Gold,
            // Fourth year and more
            Some(_) => Tier::Platinum,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Tier {
    None,
    Basic,
    Silver,
    Gold,
    Platinum,
}

impl Tier {
    pub fn ratio(&self) -> i32 {
        match self {
            Tier::None => 0,
            Tier::Basic => 10,
            Tier::Silver => 12,
            Tier::Gold => 15,
            Tier::Platinum => 20,
        }
    }
}

/// Loyalty data about a member
#[derive(Clone, Debug)]
pub struct Loyalty {
    pub member_id: Uuid,

    /// Current amount of loyalty points
    pub points: u32,

    /// Loyalty events for the user
    pub events: Vec<LoyaltyEvent>,
}

impl Loyalty {
    pub fn new(member_id: Uuid) -> Self {
        Self {
            member_id,
            points: 0,
            events: Vec::default(),
        }
    }
}

/// Details for a loyalty event
#[derive(Clone, Debug)]
pub struct LoyaltyEvent {
    pub event_id: Uuid,
    /// Difference in points
    ///
    /// A positive number adds points to the current total. A negative number removes from it.
    pub delta_points: i32,
    /// Message explaining the reason for this event.
    ///
    /// Since the reasons could evolve over time, we log this as a string instead of an enum.
    pub reason: String,
}
