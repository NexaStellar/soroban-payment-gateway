use soroban_sdk::{contractevent, contracttype, Address, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Escrow {
    pub id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub amount: i128,
    pub order_id: String,
    pub created_at: u64,
    pub release_time: u64,
    pub status: EscrowStatus,
}

/// Data payload of the `escrow_created` event.
#[contractevent(topics = ["escrow_created"])]
pub struct EscrowCreatedEvent {
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub order_id: String,
    pub amount: i128,
    /// Unix timestamp at or after which anyone may release the escrow.
    pub release_time: u64,
}

/// Data payload of the `escrow_released` event.
#[contractevent(topics = ["escrow_released"])]
pub struct EscrowReleasedEvent {
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub order_id: String,
    pub amount: i128,
}

/// Data payload of the `escrow_refunded` event.
#[contractevent(topics = ["escrow_refunded"])]
pub struct EscrowRefundedEvent {
    pub escrow_id: u64,
    pub buyer: Address,
    pub seller: Address,
    pub token: Address,
    pub order_id: String,
    pub amount: i128,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EscrowStatus {
    Active,
    Released,
    Refunded,
    Disputed,
}
