use soroban_sdk::{contracttype, Address, String};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Payment {
    pub id: u64,
    pub payer: Address,
    pub merchant_id: String,
    pub token: Address,
    pub amount: i128,
    pub fee_amount: i128,
    pub order_id: String,
    pub timestamp: u64,
    pub status: PaymentStatus,
}

/// Data payload of the `payment_processed` event.
///
/// It is emitted once a payment has settled, so indexers can identify the
/// asset and correlate the transaction with an application order without
/// reading contract storage separately.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PaymentProcessedEvent {
    pub payment_id: u64,
    pub payer: Address,
    pub merchant_id: String,
    pub merchant_address: Address,
    pub token: Address,
    pub order_id: String,
    /// Gross amount charged to the payer, before the platform fee is deducted.
    pub amount: i128,
    /// Portion of `amount` that is transferred to the admin fee recipient.
    pub platform_fee: i128,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PaymentStatus {
    Pending,
    Completed,
    Failed,
    Refunded,
}
